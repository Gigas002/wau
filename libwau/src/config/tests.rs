use super::*;
use crate::model::Flavour;

// ---------------------------------------------------------------------------
// Dirs
// ---------------------------------------------------------------------------

#[test]
fn config_dir_ends_with_app_name() {
    assert_eq!(config_dir().file_name().unwrap(), "wau");
}

#[test]
fn cache_dir_ends_with_app_name() {
    assert_eq!(cache_dir().file_name().unwrap(), "wau");
}

// ---------------------------------------------------------------------------
// SecretString
// ---------------------------------------------------------------------------

#[test]
fn secret_string_redacts_debug_and_display() {
    let s = SecretString::new("super-secret");
    assert_eq!(format!("{s}"), "**********");
    assert!(format!("{s:?}").contains("**********"));
    assert!(!format!("{s:?}").contains("super-secret"));
}

#[test]
fn secret_string_serializes_in_plaintext() {
    let s = SecretString::new("super-secret");
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, "\"super-secret\"");
}

// ---------------------------------------------------------------------------
// LogLevel
// ---------------------------------------------------------------------------

#[test]
fn log_level_parse_accepts_known_spellings() {
    assert_eq!(LogLevel::parse("info"), Some(LogLevel::Info));
    assert_eq!(LogLevel::parse("WARN"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::parse("warning"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::parse("bogus"), None);
}

#[test]
fn log_level_defaults_to_warn() {
    assert_eq!(LogLevel::default(), LogLevel::Warn);
}

// ---------------------------------------------------------------------------
// GlobalConfig
// ---------------------------------------------------------------------------

fn global_config_in(dir: &std::path::Path) -> GlobalConfig {
    let mut config = GlobalConfig::defaults();
    config.dirs = Dirs {
        cache: dir.join("cache"),
        config: dir.join("config"),
    };
    config
}

#[test]
fn global_config_defaults_when_no_file() {
    let config = GlobalConfig::defaults();
    assert_eq!(config.log_level, LogLevel::Warn);
    assert!(config.access_tokens.cfcore.is_none());
    assert!(config.access_tokens.github.is_none());
    assert!(config.access_tokens.wago_addons.is_none());
}

#[test]
fn global_config_write_then_read_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = global_config_in(dir.path());
    config.log_level = LogLevel::Debug;
    config.access_tokens.cfcore = Some(SecretString::new("cf-key"));
    config.write().unwrap();

    let raw = fs::read_to_string(config.config_file_path()).unwrap();
    let file: GlobalConfigFile = toml::from_str(&raw).unwrap();
    let read_back = GlobalConfig::from_file(file);
    assert_eq!(read_back.log_level, LogLevel::Debug);
    assert_eq!(read_back.access_tokens.cfcore.unwrap().expose(), "cf-key");
}

#[test]
fn global_config_write_persists_all_three_providers() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = global_config_in(dir.path());
    config.access_tokens.cfcore = Some(SecretString::new("cf-key"));
    config.access_tokens.github = Some(SecretString::new("gh-token"));
    config.access_tokens.wago_addons = Some(SecretString::new("wago-token"));
    config.write().unwrap();

    let raw = fs::read_to_string(config.config_file_path()).unwrap();
    let file: GlobalConfigFile = toml::from_str(&raw).unwrap();
    let providers = file.providers.unwrap();
    assert_eq!(
        providers.curseforge.unwrap().api_key.unwrap().expose(),
        "cf-key"
    );
    assert_eq!(
        providers.github.unwrap().api_key.unwrap().expose(),
        "gh-token"
    );
    assert_eq!(
        providers.wago.unwrap().api_key.unwrap().expose(),
        "wago-token"
    );
}

#[test]
fn global_config_paths_cache_override_expands_tilde_and_is_used() {
    let toml = r#"
        [paths]
        cache = "~/custom-cache"
    "#;
    let file: GlobalConfigFile = toml::from_str(toml).unwrap();
    let config = GlobalConfig::from_file(file);
    assert!(!config.dirs.cache.starts_with("~"));
    assert!(config.dirs.cache.ends_with("custom-cache"));
}

#[test]
fn global_config_logging_level_round_trips() {
    let toml = r#"
        [logging]
        level = "trace"
    "#;
    let file: GlobalConfigFile = toml::from_str(toml).unwrap();
    let config = GlobalConfig::from_file(file);
    assert_eq!(config.log_level, LogLevel::Trace);
}

#[test]
fn global_config_read_from_none_matches_read() {
    let config = GlobalConfig::read_from(None).unwrap();
    assert_eq!(config.dirs.config, config_dir());
}

