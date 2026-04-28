mod app;
mod cli;
mod config;
mod output;
mod settings;

use clap::Parser;

fn main() {
    tracing_subscriber::fmt::init();

    let cli = cli::Cli::parse();

    if let Err(e) = app::run(&cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
