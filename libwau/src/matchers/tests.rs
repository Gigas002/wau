use std::io::Write as _;

use chrono::{TimeZone, Utc};

use super::*;
use crate::{
    catalogue::{AddonKey, CatalogueEntry},
    lockfile::PkgFolder,
    model::ChangelogFormat,
};

fn write_toc(dir: &Path, folder: &str, filename: &str, content: &str) -> PathBuf {
    let addon_dir = dir.join(folder);
    std::fs::create_dir_all(&addon_dir).unwrap();
    let path = addon_dir.join(filename);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    addon_dir
}

struct TestResolver {
    id: &'static str,
    toc_key: Option<&'static str>,
}

#[async_trait::async_trait]
impl Resolver for TestResolver {
    fn metadata(&self) -> crate::model::SourceMetadata {
        crate::model::SourceMetadata {
            id: self.id,
            name: "Test",
            strategies: &[],
            changelog_format: ChangelogFormat::Raw,
            addon_toc_key: self.toc_key,
        }
    }

    async fn resolve_one_impl(
        &self,
        _http: &crate::http::HttpClient,
        _flavour: Flavour,
        _defn: &Defn,
    ) -> crate::results::AnyOutcome<crate::sources::PkgCandidate> {
        unreachable!("not exercised by matcher tests")
    }
}

fn sources() -> Vec<Box<dyn Resolver>> {
    vec![
        Box::new(TestResolver {
            id: "github",
            toc_key: None,
        }),
        Box::new(TestResolver {
            id: "curse",
            toc_key: Some("X-Curse-Project-ID"),
        }),
        Box::new(TestResolver {
            id: "wowi",
            toc_key: Some("X-WoWI-ID"),
        }),
    ]
}

fn entry(
    source: &str,
    id: &str,
    name: &str,
    folders: Vec<Vec<&str>>,
    same_as: Vec<(&str, &str)>,
) -> CatalogueEntry {
    CatalogueEntry {
        source: source.to_owned(),
        id: id.to_owned(),
        slug: format!("{name}-slug"),
        name: name.to_owned(),
        url: format!("https://example.invalid/{name}"),
        game_flavours: vec![Flavour::Mainline],
        download_count: 1,
        last_updated: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        folders: folders
            .into_iter()
            .map(|f| f.into_iter().map(str::to_owned).collect())
            .collect(),
        same_as: same_as
            .into_iter()
            .map(|(s, i)| AddonKey {
                source: s.to_owned(),
                id: i.to_owned(),
            })
            .collect(),
        normalised_name: crate::catalogue::normalise_name(name),
        derived_download_score: 1.0,
    }
}

fn catalogue(entries: Vec<CatalogueEntry>) -> ComputedCatalogue {
    ComputedCatalogue { entries }
}

// ---------------------------------------------------------------------------
// AddonFolder
// ---------------------------------------------------------------------------

#[test]
fn addon_folder_from_path_finds_flavour_specific_toc_first() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(
        dir.path(),
        "Foo",
        "Foo-Mainline.toc",
        "## Interface: 110000\n## Version: 1.0",
    );
    let addon = AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap();
    assert_eq!(addon.name, "Foo");
    assert_eq!(addon.toc.version.as_deref(), Some("1.0"));
}

#[test]
fn addon_folder_from_path_falls_back_to_plain_toc() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(dir.path(), "Foo", "Foo.toc", "## Version: 2.0");
    let addon = AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap();
    assert_eq!(addon.toc.version.as_deref(), Some("2.0"));
}

#[test]
fn addon_folder_from_path_none_when_no_toc() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = dir.path().join("Foo");
    std::fs::create_dir_all(&addon_dir).unwrap();
    std::fs::write(addon_dir.join("readme.txt"), "hi").unwrap();
    assert!(AddonFolder::from_path(Flavour::Mainline, &addon_dir).is_none());
}

#[test]
fn get_defns_from_toc_keys_reads_x_fields() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(
        dir.path(),
        "Foo",
        "Foo.toc",
        "## X-Curse-Project-ID: 12345\n## X-WoWI-ID: 999",
    );
    let addon = AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap();
    let pairs = [("X-Curse-Project-ID", "curse"), ("X-WoWI-ID", "wowi")];
    let defns = addon.get_defns_from_toc_keys(&pairs);
    assert!(defns.contains(&Defn::new("curse", "12345")));
    assert!(defns.contains(&Defn::new("wowi", "999")));
}

// ---------------------------------------------------------------------------
// get_unreconciled_folders
// ---------------------------------------------------------------------------

