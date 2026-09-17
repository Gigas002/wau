//! Install/update/remove/replace/pin orchestration.
//!
//! Download progress is reported through `Ctx::progress` (see
//! [`crate::progress`]); nothing else here reports progress, and there's no
//! locking beyond per-URL download deduplication (`DownloadLocks`) —
//! operations aren't otherwise serialized against each other.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::{Path, PathBuf},
};

use crate::{
    fs as trash_fs,
    http::HttpClient,
    lockfile::{LockFile, Pkg, PkgDep, PkgFolder, PkgOptions},
    model::{Defn, Flavour, HeadersIntent, Strategy},
    pkg_archives::{self, DownloadLocks},
    progress::ProgressBus,
    results::{AnyOutcome, Failure, InternalError, ManagerError, PkgRef},
    sources::{PkgCandidate, Resolver, find_source},
};

#[cfg(test)]
mod tests;

/// The shared, read-only collaborators every orchestration function needs.
/// `LockFile` is passed separately since mutating operations need `&mut`.
pub struct Ctx<'a> {
    pub http: &'a HttpClient,
    pub sources: &'a [Box<dyn Resolver>],
    pub download_locks: &'a DownloadLocks,
    pub addon_dir: &'a Path,
    pub cache_dir: &'a Path,
    pub progress: &'a ProgressBus,
    pub flavour: Flavour,
}

/// A successful package operation outcome.
#[derive(Debug, Clone)]
pub enum Outcome {
    PkgInstalled {
        pkg: Pkg,
        dry_run: bool,
    },
    PkgUpdated {
        old: Pkg,
        new: Box<Pkg>,
        dry_run: bool,
    },
    PkgRemoved {
        pkg: Pkg,
    },
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::PkgInstalled { pkg, dry_run } => {
                let verb = if *dry_run {
                    "would have installed"
                } else {
                    "installed"
                };
                write!(f, "{verb} {}", pkg.version)
            }
            Outcome::PkgUpdated { old, new, dry_run } => {
                let verb = if *dry_run {
                    "would have updated"
                } else {
                    "updated"
                };
                write!(f, "{verb} {} to {}", old.version, new.version)?;
                if old.slug != new.slug {
                    write!(f, " with new slug {:?}", new.slug)?;
                }
                if old.options != new.options {
                    let diff = strategies_diff(&old.options, &new.options, &new.version);
                    if !diff.is_empty() {
                        write!(f, " with new strategies: {diff}")?;
                    }
                }
                Ok(())
            }
            Outcome::PkgRemoved { .. } => write!(f, "removed"),
        }
    }
}

fn strategies_diff(old: &PkgOptions, new: &PkgOptions, new_version: &str) -> String {
    let mut parts = Vec::new();
    if new.any_flavour && !old.any_flavour {
        parts.push("any_flavour=True".to_owned());
    }
    if new.any_release_type && !old.any_release_type {
        parts.push("any_release_type=True".to_owned());
    }
    if new.version_eq && !old.version_eq {
        parts.push(format!("version_eq={new_version:?}"));
    }
    parts.join("; ")
}

/// Splits a batch resolve result into successes and failures.
pub fn split_results<T>(
    results: HashMap<Defn, AnyOutcome<T>>,
) -> (HashMap<Defn, T>, HashMap<Defn, Failure>) {
    let mut oks = HashMap::new();
    let mut errs = HashMap::new();
    for (defn, result) in results {
        match result {
            Ok(v) => {
                oks.insert(defn, v);
            }
            Err(e) => {
                errs.insert(defn, e);
            }
        }
    }
    (oks, errs)
}

fn pkg_to_ref(pkg: &Pkg) -> PkgRef {
    PkgRef {
        source: pkg.source.clone(),
        id: pkg.id.clone(),
        name: pkg.name.clone(),
    }
}