#[test]
fn global_config_read_from_a_path_uses_its_parent_as_the_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("fixture-config.toml");
    fs::write(&config_path, "[logging]\nlevel = \"debug\"\n").unwrap();

    let config = GlobalConfig::read_from(Some(&config_path)).unwrap();
    assert_eq!(config.log_level, LogLevel::Debug);
    assert_eq!(config.dirs.config, dir.path());
    assert_eq!(config.config_file_path(), config_path);
}

#[test]
fn global_config_read_from_a_missing_path_falls_back_to_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("does-not-exist.toml");

    let config = GlobalConfig::read_from(Some(&config_path)).unwrap();
    assert_eq!(config.log_level, LogLevel::Warn);
    assert_eq!(config.config_file_path(), config_path);
}

// ---------------------------------------------------------------------------
// ProfileConfig
// ---------------------------------------------------------------------------

#[test]
fn profile_config_new_rejects_empty_profile_name() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let err = ProfileConfig::new(global, "   ", addon_dir, Some(Flavour::Mainline)).unwrap_err();
    assert!(matches!(err, ConfigError::EmptyProfileName));
}

#[test]
fn profile_config_new_rejects_non_writable_addon_dir() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let missing = dir.path().join("does-not-exist");

    let err = ProfileConfig::new(global, "default", missing, Some(Flavour::Mainline)).unwrap_err();
    assert!(matches!(err, ConfigError::AddonDirNotWritable { .. }));
}

#[test]
fn profile_config_new_uses_flavour_override_without_detection() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("some-custom-server-dir");
    fs::create_dir_all(&addon_dir).unwrap();

    let profile =
        ProfileConfig::new(global, "custom", &addon_dir, Some(Flavour::TbcClassic)).unwrap();
    assert_eq!(
        profile.product,
        InstalledProduct::Overridden(Flavour::TbcClassic)
    );
    assert_eq!(profile.product.flavour(), Flavour::TbcClassic);
}

#[test]
fn profile_config_new_detects_flavour_from_known_subfolder() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir
        .path()
        .join("_classic_era_")
        .join("Interface")
        .join("AddOns");
    fs::create_dir_all(&addon_dir).unwrap();

    let profile = ProfileConfig::new(global, "classic", &addon_dir, None).unwrap();
    assert_eq!(profile.product.flavour(), Flavour::VanillaClassic);
    assert!(matches!(profile.product, InstalledProduct::Known(_)));
}

#[test]
fn profile_config_new_errors_when_flavour_cannot_be_detected() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("unrecognised-subfolder");
    fs::create_dir_all(&addon_dir).unwrap();

    let err = ProfileConfig::new(global, "mystery", &addon_dir, None).unwrap_err();
    assert!(matches!(err, ConfigError::NoFlavourDetected { .. }));
}

#[test]
fn profile_config_write_then_read_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let profile = ProfileConfig::new(
        global.clone(),
        "retail-main",
        &addon_dir,
        Some(Flavour::Mainline),
    )
    .unwrap();
    profile.write().unwrap();

    assert!(
        profile
            .config_file_path()
            .starts_with(profiles_dir_path(&global))
    );
    assert_eq!(profile.config_file_path().extension().unwrap(), "toml");
    assert_eq!(profile.lock_file_path().extension().unwrap(), "toml");

    let read_back = ProfileConfig::read(global, "retail-main").unwrap();
    assert_eq!(read_back.profile, "retail-main");
    assert_eq!(read_back.addon_dir, addon_dir);
    assert_eq!(read_back.flavour_override, Some(Flavour::Mainline));
}

#[test]
fn profile_config_uses_a_name_subdirectory_with_a_fixed_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let profile =
        ProfileConfig::new(global.clone(), "retail", &addon_dir, Some(Flavour::Mainline)).unwrap();
    profile.write().unwrap();

    assert_eq!(
        profile.config_file_path(),
        profiles_dir_path(&global)
            .join("retail")
            .join("profile.toml")
    );
    assert_eq!(
        profile.lock_file_path(),
        profiles_dir_path(&global).join("retail").join("lock.toml")
    );
}

#[test]
fn iter_profiles_ignores_directories_without_a_profile_file() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    ProfileConfig::new(global.clone(), "real", &addon_dir, Some(Flavour::Mainline))
        .unwrap()
        .write()
        .unwrap();
    fs::create_dir_all(profiles_dir_path(&global).join("junk")).unwrap();

    assert_eq!(ProfileConfig::iter_profiles(&global), vec!["real".to_owned()]);
}

