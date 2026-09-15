//! Assembles one profile's config, DB connection, HTTP client, and source
//! registry into a single struct threaded through command dispatch.

use libwau::{
    config::{ConfigError, GlobalConfig, ProfileConfig},
    db,
    http::{HttpClient, HttpError},
    pkg_archives::DownloadLocks,
    sources::{self, Resolver, SourceConfig},
};
use rusqlite::Connection;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum CtxError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Db(#[from] db::DbError),
    #[error(transparent)]
    Http(#[from] HttpError),
}

/// Everything a command needs to operate on one profile.
pub struct AppCtx {
    pub profile: ProfileConfig,
    pub conn: Connection,
    pub http: HttpClient,
    pub sources: Vec<Box<dyn Resolver>>,
    pub download_locks: DownloadLocks,
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
    /// Reads the global config and the named profile's config, then builds a
    /// full [`AppCtx`] from them. Returns [`CtxError::Config`] with
    /// [`ConfigError::NotFound`] if the profile isn't configured yet — the
    /// caller (`app::run`) is expected to catch that and bootstrap the
    /// profile interactively before retrying.
    pub fn build(profile_name: &str, no_cache: bool) -> Result<Self, CtxError> {
        let global = GlobalConfig::read()?;
        let profile = ProfileConfig::read(global, profile_name)?;
        Self::from_profile(profile, no_cache)
    }

    /// Builds an [`AppCtx`] from an already-resolved [`ProfileConfig`] (e.g.
    /// one just created by an interactive `configure` prompt).
    pub fn from_profile(profile: ProfileConfig, no_cache: bool) -> Result<Self, CtxError> {
        profile.ensure_dirs()?;
        let conn = db::prepare_database(&profile.db_file_path())?;
        let cache_dir = (!no_cache).then_some(profile.global_config.dirs.cache.as_path());
        let http = HttpClient::new(cache_dir)?;
        let sources = sources::default_sources(&source_config(&profile.global_config));

        Ok(Self {
            profile,
            conn,
            http,
            sources,
            download_locks: DownloadLocks::new(),
        })
    }
}
