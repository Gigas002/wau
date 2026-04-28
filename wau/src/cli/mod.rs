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
    /// Install or update addons from the manifest.
    Sync(SyncArgs),
    /// Remove installed addons recorded in the lock.
    Remove(RemoveArgs),
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
