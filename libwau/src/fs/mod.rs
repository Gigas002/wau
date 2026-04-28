//! Addon filesystem operations: `Interface/AddOns` discovery and scanning.
//!
//! `.toc` file parsing is delegated to [`crate::toc`].

use std::{fs, path::Path};

use crate::toc::TocFile;

#[cfg(test)]
mod tests;

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

/// Scans `addons_dir` and returns one [`InstalledAddon`] per subdirectory that contains
/// at least one `.toc` file.  Results are sorted alphabetically by folder name.
///
/// Unreadable subdirectories and unparsable `.toc` files are silently skipped (best-effort).
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
// Internal helpers
// ---------------------------------------------------------------------------

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
