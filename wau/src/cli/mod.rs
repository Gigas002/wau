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

    /// Skip interactive confirmation prompts.
    #[arg(long, global = true)]
    pub noconfirm: bool,

    /// Suppress non-essential output.
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Increase output verbosity (sets log level to debug).
    #[arg(long, short = 'v', global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List installed addons for an install tag.
    List(ListArgs),
    /// Install or update addons from the manifest.
    Sync(SyncArgs),
    /// Remove installed addons recorded in the lock.
    Remove(RemoveArgs),
    /// Search installed addons and manifest entries.
    Search(SearchArgs),
    /// Show details for an addon from the manifest and lock.
    Info(InfoArgs),
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Install tag to use (default: config `defaults.install_tag`).
    #[arg(short, long, value_name = "TAG")]
    pub tag: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct SyncArgs {
    /// Install tag to use (default: config `defaults.install_tag`).
    #[arg(short, long, value_name = "TAG")]
    pub tag: Option<String>,

    /// Path to manifest file (default: `$XDG_CONFIG_HOME/wau/manifest.toml`).
    #[arg(short, long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,

    /// Re-install addons already present in the lock (apply updates).
    #[arg(long)]
    pub update: bool,

    /// Override the default flavor for this sync (e.g. retail, classic-era).
    #[arg(long, value_name = "FLAVOR")]
    pub flavor: Option<String>,

    /// Override the default channel for this sync (stable, beta, alpha).
    #[arg(long, value_name = "CHANNEL")]
    pub channel: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct RemoveArgs {
    /// Install tag to use (default: config `defaults.install_tag`).
    #[arg(short, long, value_name = "TAG")]
    pub tag: Option<String>,

    /// Names of addons to remove (as listed in the manifest/lock).
    #[arg(required = true, value_name = "ADDON")]
    pub addons: Vec<String>,
}

#[derive(Debug, clap::Args)]
pub struct SearchArgs {
    /// Search query (matches addon name, title, or folder).
    #[arg(value_name = "QUERY")]
    pub query: String,

    /// Install tag to use (default: config `defaults.install_tag`).
    #[arg(short, long, value_name = "TAG")]
    pub tag: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct InfoArgs {
    /// Addon name as listed in the manifest or lock.
    #[arg(value_name = "ADDON")]
    pub addon: String,

    /// Install tag to use (default: config `defaults.install_tag`).
    #[arg(short, long, value_name = "TAG")]
    pub tag: Option<String>,
}