fn list_dir_names(dir: &Path) -> HashSet<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn build_pkg(defn: &Defn, candidate: &PkgCandidate, folders: Vec<String>) -> Pkg {
    Pkg {
        source: defn.source.clone(),
        id: candidate.id.clone(),
        slug: candidate.slug.clone(),
        name: candidate.name.clone(),
        description: candidate.description.clone(),
        url: candidate.url.clone(),
        download_url: candidate.download_url.clone(),
        date_published: candidate.date_published,
        version: candidate.version.clone(),
        changelog_url: candidate.changelog_url.clone(),
        options: PkgOptions {
            any_flavour: defn.strategies.any_flavour,
            any_release_type: defn.strategies.any_release_type,
            version_eq: defn.strategies.version_eq.is_some(),
        },
        folders: folders.into_iter().map(|name| PkgFolder { name }).collect(),
        deps: candidate
            .deps
            .iter()
            .map(|id| PkgDep { id: id.clone() })
            .collect(),
    }
}

fn download_headers(sources: &[Box<dyn Resolver>], source: &str) -> Vec<(String, String)> {
    find_source(sources, source)
        .map(|r| r.make_request_headers(HeadersIntent::Download))
        .unwrap_or_default()
}

// ============================================================================
// resolve
// ============================================================================

/// Resolves `defns` into packages, bucketed by source and resolved
/// concurrently. `with_deps` follows one level of `PkgCandidate::deps` —
/// dependencies of dependencies are not resolved.
pub async fn resolve(
    ctx: &Ctx<'_>,
    defns: &[Defn],
    with_deps: bool,
) -> HashMap<Defn, AnyOutcome<PkgCandidate>> {
    if defns.is_empty() {
        return HashMap::new();
    }

    let mut by_source: HashMap<&str, Vec<Defn>> = HashMap::new();
    for d in defns {
        by_source
            .entry(d.source.as_str())
            .or_default()
            .push(d.clone());
    }

    let futures = by_source.into_values().map(|group| async move {
        let outcomes: Vec<AnyOutcome<PkgCandidate>> =
            match find_source(ctx.sources, &group[0].source) {
                None => {
                    let err: Failure = ManagerError::PkgSourceInvalid.into();
                    group.iter().map(|_| Err(err.clone())).collect()
                }
                Some(resolver) => match resolver.get_disabled_reason() {
                    Some(reason) => {
                        let err: Failure = ManagerError::PkgSourceDisabled {
                            reason: Some(reason),
                        }
                        .into();
                        group.iter().map(|_| Err(err.clone())).collect()
                    }
                    None => resolver.resolve(ctx.http, ctx.flavour, &group).await,
                },
            };
        (group, outcomes)
    });

    let mut results = HashMap::new();
    for (group, outcomes) in futures_util::future::join_all(futures).await {
        for (defn, outcome) in group.into_iter().zip(outcomes) {
            results.insert(defn, outcome);
        }
    }

    if with_deps {
        let dep_results = resolve_deps(ctx, &results).await;
        results.extend(dep_results);
    }

    results
}

fn resolve_deps<'a>(
    ctx: &'a Ctx<'a>,
    results: &'a HashMap<Defn, AnyOutcome<PkgCandidate>>,
) -> std::pin::Pin<Box<dyn Future<Output = HashMap<Defn, AnyOutcome<PkgCandidate>>> + 'a>> {
    Box::pin(async move {
        let existing: HashSet<(String, String)> = results
            .iter()
            .filter_map(|(d, r)| r.as_ref().ok().map(|c| (d.source.clone(), c.id.clone())))
            .collect();

        let mut dep_pairs: Vec<(String, String)> = Vec::new();
        for (defn, outcome) in results {
            if let Ok(candidate) = outcome {
                for dep_id in &candidate.deps {
                    let pair = (defn.source.clone(), dep_id.clone());
                    if !existing.contains(&pair) && !dep_pairs.contains(&pair) {
                        dep_pairs.push(pair);
                    }
                }
            }
        }

        if dep_pairs.is_empty() {
            return HashMap::new();
        }

        let dep_defns: Vec<Defn> = dep_pairs
            .iter()
            .map(|(source, id)| {
                let mut d = Defn::new(source.clone(), id.clone());
                d.id = Some(id.clone());
                d
            })
            .collect();

        let resolved = resolve(ctx, &dep_defns, false).await;

        resolved
            .into_iter()
            .map(|(defn, outcome)| {
                let pretty_defn = match &outcome {
                    Ok(candidate) => {
                        let mut d = defn.clone();
                        d.alias = candidate.slug.clone();
                        d
                    }
                    Err(_) => defn.clone(),
                };
                (pretty_defn, outcome)
            })
            .collect()
    })
}

