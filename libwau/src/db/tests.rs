use chrono::{TimeZone, Timelike, Utc};

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
// Connection prep
// ---------------------------------------------------------------------------

#[test]
fn prepare_in_memory_creates_schema_at_current_version() {
    let conn = prepare_in_memory().unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, CURRENT_VERSION);
}

#[test]
fn prepare_database_creates_file_and_reopens_without_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");

    {
        let conn = prepare_database(&path).unwrap();
        insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();
    }

    let conn = prepare_database(&path).unwrap();
    let pkgs = get_all_pkgs(&conn).unwrap();
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].slug, "foo");
}

#[test]
fn migrate_downgrades_and_upgrades_back_to_current_version() {
    let conn = prepare_in_memory().unwrap();
    let mut conn = conn;

    migrations::migrate(&mut conn, CURRENT_VERSION, 0).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 0);

    migrations::migrate(&mut conn, 0, CURRENT_VERSION).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, CURRENT_VERSION);

    // The table itself must still be usable after the round trip.
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();
    assert_eq!(get_all_pkgs(&conn).unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Insert / get / delete round trip
// ---------------------------------------------------------------------------

#[test]
fn insert_then_get_all_round_trips_every_field() {
    let conn = prepare_in_memory().unwrap();
    let mut pkg = make_pkg("curse", "1", "foo", "1.0.0");
    pkg.deps.push(PkgDep { id: "2".into() });
    pkg.folders.push(PkgFolder {
        name: "foo_Config".into(),
    });
    insert_pkg(&conn, &pkg).unwrap();

    let all = get_all_pkgs(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert_pkgs_equivalent(&all[0], &pkg);
}

#[test]
fn insert_also_logs_version() {
    let conn = prepare_in_memory().unwrap();
    let pkg = make_pkg("curse", "1", "foo", "1.0.0");
    insert_pkg(&conn, &pkg).unwrap();

    let log = get_pkg_logged_versions(&conn, "curse", "1").unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].version, "1.0.0");
}

#[test]
fn reinstalling_same_version_does_not_duplicate_log_entry() {
    let conn = prepare_in_memory().unwrap();
    let pkg = make_pkg("curse", "1", "foo", "1.0.0");
    insert_pkg(&conn, &pkg).unwrap();
    delete_pkg(&conn, "curse", "1").unwrap();
    insert_pkg(&conn, &pkg).unwrap();

    let log = get_pkg_logged_versions(&conn, "curse", "1").unwrap();
    assert_eq!(log.len(), 1);
}

#[test]
fn delete_pkg_cascades_to_options_folders_and_deps_but_not_version_log() {
    let conn = prepare_in_memory().unwrap();
    let mut pkg = make_pkg("curse", "1", "foo", "1.0.0");
    pkg.deps.push(PkgDep { id: "2".into() });
    insert_pkg(&conn, &pkg).unwrap();

    delete_pkg(&conn, "curse", "1").unwrap();

    assert!(get_all_pkgs(&conn).unwrap().is_empty());
    let options_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pkg_options", [], |r| r.get(0))
        .unwrap();
    let folders_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pkg_folder", [], |r| r.get(0))
        .unwrap();
    let deps_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pkg_dep", [], |r| r.get(0))
        .unwrap();
    assert_eq!((options_count, folders_count, deps_count), (0, 0, 0));

    // Version log survives (no FK, deliberately).
    assert_eq!(
        get_pkg_logged_versions(&conn, "curse", "1").unwrap().len(),
        1
    );
}

#[test]
fn get_pkg_logged_versions_orders_most_recent_first_and_caps_at_ten() {
    let conn = prepare_in_memory().unwrap();

    for i in 1..=11 {
        let version = format!("1.0.{i}");
        let mut versioned = make_pkg("curse", "1", "foo", &version);
        versioned.date_published = Utc.with_ymd_and_hms(2026, 1, i, 0, 0, 0).unwrap();
        insert_pkg(&conn, &versioned).unwrap();
        conn.execute(
            "UPDATE pkg_version_log SET install_time = ?1 WHERE pkg_source = 'curse' AND pkg_id = '1' AND version = ?2",
            rusqlite::params![
                format_datetime(&Utc.with_ymd_and_hms(2026, 1, i, 0, 0, 0).unwrap()),
                version,
            ],
        )
        .unwrap();
        delete_pkg(&conn, "curse", "1").unwrap();
    }

    let log = get_pkg_logged_versions(&conn, "curse", "1").unwrap();
    assert_eq!(log.len(), 10);
    assert_eq!(log[0].version, "1.0.11");
    assert_eq!(log[9].version, "1.0.2");
}

// ---------------------------------------------------------------------------
// get_pkgs / check_pkgs_not_exist — defn matching by id/alias/slug
// ---------------------------------------------------------------------------

