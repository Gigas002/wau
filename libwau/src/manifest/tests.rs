use super::*;
use crate::model::Provider;

const SIMPLE: &str = r#"
schema = 1

[[addon]]
name = "Bagnon"
provider = "curseforge"
project_id = 12345
channel = "stable"
flavors = ["retail"]

[[addon]]
name = "WeakAuras"
provider = "curseforge"
project_id = 90003
channel = "stable"
flavors = ["retail"]
"#;

const WITH_PINS: &str = r#"
schema = 1

[[addon]]
name = "DeadlyBossMods"
provider = "curseforge"
project_id = 90001
channel = "stable"
flavors = ["retail"]
pin = { version = "11.0.0" }

[[addon]]
name = "WeakAuras"
provider = "curseforge"
project_id = 90003
channel = "stable"
flavors = ["retail"]
pin = { file_id = 99887766 }
"#;

const GITHUB_ADDON: &str = r#"
schema = 1

[[addon]]
name = "SomeAddon"
provider = "github"
repo = "owner/repo"
asset_regex = "SomeAddon-.*\\.zip"
channel = "beta"
pin = { tag = "v2.1.0-beta.1" }

[[addon]]
name = "CommunityFork"
provider = "github"
repo = "org/wow-addon"
asset_regex = "release-.*\\.zip"
channel = "stable"
flavors = ["retail"]
pin = { tag = "v3.0.0", sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }

[[addon]]
name = "TipAddon"
provider = "github"
repo = "org/tip-addon"
git_ref = "main"
channel = "stable"
pin = { commit = "deadbeef0123456789deadbeef0123456789abcd" }
"#;

const MINIMAL: &str = r#"
schema = 1

[[addon]]
name = "MythicDungeonTools"
provider = "curseforge"
project_id = 90002
channel = "stable"
"#;

#[test]
fn parse_simple_manifest() {
    let m = parse(SIMPLE).unwrap();
    assert_eq!(m.schema, 1);
    assert_eq!(m.addon.len(), 2);

    let bagnon = &m.addon[0];
    assert_eq!(bagnon.name, "Bagnon");
    assert_eq!(bagnon.provider.as_str(), "curseforge");
    assert_eq!(bagnon.project_id, Some(12345));
    assert_eq!(bagnon.channel.as_ref().map(|c| c.as_str()), Some("stable"));
    assert!(bagnon.pin.is_none());
}

#[test]
fn parse_manifest_with_pins_does_not_error() {
    let m = parse(WITH_PINS).unwrap();
    assert_eq!(m.addon.len(), 2);
    assert!(m.addon[0].pin.is_some());
    assert!(m.addon[1].pin.is_some());
}

#[test]
fn pin_variants_parse_correctly() {
    let m = parse(GITHUB_ADDON).unwrap();
    assert_eq!(m.addon.len(), 3);

    // pin = { tag = "..." } -> Pin::Tag
    match &m.addon[0].pin {
        Some(Pin::Tag { tag }) => assert_eq!(tag, "v2.1.0-beta.1"),
        other => panic!("expected Pin::Tag, got {other:?}"),
    }

    // pin = { tag = "...", sha256 = "..." } -> Pin::TagWithSha
    match &m.addon[1].pin {
        Some(Pin::TagWithSha { tag, sha256 }) => {
            assert_eq!(tag, "v3.0.0");
            assert_eq!(sha256.len(), 64);
        }
        other => panic!("expected Pin::TagWithSha, got {other:?}"),
    }

    // pin = { commit = "..." } -> Pin::Commit
    match &m.addon[2].pin {
        Some(Pin::Commit { commit }) => assert_eq!(commit.len(), 40),
        other => panic!("expected Pin::Commit, got {other:?}"),
    }
}

#[test]
fn pin_version_and_file_id() {
    let m = parse(WITH_PINS).unwrap();

    match &m.addon[0].pin {
        Some(Pin::Version { version }) => assert_eq!(version, "11.0.0"),
        other => panic!("expected Pin::Version, got {other:?}"),
    }

    match &m.addon[1].pin {
        Some(Pin::FileId { file_id }) => assert_eq!(*file_id, 99887766),
        other => panic!("expected Pin::FileId, got {other:?}"),
    }
}

#[test]
fn optional_fields_default_to_none() {
    let m = parse(MINIMAL).unwrap();
    let addon = &m.addon[0];
    assert!(addon.flavors.is_none());
    assert!(addon.pin.is_none());
    assert!(addon.repo.is_none());
    assert!(addon.asset_regex.is_none());
    assert!(addon.git_ref.is_none());
}

#[test]
fn empty_addon_list_parses() {
    let m = parse("schema = 1\n").unwrap();
    assert_eq!(m.addon.len(), 0);
}

// ── from_installed ────────────────────────────────────────────────────────────

fn make_installed(folder: &str, x_fields: &[(&str, &str)]) -> crate::fs::InstalledAddon {
    let toc = crate::toc::TocFile {
        title: Some(folder.to_owned()),
        x_fields: x_fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Default::default()
    };
    crate::fs::InstalledAddon {
        folder: folder.to_owned(),
        toc_files: vec![toc],
    }
}

