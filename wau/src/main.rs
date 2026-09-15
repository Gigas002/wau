mod app;
mod cli;
mod ctx;
mod output;
mod prompts;

use clap::Parser;
use libwau::config::LogLevel;

/// Maps [`LogLevel`] (`config.toml`'s `[logging].level` — the only source of
/// verbosity; no `-v` flag, no `$RUST_LOG`) onto `tracing`'s level type.
fn to_tracing_level(level: LogLevel) -> tracing::Level {
    match level {
        LogLevel::Error => tracing::Level::ERROR,
        LogLevel::Warn => tracing::Level::WARN,
        LogLevel::Info => tracing::Level::INFO,
        LogLevel::Debug => tracing::Level::DEBUG,
        LogLevel::Trace => tracing::Level::TRACE,
    }
}

fn init_logging(level: LogLevel) {
    tracing_subscriber::fmt()
        .with_max_level(to_tracing_level(level))
        .with_writer(std::io::stderr)
        .init();
}

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();
    let log_level = libwau::config::GlobalConfig::read()
        .map(|g| g.log_level)
        .unwrap_or_default();
    init_logging(log_level);

    match app::run(&cli).await {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
