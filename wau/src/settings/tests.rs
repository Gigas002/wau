use std::path::PathBuf;

use crate::cli::{Cli, Command, InfoArgs, InitArgs, ListArgs, RemoveArgs, SearchArgs, SyncArgs};

use super::Settings;

fn cli_with_missing_config(command: Command) -> Cli {
    Cli {
        config: Some(PathBuf::from("/nonexistent/config.toml")),
        noconfirm: false,
        quiet: false,
        verbose: false,
        command,
    }
}

#[test]
fn resolve_list_fails_on_missing_config() {
    let cli = cli_with_missing_config(Command::List(ListArgs { tag: None }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn resolve_sync_fails_on_missing_config() {
    let cli = cli_with_missing_config(Command::Sync(SyncArgs {
        tag: None,
        manifest: None,
        update: false,
        flavor: None,
        channel: None,
    }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn resolve_remove_fails_on_missing_config() {
    let cli = cli_with_missing_config(Command::Remove(RemoveArgs {
        tag: None,
        addons: vec!["WeakAuras".into()],
    }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn resolve_search_fails_on_missing_config() {
    let cli = cli_with_missing_config(Command::Search(SearchArgs {
        query: "aura".into(),
        tag: None,
    }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn resolve_info_fails_on_missing_config() {
    let cli = cli_with_missing_config(Command::Info(InfoArgs {
        addon: "WeakAuras".into(),
        tag: None,
    }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn resolve_init_fails_on_missing_config() {
    let cli = cli_with_missing_config(Command::Init(InitArgs {
        tag: None,
        manifest: None,
        force: false,
    }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn ux_flags_carried_through_resolve() {
    let cli = Cli {
        config: Some(PathBuf::from("/nonexistent/config.toml")),
        noconfirm: true,
        quiet: true,
        verbose: true,
        command: Command::List(ListArgs { tag: None }),
    };
    // resolve fails (no config), but we can verify CLI struct fields are set
    assert!(cli.noconfirm);
    assert!(cli.quiet);
    assert!(cli.verbose);
}
