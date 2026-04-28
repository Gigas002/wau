use super::*;
use crate::cli::{Cli, Command, ListArgs};

// Integration-level tests require a real config file + addons directory on disk,
// so they live in libwau/tests/ once the full pipeline is wired (Phase 3+).
// These unit tests only cover error-path behaviour that can be exercised without I/O.

#[test]
fn run_returns_settings_error_when_config_missing() {
    let cli = Cli {
        config: Some(std::path::PathBuf::from("/nonexistent/config.toml")),
        command: Command::List(ListArgs { tag: None }),
    };
    let result = run(&cli);
    assert!(result.is_err());
}
