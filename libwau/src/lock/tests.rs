use super::*;
use crate::model::{Channel, Flavor, Provider, Tag};

const EXAMPLE_LOCK: &str = r#"
schema = 1
generated_at = "2026-04-22T00:00:00Z"
install_tag = "retail-main"

[[addon]]
name = "Bagnon"
provider = "curseforge"
project_id = 12345
flavor = "retail"
channel = "stable"
resolved_version = "10.0.0"
resolved_id = "cf-file-999999"
download_url = "https://example.invalid/Bagnon.zip"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
installed_dirs = ["Bagnon", "Bagnon_Config"]
installed_at = "2026-04-22T00:00:00Z"
"#;

#[test]
fn parse_example_lock() {
    let lock = parse(EXAMPLE_LOCK).unwrap();
    assert_eq!(lock.schema, 1);
    assert_eq!(lock.install_tag.as_str(), "retail-main");
    assert_eq!(lock.addon.len(), 1);

    let addon = &lock.addon[0];
    assert_eq!(addon.name, "Bagnon");
    assert_eq!(addon.provider.as_str(), "curseforge");
    assert_eq!(addon.project_id, Some(12345));
    assert_eq!(addon.flavor.as_str(), "retail");
    assert_eq!(addon.channel.as_str(), "stable");
    assert_eq!(addon.resolved_version, "10.0.0");
    assert_eq!(addon.resolved_id, "cf-file-999999");
    assert_eq!(addon.installed_dirs, vec!["Bagnon", "Bagnon_Config"]);
    assert!(addon.sha256.is_some());
}

#[test]
fn empty_addon_list_parses() {
    let toml = r#"
schema = 1
generated_at = "2026-04-22T00:00:00Z"
install_tag = "retail-main"
"#;
    let lock = parse(toml).unwrap();
    assert_eq!(lock.addon.len(), 0);
}

#[test]
fn lock_new_constructor() {
    let tag = Tag::new("retail-main");
    let lock = Lock::new(tag.clone());
    assert_eq!(lock.schema, SUPPORTED_SCHEMA);
    assert_eq!(lock.install_tag, tag);
    assert!(lock.addon.is_empty());
}

#[test]
fn save_and_reload_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock.toml");

    let mut lock = Lock::new(Tag::new("classic-official"));
    lock.addon.push(LockedAddon {
        name: "Questie".into(),
        provider: Provider::GitHub,
        flavor: Flavor::Era,
        channel: Channel::Stable,
        project_id: None,
        resolved_version: "6.5.0".into(),
        resolved_id: "v6.5.0".into(),
        download_url: "https://example.invalid/Questie.zip".into(),
        sha256: Some("abcd".repeat(16)),
        installed_dirs: vec!["Questie".into()],
        installed_at: Utc::now(),
    });

    save(&lock, &path).unwrap();
    let reloaded = load(&path).unwrap();

    assert_eq!(reloaded.install_tag, lock.install_tag);
    assert_eq!(reloaded.addon.len(), 1);
    assert_eq!(reloaded.addon[0].name, "Questie");
    assert_eq!(reloaded.addon[0].resolved_version, "6.5.0");
    assert_eq!(reloaded.addon[0].installed_dirs, vec!["Questie"]);
}

#[test]
fn load_returns_not_found_error() {
    let result = load(std::path::Path::new("/nonexistent/path/lock.toml"));
    assert!(matches!(result, Err(crate::Error::LockNotFound { .. })));
}
