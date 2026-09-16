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

    let app_ctx = AppCtx::from_profile(profile).unwrap();

    assert_eq!(app_ctx.profile.product.flavour(), Flavour::Mainline);
    assert_eq!(app_ctx.profile.addon_dir, addon_dir.path());
    assert!(db::get_all_pkgs(&app_ctx.conn).unwrap().is_empty());
}

#[test]
fn read_profile_resolves_a_bare_name_inside_the_config_dir() {
    let dir = tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    std::fs::create_dir_all(&addon_dir).unwrap();
    ProfileConfig::new(global.clone(), "named", &addon_dir, Some(Flavour::Mainline))
        .unwrap()
        .write()
        .unwrap();

    let profile = read_profile(global, "named").unwrap();
    assert_eq!(profile.profile, "named");
}

#[test]
fn read_profile_reads_directly_from_a_toml_path() {
    let dir = tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    std::fs::create_dir_all(&addon_dir).unwrap();

    let fixture_path = dir.path().join("fixture-profile.toml");
    std::fs::write(
        &fixture_path,
        format!(
            "profile = \"fixture\"\npath = {:?}\nflavour = \"mainline\"\n",
            addon_dir
        ),
    )
    .unwrap();

    let profile = read_profile(global, fixture_path.to_str().unwrap()).unwrap();
    assert_eq!(profile.profile, "fixture");
    assert_eq!(profile.config_file_path(), fixture_path);
    assert_eq!(
        profile.db_file_path(),
        fixture_path.with_extension("sqlite")
    );
}

#[test]
fn new_profile_with_a_path_argument_writes_at_that_exact_path() {
    let dir = tempdir().unwrap();
    let global = global_config_in(dir.path());
    let addon_dir = dir.path().join("addons");
    std::fs::create_dir_all(&addon_dir).unwrap();

    let target = dir.path().join("bootstrapped.toml");
    let profile = new_profile(
        global,
        target.to_str().unwrap(),
        &addon_dir,
        Some(Flavour::Mainline),
    )
    .unwrap();
    profile.write().unwrap();

    assert_eq!(profile.profile, "bootstrapped");
    assert!(target.exists());
    assert_eq!(profile.config_file_path(), target);
}
