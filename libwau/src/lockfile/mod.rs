//! TOML lock-file persistence for installed packages — one `lock.toml` per
//! profile, sibling to its `profile.toml`, in the spirit of `Cargo.lock`.
//!
//! Replaces the prior SQLite `pkg`/`pkg_options`/`pkg_folder`/`pkg_dep`
//! tables: [`LockFile`] reads the whole file into memory on
//! [`LockFile::open`] and rewrites it atomically (temp file + rename, same
//! directory) on every [`LockFile::save`] — there is no separate "connection"
//! object, no transactions, and no cross-process locking. Two `wau`
//! processes writing the same profile concurrently can race (last `save()`
//! wins); this is a known trade-off of dropping SQLite, not an oversight.
//!
//! See `examples/profiles/example/lock.toml` for an annotated sample of the
//! on-disk format.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod models;
mod queries;

#[cfg(test)]
mod tests;

pub use models::{Pkg, PkgDep, PkgFolder, PkgLoggedVersion, PkgOptions};

/// The lock-file format version this build writes and reads. There are no
/// older versions yet — this only exists so a future format change has
/// somewhere to hang a migration off of, the same role `PRAGMA user_version`
/// played for the SQLite schema.
pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML (read): {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("TOML (write): {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error(
        "lock file version {found} is newer than the version this build of wau supports ({supported})"
    )]
    UnsupportedVersion { found: u32, supported: u32 },
}

/// On-disk shape of one `[[package]]` entry — `Pkg` minus the wrapper
/// structs around `folders`/`deps`, which serialize as plain string arrays
/// here for readability (`folders = ["Foo", "Foo_Config"]` rather than an
/// array of one-field tables).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RawPkg {
    source: String,
    id: String,
    slug: String,
    name: String,
    description: String,
    url: String,
    download_url: String,
    date_published: DateTime<Utc>,
    version: String,
    changelog_url: String,
    options: PkgOptions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folders: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    deps: Vec<String>,
}

impl From<Pkg> for RawPkg {
    fn from(p: Pkg) -> Self {
        RawPkg {
            source: p.source,
            id: p.id,
            slug: p.slug,
            name: p.name,
            description: p.description,
            url: p.url,
            download_url: p.download_url,
            date_published: p.date_published,
            version: p.version,
            changelog_url: p.changelog_url,
            options: p.options,
            folders: p.folders.into_iter().map(|f| f.name).collect(),
            deps: p.deps.into_iter().map(|d| d.id).collect(),
        }
    }
}

impl From<RawPkg> for Pkg {
    fn from(r: RawPkg) -> Self {
        Pkg {
            source: r.source,
            id: r.id,
            slug: r.slug,
            name: r.name,
            description: r.description,
            url: r.url,
            download_url: r.download_url,
            date_published: r.date_published,
            version: r.version,
            changelog_url: r.changelog_url,
            options: r.options,
            folders: r
                .folders
                .into_iter()
                .map(|name| PkgFolder { name })
                .collect(),
            deps: r.deps.into_iter().map(|id| PkgDep { id }).collect(),
        }
    }
}

/// One `[[version_log]]` entry — kept at the file's root, not nested under
/// its package, since it deliberately survives that package's removal (an
/// addon's install history outlives any one install of it).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoggedVersion {
    pkg_source: String,
    pkg_id: String,
    version: String,
    install_time: DateTime<Utc>,
}

/// On-disk shape of `lock.toml` as a whole.
#[derive(Debug, Default, Serialize, Deserialize)]
struct LockFileData {
    version: u32,
    #[serde(default, rename = "package", skip_serializing_if = "Vec::is_empty")]
    packages: Vec<Pkg>,
    #[serde(default, rename = "version_log", skip_serializing_if = "Vec::is_empty")]
    version_log: Vec<LoggedVersion>,
}

/// In-memory installed-package store for one profile, backed by `lock.toml`.
/// All queries/mutations operate purely in memory; call [`Self::save`] to
/// persist. Mutating helpers used by `pkg_management` call it for you after
/// each higher-level operation.
#[derive(Debug)]
pub struct LockFile {
    path: Option<PathBuf>,
    packages: Vec<Pkg>,
    version_log: Vec<LoggedVersion>,
}

impl LockFile {
    /// Opens `path`, or starts from an empty in-memory state if it doesn't
    /// exist yet — nothing is written to disk until the first [`Self::save`].
    pub fn open(path: &Path) -> Result<Self, LockError> {
        let data = match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str::<LockFileData>(&s)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => LockFileData::default(),
            Err(e) => return Err(e.into()),
        };
        if data.version > CURRENT_VERSION {
            return Err(LockError::UnsupportedVersion {
                found: data.version,
                supported: CURRENT_VERSION,
            });
        }

        Ok(Self {
            path: Some(path.to_path_buf()),
            packages: data.packages,
            version_log: data.version_log,
        })
    }

    /// An empty, disk-less lock file — for tests. [`Self::save`] is a no-op.
    pub fn in_memory() -> Self {
        Self {
            path: None,
            packages: Vec::new(),
            version_log: Vec::new(),
        }
    }

    /// Writes the current state to `path` atomically (write to a sibling
    /// temp file, then rename over the target). A no-op for
    /// [`Self::in_memory`] instances.
    pub fn save(&self) -> Result<(), LockError> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        let mut packages = self.packages.clone();
        packages.sort_by(|a, b| (&a.source, &a.id).cmp(&(&b.source, &b.id)));
        let mut version_log = self.version_log.clone();
        version_log.sort_by(|a, b| {
            (&a.pkg_source, &a.pkg_id, &a.version).cmp(&(&b.pkg_source, &b.pkg_id, &b.version))
        });

        let data = LockFileData {
            version: CURRENT_VERSION,
            packages,
            version_log,
        };
        let rendered = toml::to_string_pretty(&data)?;

        let tmp_path = path.with_file_name(format!(
            "{}.tmp",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&tmp_path, rendered)?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }
}
