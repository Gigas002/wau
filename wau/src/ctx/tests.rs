use libwau::model::Flavour;
use tempfile::tempdir;

use super::*;

fn global_config_in(dir: &std::path::Path) -> GlobalConfig {
    let mut config = GlobalConfig::defaults();
    config.dirs = libwau::config::Dirs {
        cache: dir.join("cache"),
        config: dir.join("config"),
    };
    config
}

#[test]
fn profile_config_read_reports_not_found_for_an_unconfigured_profile() {
    let dir = tempdir().unwrap();
    let global = global_config_in(dir.path());

    let err = ProfileConfig::read(global, "nonexistent-profile").unwrap_err();
    assert!(matches!(err, ConfigError::NotFound { .. }));
}

#[test]
fn from_profile_assembles_a_working_ctx() {
    let dir = tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = tempdir().unwrap();

    let profile =
        ProfileConfig::new(global, "default", addon_dir.path(), Some(Flavour::Mainline)).unwrap();

    let app_ctx = AppCtx::from_profile(profile, true).unwrap();

    assert_eq!(app_ctx.profile.product.flavour(), Flavour::Mainline);
    assert_eq!(app_ctx.profile.addon_dir, addon_dir.path());
    assert!(db::get_all_pkgs(&app_ctx.conn).unwrap().is_empty());
}