#[test]
fn get_unreconciled_folders_skips_tracked_and_untracked_without_toc() {
    let dir = tempfile::tempdir().unwrap();
    write_toc(dir.path(), "Tracked", "Tracked.toc", "## Version: 1");
    write_toc(dir.path(), "Untracked", "Untracked.toc", "## Version: 1");
    std::fs::create_dir_all(dir.path().join("NoToc")).unwrap();

    let mut lock = crate::lockfile::LockFile::in_memory();
    lock.insert_pkg(crate::lockfile::Pkg {
        source: "s".into(),
        id: "1".into(),
        slug: "slug".into(),
        name: "name".into(),
        description: "".into(),
        url: "".into(),
        download_url: "".into(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1".into(),
        changelog_url: "".into(),
        options: crate::lockfile::PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![PkgFolder {
            name: "Tracked".into(),
        }],
        deps: vec![],
    });

    let unreconciled = get_unreconciled_folders(&lock, dir.path(), Flavour::Mainline);
    let names: Vec<&str> = unreconciled.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Untracked"]);
}

// ---------------------------------------------------------------------------
// match_toc_source_ids
// ---------------------------------------------------------------------------

#[test]
fn match_toc_source_ids_matches_by_provider_id_and_sorts_by_priority() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(dir.path(), "Foo", "Foo.toc", "## X-Curse-Project-ID: 100");
    let leftovers = vec![AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap()];

    let cat = catalogue(vec![entry(
        "curse",
        "100",
        "Foo",
        vec![],
        vec![("wowi", "200")],
    )]);
    let src = sources();

    let groups = match_toc_source_ids(&leftovers, &cat, &src);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].folders.len(), 1);
    // github > curse > wowi priority order; curse comes before wowi here (github absent from defns).
    assert_eq!(groups[0].defns[0].source, "curse");
    assert!(groups[0].defns.iter().any(|d| d.source == "wowi"));
}

#[test]
fn match_toc_source_ids_ignores_folders_without_provider_ids() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(dir.path(), "Foo", "Foo.toc", "## Version: 1");
    let leftovers = vec![AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap()];

    let cat = catalogue(vec![]);
    let groups = match_toc_source_ids(&leftovers, &cat, &sources());
    assert!(groups.is_empty());
}

#[test]
fn match_toc_source_ids_uses_tukui_slug_instead_of_id() {
    // Tukui's API has no id-based lookup at all (confirmed live against the
    // real API: `GET /addon/-2` and `GET /addon?id=-2` both 404 for its
    // flagship addons' sentinel ids) — unlike every other source, a
    // Tukui-sourced catalogue match must resolve to the catalogue's `slug`,
    // not its `id`, or `install`/`sync` on the reconciled package would
    // 404 forever.
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(dir.path(), "ElvUI", "ElvUI.toc", "## X-Tukui-ProjectID: -2");
    let leftovers = vec![AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap()];

    let cat = catalogue(vec![entry("tukui", "-2", "ElvUI", vec![], vec![])]);
    let mut src = sources();
    src.push(Box::new(TestResolver {
        id: "tukui",
        toc_key: Some("X-Tukui-ProjectID"),
    }));

    let groups = match_toc_source_ids(&leftovers, &cat, &src);
    assert_eq!(groups.len(), 1);
    let defn = groups[0]
        .defns
        .iter()
        .find(|d| d.source == "tukui")
        .unwrap();
    assert_eq!(defn.alias, "ElvUI-slug");
    assert_eq!(defn.id.as_deref(), Some("-2"));
}

#[test]
fn match_toc_source_ids_merges_folders_sharing_a_cross_referenced_defn() {
    let dir = tempfile::tempdir().unwrap();
    let dir_a = write_toc(dir.path(), "A", "A.toc", "## X-Curse-Project-ID: 100");
    let dir_b = write_toc(dir.path(), "B", "B.toc", "## X-WoWI-ID: 200");
    let leftovers = vec![
        AddonFolder::from_path(Flavour::Mainline, &dir_a).unwrap(),
        AddonFolder::from_path(Flavour::Mainline, &dir_b).unwrap(),
    ];

    // The catalogue says curse:100 and wowi:200 are the same addon.
    let cat = catalogue(vec![
        entry("curse", "100", "Foo", vec![], vec![("wowi", "200")]),
        entry("wowi", "200", "Foo", vec![], vec![("curse", "100")]),
    ]);

    let groups = match_toc_source_ids(&leftovers, &cat, &sources());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].folders.len(), 2);
}

// ---------------------------------------------------------------------------
// match_folder_name_subsets
// ---------------------------------------------------------------------------

