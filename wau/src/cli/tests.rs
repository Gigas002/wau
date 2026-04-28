use super::*;
use clap::Parser;

#[test]
fn list_no_args_uses_defaults() {
    let cli = Cli::try_parse_from(["wau", "list"]).unwrap();
    assert!(matches!(cli.command, Command::List(_)));
    let Command::List(args) = cli.command else {
        panic!()
    };
    assert!(args.tag.is_none());
}

#[test]
fn list_with_tag() {
    let cli = Cli::try_parse_from(["wau", "list", "--tag", "retail-main"]).unwrap();
    let Command::List(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("retail-main"));
}

#[test]
fn list_with_config_override() {
    let cli = Cli::try_parse_from(["wau", "--config", "/tmp/test.toml", "list"]).unwrap();
    assert_eq!(
        cli.config.as_deref(),
        Some(std::path::Path::new("/tmp/test.toml"))
    );
}

#[test]
fn sync_no_args_uses_defaults() {
    let cli = Cli::try_parse_from(["wau", "sync"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert!(args.tag.is_none());
    assert!(args.manifest.is_none());
    assert!(!args.update);
}

#[test]
fn sync_with_update_flag() {
    let cli = Cli::try_parse_from(["wau", "sync", "--update"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert!(args.update);
}

#[test]
fn sync_with_manifest_override() {
    let cli = Cli::try_parse_from(["wau", "sync", "--manifest", "/tmp/manifest.toml"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert_eq!(
        args.manifest.as_deref(),
        Some(std::path::Path::new("/tmp/manifest.toml"))
    );
}

#[test]
fn sync_with_tag() {
    let cli = Cli::try_parse_from(["wau", "sync", "--tag", "classic-era"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("classic-era"));
}

#[test]
fn remove_requires_addon_names() {
    assert!(Cli::try_parse_from(["wau", "remove"]).is_err());
}

#[test]
fn remove_one_addon() {
    let cli = Cli::try_parse_from(["wau", "remove", "WeakAuras"]).unwrap();
    let Command::Remove(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.addons, vec!["WeakAuras"]);
}

#[test]
fn remove_multiple_addons() {
    let cli = Cli::try_parse_from(["wau", "remove", "WeakAuras", "Bagnon", "Details"]).unwrap();
    let Command::Remove(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.addons, vec!["WeakAuras", "Bagnon", "Details"]);
}

#[test]
fn remove_with_tag() {
    let cli = Cli::try_parse_from(["wau", "remove", "--tag", "classic-era", "Questie"]).unwrap();
    let Command::Remove(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("classic-era"));
    assert_eq!(args.addons, vec!["Questie"]);
}