// ============================================================================
// Mutations (run sequentially, one item at a time)
// ============================================================================

fn mutate_install(
    lock: &mut LockFile,
    addon_dir: &Path,
    defn: &Defn,
    candidate: &PkgCandidate,
    archive_path: &Path,
    replace_folders: bool,
) -> AnyOutcome<Outcome> {
    let archive = pkg_archives::open_zip_archive(archive_path).map_err(InternalError::new)?;
    let folder_names: Vec<String> = archive.top_level_folders.iter().cloned().collect();

    let conflicts = lock.find_pkgs_owning_folders(&folder_names);
    if !conflicts.is_empty() {
        return Err(ManagerError::PkgConflictsWithInstalled {
            conflicting: conflicts.iter().map(pkg_to_ref).collect(),
        }
        .into());
    }

    if replace_folders {
        for name in &folder_names {
            let path = addon_dir.join(name);
            if path.exists() {
                let _ = trash_fs::trash(&path);
            }
        }
    } else {
        let existing_on_disk = list_dir_names(addon_dir);
        let unreconciled: Vec<String> = folder_names
            .iter()
            .filter(|n| existing_on_disk.contains(*n))
            .cloned()
            .collect();
        if !unreconciled.is_empty() {
            return Err(ManagerError::PkgConflictsWithUnreconciled {
                folders: unreconciled,
            }
            .into());
        }
    }

    archive.extract(addon_dir).map_err(InternalError::new)?;

    let pkg = build_pkg(defn, candidate, folder_names);
    lock.insert_pkg(pkg.clone());
    lock.save().map_err(InternalError::new)?;

    Ok(Outcome::PkgInstalled {
        pkg,
        dry_run: false,
    })
}

fn mutate_update(
    lock: &mut LockFile,
    addon_dir: &Path,
    defn: &Defn,
    old_pkg: &Pkg,
    candidate: &PkgCandidate,
    archive_path: &Path,
) -> AnyOutcome<Outcome> {
    let archive = pkg_archives::open_zip_archive(archive_path).map_err(InternalError::new)?;
    let folder_names: Vec<String> = archive.top_level_folders.iter().cloned().collect();

    let conflicts =
        lock.find_pkgs_owning_folders_excluding(&folder_names, &defn.source, &candidate.id);
    if !conflicts.is_empty() {
        return Err(ManagerError::PkgConflictsWithInstalled {
            conflicting: conflicts.iter().map(pkg_to_ref).collect(),
        }
        .into());
    }

    let old_folder_names: HashSet<String> =
        old_pkg.folders.iter().map(|f| f.name.clone()).collect();
    let new_folder_set: HashSet<String> = folder_names.iter().cloned().collect();
    let existing_on_disk = list_dir_names(addon_dir);
    let unreconciled: Vec<String> = new_folder_set
        .difference(&old_folder_names)
        .filter(|n| existing_on_disk.contains(*n))
        .cloned()
        .collect();
    if !unreconciled.is_empty() {
        return Err(ManagerError::PkgConflictsWithUnreconciled {
            folders: unreconciled,
        }
        .into());
    }

    for folder in &old_pkg.folders {
        let path = addon_dir.join(&folder.name);
        if path.exists() {
            let _ = trash_fs::trash(&path);
        }
    }
    archive.extract(addon_dir).map_err(InternalError::new)?;

    let new_pkg = build_pkg(defn, candidate, folder_names);
    lock.delete_pkg(&old_pkg.source, &old_pkg.id);
    lock.insert_pkg(new_pkg.clone());
    lock.save().map_err(InternalError::new)?;

    Ok(Outcome::PkgUpdated {
        old: old_pkg.clone(),
        new: Box::new(new_pkg),
        dry_run: false,
    })
}

