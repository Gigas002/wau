use libwau::model::LogLevel;

use super::level_str;

#[test]
fn verbose_wins_over_quiet_and_config() {
    assert_eq!(level_str(true, true, &LogLevel::Error), "debug");
}

#[test]
fn quiet_wins_over_config() {
    assert_eq!(level_str(false, true, &LogLevel::Debug), "warn");
}

#[test]
fn config_level_used_when_no_flags() {
    assert_eq!(level_str(false, false, &LogLevel::Error), "error");
    assert_eq!(level_str(false, false, &LogLevel::Trace), "trace");
}

#[test]
fn default_config_level_info() {
    assert_eq!(level_str(false, false, &LogLevel::Info), "info");
}
