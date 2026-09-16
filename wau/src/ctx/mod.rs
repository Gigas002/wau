//! Assembles one profile's config, DB connection, HTTP client, and source
//! registry into a single struct threaded through command dispatch.

use std::path::{Path, PathBuf};

use libwau::{
    config::{ConfigError, GlobalConfig, ProfileConfig},
    http::{HttpClient, HttpError},
    lockfile::{LockError, LockFile},
    model::Flavour,
    pkg_archives::DownloadLocks,
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

fn is_profile_path(value: &str) -> bool {
    value.ends_with(".toml")
        || value.contains('/')
        || value.contains(std::path::MAIN_SEPARATOR)
        || Path::new(value).is_absolute()
}

/// Builds a brand-new [`ProfileConfig`] for `profile_arg` (same name-or-path
/// detection as [`read_profile`]), used when bootstrapping an unconfigured
/// profile. When `profile_arg` is a path, the profile's internal name is its
/// file stem and the config/DB are written at that exact path rather than
/// under `profiles/`.
pub fn new_profile(
    global: GlobalConfig,
    profile_arg: &str,
    addon_dir: impl Into<PathBuf>,
    flavour_override: Option<Flavour>,
) -> Result<ProfileConfig, ConfigError> {
    if is_profile_path(profile_arg) {
        let path = PathBuf::from(profile_arg);
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(profile_arg);
        let profile = ProfileConfig::new(global, name, addon_dir, flavour_override)?;
        Ok(profile.with_path_override(path))
    } else {
        ProfileConfig::new(global, profile_arg, addon_dir, flavour_override)
    }
}

pub fn source_config(global: &GlobalConfig) -> SourceConfig {
    SourceConfig {
        cfcore_api_key: global.access_tokens.cfcore.clone(),
        cfcore_api_url: None,
        github_token: global.access_tokens.github.clone(),
        wago_addons_token: global.access_tokens.wago_addons.clone(),
    }
}

impl AppCtx {
    /// Reads the global config (from `cli.config`, if given, else the
    /// platform-conventional dir) and `cli.profile`'s config, then builds a
    /// full [`AppCtx`] from them. Returns [`CtxError::Config`] with
    /// [`ConfigError::NotFound`] if the profile isn't configured yet — the
    /// caller (`app::run`) is expected to catch that and bootstrap the
    /// profile interactively before retrying.
    pub fn build(cli: &Cli) -> Result<Self, CtxError> {
        let global = GlobalConfig::read_from(cli.config.as_deref())?;
        let profile = read_profile(global, &cli.profile)?;
        Self::from_profile(profile)
    }

    /// Builds an [`AppCtx`] from an already-resolved [`ProfileConfig`] (e.g.
    /// one just created by an interactive `configure` prompt).
    pub fn from_profile(profile: ProfileConfig) -> Result<Self, CtxError> {
        profile.ensure_dirs()?;
        let lock = LockFile::open(&profile.lock_file_path())?;
        let http = HttpClient::new()?;
        let sources = sources::default_sources(&source_config(&profile.global_config));

        Ok(Self {
            profile,
            lock,
            http,
            sources,
            download_locks: DownloadLocks::new(),
        })
    }
}