fn mutate_remove(
    lock: &mut LockFile,
    addon_dir: &Path,
    pkg: &Pkg,
    keep_folders: bool,
) -> AnyOutcome<Outcome> {
    if !keep_folders {
        for folder in &pkg.folders {
            let path = addon_dir.join(&folder.name);
            if path.exists() {
                let _ = trash_fs::trash(&path);
            }
        }
    }
    lock.delete_pkg(&pkg.source, &pkg.id);
    lock.save().map_err(InternalError::new)?;

    Ok(Outcome::PkgRemoved { pkg: pkg.clone() })
}

fn mutate_pin(lock: &mut LockFile, defn: &Defn, pkg: &Pkg) -> AnyOutcome<Outcome> {
    let version_eq = defn.strategies.version_eq.is_some();
    let new_value = lock
        .pin_pkg(&pkg.source, &pkg.id, version_eq)
        .ok_or(ManagerError::PkgNotInstalled)?;
    lock.save().map_err(InternalError::new)?;
    let mut updated = pkg.clone();
    updated.options.version_eq = new_value;
    Ok(Outcome::PkgInstalled {
        pkg: updated,
        dry_run: false,
    })
}

fn check_installed_pkg_integrity(addon_dir: &Path, pkg: &Pkg) -> bool {
    pkg.folders.iter().all(|f| addon_dir.join(&f.name).exists())
}

async fn download_all(
    ctx: &Ctx<'_>,
    candidates: &HashMap<Defn, PkgCandidate>,
) -> (HashMap<Defn, PathBuf>, HashMap<Defn, Failure>) {
    let staging_dir = ctx.cache_dir.join("staging");
    let futures = candidates.iter().map(|(d, c)| {
        let headers = download_headers(ctx.sources, &d.source);
        let staging_dir = staging_dir.clone();
        async move {
            let headers: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let result = pkg_archives::download_pkg_archive(
                ctx.http,
                ctx.download_locks,
                &c.download_url,
                &headers,
                &staging_dir,
                ctx.progress,
                &c.name,
            )
            .await;
            (d.clone(), result)
        }
    });

    let mut paths = HashMap::new();
    let mut errors = HashMap::new();
    for (d, r) in futures_util::future::join_all(futures).await {
        match r {
            Ok(path) => {
                paths.insert(d, path);
            }
            Err(e) => {
                errors.insert(d, InternalError::new(e).into());
            }
        }
    }
    (paths, errors)
}

// ============================================================================
// Public operations
// ============================================================================

