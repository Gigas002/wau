//! Row models for the `pkg`/`pkg_options`/`pkg_folder`/`pkg_dep` tables.

use chrono::{DateTime, Utc};

use crate::model::{Defn, Strategies};

/// Maps to `pkg_options` (minus the `pkg_source`/`pkg_id` foreign key columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgOptions {
    pub any_flavour: bool,
    pub any_release_type: bool,
    pub version_eq: bool,
}

/// Maps to one `pkg_folder` row (minus the foreign key columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgFolder {
    pub name: String,
}

/// Maps to one `pkg_dep` row (minus the foreign key columns) — the
/// *dependency's* id within the same source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgDep {
    pub id: String,
}

/// Maps to one `pkg_version_log` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgLoggedVersion {
    pub version: String,
    pub install_time: DateTime<Utc>,
}

/// An installed package: the `pkg` row plus its `pkg_options` (1:1),
/// `pkg_folder` (1:N), and `pkg_dep` (1:N) rows.
#[derive(Debug, Clone)]
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
