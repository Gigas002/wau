use std::sync::Mutex;

use super::*;
use crate::model::Flavour;

/// Env-var-touching tests must not run concurrently (process-global state).
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    /// Sets `key` for the duration of this guard, restoring (or clearing) it on drop.
    /// Callers must hold [`ENV_LOCK`] for the guard's whole lifetime.
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = env::var(key).ok();
        // SAFETY: caller holds `ENV_LOCK`, so no other test thread reads/writes env concurrently.
        unsafe { env::set_var(key, value.as_ref()) };
        Self { key, original }
    }

    /// Removes `key` for the duration of this guard, restoring it on drop.
    /// Callers must hold [`ENV_LOCK`] for the guard's whole lifetime.
    fn unset(key: &'static str) -> Self {
        let original = env::var(key).ok();
        // SAFETY: see `set`.
        unsafe { env::remove_var(key) };
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: see `set`.
        unsafe {
            match &self.original {
                Some(v) => env::set_var(self.key, v),
                None => env::remove_var(self.key),
            }
        }
    }
}

fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// Dirs
// ---------------------------------------------------------------------------

#[test]
fn wau_home_overrides_all_three_dirs_without_app_name_segment() {
    let _lock = lock_env();
    let home = EnvVarGuard::set(HOME_ENV_VAR, "/tmp/wau-home-test");

    assert_eq!(config_dir(&[]), PathBuf::from("/tmp/wau-home-test/config"));
    assert_eq!(cache_dir(&[]), PathBuf::from("/tmp/wau-home-test/cache"));
    assert_eq!(state_dir(&[]), PathBuf::from("/tmp/wau-home-test/state"));

    drop(home);
}

#[test]
fn wau_home_appends_extra_parts() {
    let _lock = lock_env();
    let _home = EnvVarGuard::set(HOME_ENV_VAR, "/tmp/wau-home-test");

    assert_eq!(
        config_dir(&["profiles", "default"]),
        PathBuf::from("/tmp/wau-home-test/config/profiles/default")
    );
}

#[test]
fn xdg_config_home_is_honoured_when_wau_home_unset() {
    let _lock = lock_env();
    let _clear_home = EnvVarGuard::unset(HOME_ENV_VAR);
    let _xdg = EnvVarGuard::set("XDG_CONFIG_HOME", "/tmp/xdg-config");

    assert_eq!(config_dir(&[]), PathBuf::from("/tmp/xdg-config/wau"));
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
// GlobalConfig
// ---------------------------------------------------------------------------

#[test]
fn global_config_defaults_when_no_file_or_env() {
    let _lock = lock_env();
    let home = EnvVarGuard::set(HOME_ENV_VAR, tempfile::tempdir().unwrap().keep());

    let config = GlobalConfig::read().unwrap();
    assert!(config.auto_update_check);
    assert!(config.access_tokens.is_empty());

    drop(home);
}

#[test]
fn global_config_env_var_overrides_default() {
    let _lock = lock_env();
    let _home = EnvVarGuard::set(HOME_ENV_VAR, tempfile::tempdir().unwrap().keep());
    let _flag = EnvVarGuard::set("WAU_AUTO_UPDATE_CHECK", "false");
    let _token = EnvVarGuard::set("WAU_ACCESS_TOKENS_GITHUB", "env-token");

    let config = GlobalConfig::read().unwrap();
    assert!(!config.auto_update_check);
    assert_eq!(config.access_tokens.github.unwrap().expose(), "env-token");
}

#[test]
fn global_config_write_then_read_round_trips() {
    let _lock = lock_env();
    let _home = EnvVarGuard::set(HOME_ENV_VAR, tempfile::tempdir().unwrap().keep());

    let mut config = GlobalConfig::from_env();
    config.auto_update_check = false;
    config.access_tokens.cfcore = Some(SecretString::new("cf-key"));
    config.write().unwrap();

    let read_back = GlobalConfig::read().unwrap();
    assert!(!read_back.auto_update_check);
    assert_eq!(read_back.access_tokens.cfcore.unwrap().expose(), "cf-key");
}

#[test]
fn global_config_env_wins_over_file() {
    let _lock = lock_env();
    let _home = EnvVarGuard::set(HOME_ENV_VAR, tempfile::tempdir().unwrap().keep());

    let mut config = GlobalConfig::from_env();
    config.auto_update_check = true;
    config.write().unwrap();

    let _flag = EnvVarGuard::set("WAU_AUTO_UPDATE_CHECK", "false");
    let read_back = GlobalConfig::read().unwrap();
    assert!(!read_back.auto_update_check);
}

#[test]
fn global_config_access_tokens_independent_file_overrides_main_file() {
    let _lock = lock_env();
    let _home = EnvVarGuard::set(HOME_ENV_VAR, tempfile::tempdir().unwrap().keep());

    let mut config = GlobalConfig::from_env();
    config.access_tokens.github = Some(SecretString::new("from-main-file"));
    config.write().unwrap();

    let tokens = AccessTokens {
        github: Some(SecretString::new("from-sibling-file")),
        ..Default::default()
    };
    fs::write(
        config.access_tokens_file_path(),
        serde_json::to_string(&tokens).unwrap(),
    )
    .unwrap();

    let read_back = GlobalConfig::read().unwrap();
    assert_eq!(
        read_back.access_tokens.github.unwrap().expose(),
        "from-sibling-file"
    );
}

// ---------------------------------------------------------------------------
// ProfileConfig
// ---------------------------------------------------------------------------

fn global_config_in(dir: &std::path::Path) -> GlobalConfig {
    let mut config = GlobalConfig::from_env();
    config.dirs = Dirs {
        cache: dir.join("cache"),
        config: dir.join("config"),
        state: dir.join("state"),
    };
    config
}

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

    assert!(profile.db_file_path().starts_with(profile.config_path()));

    let read_back = ProfileConfig::read(global, "retail-main").unwrap();
    assert_eq!(read_back.profile, "retail-main");
    assert_eq!(read_back.addon_dir, addon_dir);
    assert_eq!(read_back.flavour_override, Some(Flavour::Mainline));
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
fn profile_config_delete_trashes_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    fs::create_dir_all(&addon_dir).unwrap();

    let profile =
        ProfileConfig::new(global, "gone-soon", &addon_dir, Some(Flavour::Mainline)).unwrap();
    profile.write().unwrap();
    let config_path = profile.config_path();
    assert!(config_path.exists());

    profile.delete().unwrap();
    assert!(!config_path.exists());
}
