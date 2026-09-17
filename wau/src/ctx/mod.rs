//! Assembles one profile's config, DB connection, HTTP client, and source
//! registry into a single struct threaded through command dispatch.

use std::path::Path;

use libwau::{
    config::{ConfigError, GlobalConfig, ProfileConfig},
    http::{HttpClient, HttpError},
    lockfile::{LockError, LockFile},
    pkg_archives::DownloadLocks,
    progress::ProgressBus,
    sources::{self, Resolver, SourceConfig},
};

use crate::cli::Cli;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum CtxError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Http(#[from] HttpError),
}

/// Everything a command needs to operate on one profile.
pub struct AppCtx {
    pub profile: ProfileConfig,
    pub lock: LockFile,
    pub http: HttpClient,
    pub sources: Vec<Box<dyn Resolver>>,
    pub download_locks: DownloadLocks,
    pub progress: ProgressBus,
}

/// Resolves `--profile`'s value: a bare name looked up by
/// [`ProfileConfig::read`] inside the config dir's `profiles/`, or — if it
/// looks like a filesystem path — read directly via
/// [`ProfileConfig::read_from_path`], bypassing name-based lookup entirely.
/// Lets integration tests point at a fixture profile file without needing
/// the platform config-dir layout.
pub fn read_profile(global: GlobalConfig, profile_arg: &str) -> Result<ProfileConfig, ConfigError> {
    if is_profile_path(profile_arg) {
        ProfileConfig::read_from_path(global, profile_arg)
    } else {
        ProfileConfig::read(global, profile_arg)
    }
}

/// Resolves `--profile` end to end: `Some` goes through [`read_profile`]
/// (name-or-path) as always; `None` — no implicit "default"-named profile —
/// auto-picks the sole configured profile via
/// [`ProfileConfig::read_sole`], erroring out (listing what's available) if
/// zero or several are configured instead of guessing.
pub fn resolve_profile(
    global: GlobalConfig,
    profile_arg: Option<&str>,
) -> Result<ProfileConfig, ConfigError> {
    match profile_arg {
        Some(arg) => read_profile(global, arg),
        None => ProfileConfig::read_sole(global),
    }
}

fn is_profile_path(value: &str) -> bool {
    value.ends_with(".toml")
        || value.contains('/')
        || value.contains(std::path::MAIN_SEPARATOR)
        || Path::new(value).is_absolute()
}

pub fn source_config(global: &GlobalConfig) -> SourceConfig {
    SourceConfig {
        cfcore_api_key: global.access_tokens.cfcore.clone(),
        cfcore_api_url: None,
        github_token: global.access_tokens.github.clone(),
        github_handler: global.github_handler,
        wago_addons_token: global.access_tokens.wago_addons.clone(),
    }
}

impl AppCtx {
    /// Reads the global config (from `cli.config`, if given, else the
    /// platform-conventional dir) and `cli.profile`'s config, then builds a
    /// full [`AppCtx`] from them. Returns [`CtxError::Config`] with
    /// [`ConfigError::NotFound`] if the profile isn't configured yet — every
    /// command surfaces that as an error pointing at the example configs;
    /// there is no interactive bootstrap.
    pub fn build(cli: &Cli) -> Result<Self, CtxError> {
        let global = GlobalConfig::read_from(cli.config.as_deref())?;
        let profile = resolve_profile(global, cli.profile.as_deref())?;
        Self::from_profile(profile)
    }

    /// Builds an [`AppCtx`] from an already-resolved [`ProfileConfig`] (e.g.
    /// one `init` is reconciling by name rather than via `cli.profile`).
    pub fn from_profile(profile: ProfileConfig) -> Result<Self, CtxError> {
        profile.ensure_dirs()?;
        let lock = LockFile::open(&profile.lock_file_path())?;
        let http = HttpClient::with_cache_dir(&profile.global_config.dirs.cache)?;
        let sources = sources::default_sources(&source_config(&profile.global_config));

        Ok(Self {
            profile,
            lock,
            http,
            sources,
            download_locks: DownloadLocks::new(),
            progress: ProgressBus::new(),
        })
    }
}
