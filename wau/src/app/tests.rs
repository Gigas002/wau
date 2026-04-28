// Integration-level tests that exercise the full pipeline live in libwau/tests/.
// These unit tests cover the app module's dependency on settings resolution.
// Settings::resolve is the gate that catches missing/invalid config before run;
// app::run itself no longer surfaces settings errors.
use std::path::PathBuf;

use crate::cli::{Cli, Command, InfoArgs, ListArgs, RemoveArgs, SearchArgs, SyncArgs};
use crate::settings::Settings;

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
fn list_settings_error_when_config_missing() {
    let cli = cli_with_missing_config(Command::List(ListArgs { tag: None }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn sync_settings_error_when_config_missing() {
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
fn remove_settings_error_when_config_missing() {
    let cli = cli_with_missing_config(Command::Remove(RemoveArgs {
        tag: None,
        addons: vec!["WeakAuras".into()],
    }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn search_settings_error_when_config_missing() {
    let cli = cli_with_missing_config(Command::Search(SearchArgs {
        query: "aura".into(),
        tag: None,
    }));
    assert!(Settings::resolve(&cli).is_err());
}

#[test]
fn info_settings_error_when_config_missing() {
    let cli = cli_with_missing_config(Command::Info(InfoArgs {
        addon: "WeakAuras".into(),
        tag: None,
    }));
    assert!(Settings::resolve(&cli).is_err());
}
