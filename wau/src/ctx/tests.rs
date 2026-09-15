use std::sync::Mutex;

use libwau::model::Flavour;
use tempfile::tempdir;

use super::*;

/// Env-var-touching tests must not run concurrently (process-global state).
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var(key).ok();
        // SAFETY: caller holds `ENV_LOCK` for the guard's whole lifetime.
        unsafe { std::env::set_var(key, value.as_ref()) };
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: see `set`.
        unsafe {
            match &self.original {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn build_reports_not_found_for_an_unconfigured_profile() {
    let _lock = lock_env();
    let wau_home = tempdir().unwrap();
    let _guard = EnvVarGuard::set("WAU_HOME", wau_home.path());

    let Err(err) = AppCtx::build("nonexistent-profile", true) else {
        panic!("expected an unconfigured-profile error");
    };
    assert!(matches!(
        err,
        CtxError::Config(ConfigError::NotFound { .. })
    ));
}

#[test]
fn from_profile_assembles_a_working_ctx() {
    let _lock = lock_env();
    let wau_home = tempdir().unwrap();
    let _guard = EnvVarGuard::set("WAU_HOME", wau_home.path());
    let addon_dir = tempdir().unwrap();

    let global = GlobalConfig::from_env();
    let profile =
        ProfileConfig::new(global, "default", addon_dir.path(), Some(Flavour::Mainline)).unwrap();

    let app_ctx = AppCtx::from_profile(profile, true).unwrap();

    assert_eq!(app_ctx.profile.product.flavour(), Flavour::Mainline);
    assert_eq!(app_ctx.profile.addon_dir, addon_dir.path());
    assert!(db::get_all_pkgs(&app_ctx.conn).unwrap().is_empty());
}