#[test]
fn from_installed_detects_curseforge() {
    let addons = vec![make_installed(
        "WeakAuras",
        &[("X-Curse-Project-ID", "13584")],
    )];
    let m = from_installed(&addons, &Flavor::Retail);
    assert_eq!(m.addon.len(), 1);
    let a = &m.addon[0];
    assert_eq!(a.name, "WeakAuras");
    assert_eq!(a.provider, Provider::CurseForge);
    assert_eq!(a.project_id, Some(13584));
    assert_eq!(a.flavors, Some(vec![Flavor::Retail]));
    assert_eq!(a.channel.as_ref().map(|c| c.as_str()), Some("stable"));
}

#[test]
fn from_installed_detects_wowinterface_wowi_id() {
    let addons = vec![make_installed("SomeAddon", &[("X-WoWI-ID", "9900")])];
    let m = from_installed(&addons, &Flavor::Retail);
    assert_eq!(m.addon.len(), 1);
    let a = &m.addon[0];
    assert_eq!(a.provider, Provider::WoWInterface);
    assert_eq!(a.wowi_id, Some(9900));
}

#[test]
fn from_installed_detects_wowinterface_wowinterface_id() {
    let addons = vec![make_installed(
        "OtherAddon",
        &[("X-WoWInterface-ID", "1234")],
    )];
    let m = from_installed(&addons, &Flavor::Era);
    assert_eq!(m.addon.len(), 1);
    assert_eq!(m.addon[0].provider, Provider::WoWInterface);
    assert_eq!(m.addon[0].wowi_id, Some(1234));
    assert_eq!(m.addon[0].flavors, Some(vec![Flavor::Era]));
}

#[test]
fn from_installed_skips_addon_with_no_provider() {
    let addons = vec![make_installed("NoProviderAddon", &[])];
    let m = from_installed(&addons, &Flavor::Retail);
    assert_eq!(m.addon.len(), 0);
}

#[test]
fn from_installed_deduplicates_by_curseforge_project_id() {
    let addons = vec![
        make_installed("Bagnon", &[("X-Curse-Project-ID", "12345")]),
        make_installed("Bagnon_Config", &[("X-Curse-Project-ID", "12345")]),
    ];
    let m = from_installed(&addons, &Flavor::Retail);
    assert_eq!(m.addon.len(), 1);
    assert_eq!(m.addon[0].name, "Bagnon");
}

#[test]
fn from_installed_deduplicates_by_wowi_id() {
    let addons = vec![
        make_installed("Addon", &[("X-WoWI-ID", "42")]),
        make_installed("Addon_Locale", &[("X-WoWI-ID", "42")]),
    ];
    let m = from_installed(&addons, &Flavor::Retail);
    assert_eq!(m.addon.len(), 1);
}

#[test]
fn from_installed_x_field_key_is_case_insensitive() {
    let addons = vec![make_installed(
        "CaseAddon",
        &[("x-curse-project-id", "777")],
    )];
    let m = from_installed(&addons, &Flavor::Retail);
    assert_eq!(m.addon.len(), 1);
    assert_eq!(m.addon[0].project_id, Some(777));
}

#[test]
fn from_installed_empty_list_gives_empty_manifest() {
    let m = from_installed(&[], &Flavor::Retail);
    assert_eq!(m.schema, SUPPORTED_SCHEMA);
    assert!(m.addon.is_empty());
}

#[test]
fn from_installed_mixed_providers() {
    let addons = vec![
        make_installed("CFAddon", &[("X-Curse-Project-ID", "100")]),
        make_installed("WowiAddon", &[("X-WoWI-ID", "200")]),
        make_installed("Unknown", &[]),
    ];
    let m = from_installed(&addons, &Flavor::Retail);
    assert_eq!(m.addon.len(), 2);
    assert_eq!(m.addon[0].provider, Provider::CurseForge);
    assert_eq!(m.addon[1].provider, Provider::WoWInterface);
}

// ── Manifest::new / Default / save ───────────────────────────────────────────

#[test]
fn new_manifest_default_is_empty() {
    let m = Manifest::new();
    assert_eq!(m.schema, SUPPORTED_SCHEMA);
    assert!(m.addon.is_empty());

    let d = Manifest::default();
    assert_eq!(d.schema, SUPPORTED_SCHEMA);
    assert!(d.addon.is_empty());
}

#[test]
fn save_empty_manifest_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manifest.toml");

    save(&Manifest::new(), &path).unwrap();

    let reloaded = load(&path).unwrap();
    assert_eq!(reloaded.schema, SUPPORTED_SCHEMA);
    assert!(reloaded.addon.is_empty());

    // An empty manifest should serialize to just `schema = 1` (no [[addon]] table).
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(!content.contains("addon"));
    assert!(content.contains("schema = 1"));
}

#[test]
fn save_manifest_with_addons_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("manifest.toml");

    let addon = ManifestAddon {
        name: "WeakAuras".into(),
        provider: Provider::CurseForge,
        channel: None,
        flavors: None,
        pin: None,
        project_id: Some(13584),
        wowi_id: None,
        repo: None,
        asset_regex: None,
        git_ref: None,
        url: None,
    };

    let mut m = Manifest::new();
    m.addon.push(addon);

    save(&m, &path).unwrap();

    let reloaded = load(&path).unwrap();
    assert_eq!(reloaded.schema, SUPPORTED_SCHEMA);
    assert_eq!(reloaded.addon.len(), 1);
    assert_eq!(reloaded.addon[0].name, "WeakAuras");
    assert_eq!(reloaded.addon[0].provider.as_str(), "curseforge");
    assert_eq!(reloaded.addon[0].project_id, Some(13584));
}
