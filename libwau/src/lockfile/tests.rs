use chrono::{TimeZone, Utc};

use super::*;
use crate::model::Defn;

fn make_pkg(source: &str, id: &str, slug: &str, version: &str) -> Pkg {
    Pkg {
        source: source.to_owned(),
        id: id.to_owned(),
        slug: slug.to_owned(),
        name: format!("{slug} display name"),
        description: "a fine addon".to_owned(),
        url: format!("https://example.invalid/{slug}"),
        download_url: format!("https://example.invalid/{slug}.zip"),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
        version: version.to_owned(),
        changelog_url: format!("https://example.invalid/{slug}/changelog"),
        options: PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![PkgFolder {
            name: slug.to_owned(),
        }],
        deps: Vec::new(),
    }
}

fn assert_pkgs_equivalent(a: &Pkg, b: &Pkg) {
    assert_eq!(a.source, b.source);
    assert_eq!(a.id, b.id);
    assert_eq!(a.slug, b.slug);
    assert_eq!(a.name, b.name);
    assert_eq!(a.version, b.version);
    assert_eq!(a.date_published, b.date_published);
    assert_eq!(a.options, b.options);
    assert_eq!(a.folders, b.folders);
    assert_eq!(a.deps, b.deps);
}

// ---------------------------------------------------------------------------
// open / save
// ---------------------------------------------------------------------------

#[test]
fn open_missing_file_starts_empty_without_touching_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock.toml");

    let lock = LockFile::open(&path).unwrap();
    assert!(lock.get_all_pkgs().is_empty());
    assert!(!path.exists());
}

#[test]
fn save_creates_file_and_reopen_sees_the_same_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock.toml");

    {
        let mut lock = LockFile::open(&path).unwrap();
        lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));
        lock.save().unwrap();
    }
    assert!(path.exists());

    let lock = LockFile::open(&path).unwrap();
    let pkgs = lock.get_all_pkgs();
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].slug, "foo");
}

#[test]
fn save_is_a_no_op_for_in_memory_instances() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));
    lock.save().unwrap();
    assert_eq!(lock.get_all_pkgs().len(), 1);
}

#[test]
fn open_rejects_a_lock_file_from_a_newer_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock.toml");
    std::fs::write(&path, format!("version = {}\n", CURRENT_VERSION + 1)).unwrap();

    let err = LockFile::open(&path).unwrap_err();
    assert!(matches!(
        err,
        LockError::UnsupportedVersion { found, supported }
        if found == CURRENT_VERSION + 1 && supported == CURRENT_VERSION
    ));
}

// ---------------------------------------------------------------------------
// Insert / get / delete round trip
// ---------------------------------------------------------------------------

#[test]
fn insert_then_get_all_round_trips_every_field() {
    let mut lock = LockFile::in_memory();
    let mut pkg = make_pkg("curse", "1", "foo", "1.0.0");
    pkg.deps.push(PkgDep { id: "2".into() });
    pkg.folders.push(PkgFolder {
        name: "foo_Config".into(),
    });
    lock.insert_pkg(pkg.clone());

    let all = lock.get_all_pkgs();
    assert_eq!(all.len(), 1);
    assert_pkgs_equivalent(&all[0], &pkg);
}

#[test]
fn insert_also_logs_version() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let log = lock.get_pkg_logged_versions("curse", "1");
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].version, "1.0.0");
}

#[test]
fn reinstalling_same_version_does_not_duplicate_log_entry() {
    let mut lock = LockFile::in_memory();
    let pkg = make_pkg("curse", "1", "foo", "1.0.0");
    lock.insert_pkg(pkg.clone());
    lock.delete_pkg("curse", "1");
    lock.insert_pkg(pkg);

    let log = lock.get_pkg_logged_versions("curse", "1");
    assert_eq!(log.len(), 1);
}

#[test]
fn insert_upserts_an_existing_package_instead_of_duplicating() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));
    lock.insert_pkg(make_pkg("curse", "1", "foo", "2.0.0"));

    let all = lock.get_all_pkgs();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].version, "2.0.0");
}

#[test]
fn delete_pkg_removes_the_package_but_not_its_version_log() {
    let mut lock = LockFile::in_memory();
    let mut pkg = make_pkg("curse", "1", "foo", "1.0.0");
    pkg.deps.push(PkgDep { id: "2".into() });
    lock.insert_pkg(pkg);

    lock.delete_pkg("curse", "1");

    assert!(lock.get_all_pkgs().is_empty());
    // Version log survives (no FK-equivalent, deliberately).
    assert_eq!(lock.get_pkg_logged_versions("curse", "1").len(), 1);
}

#[test]
fn get_pkg_logged_versions_orders_most_recent_first_and_caps_at_ten() {
    let mut lock = LockFile::in_memory();

    for i in 1..=11 {
        let version = format!("1.0.{i}");
        lock.insert_pkg(make_pkg("curse", "1", "foo", &version));
        // Force distinct, ascending install times so ordering is unambiguous
        // (real inserts happen fast enough that `Utc::now()` alone could tie).
        for entry in &mut lock.version_log {
            if entry.pkg_source == "curse" && entry.pkg_id == "1" && entry.version == version {
                entry.install_time = Utc.with_ymd_and_hms(2026, 1, i, 0, 0, 0).unwrap();
            }
        }
        lock.delete_pkg("curse", "1");
    }

    let log = lock.get_pkg_logged_versions("curse", "1");
    assert_eq!(log.len(), 10);
    assert_eq!(log[0].version, "1.0.11");
    assert_eq!(log[9].version, "1.0.2");
}