#[test]
fn get_pkgs_empty_input_returns_empty() {
    let conn = prepare_in_memory().unwrap();
    assert!(get_pkgs(&conn, &[]).unwrap().is_empty());
}

#[test]
fn get_pkgs_matches_by_slug_alias() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let results = get_pkgs(&conn, &[Defn::new("curse", "foo")]).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_ref().unwrap().id, "1");
}

#[test]
fn get_pkgs_slug_match_is_case_insensitive() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let results = get_pkgs(&conn, &[Defn::new("curse", "FOO")]).unwrap();
    assert!(results[0].is_some());
}

#[test]
fn get_pkgs_matches_by_id_when_alias_is_stale() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let mut defn = Defn::new("curse", "some-old-slug");
    defn.id = Some("1".to_owned());
    let results = get_pkgs(&conn, &[defn]).unwrap();
    assert!(results[0].is_some());
}

#[test]
fn get_pkgs_returns_none_for_unmatched_defn_preserving_order() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let results = get_pkgs(
        &conn,
        &[Defn::new("curse", "missing"), Defn::new("curse", "foo")],
    )
    .unwrap();
    assert!(results[0].is_none());
    assert!(results[1].is_some());
}

#[test]
fn check_pkgs_not_exist_reflects_install_state() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let result = check_pkgs_not_exist(
        &conn,
        &[Defn::new("curse", "foo"), Defn::new("curse", "bar")],
    )
    .unwrap();
    assert_eq!(result, vec![false, true]);
}

// ---------------------------------------------------------------------------
// pin_pkg
// ---------------------------------------------------------------------------

#[test]
fn pin_pkg_flips_version_eq_without_touching_version() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let pinned = pin_pkg(&conn, "curse", "1", true).unwrap();
    assert!(pinned);

    let pkg = get_pkgs(&conn, &[Defn::new("curse", "foo")])
        .unwrap()
        .remove(0)
        .unwrap();
    assert!(pkg.options.version_eq);
    assert_eq!(pkg.version, "1.0.0");

    let unpinned = pin_pkg(&conn, "curse", "1", false).unwrap();
    assert!(!unpinned);
}

// ---------------------------------------------------------------------------
// Folder conflict checks
// ---------------------------------------------------------------------------

#[test]
fn find_pkgs_owning_folders_matches_install_time_conflicts() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let conflicts = find_pkgs_owning_folders(&conn, &["foo".to_owned()]).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].id, "1");

    let none = find_pkgs_owning_folders(&conn, &["bar".to_owned()]).unwrap();
    assert!(none.is_empty());
}

#[test]
fn find_pkgs_owning_folders_excluding_skips_the_named_package() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();

    let conflicts =
        find_pkgs_owning_folders_excluding(&conn, &["foo".to_owned()], "curse", "1").unwrap();
    assert!(conflicts.is_empty());
}

#[test]
fn find_pkgs_owning_folders_excluding_still_reports_other_packages() {
    let conn = prepare_in_memory().unwrap();
    insert_pkg(&conn, &make_pkg("curse", "1", "foo", "1.0.0")).unwrap();
    insert_pkg(&conn, &make_pkg("wowi", "9", "bar", "1.0.0")).unwrap();

    let conflicts =
        find_pkgs_owning_folders_excluding(&conn, &["bar".to_owned()], "curse", "1").unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].id, "9");
}

// ---------------------------------------------------------------------------
// Datetime formatting
// ---------------------------------------------------------------------------

#[test]
fn format_datetime_omits_fraction_when_zero() {
    let dt = Utc.with_ymd_and_hms(2026, 4, 22, 0, 0, 0).unwrap();
    assert_eq!(format_datetime(&dt), "2026-04-22 00:00:00");
}

#[test]
fn format_datetime_includes_microsecond_fraction() {
    let dt = Utc
        .with_ymd_and_hms(2026, 4, 22, 0, 0, 0)
        .unwrap()
        .with_nanosecond(123_000)
        .unwrap();
    assert_eq!(format_datetime(&dt), "2026-04-22 00:00:00.000123");
}

#[test]
fn parse_datetime_round_trips_format_datetime_output() {
    let dt = Utc.with_ymd_and_hms(2026, 4, 22, 13, 37, 5).unwrap();
    let text = format_datetime(&dt);
    assert_eq!(parse_datetime(&text), Some(dt));
}

#[test]
fn parse_datetime_accepts_sqlite_current_timestamp_format() {
    // What SQLite's own `CURRENT_TIMESTAMP` default produces: no fractional part.
    assert_eq!(
        parse_datetime("2026-04-22 13:37:05"),
        Some(Utc.with_ymd_and_hms(2026, 4, 22, 13, 37, 5).unwrap())
    );
}
