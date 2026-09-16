use std::collections::HashSet;

use chrono::{TimeZone, Utc};

use super::*;
use crate::catalogue::AddonKey;

fn entry(
    source: &str,
    id: &str,
    name: &str,
    downloads: f64,
    flavours: Vec<Flavour>,
) -> CatalogueEntry {
    CatalogueEntry {
        source: source.to_owned(),
        id: id.to_owned(),
        slug: format!("{name}-slug"),
        name: name.to_owned(),
        url: format!("https://example.invalid/{name}"),
        game_flavours: flavours,
        download_count: downloads as u64,
        last_updated: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        folders: vec![vec![name.to_owned()]],
        same_as: vec![],
        normalised_name: crate::catalogue::normalise_name(name),
        derived_download_score: downloads,
    }
}

fn no_installs() -> HashSet<(String, String)> {
    HashSet::new()
}

#[test]
fn search_matches_close_names_and_filters_by_flavour() {
    let entries = vec![
        entry("curse", "1", "WeakAuras", 1.0, vec![Flavour::Mainline]),
        entry(
            "curse",
            "2",
            "Totally Unrelated",
            1.0,
            vec![Flavour::Mainline],
        ),
        entry(
            "curse",
            "3",
            "WeakAuras Classic",
            1.0,
            vec![Flavour::VanillaClassic],
        ),
    ];
    let results = search(
        &entries,
        "WeakAuras",
        Flavour::Mainline,
        &no_installs(),
        &SearchOptions::default(),
    );
    let ids: Vec<&str> = results.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"1"));
    assert!(!ids.contains(&"3")); // wrong flavour
}

#[test]
fn search_wildcard_returns_everything_matching_flavour() {
    let entries = vec![
        entry("curse", "1", "Aaa", 1.0, vec![Flavour::Mainline]),
        entry("curse", "2", "Zzz", 1.0, vec![Flavour::Mainline]),
    ];
    let results = search(
        &entries,
        "*",
        Flavour::Mainline,
        &no_installs(),
        &SearchOptions::default(),
    );
    assert_eq!(results.len(), 2);
}

#[test]
fn search_ranks_higher_download_score_first_for_equal_similarity() {
    let entries = vec![
        entry("curse", "low", "SameName", 0.1, vec![Flavour::Mainline]),
        entry("curse", "high", "SameName", 0.9, vec![Flavour::Mainline]),
    ];
    let results = search(
        &entries,
        "SameName",
        Flavour::Mainline,
        &no_installs(),
        &SearchOptions::default(),
    );
    assert_eq!(results[0].id, "high");
}

#[test]
fn search_filters_by_source() {
    let entries = vec![
        entry("curse", "1", "Foo", 1.0, vec![Flavour::Mainline]),
        entry("wowi", "2", "Foo", 1.0, vec![Flavour::Mainline]),
    ];
    let sources = vec!["wowi".to_owned()];
    let options = SearchOptions {
        sources: &sources,
        ..SearchOptions::default()
    };
    let results = search(&entries, "Foo", Flavour::Mainline, &no_installs(), &options);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source, "wowi");
}

#[test]
fn search_exclude_installed_filters_matching_key() {
    let entries = vec![entry("curse", "1", "Foo", 1.0, vec![Flavour::Mainline])];
    let mut installed = HashSet::new();
    installed.insert(("curse".to_owned(), "1".to_owned()));
    let options = SearchOptions {
        filter_installed: FilterInstalled::Exclude,
        ..SearchOptions::default()
    };
    let results = search(&entries, "Foo", Flavour::Mainline, &installed, &options);
    assert!(results.is_empty());
}

#[test]
fn search_exclude_from_all_sources_also_excludes_same_as_equivalents() {
    let mut foo = entry("curse", "1", "Foo", 1.0, vec![Flavour::Mainline]);
    let mut wowi_foo = entry("wowi", "2", "Foo", 1.0, vec![Flavour::Mainline]);
    foo.same_as = vec![AddonKey {
        source: "wowi".into(),
        id: "2".into(),
    }];
    wowi_foo.same_as = vec![AddonKey {
        source: "curse".into(),
        id: "1".into(),
    }];
    let entries = vec![foo, wowi_foo];

    let mut installed = HashSet::new();
    installed.insert(("curse".to_owned(), "1".to_owned()));
    let options = SearchOptions {
        filter_installed: FilterInstalled::ExcludeFromAllSources,
        ..SearchOptions::default()
    };
    let results = search(&entries, "Foo", Flavour::Mainline, &installed, &options);
    assert!(results.is_empty());
}

#[test]
fn search_include_only_restricts_to_installed() {
    let entries = vec![
        entry("curse", "1", "Foo", 1.0, vec![Flavour::Mainline]),
        entry("curse", "2", "Foo", 1.0, vec![Flavour::Mainline]),
    ];
    let mut installed = HashSet::new();
    installed.insert(("curse".to_owned(), "1".to_owned()));
    let options = SearchOptions {
        filter_installed: FilterInstalled::IncludeOnly,
        ..SearchOptions::default()
    };
    let results = search(&entries, "*", Flavour::Mainline, &installed, &options);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "1");
}

#[test]
fn search_prefer_source_excludes_entries_with_that_same_as() {
    let mut foo = entry("wowi", "2", "Foo", 1.0, vec![Flavour::Mainline]);
    foo.same_as = vec![AddonKey {
        source: "curse".into(),
        id: "1".into(),
    }];
    let entries = vec![
        foo,
        entry("curse", "1", "Foo", 1.0, vec![Flavour::Mainline]),
    ];

    let options = SearchOptions {
        prefer_source: Some("curse"),
        ..SearchOptions::default()
    };
    let results = search(&entries, "Foo", Flavour::Mainline, &no_installs(), &options);
    // wowi's entry has a `curse` same_as, so it's excluded; the curse entry itself remains.
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source, "curse");
}

#[test]
fn search_matches_abbreviation_of_a_multi_word_name() {
    let entries = vec![entry(
        "curse",
        "1",
        "Deadly Boss Mods",
        1.0,
        vec![Flavour::Mainline],
    )];
    let results = search(
        &entries,
        "dbm",
        Flavour::Mainline,
        &no_installs(),
        &SearchOptions::default(),
    );
    assert_eq!(results.len(), 1);
}

#[test]
fn search_respects_limit() {
    let entries: Vec<CatalogueEntry> = (0..5)
        .map(|i| entry("curse", &i.to_string(), "Foo", 1.0, vec![Flavour::Mainline]))
        .collect();
    let options = SearchOptions {
        limit: 2,
        ..SearchOptions::default()
    };
    let results = search(&entries, "Foo", Flavour::Mainline, &no_installs(), &options);
    assert_eq!(results.len(), 2);
}
