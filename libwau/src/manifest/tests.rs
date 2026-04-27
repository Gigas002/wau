use super::*;

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