/// Installs `defns`, following one level of dependencies. Already-installed
/// defns are reported as `PkgAlreadyInstalled` without re-resolving.
pub async fn install(
    lock: &mut LockFile,
    ctx: &Ctx<'_>,
    defns: &[Defn],
    replace_folders: bool,
    dry_run: bool,
) -> HashMap<Defn, AnyOutcome<Outcome>> {
    if defns.is_empty() {
        return HashMap::new();
    }

    let not_installed = lock.check_pkgs_not_exist(defns);
    let to_resolve: Vec<Defn> = defns
        .iter()
        .zip(&not_installed)
        .filter(|(_, ok)| **ok)
        .map(|(d, _)| d.clone())
        .collect();

    let resolve_results = resolve(ctx, &to_resolve, true).await;
    let (candidates, resolve_errors) = split_results(resolve_results);

    let with_id: Vec<Defn> = candidates
        .iter()
        .map(|(d, c)| {
            let mut d2 = d.clone();
            d2.id = Some(c.id.clone());
            d2
        })
        .collect();
    let still_not_installed = lock.check_pkgs_not_exist(&with_id);
    let candidates: HashMap<Defn, PkgCandidate> = candidates
        .into_iter()
        .zip(still_not_installed)
        .filter(|(_, ok)| *ok)
        .map(|((d, c), _)| (d, c))
        .collect();

    let mut results: HashMap<Defn, AnyOutcome<Outcome>> = defns
        .iter()
        .map(|d| (d.clone(), Err(ManagerError::PkgAlreadyInstalled.into())))
        .collect();
    for (d, e) in resolve_errors {
        results.insert(d, Err(e));
    }

    if dry_run {
        for (d, c) in &candidates {
            let pkg = build_pkg(d, c, Vec::new());
            results.insert(d.clone(), Ok(Outcome::PkgInstalled { pkg, dry_run: true }));
        }
        return results;
    }

    let (archive_paths, download_errors) = download_all(ctx, &candidates).await;
    for (d, e) in download_errors {
        results.insert(d, Err(e));
    }

    for (d, path) in archive_paths {
        let candidate = &candidates[&d];
        let outcome = mutate_install(lock, ctx.addon_dir, &d, candidate, &path, replace_folders);
        results.insert(d, outcome);
    }

    results
}

/// What `update` targets — every installed package, or a specific list.
pub enum UpdateTarget {
    All,
    Specific(Vec<Defn>),
}

