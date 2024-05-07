pub mod cli;
pub mod config;

use clap::Parser;
use cli::Cli;
use config::Config;
use libwau::providers::Provider;

#[tokio::main]
async fn main() {
    // cli args
    let cli = Cli::parse();

    // config path
    let config_path = cli
        .config
        .unwrap_or(Config::get_default_path().unwrap_or_default());

    // config
    let config = Config::load(&config_path).unwrap_or_default();
    let access_token = config.token_curse.unwrap();

    let curse = Provider::Curse(access_token);

    // search operation
    let addons = curse.search("bagnon").await;
    let addons = addons.unwrap();

    for addon in addons {
        println!("{}", &addon.name);
    }
}
