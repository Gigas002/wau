//! Reconciliation: matching untracked, on-disk addon folders, or
//! already-installed packages, against catalogue/TOC metadata.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use rusqlite::Connection;

use crate::{
    catalogue::ComputedCatalogue,
    db::Pkg,
    model::{Defn, Flavour},
    sources::Resolver,
    toc::{self, TocFile},
};

#[cfg(test)]
mod tests;

/// An on-disk addon folder with a parsed `.toc` file.
#[derive(Debug, Clone)]
pub struct AddonFolder {
    pub path: PathBuf,
    pub toc: TocFile,
    pub name: String,
}

impl AddonFolder {
    /// Finds the flavour-appropriate (or plain) `.toc` file directly inside
    /// `parent_path` and parses it. Directory iteration order is
    /// unspecified, so if more than one file would match, which one wins is
    /// not guaranteed.
    pub fn from_path(flavour: Flavour, parent_path: &Path) -> Option<Self> {
        let mut suffixes: Vec<String> = flavour
            .toc_suffixes()
            .iter()
            .flat_map(|s| ['-', '_'].map(|sep| format!("{sep}{s}.toc").to_lowercase()))
            .collect();
        suffixes.push(".toc".to_owned());

        let entries = std::fs::read_dir(parent_path).ok()?;
        let match_path = entries.flatten().map(|e| e.path()).find(|p| {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            suffixes.iter().any(|s| name.ends_with(s.as_str()))
        })?;

        let toc = toc::parse(&match_path)?;
        let name = parent_path.file_name()?.to_str()?.to_owned();
        Some(AddonFolder {
            path: parent_path.to_path_buf(),
            toc,
            name,
        })
    }

    /// Extracts `Defn`s from this folder's `.toc` provider-id fields (e.g.
    /// `X-WoWI-ID`) — `keys_and_ids` is `(toc_key, source_id)` pairs.
    pub fn get_defns_from_toc_keys(
        &self,
        keys_and_ids: &[(&'static str, &'static str)],
    ) -> HashSet<Defn> {
        keys_and_ids
            .iter()
            .filter_map(|(key, source)| {
                self.toc
                    .x_fields
                    .get(*key)
                    .map(|id| Defn::new(*source, id.clone()))
            })
            .collect()
    }
}

fn priority_of(sources: &[Box<dyn Resolver>], source_id: &str) -> usize {
    sources
        .iter()
        .position(|r| r.metadata().id == source_id)
        .unwrap_or(usize::MAX)
}

fn toc_key_pairs(sources: &[Box<dyn Resolver>]) -> Vec<(&'static str, &'static str)> {
    sources
        .iter()
        .filter_map(|r| r.metadata().addon_toc_key.map(|k| (k, r.metadata().id)))
        .collect()
}

fn source_ids(sources: &[Box<dyn Resolver>]) -> HashSet<String> {
    sources.iter().map(|r| r.metadata().id.to_owned()).collect()
}

/// Merges any sets that share at least one element into disjoint unions
/// (union-find via repeated scanning — fine for the small counts of addon
/// folders/catalogue matches involved; not a hot path).
fn merge_intersecting_sets<T: Clone + Eq + std::hash::Hash>(
    sets: Vec<HashSet<T>>,
) -> Vec<HashSet<T>> {
    let mut merged: Vec<HashSet<T>> = Vec::new();
    for set in sets {
        let mut combined = set;
        let mut i = 0;
        while i < merged.len() {
            if !merged[i].is_disjoint(&combined) {
                let existing = merged.remove(i);
                combined.extend(existing);
            } else {
                i += 1;
            }
        }
        merged.push(combined);
    }
    merged
}

