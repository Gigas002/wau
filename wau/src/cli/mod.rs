//! Command-line surface — clap definitions only, no addon logic. Profiles are
//! configured by hand-editing TOML or via the interactive bootstrap built
//! into `init`.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[command(
    name = "wau",
    about = "A World of Warcraft addon manager",
    version,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Bypass the on-disk HTTP response cache for this invocation.
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Target profile (a configured WoW installation).
    #[arg(short, long, global = true, default_value = "default")]
    pub profile: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install addons.
    Install(InstallArgs),
    /// Sync installed addons (all, if none named).
    Sync(SyncArgs),
    /// Remove installed addons.
    Remove(RemoveArgs),
    /// Bootstrap an unconfigured profile interactively, then match untracked
    /// addon folders against the catalogue and import them.
    Init(InitArgs),
    /// Fuzzy-search the addon catalogue.
    Search(SearchArgs),
    /// List installed addons.
    List(ListArgs),
    /// Show detailed info for one installed addon (alias for `list -f detailed`).
    #[command(hide = true)]
    Info(InfoArgs),
    /// Manage the on-disk HTTP response cache.
    #[command(subcommand)]
    Cache(CacheCommand),
    /// Manage profiles (configured WoW installations).
    #[command(subcommand)]
    Profile(ProfileCommand),
    /// Print the active profile config, all profile names, and source metadata as JSON.
    Stats,
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// Clear the on-disk HTTP response cache.
    Clear,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    /// Delete the active profile's config and database.
    Erase,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// `source:alias` (or a source website URL) for each addon.
    #[arg(required = true)]
    pub addons: Vec<String>,
    /// Overwrite unreconciled on-disk folders that collide with the new install.
    #[arg(long)]
    pub replace: bool,
    /// Resolve and report without installing.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Addons to sync (default: everything installed).
    pub addons: Vec<String>,
    /// Resolve and report without installing.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    #[arg(required = true)]
    pub addons: Vec<String>,
    /// Delete the DB row but leave the on-disk folders in place.
    #[arg(long)]
    pub keep_folders: bool,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Pick the top match for every group without prompting.
    #[arg(short, long)]
    pub auto: bool,
    /// Only list what would be matched.
    #[arg(long)]
    pub list_unreconciled: bool,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    #[arg(required = true)]
    pub terms: Vec<String>,
    #[arg(short, long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=20))]
    pub limit: u32,
    /// Only include entries updated on/after this date (YYYY-MM-DD).
    #[arg(long)]
    pub start_date: Option<String>,
    /// Restrict to these sources (repeatable).
    #[arg(long = "source")]
    pub sources: Vec<String>,
    #[arg(long)]
    pub prefer_source: Option<String>,
    #[arg(long)]
    pub no_exclude_installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ListFormat {
    Simple,
    Detailed,
    Json,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Addons to list (default: everything installed).
    pub addons: Vec<String>,
    #[arg(short, long, value_enum, default_value_t = ListFormat::Simple)]
    pub format: ListFormat,
}

#[derive(Debug, Args)]
pub struct InfoArgs {
    pub addon: String,
}
