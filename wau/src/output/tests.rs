use chrono::{TimeZone, Utc};
use libwau::{
    lockfile::{Pkg, PkgDep, PkgFolder, PkgOptions},
    model::Flavour,
    results::{InternalError, ManagerError},
};

use super::*;

fn catalogue_entry(
    source: &str,
    id: &str,
    slug: &str,
    name: &str,
    download_count: u64,
) -> CatalogueEntry {
    CatalogueEntry {
        source: source.to_owned(),
        id: id.to_owned(),
        slug: slug.to_owned(),
        name: name.to_owned(),
        url: format!("https://example.com/{slug}"),
        game_flavours: vec![Flavour::Mainline],
        download_count,
        last_updated: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        folders: vec![],
        same_as: vec![],
        normalised_name: slug.to_owned(),
        derived_download_score: download_count as f64,
    }
}

fn sample_pkg(source: &str, slug: &str) -> Pkg {
    Pkg {
        source: source.to_owned(),
        id: "123".to_owned(),
        slug: slug.to_owned(),
        name: "Foo".to_owned(),
        description: "A test addon".to_owned(),
        url: "https://example.com/foo".to_owned(),
        download_url: "https://example.com/foo.zip".to_owned(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1.0.0".to_owned(),
        changelog_url: "https://example.com/foo/changelog".to_owned(),
        options: PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![PkgFolder {
            name: "Foo".to_owned(),
        }],
        deps: vec![PkgDep {
            id: "456".to_owned(),
        }],
    }
}

#[test]
fn symbol_for_maps_ok_manager_and_internal_distinctly() {
    let pkg = sample_pkg("curse", "foo");
    let ok: AnyOutcome<Outcome> = Ok(Outcome::PkgInstalled {
        pkg,
        dry_run: false,
    });
    let manager_err: AnyOutcome<Outcome> = Err(ManagerError::PkgAlreadyInstalled.into());
    let internal_err: AnyOutcome<Outcome> = Err(InternalError::new("boom").into());

    assert_eq!(symbol_for(&ok), "✓");
    assert_eq!(symbol_for(&manager_err), "✗");
    assert_eq!(symbol_for(&internal_err), "!");
}

#[test]
fn format_result_includes_uri_and_indented_detail() {
    let defn = Defn::new("curse", "foo");
    let outcome: AnyOutcome<Outcome> = Err(ManagerError::PkgAlreadyInstalled.into());
    let rendered = format_result(&defn, &outcome, false);
    assert_eq!(rendered, "✗ curse:foo\n  package already installed");
}

#[test]
fn format_result_with_color_emits_ansi_escapes() {
    let defn = Defn::new("curse", "foo");
    let outcome: AnyOutcome<Outcome> = Err(ManagerError::PkgAlreadyInstalled.into());
    let rendered = format_result(&defn, &outcome, true);
    assert!(rendered.contains('\u{1b}'));
    // Still readable as plain text once escapes are stripped isn't checked
    // here — the point is just that *something* got colored.
    assert!(rendered.contains("curse:foo"));
}

#[test]
fn format_results_sorts_by_uri() {
    let mut results = HashMap::new();
    results.insert(
        Defn::new("curse", "zeta"),
        Ok(Outcome::PkgRemoved {
            pkg: sample_pkg("curse", "zeta"),
        }),
    );
    results.insert(
        Defn::new("curse", "alpha"),
        Ok(Outcome::PkgRemoved {
            pkg: sample_pkg("curse", "alpha"),
        }),
    );

    let rendered = format_results(&results, false);
    let alpha_pos = rendered.find("alpha").unwrap();
    let zeta_pos = rendered.find("zeta").unwrap();
    assert!(alpha_pos < zeta_pos);
}

#[test]
fn any_errors_detects_at_least_one_failure() {
    let mut results = HashMap::new();
    results.insert(
        Defn::new("curse", "foo"),
        Ok(Outcome::PkgRemoved {
            pkg: sample_pkg("curse", "foo"),
        }),
    );
    assert!(!any_errors(&results));

    results.insert(
        Defn::new("curse", "bar"),
        Err(ManagerError::PkgNotInstalled.into()),
    );
    assert!(any_errors(&results));
}

#[test]
fn format_list_simple_renders_source_slug_uris_with_version() {
    let pkgs = [sample_pkg("curse", "foo"), sample_pkg("github", "bar")];
    let refs: Vec<&Pkg> = pkgs.iter().collect();
    assert_eq!(
        format_list_simple(&refs, false),
        "curse:foo 1.0.0\ngithub:bar 1.0.0"
    );
}

#[test]
fn format_list_detailed_includes_key_fields() {
    let pkg = sample_pkg("curse", "foo");
    let rendered = format_list_detailed(&[&pkg], false);
    assert!(rendered.contains("name: Foo"));
    assert!(rendered.contains("description: A test addon"));
    assert!(rendered.contains("folders: Foo"));
    assert!(rendered.contains("dependencies: curse:456"));
    assert!(rendered.contains("version: 1.0.0"));
}

#[test]
fn format_search_results_numbers_best_match_as_one_at_the_bottom() {
    let best = catalogue_entry("curse", "1", "best-match", "Best Match", 500);
    let worst = catalogue_entry("curse", "2", "worst-match", "Worst Match", 10);
    let entries = vec![&best, &worst];

    let rendered = format_search_results(&entries, &HashMap::new(), false);
    let lines: Vec<&str> = rendered.lines().collect();
    // best-first input -> best (index 0) printed last, labelled "1".
    assert!(lines[0].starts_with("2 curse/worst-match"));
    assert!(lines[2].starts_with("1 curse/best-match"));
}

#[test]
fn format_search_results_tags_installed_entries_with_their_version() {
    let entry = catalogue_entry("curse", "1", "foo", "Foo", 1);
    let entries = vec![&entry];
    let installed: HashMap<(String, String), String> =
        [(("curse".to_owned(), "1".to_owned()), "1.2.3".to_owned())]
            .into_iter()
            .collect();

    let rendered = format_search_results(&entries, &installed, false);
    assert!(rendered.contains("[Installed: 1.2.3]"));
}

#[test]
fn format_search_results_leaves_uninstalled_entries_untagged() {
    let entry = catalogue_entry("curse", "1", "foo", "Foo", 1);
    let entries = vec![&entry];

    let rendered = format_search_results(&entries, &HashMap::new(), false);
    assert!(!rendered.contains("[Installed"));
}

#[test]
fn format_list_json_round_trips_through_serde_json() {
    let pkg = sample_pkg("curse", "foo");
    let rendered = format_list_json(&[&pkg]);
    let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed[0]["source"], "curse");
    assert_eq!(parsed[0]["slug"], "foo");
    assert_eq!(parsed[0]["folders"][0], "Foo");
}

#[test]
fn pkg_to_json_includes_options() {
    let pkg = sample_pkg("curse", "foo");
    let json = pkg_to_json(&pkg);
    assert_eq!(json["options"]["any_flavour"], false);
    assert_eq!(json["options"]["version_eq"], false);
}

#[test]
fn format_bytes_picks_the_right_unit() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(999), "999 B");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
}

fn cache_category(name: &'static str, bytes: u64) -> CacheCategory {
    CacheCategory {
        name,
        description: "a cache category",
        path: std::path::PathBuf::from(name),
        bytes,
    }
}

#[test]
fn format_cache_usage_lists_each_category_and_a_total() {
    let categories = vec![cache_category("http", 1024), cache_category("staging", 0)];
    let rendered = format_cache_usage(&categories, false);
    assert!(rendered.contains("http 1.0 KB"));
    assert!(rendered.contains("staging 0 B"));
    assert!(rendered.contains("total 1.0 KB"));
}

#[test]
fn format_cache_cleaned_reports_the_freed_amount() {
    let rendered = format_cache_cleaned(2 * 1024 * 1024, false);
    assert!(rendered.contains("cleaned 2.0 MB"));
}
