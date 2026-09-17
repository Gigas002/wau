//! Query/mutation methods on [`super::LockFile`] — in-memory equivalents of
//! the old `db::queries` SQL, operating over `Vec<Pkg>` instead of tables.
//! Infallible except for [`super::LockFile::save`]/`open`, since there's no
//! I/O on the read/mutate path anymore.

use chrono::Utc;

use super::models::{Pkg, PkgLoggedVersion};
use super::{LockFile, LoggedVersion};
use crate::model::Defn;

/// The alias/id/slug match [`LockFile::get_pkgs`]/[`LockFile::check_pkgs_not_exist`]
/// share: a `defn` matches a package when the package's `id` equals the
/// defn's `alias` *or* its `id`, or the package's `slug` case-insensitively
/// equals the defn's `alias`.
fn matches_defn(pkg: &Pkg, defn: &Defn) -> bool {
    pkg.source == defn.source
        && (pkg.id == defn.alias
            || defn.id.as_deref() == Some(pkg.id.as_str())
            || pkg.slug.eq_ignore_ascii_case(&defn.alias))
}

impl LockFile {
    fn find_by_defn(&self, defn: &Defn) -> Option<&Pkg> {
        self.packages.iter().find(|p| matches_defn(p, defn))
    }

    /// Looks up one package per `defn`, in the same order, `None` where no
    /// package is installed for that definition.
    pub fn get_pkgs(&self, defns: &[Defn]) -> Vec<Option<Pkg>> {
        defns
            .iter()
            .map(|d| self.find_by_defn(d).cloned())
            .collect()
    }

    /// Every installed package.
    pub fn get_all_pkgs(&self) -> Vec<Pkg> {
        self.packages.clone()
    }

    /// For each `defn`, `true` if no package is currently installed for it.
    pub fn check_pkgs_not_exist(&self, defns: &[Defn]) -> Vec<bool> {
        defns
            .iter()
            .map(|d| self.find_by_defn(d).is_none())
            .collect()
    }

    /// Last 10 logged versions for a package, most recent first.
    pub fn get_pkg_logged_versions(&self, source: &str, id: &str) -> Vec<PkgLoggedVersion> {
        let mut versions: Vec<PkgLoggedVersion> = self
            .version_log
            .iter()
            .filter(|v| v.pkg_source == source && v.pkg_id == id)
            .map(|v| PkgLoggedVersion {
                version: v.version.clone(),
                install_time: v.install_time,
            })
            .collect();
        versions.sort_by_key(|v| std::cmp::Reverse(v.install_time));
        versions.truncate(10);
        versions
    }

    /// Upserts `pkg` and logs its version (skipped if that exact
    /// `(source, id, version)` is already logged, so reinstalling the same
    /// version doesn't duplicate/refresh the log entry). Callers persist via
    /// [`Self::save`] once they're done batching mutations.
    pub fn insert_pkg(&mut self, pkg: Pkg) {
        let already_logged = self
            .version_log
            .iter()
            .any(|v| v.pkg_source == pkg.source && v.pkg_id == pkg.id && v.version == pkg.version);
        if !already_logged {
            self.version_log.push(LoggedVersion {
                pkg_source: pkg.source.clone(),
                pkg_id: pkg.id.clone(),
                version: pkg.version.clone(),
                install_time: Utc::now(),
            });
        }

        self.packages
            .retain(|p| !(p.source == pkg.source && p.id == pkg.id));
        self.packages.push(pkg);
    }

    /// Removes the package matching `(source, id)`, if any. `version_log`
    /// entries are untouched (deliberately: no FK-equivalent, survives
    /// deletion so reinstall history is preserved).
    pub fn delete_pkg(&mut self, source: &str, id: &str) {
        self.packages
            .retain(|p| !(p.source == source && p.id == id));
    }

    /// Flips `options.version_eq` for an installed package, returning the
    /// new value, or `None` if no such package is installed. This only sets
    /// the flag — the pinned version is whatever `pkg.version` already
    /// holds, there's no separate stored pin target.
    pub fn pin_pkg(&mut self, source: &str, id: &str, version_eq: bool) -> Option<bool> {
        let pkg = self
            .packages
            .iter_mut()
            .find(|p| p.source == source && p.id == id)?;
        pkg.options.version_eq = version_eq;
        Some(pkg.options.version_eq)
    }

    /// Installed packages that own any of `folder_names` (the
    /// `mutate_install` conflict check: a new archive can't claim a folder
    /// another package tracks).
    pub fn find_pkgs_owning_folders(&self, folder_names: &[String]) -> Vec<Pkg> {
        self.packages
            .iter()
            .filter(|p| p.folders.iter().any(|f| folder_names.contains(&f.name)))
            .cloned()
            .collect()
    }

    /// Same conflict check as [`Self::find_pkgs_owning_folders`], excluding
    /// the package being updated. Uses an AND (not OR) exclusion: a
    /// *different* package that happens to share `exclude_source` (with a
    /// different id) is also excluded from the results — this is
    /// intentional, ported verbatim from the original SQL quirk, not a bug.
    pub fn find_pkgs_owning_folders_excluding(
        &self,
        folder_names: &[String],
        exclude_source: &str,
        exclude_id: &str,
    ) -> Vec<Pkg> {
        self.packages
            .iter()
            .filter(|p| p.source != exclude_source && p.id != exclude_id)
            .filter(|p| p.folders.iter().any(|f| folder_names.contains(&f.name)))
            .cloned()
            .collect()
    }
}
