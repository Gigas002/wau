use super::*;
use crate::cli::{Cli, Command, ListArgs, RemoveArgs, SyncArgs};

// Integration-level tests require a real config file + addons directory on disk,
// so they live in libwau/tests/ once the full pipeline is wired (Phase 3+).
// These unit tests only cover error-path behaviour that can be exercised without I/O.

fn missing_config_cli(command: Command) -> Cli {
    Cli {
        config: Some(std::path::PathBuf::from("/nonexistent/config.toml")),
        command,
    }
}

#[tokio::test]
async fn list_returns_settings_error_when_config_missing() {
    let cli = missing_config_cli(Command::List(ListArgs { tag: None }));
    assert!(run(&cli).await.is_err());
}

#[tokio::test]
async fn sync_returns_settings_error_when_config_missing() {
    let cli = missing_config_cli(Command::Sync(SyncArgs {
        tag: None,
        manifest: None,
        update: false,
    }));
    assert!(run(&cli).await.is_err());
}

#[tokio::test]
async fn remove_returns_settings_error_when_config_missing() {
    let cli = missing_config_cli(Command::Remove(RemoveArgs {
        tag: None,
        addons: vec!["WeakAuras".into()],
    }));
    assert!(run(&cli).await.is_err());
}
