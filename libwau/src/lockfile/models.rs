//! Row-equivalent models for the installed-package lock file.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{Defn, Strategies};

/// Maps to one package's `[package.options]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgOptions {
    pub any_flavour: bool,
    pub any_release_type: bool,
    pub version_eq: bool,
}

/// One folder a package owns on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgFolder {
    pub name: String,
}

/// One of a package's dependencies — the *dependency's* id within the same source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgDep {
    pub id: String,
}

/// One entry from the install-history log (see [`super::LockFile::get_pkg_logged_versions`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgLoggedVersion {
    pub version: String,
    pub install_time: DateTime<Utc>,
}

/// An installed package: its own fields plus `options` (1:1), `folders`
/// (1:N), and `deps` (1:N) — one `[[package]]` entry in `lock.toml`.
///
/// (De)serializes through [`super::RawPkg`] (`#[serde(into, from)]`) so the
/// on-disk shape can store `folders`/`deps` as plain string arrays while this
/// type keeps the richer wrapper structs its callers already expect.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(into = "super::RawPkg", from = "super::RawPkg")]
pub struct Pkg {
    pub source: String,
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub download_url: String,
    pub date_published: DateTime<Utc>,
    pub version: String,
    pub changelog_url: String,
    pub options: PkgOptions,
    pub folders: Vec<PkgFolder>,
    pub deps: Vec<PkgDep>,
}

impl Pkg {
    /// Reconstructs the [`Defn`] that would resolve back to this installed
    /// package. `pkg_options.version_eq` is just a bool flag — the pinned
    /// version string itself is derived from `self.version` here, since
    /// there's no separate "pinned version" column: pinning means "trust the
    /// currently-installed version."
    pub fn to_defn(&self) -> Defn {
        Defn {
            source: self.source.clone(),
            alias: self.slug.clone(),
            id: Some(self.id.clone()),
            strategies: Strategies {
                any_flavour: self.options.any_flavour,
                any_release_type: self.options.any_release_type,
                version_eq: self.options.version_eq.then(|| self.version.clone()),
            },
        }
    }
}
