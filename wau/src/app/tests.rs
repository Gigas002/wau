use chrono::{TimeZone, Utc};
use libwau::lockfile::{Pkg, PkgDep, PkgFolder, PkgOptions};

use super::*;

fn sample_pkg(source: &str, id: &str, slug: &str, name: &str) -> Pkg {
    Pkg {
        source: source.to_owned(),
        id: id.to_owned(),
        slug: slug.to_owned(),
        name: name.to_owned(),
        description: "desc".to_owned(),
        url: "https://example.com".to_owned(),
        download_url: "https://example.com/dl".to_owned(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1.0.0".to_owned(),
        changelog_url: "https://example.com/changelog".to_owned(),
        options: PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![PkgFolder {
            name: "Foo".to_owned(),
        }],
        deps: Vec::<PkgDep>::new(),
    }
}

#[test]
fn filter_pkgs_by_addons_returns_everything_when_empty() {
    let all = vec![
        sample_pkg("curse", "1", "foo", "Foo"),
        sample_pkg("github", "2", "bar", "Bar"),
    ];
    let filtered = filter_pkgs_by_addons(&all, &[]);
    assert_eq!(filtered.len(), 2);
}

#[test]
fn filter_pkgs_by_addons_matches_source_qualified_slug() {
    let all = vec![
        sample_pkg("curse", "1", "foo", "Foo"),
        sample_pkg("github", "2", "bar", "Bar"),
    ];
    let filtered = filter_pkgs_by_addons(&all, &["curse:foo".to_owned()]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].slug, "foo");
}

#[test]
fn filter_pkgs_by_addons_matches_bare_slug_substring_case_insensitively() {
    let all = vec![sample_pkg("curse", "1", "foo-bar", "Foo Bar")];
    let filtered = filter_pkgs_by_addons(&all, &["FOO".to_owned()]);
    assert_eq!(filtered.len(), 1);
}

#[test]
fn filter_pkgs_by_addons_deduplicates_across_overlapping_args() {
    let all = vec![sample_pkg("curse", "1", "foo", "Foo")];
    let filtered = filter_pkgs_by_addons(&all, &["foo".to_owned(), "curse:foo".to_owned()]);
    assert_eq!(filtered.len(), 1);
}

#[test]
fn parse_defn_defers_an_unresolvable_but_colon_bearing_alias() {
    // With no sources registered, "curse" isn't a known scheme, so the whole
    // string becomes the alias with an empty source — but since it still
    // contains a ':', the "unknown source" failure is deferred to
    // resolution time rather than erroring out immediately.
    let defn = parse_defn("curse:foo", &[], false).unwrap();
    assert_eq!(defn.source, "");
    assert_eq!(defn.alias, "curse:foo");
}

#[test]
fn parse_defn_errors_on_a_bare_alias_with_no_colon_and_no_matching_source() {
    let err = parse_defn("just-a-name", &[], false).unwrap_err();
    assert!(matches!(err, AppError::Other(_)));
}

#[test]
fn parse_selection_accepts_space_and_comma_separated_numbers() {
    assert_eq!(parse_selection("1 3", 5), vec![0, 2]);
    assert_eq!(parse_selection("1,3", 5), vec![0, 2]);
    assert_eq!(parse_selection("1, 3 5", 5), vec![0, 2, 4]);
}

#[test]
fn parse_selection_expands_ranges_in_either_order() {
    // A "reversed" range (`3-1`) still normalizes to ascending bounds, not a
    // reversed iteration order.
    assert_eq!(parse_selection("1-3", 5), vec![0, 1, 2]);
    assert_eq!(parse_selection("3-1", 5), vec![0, 1, 2]);
}

#[test]
fn parse_selection_drops_out_of_range_and_unparsable_tokens() {
    assert_eq!(parse_selection("0 6 2", 5), vec![1]);
    assert_eq!(parse_selection("abc 2 1-abc", 5), vec![1]);
}

#[test]
fn parse_selection_deduplicates_preserving_first_seen_order() {
    assert_eq!(parse_selection("2 1-3 2", 5), vec![1, 0, 2]);
}

#[test]
fn parse_selection_of_blank_input_is_empty() {
    assert!(parse_selection("", 5).is_empty());
    assert!(parse_selection("   ", 5).is_empty());
}
