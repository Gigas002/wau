mod app;
mod cli;
mod config;
mod logger;
mod output;
mod settings;

use clap::Parser;
use settings::Settings;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();

    let settings = match Settings::resolve(&cli) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    logger::init(settings.verbose, settings.quiet, &settings.log_level);

    if let Err(e) = app::run(settings).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
