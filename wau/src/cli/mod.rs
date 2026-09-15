//! Command-line surface — clap definitions only, no addon logic. Mirrors
//! instawow's `cli/__init__.py` command set (minus the GUI/weakauras/plugins
//! commands, out of scope for this port — see `docs/WAU_RS_PLAN.md`).

use clap::{Args, Parser, Subcommand, ValueEnum};

#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[command(
    name = "wau",
    about = "A 1:1 instawow-parity WoW addon manager",
    version
)]
pub struct Cli {
    /// Increase log verbosity (repeatable: -v info, -vv debug, -vvv trace).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Bypass the on-disk HTTP response cache for this invocation.
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Target profile (a configured WoW installation).
    #[arg(short, long, global = true, default_value = "__default__")]
    pub profile: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install addons.
    Install(InstallArgs),
    /// Update installed addons (all, if none named).
    Update(UpdateArgs),
    /// Remove installed addons.
    Remove(RemoveArgs),
    /// Reinstall an addon at a previous version from its install history.
    Rollback(RollbackArgs),
    /// Match untracked addon folders against the catalogue and import them.
    Reconcile(ReconcileArgs),
    /// Switch installed addons to an equivalent package on another source.
    Rereconcile(RereconcileArgs),
    /// Fuzzy-search the addon catalogue.
    Search(SearchArgs),
    /// List installed addons.
    List(ListArgs),
    /// Show detailed info for one installed addon (alias for `list -f detailed`).
    #[command(hide = true)]
    Info(InfoArgs),
    /// Open an addon's installed folder in the OS file manager.
    Reveal(RevealArgs),
    /// Show changelogs for installed addons.
    ViewChangelog(ViewChangelogArgs),
    /// Manage the on-disk HTTP response cache.
    #[command(subcommand)]
    Cache(CacheCommand),
    /// Manage profiles (configured WoW installations).
    #[command(subcommand)]
    Profile(ProfileCommand),
    /// Configure the active profile (alias for `profile configure`).
    Configure(ConfigureArgs),
    /// Print the active profile config, all profile names, and source metadata as JSON.
    Debug,
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// Clear the on-disk HTTP response cache.
    Clear,
}

#[derive(Debug, Subcommand)]
pub enum ProfileCommand {
    /// Interactively (or via `key=value` pairs) configure the active profile.
    Configure(ConfigureArgs),
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
pub struct UpdateArgs {
    /// Addons to update (default: everything installed).
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
pub struct RollbackArgs {
    pub addon: String,
    /// Clear the pin and reinstall at the default strategy.
    #[arg(long)]
    pub undo: bool,
}

#[derive(Debug, Args)]
pub struct ReconcileArgs {
    /// Pick the top match for every group without prompting.
    #[arg(short, long)]
    pub auto: bool,
    /// Only list what would be matched.
    #[arg(long)]
    pub list_unreconciled: bool,
}

#[derive(Debug, Args)]
pub struct RereconcileArgs {
    /// Addons to re-reconcile (default: everything installed).
    pub addons: Vec<String>,
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

#[derive(Debug, Args)]
pub struct RevealArgs {
    pub addon: String,
}

#[derive(Debug, Args)]
pub struct ViewChangelogArgs {
    /// Addons to show changelogs for (default: everything installed).
    pub addons: Vec<String>,
    /// Fetch the latest changelog from the source instead of the installed one.
    #[arg(long)]
    pub remote: bool,
}

#[derive(Debug, Args)]
pub struct ConfigureArgs {
    /// `key=value` pairs (addon_dir, flavour_override, auto_update_check, github_token,
    /// cfcore_api_key, wago_addons_token). With none given, prompts interactively.
    pub options: Vec<String>,
}
