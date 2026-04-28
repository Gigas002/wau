use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[command(name = "wau", about = "WoW addon updater", version)]
pub struct Cli {
    /// Path to config file (default: $XDG_CONFIG_HOME/wau/config.toml).
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List installed addons for an install tag.
    List(ListArgs),
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Install tag to use (default: config `defaults.install_tag`).
    #[arg(short, long, value_name = "TAG")]
    pub tag: Option<String>,
}
