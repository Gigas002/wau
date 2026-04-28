//! Addon filesystem operations: discovery, extraction, install, and removal.
//!
//! This module is the sole place that talks to the filesystem for addon-related
//! work. `.toc` parsing is delegated to [`crate::toc`].

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use zip::ZipArchive;

use crate::toc::TocFile;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Installed-addon inventory
// ---------------------------------------------------------------------------

/// An addon directory under `Interface/AddOns` that contains at least one `.toc` file.
#[derive(Debug, Clone)]
pub struct InstalledAddon {
    /// Directory name (not a full path) — e.g. `"WeakAuras"`.
    pub folder: String,
    /// Parsed `.toc` files found directly inside this directory.
    pub toc_files: Vec<TocFile>,
}

impl InstalledAddon {
    /// Returns the best available display title, falling back to the folder name.
    pub fn display_title(&self) -> &str {
        self.toc_files
            .iter()
            .find_map(|t| t.title.as_deref())
            .unwrap_or(&self.folder)
    }

    /// Returns the version string from the first `.toc` that has one.
    pub fn version(&self) -> Option<&str> {
        self.toc_files.iter().find_map(|t| t.version.as_deref())
    }

    /// Returns all unique interface version numbers across all `.toc` files, sorted ascending.
    pub fn all_interface_versions(&self) -> Vec<u32> {
        let mut versions: Vec<u32> = self
            .toc_files
            .iter()
            .flat_map(|t| t.interface.iter().copied())
            .collect();
        versions.sort_unstable();
        versions.dedup();
        versions
    }
}

/// Scans `addons_dir` and returns one [`InstalledAddon`] per subdirectory that
/// contains at least one `.toc` file. Results are sorted alphabetically.
///
/// Unreadable entries and unparsable `.toc` files are silently skipped (best-effort).
/// Returns `Err` only when `addons_dir` itself cannot be read.
pub fn scan(addons_dir: &Path) -> crate::Result<Vec<InstalledAddon>> {
    let mut addons = Vec::new();

    for entry in fs::read_dir(addons_dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!(error = %e, "skipping unreadable entry in addons dir");
                continue;
            }
        };

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let folder = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };

        let toc_files = collect_toc_files(&path);
        if toc_files.is_empty() {
            continue;
        }

        addons.push(InstalledAddon { folder, toc_files });
    }

    addons.sort_by(|a, b| a.folder.cmp(&b.folder));
    Ok(addons)
}

// ---------------------------------------------------------------------------
// Zip extraction
// ---------------------------------------------------------------------------

/// Extracts `zip_path` into `dest` and returns the paths of top-level directories
/// that contain at least one `.toc` file (the installable addon directories).
///
/// `dest` must already exist. The returned paths are absolute and sorted.
pub fn extract_addon_zip(zip_path: &Path, dest: &Path) -> crate::Result<Vec<PathBuf>> {
    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let out_path = match entry.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = fs::File::create(&out_path)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            out_file.write_all(&buf)?;
        }
    }

    let mut addon_dirs = Vec::new();
    for entry in fs::read_dir(dest)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && dir_has_toc(&path) {
            addon_dirs.push(path);
        }
    }
    addon_dirs.sort();
    Ok(addon_dirs)
}

// ---------------------------------------------------------------------------
// Addon dir install / remove
// ---------------------------------------------------------------------------

/// Copies each directory in `addon_dirs` into `addons_path`, replacing any
/// existing directory with the same name. Returns the installed directory names.
///
/// `addons_path` is created if it does not already exist.
pub fn install_addon_dirs(
    addon_dirs: &[PathBuf],
    addons_path: &Path,
) -> crate::Result<Vec<String>> {
    fs::create_dir_all(addons_path)?;

    let mut installed = Vec::with_capacity(addon_dirs.len());
    for dir in addon_dirs {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let dest = addons_path.join(&name);
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        copy_dir_all(dir, &dest)?;
        tracing::debug!(dir = %name, "installed addon dir");
        installed.push(name);
    }
    Ok(installed)
}

/// Removes each directory name in `dir_names` from `addons_path`, silently
/// skipping names that are already absent.
pub fn remove_addon_dirs(dir_names: &[String], addons_path: &Path) -> crate::Result<()> {
    for name in dir_names {
        let path = addons_path.join(name);
        if path.exists() {
            fs::remove_dir_all(&path)?;
            tracing::debug!(path = %path.display(), "removed addon dir");
        } else {
            tracing::debug!(path = %path.display(), "addon dir already absent");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `dir` contains at least one `.toc` file at its top level.
fn dir_has_toc(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("toc"))
        })
        .unwrap_or(false)
}

/// Recursively copies the `src` directory tree into `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> crate::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Creates a zip archive in memory containing the given `(path, content)` entries.
/// Entries ending with `/` are added as directory entries.
#[cfg(test)]
pub(crate) fn make_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Cursor;
    use zip::{ZipWriter, write::FileOptions};
    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        if name.ends_with('/') {
            zip.add_directory(*name, opts).unwrap();
        } else {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}

/// Collects and parses all `.toc` files immediately inside `addon_dir`.
fn collect_toc_files(addon_dir: &Path) -> Vec<TocFile> {
    let entries = match fs::read_dir(addon_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(dir = %addon_dir.display(), error = %e, "cannot read addon directory");
            return Vec::new();
        }
    };

    let mut toc_files: Vec<TocFile> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("toc") {
                crate::toc::parse(&p)
            } else {
                None
            }
        })
        .collect();

    toc_files.sort_by(|a, b| a.path.cmp(&b.path));
    toc_files
}