/// Updates installed packages to the latest version per their stored
/// strategies. Requesting a `Defn` that isn't installed reports
/// `PkgNotInstalled`; one that's already current reports `PkgUpToDate`.
pub async fn update(
    lock: &mut LockFile,
    ctx: &Ctx<'_>,
    target: UpdateTarget,
    dry_run: bool,
) -> HashMap<Defn, AnyOutcome<Outcome>> {
    let (defns, defns_to_pkgs, resolve_defns) = match target {
        UpdateTarget::All => {
            let all_pkgs = lock.get_all_pkgs();
            let mut defns = Vec::new();
            let mut defns_to_pkgs = HashMap::new();
            let mut resolve_defns = HashMap::new();
            for pkg in all_pkgs {
                let d = pkg.to_defn();
                defns.push(d.clone());
                resolve_defns.insert(d.clone(), d.clone());
                defns_to_pkgs.insert(d, pkg);
            }
            (defns, defns_to_pkgs, resolve_defns)
        }
        UpdateTarget::Specific(requested) => {
            let pkgs = lock.get_pkgs(&requested);
            let mut defns_to_pkgs = HashMap::new();
            for (d, p) in requested.iter().zip(pkgs) {
                if let Some(p) = p {
                    defns_to_pkgs.insert(d.clone(), p);
                }
            }
            let mut resolve_defns = HashMap::new();
            for (d, p) in &defns_to_pkgs {
                let key = if !d.strategies.is_empty() {
                    let mut d2 = d.clone();
                    d2.id = Some(p.id.clone());
                    d2
                } else {
                    p.to_defn()
                };
                resolve_defns.insert(key, d.clone());
            }
            (requested, defns_to_pkgs, resolve_defns)
        }
    };

    let resolve_keys: Vec<Defn> = resolve_defns.keys().cloned().collect();
    let resolve_results = resolve(ctx, &resolve_keys, true).await;

    let mut pkg_candidates: HashMap<Defn, PkgCandidate> = HashMap::new();
    let mut resolve_errors: HashMap<Defn, Failure> = HashMap::new();
    for (resolve_key, outcome) in resolve_results {
        let original = resolve_defns
            .get(&resolve_key)
            .cloned()
            .unwrap_or(resolve_key);
        match outcome {
            Ok(c) => {
                pkg_candidates.insert(original, c);
            }
            Err(e) => {
                resolve_errors.insert(original, e);
            }
        }
    }

    let mut updatables: HashMap<Defn, (Option<Pkg>, PkgCandidate)> = HashMap::new();
    for (d, candidate) in &pkg_candidates {
        let old = defns_to_pkgs.get(d).cloned();
        let needs_update = match &old {
            None => true,
            Some(o) => {
                o.version != candidate.version || !check_installed_pkg_integrity(ctx.addon_dir, o)
            }
        };
        if needs_update {
            updatables.insert(d.clone(), (old, candidate.clone()));
        }
    }

    let mut results: HashMap<Defn, AnyOutcome<Outcome>> = defns
        .iter()
        .map(|d| (d.clone(), Err(ManagerError::PkgNotInstalled.into())))
        .collect();
    for (d, e) in resolve_errors {
        results.insert(d, Err(e));
    }
    for d in pkg_candidates.keys() {
        if !updatables.contains_key(d) {
            let is_pinned = defns_to_pkgs
                .get(d)
                .map(|p| p.options.version_eq)
                .unwrap_or(false);
            results.insert(
                d.clone(),
                Err(ManagerError::PkgUpToDate { is_pinned }.into()),
            );
        }
    }

    if dry_run {
        for (d, (old, candidate)) in &updatables {
            let new_pkg = build_pkg(d, candidate, Vec::new());
            let outcome = match old {
                Some(o) => Outcome::PkgUpdated {
                    old: o.clone(),
                    new: Box::new(new_pkg),
                    dry_run: true,
                },
                None => Outcome::PkgInstalled {
                    pkg: new_pkg,
                    dry_run: true,
                },
            };
            results.insert(d.clone(), Ok(outcome));
        }
        return results;
    }

    let candidates_only: HashMap<Defn, PkgCandidate> = updatables
        .iter()
        .map(|(d, (_, c))| (d.clone(), c.clone()))
        .collect();
    let (archive_paths, download_errors) = download_all(ctx, &candidates_only).await;
    for (d, e) in download_errors {
        results.insert(d, Err(e));
    }

    for (d, path) in archive_paths {
        let (old, candidate) = &updatables[&d];
        let outcome = match old {
            Some(o) => mutate_update(lock, ctx.addon_dir, &d, o, candidate, &path),
            None => mutate_install(lock, ctx.addon_dir, &d, candidate, &path, false),
        };
        results.insert(d, outcome);
    }

    results
}

/// Removes installed packages by `Defn`.
pub fn remove(
    lock: &mut LockFile,
    addon_dir: &Path,
    defns: &[Defn],
    keep_folders: bool,
) -> HashMap<Defn, AnyOutcome<Outcome>> {
    let pkgs = lock.get_pkgs(defns);
    defns
        .iter()
        .zip(pkgs)
        .map(|(d, p)| {
            let outcome = match p {
                Some(pkg) => mutate_remove(lock, addon_dir, &pkg, keep_folders),
                None => Err(ManagerError::PkgNotInstalled.into()),
            };
            (d.clone(), outcome)
        })
        .collect()
}

/// Pins/unpins installed packages — sets `Strategy::VersionEq` on/off. This
/// only flips a flag; the pinned version is whatever `pkg.version` already
/// holds, there's no separate stored pin target.
pub fn pin(
    lock: &mut LockFile,
    sources: &[Box<dyn Resolver>],
    defns: &[Defn],
) -> HashMap<Defn, AnyOutcome<Outcome>> {
    let pkgs = lock.get_pkgs(defns);
    defns
        .iter()
        .zip(pkgs)
        .map(|(d, p)| {
            let outcome = (|| -> AnyOutcome<Outcome> {
                let supports_version_eq = find_source(sources, &d.source)
                    .map(|r| r.metadata().strategies.contains(&Strategy::VersionEq))
                    .unwrap_or(false);
                if !supports_version_eq {
                    return Err(ManagerError::PkgStrategiesUnsupported {
                        strategies: vec![Strategy::VersionEq],
                    }
                    .into());
                }
                let Some(pkg) = p else {
                    return Err(ManagerError::PkgNotInstalled.into());
                };
                if let Some(version) = &d.strategies.version_eq
                    && *version != pkg.version
                {
                    return Err(ManagerError::PkgFilesNotMatching {
                        strategies: d.strategies.clone(),
                    }
                    .into());
                }
                mutate_pin(lock, d, &pkg)
            })();
            (d.clone(), outcome)
        })
        .collect()
}

