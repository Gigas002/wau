//! Tracing subscriber initialisation. Priority (highest wins):
//!   1. `RUST_LOG` environment variable
//!   2. `--verbose` CLI flag → `debug`
//!   3. `--quiet` CLI flag → `warn`
//!   4. `[logging].level` from config (always present after `Settings::resolve`)

use libwau::model::LogLevel;
use tracing_subscriber::EnvFilter;

#[cfg(test)]
mod tests;

/// Returns the tracing level string given CLI verbosity flags and the config level.
pub fn level_str(verbose: bool, quiet: bool, config_level: &LogLevel) -> &'static str {
    if verbose {
        "debug"
    } else if quiet {
        "warn"
    } else {
        config_level.as_str()
    }
}

/// Initialises the global tracing subscriber. Safe to call multiple times
/// (subsequent calls are silently ignored if a subscriber is already set).
pub fn init(verbose: bool, quiet: bool, config_level: &LogLevel) {
    let level = level_str(verbose, quiet, config_level);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
