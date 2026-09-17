//! Command-line surface — clap definitions only, no addon logic. Profiles are
//! configured entirely by hand-editing TOML; there is no interactive bootstrap.

use std::path::PathBuf;

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
    /// Path to `config.toml`, overriding the platform-conventional config
    /// dir (`profiles/` is resolved as its sibling directory). Mainly for
    /// integration tests that need an isolated config location.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Target profile: a name (looked up in the config dir's `profiles/`),
    /// or a path to a profile TOML file read directly — detected by
    /// containing a path separator or ending in `.toml`. If omitted, the
    /// sole configured profile is used automatically; with zero or several
    /// configured, commands that need exactly one error out listing what's
    /// available. There is no implicit "default"-named profile.
    #[arg(short, long, global = true)]
    pub profile: Option<String>,

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
    /// Switch an installed addon to a different source: resolves and
    /// installs the new one, then removes the old lock file entry, as one
    /// operation (rather than the folder-conflict-prone sequencing of doing
    /// it by hand via `remove` + `install`).
    Replace(ReplaceArgs),
    /// Reconcile a profile's untracked addon folders against the catalogue
    /// and import them, prompting per group to pick which source to use
    /// unless `--auto`. With `-p`/`--profile`, reconciles just that one;
    /// without it, reconciles every configured profile in turn — there is
    /// no interactive profile bootstrap, so profiles must already exist
    /// (hand-write them, see `examples/`).
    Init(InitArgs),
    /// Fuzzy-search the addon catalogue.
    Search(SearchArgs),
    /// List installed addons.
    List(ListArgs),
    /// Show detailed info for one installed addon (alias for `list -f detailed`).
    #[command(hide = true)]
    Info(InfoArgs),
    /// Show cache usage, or remove it all with `--clean`.
    Cache(CacheArgs),
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
    /// Delete the lock file entry but leave the on-disk folders in place.
    #[arg(long)]
    pub keep_folders: bool,
}

#[derive(Debug, Args)]
pub struct ReplaceArgs {
    /// The currently-installed `source:alias` (or URL) to replace.
    pub old: String,
    /// The `source:alias` (or URL) to replace it with.
    pub new: String,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Pick the top match for every group without prompting.
    #[arg(short, long)]
    pub auto: bool,
    /// Only list what would be matched, for every profile, without installing anything.
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
    /// Hide already-installed addons instead of showing them tagged `[Installed: <version>]`,
    /// like every other result (matching `paru`, which shows installed packages too).
    #[arg(long)]
    pub exclude_installed: bool,
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
pub struct CacheArgs {
    /// Remove all cached data (HTTP responses, catalogue mirror, download
    /// staging, `git` source checkouts/builds). Always safe: everything
    /// under the cache dir is re-derived (a fresh fetch, clone, or build)
    /// the next time it's needed.
    #[arg(long)]
    pub clean: bool,
}