/// Replaces already-installed packages named by the keys with what the
/// paired `Defn` values resolve to (used by `rereconcile`: switching an
/// installed addon to a different source while preserving its presence).
pub async fn replace(
    lock: &mut LockFile,
    ctx: &Ctx<'_>,
    defns: &[(Defn, Defn)],
) -> Result<HashMap<Defn, AnyOutcome<Outcome>>, Failure> {
    let mut seen_new: HashSet<&Defn> = HashSet::new();
    for (_, new) in defns {
        if !seen_new.insert(new) {
            return Err(InternalError::new("defns must be unique").into());
        }
    }

    let old_defns: Vec<Defn> = defns.iter().map(|(o, _)| o.clone()).collect();
    let old_pkgs_vec = lock.get_pkgs(&old_defns);
    let old_pkgs: HashMap<Defn, Pkg> = old_defns
        .iter()
        .zip(old_pkgs_vec)
        .filter_map(|(d, p)| p.map(|p| (d.clone(), p)))
        .collect();

    let new_defns: Vec<Defn> = defns.iter().map(|(_, n)| n.clone()).collect();
    let not_installed = lock.check_pkgs_not_exist(&new_defns);
    let to_resolve: Vec<Defn> = new_defns
        .iter()
        .zip(&not_installed)
        .filter(|(_, ok)| **ok)
        .map(|(d, _)| d.clone())
        .collect();

    let resolve_results = resolve(ctx, &to_resolve, true).await;
    let (candidates, resolve_errors) = split_results(resolve_results);

    let with_id: Vec<Defn> = candidates
        .iter()
        .map(|(d, c)| {
            let mut d2 = d.clone();
            d2.id = Some(c.id.clone());
            d2
        })
        .collect();
    let still_not_installed = lock.check_pkgs_not_exist(&with_id);
    let candidates: HashMap<Defn, PkgCandidate> = candidates
        .into_iter()
        .zip(still_not_installed)
        .filter(|(_, ok)| *ok)
        .map(|((d, c), _)| (d, c))
        .collect();

    let mut results: HashMap<Defn, Option<AnyOutcome<Outcome>>> = HashMap::new();
    for (old, new) in defns {
        results.insert(old.clone(), Some(Err(ManagerError::PkgNotInstalled.into())));
        results.insert(new.clone(), None);
    }
    for (d, e) in resolve_errors {
        results.insert(d, Some(Err(e)));
    }

    let inverse: HashMap<&Defn, &Defn> = defns.iter().map(|(o, n)| (n, o)).collect();

    let (archive_paths, download_errors) = download_all(ctx, &candidates).await;
    for (d, e) in download_errors {
        results.insert(d, Some(Err(e)));
    }

    for (new_defn, path) in archive_paths {
        let old_defn = (*inverse
            .get(&new_defn)
            .expect("every candidate has an inverse old defn"))
        .clone();
        let candidate = candidates[&new_defn].clone();

        if let Some(old_pkg) = old_pkgs.get(&old_defn) {
            let remove_outcome = mutate_remove(lock, ctx.addon_dir, old_pkg, false);
            results.insert(old_defn, Some(remove_outcome));
        }
        let install_outcome =
            mutate_install(lock, ctx.addon_dir, &new_defn, &candidate, &path, false);
        results.insert(new_defn, Some(install_outcome));
    }

    Ok(results
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect())
}
