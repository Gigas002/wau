//! Cache directory inspection and cleanup.
//!
//! Everything under a profile's shared cache dir (HTTP response cache,
//! catalogue mirror, download staging, `git` source checkouts/builds) is a
//! pure, re-derivable cache — a fresh network fetch, git clone, or build
//! reproduces it. Lock files and config live under the *config* dir,
//! untouched by anything here, so [`clean`] is always safe to run in full.

use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// One cache subdirectory this module knows about, and its on-disk usage.
#[derive(Debug, Clone)]
pub struct CacheCategory {
    pub name: &'static str,
    pub description: &'static str,
    pub path: PathBuf,
    pub bytes: u64,
}

/// `(name, description, path segments relative to the cache dir)` for each
/// known category — the single source of truth [`usage`] and [`clean`]
/// both walk. A category whose directory doesn't exist yet (nothing cached,
/// or its feature isn't compiled in) still gets listed, at `0` bytes.
const CATEGORIES: &[(&str, &str, &[&str])] = &[
    (
        "http",
        "HTTP response cache (API lookups, changelogs, downloaded archives)",
        &["http"],
    ),
    (
        "catalogue-data",
        "Catalogue mirror (sparse git clone of instawow-data)",
        &["catalogue-data"],
    ),
    (
        "staging",
        "Download staging (already extracted; never re-read)",
        &["staging"],
    ),
    ("git-src", "`git` source checkouts", &["git", "git-src"]),
    (
        "git-build",
        "`git` source built zips",
        &["git", "git-build"],
    ),
];

fn category_path(cache_dir: &Path, segments: &[&str]) -> PathBuf {
    segments
        .iter()
        .fold(cache_dir.to_path_buf(), |p, s| p.join(s))
}

/// Recursively sums file sizes under `path`. `0` if `path` doesn't exist.
fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries.flatten().fold(0, |acc, entry| {
        let Ok(metadata) = entry.metadata() else {
            return acc;
        };
        acc + if metadata.is_dir() {
            dir_size(&entry.path())
        } else {
            metadata.len()
        }
    })
}

/// Per-category on-disk usage under `cache_dir`.
pub fn usage(cache_dir: &Path) -> Vec<CacheCategory> {
    CATEGORIES
        .iter()
        .map(|(name, description, segments)| {
            let path = category_path(cache_dir, segments);
            CacheCategory {
                name,
                description,
                bytes: dir_size(&path),
                path,
            }
        })
        .collect()
}

/// Removes every known cache category under `cache_dir` entirely. Always
/// safe: everything here is re-derived (a fresh network fetch, git clone,
/// or build) the next time it's needed.
pub fn clean(cache_dir: &Path) -> std::io::Result<()> {
    for (_, _, segments) in CATEGORIES {
        let path = category_path(cache_dir, segments);
        match std::fs::remove_dir_all(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