#[test]
fn profile_config_file_uses_path_and_flavour_keys() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    ProfileConfig::new(global.clone(), "keyed", &addon_dir, Some(Flavour::Mainline))
        .unwrap()
        .write()
        .unwrap();

    let raw = fs::read_to_string(profile_config_file_path(&global, "keyed")).unwrap();
    assert!(raw.contains("profile ="));
    assert!(raw.contains("path ="));
    assert!(raw.contains("flavour ="));
}

#[test]
fn profile_config_read_missing_profile_errors_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());

    let err = ProfileConfig::read(global, "nope").unwrap_err();
    assert!(matches!(err, ConfigError::NotFound { .. }));
}

#[test]
fn iter_profiles_lists_every_written_profile_sorted() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    for name in ["zeta", "alpha"] {
        ProfileConfig::new(global.clone(), name, &addon_dir, Some(Flavour::Mainline))
            .unwrap()
            .write()
            .unwrap();
    }

    assert_eq!(
        ProfileConfig::iter_profiles(&global),
        vec!["alpha".to_owned(), "zeta".to_owned()]
    );
}

#[test]
fn read_sole_errors_when_no_profiles_configured() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());

    let err = ProfileConfig::read_sole(global).unwrap_err();
    assert!(matches!(err, ConfigError::NoProfilesConfigured));
}

#[test]
fn read_sole_reads_the_only_profile() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    ProfileConfig::new(global.clone(), "retail", &addon_dir, Some(Flavour::Mainline))
        .unwrap()
        .write()
        .unwrap();

    let profile = ProfileConfig::read_sole(global).unwrap();
    assert_eq!(profile.profile, "retail");
}

#[test]
fn read_sole_errors_with_available_names_when_multiple_profiles_configured() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    for name in ["zeta", "alpha"] {
        ProfileConfig::new(global.clone(), name, &addon_dir, Some(Flavour::Mainline))
            .unwrap()
            .write()
            .unwrap();
    }

    let err = ProfileConfig::read_sole(global).unwrap_err();
    assert!(matches!(
        err,
        ConfigError::AmbiguousProfile { available }
        if available == vec!["alpha".to_owned(), "zeta".to_owned()]
    ));
}

#[test]
fn iter_profile_installations_extracts_installation_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("_retail_").join("Interface").join("AddOns");
    fs::create_dir_all(&addon_dir).unwrap();

    ProfileConfig::new(global.clone(), "retail", &addon_dir, None)
        .unwrap()
        .write()
        .unwrap();

    let installs = ProfileConfig::iter_profile_installations(&global);
    assert_eq!(installs, vec![dir.path().join("_retail_")]);
}

#[test]
fn profile_config_delete_trashes_config_file_and_lock_file() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let profile =
        ProfileConfig::new(global, "gone-soon", &addon_dir, Some(Flavour::Mainline)).unwrap();
    profile.write().unwrap();
    fs::write(profile.lock_file_path(), b"version = 1\n").unwrap();
    assert!(profile.config_file_path().exists());
    assert!(profile.lock_file_path().exists());

    profile.delete().unwrap();
    assert!(!profile.config_file_path().exists());
    assert!(!profile.lock_file_path().exists());
}

#[test]
fn profile_config_read_from_path_bypasses_name_based_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let fixture_path = dir.path().join("fixture.toml");
    let contents = ProfileConfigFile {
        profile: "fixture".to_owned(),
        path: addon_dir.clone(),
        flavour: Some(Flavour::Mainline),
    };
    fs::write(&fixture_path, toml::to_string_pretty(&contents).unwrap()).unwrap();

    let profile = ProfileConfig::read_from_path(global, &fixture_path).unwrap();
    assert_eq!(profile.profile, "fixture");
    assert_eq!(profile.addon_dir, addon_dir);
    assert_eq!(profile.config_file_path(), fixture_path);
    assert_eq!(
        profile.lock_file_path(),
        fixture_path.with_file_name("lock.toml")
    );
}

#[test]
fn profile_config_with_path_override_writes_to_that_exact_path() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let target = dir.path().join("nested").join("override.toml");
    let profile = ProfileConfig::new(
        global,
        "irrelevant-name",
        &addon_dir,
        Some(Flavour::Mainline),
    )
    .unwrap()
    .with_path_override(&target);
    profile.write().unwrap();

    assert!(target.exists());
    assert_eq!(profile.config_file_path(), target);
    assert_eq!(
        profile.lock_file_path(),
        target.with_file_name("lock.toml")
    );
}

#[test]
fn profile_config_delete_without_lock_file_still_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let profile =
        ProfileConfig::new(global, "no-lock-yet", &addon_dir, Some(Flavour::Mainline)).unwrap();
    profile.write().unwrap();
    assert!(!profile.lock_file_path().exists());

    profile.delete().unwrap();
    assert!(!profile.config_file_path().exists());
}