#[test]
fn match_folder_name_subsets_prefers_more_covered_folders() {
    let dir = tempfile::tempdir().unwrap();
    let dir_a = write_toc(dir.path(), "Foo", "Foo.toc", "## Version: 1");
    let dir_b = write_toc(dir.path(), "Foo_Config", "Foo_Config.toc", "## Version: 1");
    let leftovers = vec![
        AddonFolder::from_path(Flavour::Mainline, &dir_a).unwrap(),
        AddonFolder::from_path(Flavour::Mainline, &dir_b).unwrap(),
    ];

    let cat = catalogue(vec![entry(
        "curse",
        "1",
        "Foo",
        vec![vec!["Foo", "Foo_Config"]],
        vec![],
    )]);
    let groups = match_folder_name_subsets(&leftovers, Flavour::Mainline, &cat, &sources());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].folders.len(), 2);
    assert_eq!(groups[0].defns, vec![Defn::new("curse", "1")]);
}

#[test]
fn match_folder_name_subsets_ignores_wrong_flavour_entries() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(dir.path(), "Foo", "Foo.toc", "## Version: 1");
    let leftovers = vec![AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap()];

    let mut e = entry("curse", "1", "Foo", vec![vec!["Foo"]], vec![]);
    e.game_flavours = vec![Flavour::VanillaClassic];
    let groups = match_folder_name_subsets(
        &leftovers,
        Flavour::Mainline,
        &catalogue(vec![e]),
        &sources(),
    );
    assert!(groups.is_empty());
}

// ---------------------------------------------------------------------------
// match_addon_names_with_folder_names
// ---------------------------------------------------------------------------

#[test]
fn match_addon_names_with_folder_names_matches_normalised_name() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(dir.path(), "Weak Auras", "Weak Auras.toc", "## Version: 1");
    let leftovers = vec![AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap()];

    let cat = catalogue(vec![entry("curse", "1", "WeakAuras", vec![], vec![])]);
    let groups = match_addon_names_with_folder_names(&leftovers, &cat);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].defns, vec![Defn::new("curse", "1")]);
}

#[test]
fn match_addon_names_with_folder_names_no_match_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = write_toc(
        dir.path(),
        "Totally Unrelated",
        "Totally Unrelated.toc",
        "## Version: 1",
    );
    let leftovers = vec![AddonFolder::from_path(Flavour::Mainline, &addon_dir).unwrap()];

    let cat = catalogue(vec![entry("curse", "1", "WeakAuras", vec![], vec![])]);
    let groups = match_addon_names_with_folder_names(&leftovers, &cat);
    assert!(groups.is_empty());
}

// ---------------------------------------------------------------------------
// find_equivalent_pkg_defns
// ---------------------------------------------------------------------------

#[test]
fn find_equivalent_pkg_defns_uses_catalogue_same_as_and_toc_keys() {
    let dir = tempfile::tempdir().unwrap();
    write_toc(dir.path(), "Foo", "Foo.toc", "## X-WoWI-ID: 999");

    let pkg = Pkg {
        source: "curse".into(),
        id: "1".into(),
        slug: "foo".into(),
        name: "Foo".into(),
        description: "".into(),
        url: "".into(),
        download_url: "".into(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1.0".into(),
        changelog_url: "".into(),
        options: crate::lockfile::PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![PkgFolder { name: "Foo".into() }],
        deps: vec![],
    };

    let cat = catalogue(vec![entry(
        "curse",
        "1",
        "Foo",
        vec![],
        vec![("github", "42")],
    )]);
    let result = find_equivalent_pkg_defns(&[pkg], dir.path(), Flavour::Mainline, &cat, &sources());

    let defns = result.get(&("curse".to_owned(), "1".to_owned())).unwrap();
    assert!(defns.contains(&Defn::new("github", "42")));
    assert!(defns.contains(&Defn::new("wowi", "999")));
    // github outranks wowi in priority order.
    assert_eq!(defns[0].source, "github");
}

#[test]
fn find_equivalent_pkg_defns_omits_packages_with_no_equivalents() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = Pkg {
        source: "curse".into(),
        id: "1".into(),
        slug: "foo".into(),
        name: "Foo".into(),
        description: "".into(),
        url: "".into(),
        download_url: "".into(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1.0".into(),
        changelog_url: "".into(),
        options: crate::lockfile::PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![],
        deps: vec![],
    };
    let result = find_equivalent_pkg_defns(
        &[pkg],
        dir.path(),
        Flavour::Mainline,
        &catalogue(vec![]),
        &sources(),
    );
    assert!(result.is_empty());
}