// ---------------------------------------------------------------------------
// get_pkgs / check_pkgs_not_exist — defn matching by id/alias/slug
// ---------------------------------------------------------------------------

#[test]
fn get_pkgs_empty_input_returns_empty() {
    let lock = LockFile::in_memory();
    assert!(lock.get_pkgs(&[]).is_empty());
}

#[test]
fn get_pkgs_matches_by_slug_alias() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let results = lock.get_pkgs(&[Defn::new("curse", "foo")]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_ref().unwrap().id, "1");
}

#[test]
fn get_pkgs_slug_match_is_case_insensitive() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let results = lock.get_pkgs(&[Defn::new("curse", "FOO")]);
    assert!(results[0].is_some());
}

#[test]
fn get_pkgs_matches_by_id_when_alias_is_stale() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let mut defn = Defn::new("curse", "some-old-slug");
    defn.id = Some("1".to_owned());
    let results = lock.get_pkgs(&[defn]);
    assert!(results[0].is_some());
}

#[test]
fn get_pkgs_returns_none_for_unmatched_defn_preserving_order() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let results = lock.get_pkgs(&[Defn::new("curse", "missing"), Defn::new("curse", "foo")]);
    assert!(results[0].is_none());
    assert!(results[1].is_some());
}

#[test]
fn check_pkgs_not_exist_reflects_install_state() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let result = lock.check_pkgs_not_exist(&[Defn::new("curse", "foo"), Defn::new("curse", "bar")]);
    assert_eq!(result, vec![false, true]);
}

// ---------------------------------------------------------------------------
// pin_pkg
// ---------------------------------------------------------------------------

#[test]
fn pin_pkg_flips_version_eq_without_touching_version() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let pinned = lock.pin_pkg("curse", "1", true).unwrap();
    assert!(pinned);

    let pkg = lock.get_pkgs(&[Defn::new("curse", "foo")]).remove(0).unwrap();
    assert!(pkg.options.version_eq);
    assert_eq!(pkg.version, "1.0.0");

    let unpinned = lock.pin_pkg("curse", "1", false).unwrap();
    assert!(!unpinned);
}

#[test]
fn pin_pkg_returns_none_when_not_installed() {
    let mut lock = LockFile::in_memory();
    assert!(lock.pin_pkg("curse", "1", true).is_none());
}

// ---------------------------------------------------------------------------
// Folder conflict checks
// ---------------------------------------------------------------------------

#[test]
fn find_pkgs_owning_folders_matches_installed_packages() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let conflicts = lock.find_pkgs_owning_folders(&["foo".to_owned()]);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].id, "1");

    let none = lock.find_pkgs_owning_folders(&["bar".to_owned()]);
    assert!(none.is_empty());
}

#[test]
fn find_pkgs_owning_folders_excluding_skips_the_named_package() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));

    let conflicts = lock.find_pkgs_owning_folders_excluding(&["foo".to_owned()], "curse", "1");
    assert!(conflicts.is_empty());
}

#[test]
fn find_pkgs_owning_folders_excluding_still_reports_other_packages() {
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));
    lock.insert_pkg(make_pkg("wowi", "9", "bar", "1.0.0"));

    let conflicts = lock.find_pkgs_owning_folders_excluding(&["bar".to_owned()], "curse", "1");
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].id, "9");
}

#[test]
fn find_pkgs_owning_folders_excluding_ignores_any_package_on_the_same_source() {
    // Ported verbatim from the original SQL's AND-not-OR exclusion quirk: a
    // *different* package on the excluded source is also never reported,
    // even though it doesn't share the excluded id.
    let mut lock = LockFile::in_memory();
    lock.insert_pkg(make_pkg("curse", "1", "foo", "1.0.0"));
    lock.insert_pkg(make_pkg("curse", "2", "bar", "1.0.0"));

    let conflicts = lock.find_pkgs_owning_folders_excluding(&["bar".to_owned()], "curse", "1");
    assert!(conflicts.is_empty());
}

// ---------------------------------------------------------------------------
// examples/profiles/example/lock.toml stays in sync with this format
// ---------------------------------------------------------------------------

#[test]
fn example_lock_toml_parses_and_matches_the_documented_format() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/profiles/example/lock.toml"
    );
    let raw = std::fs::read_to_string(path).unwrap();
    let data: LockFileData = toml::from_str(&raw).unwrap();

    assert_eq!(data.version, CURRENT_VERSION);
    assert_eq!(data.packages.len(), 2);
    assert_eq!(data.version_log.len(), 2);

    let weakauras = data
        .packages
        .iter()
        .find(|p| p.slug == "weakauras")
        .unwrap();
    assert!(weakauras.options.version_eq);
    assert!(weakauras.deps.is_empty());

    let bigwigs = data.packages.iter().find(|p| p.slug == "big-wigs").unwrap();
    let folder_names: Vec<&str> = bigwigs.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(folder_names, vec!["BigWigs", "BigWigs_Plugins"]);
    let dep_ids: Vec<&str> = bigwigs.deps.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(dep_ids, vec!["323002"]);
}
