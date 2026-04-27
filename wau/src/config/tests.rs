use super::*;

const FULL_CONFIG: &str = r#"
schema = 1

[defaults]
flavor = "retail"
channel = "stable"
install_tag = "retail-main"

[logging]
level = "info"

[paths]
cache = "/tmp/wau-test-cache"

[[paths.installs]]
tag = "retail-main"
flavor = "retail"
wow_root = "/games/World of Warcraft"

[[paths.installs]]
tag = "classic-official"
flavor = "classic-era"
wow_root = "/games/World of Warcraft"

[[paths.installs]]
tag = "classic-turtle"
flavor = "classic-era"
wow_root = "/games/TurtleWoW/World of Warcraft"

[providers.curseforge]
api_key = "test-token"
"#;

const MINIMAL_CONFIG: &str = r#"
schema = 1

[defaults]
flavor = "retail"
channel = "stable"
install_tag = "retail-main"

[paths]
cache = "/tmp/wau-test-cache"
"#;

#[test]
fn parse_full_config() {
    let cfg = parse(FULL_CONFIG).unwrap();
    assert_eq!(cfg.schema, 1);
    assert_eq!(cfg.defaults.flavor.as_str(), "retail");
    assert_eq!(cfg.defaults.channel.as_str(), "stable");
    assert_eq!(cfg.defaults.install_tag.as_str(), "retail-main");
    assert_eq!(cfg.logging.level, libwau::model::LogLevel::Info);
    assert_eq!(cfg.paths.installs.len(), 3);
    assert_eq!(
        cfg.providers.curseforge.as_ref().unwrap().api_key,
        "test-token"
    );
}

#[test]
fn parse_minimal_config_applies_defaults() {
    let cfg = parse(MINIMAL_CONFIG).unwrap();
    assert_eq!(cfg.schema, 1);
    assert_eq!(cfg.logging.level, libwau::model::LogLevel::Info);
    assert!(cfg.providers.curseforge.is_none());
    assert!(cfg.paths.installs.is_empty());
}

#[test]
fn installs_parsed_correctly() {
    let cfg = parse(FULL_CONFIG).unwrap();

    let retail = &cfg.paths.installs[0];
    assert_eq!(retail.tag.as_str(), "retail-main");
    assert_eq!(retail.flavor.as_str(), "retail");

    let turtle = &cfg.paths.installs[2];
    assert_eq!(turtle.tag.as_str(), "classic-turtle");
    assert_eq!(turtle.flavor.as_str(), "classic-era");
}

#[test]
fn addons_path_returns_correct_subpath() {
    let cfg = parse(FULL_CONFIG).unwrap();
    let tag = Tag::new("retail-main");
    let path = cfg.addons_path(&tag).unwrap();
    assert!(path.ends_with("Interface/AddOns"));
}

#[test]
fn addons_path_returns_none_for_unknown_tag() {
    let cfg = parse(FULL_CONFIG).unwrap();
    let tag = Tag::new("nonexistent");
    assert!(cfg.addons_path(&tag).is_none());
}

#[test]
fn tilde_expansion_in_cache_path() {
    let toml = r#"
schema = 1

[defaults]
flavor = "retail"
channel = "stable"
install_tag = "main"

[paths]
cache = "~/.cache/wau"
"#;
    let cfg = parse(toml).unwrap();
    let cache = cfg.paths.cache.to_string_lossy();
    assert!(
        !cache.starts_with('~'),
        "tilde should be expanded, got: {cache}"
    );
}

#[test]
fn resolved_path_prefers_override() {
    let custom = Path::new("/custom/config.toml");
    assert_eq!(
        resolved_path(Some(custom)),
        PathBuf::from("/custom/config.toml")
    );
}

#[test]
fn resolved_path_defaults_to_xdg() {
    let path = resolved_path(None);
    assert!(
        path.ends_with("wau/config.toml"),
        "expected XDG path, got: {}",
        path.display()
    );
}

#[test]
fn load_returns_not_found_error() {
    let result = load(Path::new("/nonexistent/path/config.toml"));
    assert!(matches!(result, Err(ConfigError::NotFound { .. })));
}

// ── examples/config.toml ─────────────────────────────────────────────────────

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples")
}

#[test]
fn example_config_parses() {
    load(&examples_dir().join("config.toml")).unwrap();
}

#[test]
fn example_config_schema_version() {
    let cfg = load(&examples_dir().join("config.toml")).unwrap();
    assert_eq!(cfg.schema, 1);
}

#[test]
fn example_config_defaults() {
    let cfg = load(&examples_dir().join("config.toml")).unwrap();
    assert!(!cfg.defaults.install_tag.as_str().is_empty());
}

#[test]
fn example_config_has_installs() {
    let cfg = load(&examples_dir().join("config.toml")).unwrap();
    assert!(
        !cfg.paths.installs.is_empty(),
        "expected at least one install in examples/config.toml"
    );
    for install in &cfg.paths.installs {
        assert!(!install.tag.as_str().is_empty());
        // wow_root tilde expansion: should not start with '~' after load
        let root = install.wow_root.to_string_lossy();
        assert!(
            !root.starts_with('~'),
            "wow_root tilde was not expanded: {root}"
        );
    }
}

#[test]
fn example_config_logging_level_is_valid() {
    let cfg = load(&examples_dir().join("config.toml")).unwrap();
    // Parsing validates the variant; just confirm it round-trips through Display.
    let _ = cfg.logging.level.to_string();
}

#[test]
fn example_config_curseforge_provider_present() {
    let cfg = load(&examples_dir().join("config.toml")).unwrap();
    assert!(
        cfg.providers.curseforge.is_some(),
        "expected [providers.curseforge] in examples/config.toml"
    );
}