/// Every addon folder under `addon_dir` not already tracked in `pkg_folder`,
/// that isn't a symlink, and successfully parses as an [`AddonFolder`] for
/// `flavour` — the working set fed to the reconciliation matchers.
pub fn get_unreconciled_folders(
    conn: &Connection,
    addon_dir: &Path,
    flavour: Flavour,
) -> Vec<AddonFolder> {
    let tracked: HashSet<String> = {
        let Ok(mut stmt) = conn.prepare("SELECT name FROM pkg_folder") else {
            return Vec::new();
        };
        stmt.query_map([], |row| row.get::<_, String>(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    };

    let Ok(entries) = std::fs::read_dir(addon_dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !tracked.contains(&name)
                && e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && !e.path().is_symlink()
        })
        .filter_map(|e| AddonFolder::from_path(flavour, &e.path()))
        .collect()
}

/// One reconciliation match: a group of folders and the priority-sorted
/// `Defn` candidates they resolved to.
#[derive(Debug, Clone)]
pub struct MatcherGroup {
    pub folders: Vec<AddonFolder>,
    pub defns: Vec<Defn>,
}

/// Most precise pass: matches folders via `.toc` provider-id keys (e.g.
/// `X-Curse-Project-ID`), expanded with the catalogue's `same_as`
/// cross-references, then merges folders whose defn sets overlap.
pub fn match_toc_source_ids(
    leftovers: &[AddonFolder],
    catalogue: &ComputedCatalogue,
    sources: &[Box<dyn Resolver>],
) -> Vec<MatcherGroup> {
    let keyed = catalogue.keyed_entries();
    let known_sources = source_ids(sources);
    let toc_pairs = toc_key_pairs(sources);

    let mut sorted_leftovers: Vec<&AddonFolder> = leftovers.iter().collect();
    sorted_leftovers.sort_by(|a, b| a.name.cmp(&b.name));

    let mut matches: Vec<(&AddonFolder, HashSet<Defn>)> = Vec::new();
    for addon in sorted_leftovers {
        let base_defns = addon.get_defns_from_toc_keys(&toc_pairs);
        if base_defns.is_empty() {
            continue;
        }
        let mut expanded = base_defns.clone();
        for defn in &base_defns {
            if let Some(entry) = keyed.get(&(defn.source.as_str(), defn.alias.as_str())) {
                for key in &entry.same_as {
                    if known_sources.contains(&key.source) {
                        expanded.insert(Defn::new(key.source.clone(), key.id.clone()));
                    }
                }
            }
        }
        matches.push((addon, expanded));
    }

    let merged = merge_intersecting_sets(matches.iter().map(|(_, d)| d.clone()).collect());
    let mut defn_to_group: HashMap<Defn, usize> = HashMap::new();
    for (i, set) in merged.iter().enumerate() {
        for d in set {
            defn_to_group.insert(d.clone(), i);
        }
    }

    let mut group_folders: HashMap<usize, Vec<AddonFolder>> = HashMap::new();
    for (addon, defns) in matches {
        let Some(first) = defns.iter().next() else {
            continue;
        };
        let Some(&idx) = defn_to_group.get(first) else {
            continue;
        };
        group_folders.entry(idx).or_default().push(addon.clone());
    }

    group_folders
        .into_iter()
        .map(|(idx, folders)| {
            let mut defns: Vec<Defn> = merged[idx].iter().cloned().collect();
            defns.sort_by_key(|d| priority_of(sources, &d.source));
            MatcherGroup { folders, defns }
        })
        .collect()
}

/// Middle pass: matches catalogue entries whose known co-installed
/// `folders` sets intersect the leftovers, preferring matches covering more
/// folders and, among ties, higher-priority sources.
pub fn match_folder_name_subsets(
    leftovers: &[AddonFolder],
    flavour: Flavour,
    catalogue: &ComputedCatalogue,
    sources: &[Box<dyn Resolver>],
) -> Vec<MatcherGroup> {
    let leftovers_by_name: HashMap<String, AddonFolder> = leftovers
        .iter()
        .map(|a| (a.name.clone(), a.clone()))
        .collect();
    let leftover_names: HashSet<&String> = leftovers_by_name.keys().collect();

    let mut matches: Vec<(HashSet<String>, Defn)> = Vec::new();
    for entry in &catalogue.entries {
        if !entry.game_flavours.contains(&flavour) {
            continue;
        }
        for folder_set in &entry.folders {
            let intersect: HashSet<String> = folder_set
                .iter()
                .filter(|n| leftover_names.contains(n))
                .cloned()
                .collect();
            if !intersect.is_empty() {
                matches.push((intersect, Defn::new(entry.source.clone(), entry.id.clone())));
            }
        }
    }

    let merged = merge_intersecting_sets(matches.iter().map(|(f, _)| f.clone()).collect());
    let mut folder_to_group: HashMap<String, usize> = HashMap::new();
    for (i, set) in merged.iter().enumerate() {
        for name in set {
            folder_to_group.insert(name.clone(), i);
        }
    }

    let mut groups: HashMap<usize, Vec<(HashSet<String>, Defn)>> = HashMap::new();
    for (folders, defn) in matches {
        let Some(first) = folders.iter().next() else {
            continue;
        };
        let Some(&idx) = folder_to_group.get(first) else {
            continue;
        };
        groups.entry(idx).or_default().push((folders, defn));
    }

    groups
        .into_iter()
        .map(|(idx, group_matches)| {
            let mut folders: Vec<AddonFolder> = merged[idx]
                .iter()
                .filter_map(|n| leftovers_by_name.get(n).cloned())
                .collect();
            folders.sort_by(|a, b| a.name.cmp(&b.name));

            let mut sorted_matches = group_matches;
            sorted_matches.sort_by_key(|(f, d)| {
                (std::cmp::Reverse(f.len()), priority_of(sources, &d.source))
            });

            let mut seen = HashSet::new();
            let mut defns = Vec::new();
            for (_, d) in sorted_matches {
                if seen.insert(d.clone()) {
                    defns.push(d);
                }
            }

            MatcherGroup { folders, defns }
        })
        .collect()
}

/// Loosest pass: exact match on normalised (casefolded, non-alphanumerics
/// stripped) addon folder name against catalogue entry name.
pub fn match_addon_names_with_folder_names(
    leftovers: &[AddonFolder],
    catalogue: &ComputedCatalogue,
) -> Vec<MatcherGroup> {
    let mut by_name: HashMap<String, Vec<&crate::catalogue::CatalogueEntry>> = HashMap::new();
    for entry in &catalogue.entries {
        by_name
            .entry(crate::catalogue::normalise_name(&entry.name))
            .or_default()
            .push(entry);
    }

    let mut sorted_leftovers: Vec<&AddonFolder> = leftovers.iter().collect();
    sorted_leftovers.sort_by(|a, b| a.name.cmp(&b.name));

    sorted_leftovers
        .into_iter()
        .filter_map(|addon| {
            let key = crate::catalogue::normalise_name(&addon.name);
            let entries = by_name.get(&key)?;
            let mut seen = HashSet::new();
            let mut defns = Vec::new();
            for e in entries {
                let d = Defn::new(e.source.clone(), e.id.clone());
                if seen.insert(d.clone()) {
                    defns.push(d);
                }
            }
            (!defns.is_empty()).then_some(MatcherGroup {
                folders: vec![addon.clone()],
                defns,
            })
        })
        .collect()
}

/// For already-installed packages, finds `Defn`s of the same addon on other
/// sources (via catalogue `same_as` and the installed folders' own `.toc`
/// provider-id keys). Keyed by `(pkg.source, pkg.id)` since [`Pkg`]
/// deliberately has no structural equality.
pub fn find_equivalent_pkg_defns(
    pkgs: &[Pkg],
    addon_dir: &Path,
    flavour: Flavour,
    catalogue: &ComputedCatalogue,
    sources: &[Box<dyn Resolver>],
) -> HashMap<(String, String), Vec<Defn>> {
    let keyed = catalogue.keyed_entries();
    let toc_pairs = toc_key_pairs(sources);

    let mut result = HashMap::new();
    for pkg in pkgs {
        let mut defns: HashSet<Defn> = HashSet::new();

        if let Some(entry) = keyed.get(&(pkg.source.as_str(), pkg.id.as_str())) {
            for s in &entry.same_as {
                defns.insert(Defn::new(s.source.clone(), s.id.clone()));
            }
        }

        for folder in &pkg.folders {
            let path = addon_dir.join(&folder.name);
            if let Some(addon) = AddonFolder::from_path(flavour, &path) {
                for d in addon.get_defns_from_toc_keys(&toc_pairs) {
                    if d.source != pkg.source {
                        defns.insert(d);
                    }
                }
            }
        }

        if !defns.is_empty() {
            let mut sorted: Vec<Defn> = defns.into_iter().collect();
            sorted.sort_by_key(|d| priority_of(sources, &d.source));
            result.insert((pkg.source.clone(), pkg.id.clone()), sorted);
        }
    }
    result
}
