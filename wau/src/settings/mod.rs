//! Resolved runtime settings — merges CLI flags over config.
//!
//! `Settings::resolve` is the single entry point: it loads config exactly once,
//! dispatches to the appropriate command builder, and returns a `Settings` value
//! that `main` passes to both `logger::init` and `app::run`.
//!
//! All downstream code must not reach back into `cli` or `config` directly.

use std::path::{Path, PathBuf};

use libwau::model::{Channel, Flavor, LogLevel, Tag};

use crate::{
    cli::{Cli, Command},
    config::{self, Config, ConfigError},
};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("{0}")]
    Config(#[from] ConfigError),

    #[error("install tag '{tag}' not found in config; check the [paths.installs] section")]
    TagNotFound { tag: String },

    #[error(
        "unknown flavor '{flavor}'; valid values: retail, classic-era, classic-tbc, classic-wrath, classic-cata, classic-mop, classic-wod, classic-legion, classic-bfa, classic-shadowlands, classic-dragonflight, classic-tww"
    )]
    UnknownFlavor { flavor: String },

    #[error("unknown channel '{channel}'; valid values: stable, beta, alpha")]
    UnknownChannel { channel: String },
}

// ---------------------------------------------------------------------------
// Top-level resolved settings
// ---------------------------------------------------------------------------

/// All resolved settings for the current invocation.
///
/// `main` calls `Settings::resolve` once; the result is passed to both
/// `logger::init` and `app::run`.
#[derive(Debug)]
pub struct Settings {
    pub verbose: bool,
    pub quiet: bool,
    pub noconfirm: bool,
    pub log_level: LogLevel,
    pub command: CommandSettings,
}

impl Settings {
    /// Loads config exactly once and returns the fully resolved settings.
    pub fn resolve(cli: &Cli) -> Result<Self, SettingsError> {
        let config_path = config::resolved_path(cli.config.as_deref());
        let config = config::load(&config_path)?;
        let config_dir = config_path.parent().unwrap_or(&config_path).to_path_buf();
        let log_level = config.logging.level.clone();

        let command = match &cli.command {
            Command::List(_) => CommandSettings::List(build_list(cli, &config)?),
            Command::Sync(_) => CommandSettings::Sync(build_sync(cli, &config, &config_dir)?),
            Command::Remove(_) => CommandSettings::Remove(build_remove(cli, &config, &config_dir)?),
            Command::Search(_) => CommandSettings::Search(build_search(cli, &config, &config_dir)?),
            Command::Info(_) => CommandSettings::Info(build_info(cli, &config, &config_dir)?),
        };

        Ok(Settings {
            verbose: cli.verbose,
            quiet: cli.quiet,
            noconfirm: cli.noconfirm,
            log_level,
            command,
        })
    }
}

// ---------------------------------------------------------------------------
// Command-specific settings
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum CommandSettings {
    List(ListSettings),
    Sync(SyncSettings),
    Remove(RemoveSettings),
    Search(SearchSettings),
    Info(InfoSettings),
}

/// Resolved settings for `wau list`.
#[derive(Debug)]
pub struct ListSettings {
    pub tag: Tag,
    pub addons_path: PathBuf,
}

/// Resolved settings for `wau sync`.
#[derive(Debug)]
pub struct SyncSettings {
    pub tag: Tag,
    pub flavor: Flavor,
    pub channel: Channel,
    pub addons_path: PathBuf,
    pub cache_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub lock_path: PathBuf,
    pub update: bool,
    pub provider_config: libwau::providers::ProviderConfig,
}

/// Resolved settings for `wau remove`.
#[derive(Debug)]
pub struct RemoveSettings {
    pub tag: Tag,
    pub flavor: Flavor,
    pub addons_path: PathBuf,
    pub lock_path: PathBuf,
    pub addons: Vec<String>,
}

/// Resolved settings for `wau search`.
#[derive(Debug)]
pub struct SearchSettings {
    pub query: String,
    pub tag: Tag,
    pub addons_path: PathBuf,
    pub manifest_path: PathBuf,
}

/// Resolved settings for `wau info`.
#[derive(Debug)]
pub struct InfoSettings {
    pub addon_name: String,
    pub tag: Tag,
    pub manifest_path: PathBuf,
    pub lock_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Private builders (one per command; all receive the already-loaded config)
// ---------------------------------------------------------------------------

fn build_list(cli: &Cli, config: &Config) -> Result<ListSettings, SettingsError> {
    let tag = if let Command::List(args) = &cli.command
        && let Some(t) = &args.tag
    {
        Tag::new(t)
    } else {
        config.defaults.install_tag.clone()
    };

    let addons_path = addons_path(config, &tag)?;
    Ok(ListSettings { tag, addons_path })
}

fn build_sync(
    cli: &Cli,
    config: &Config,
    config_dir: &Path,
) -> Result<SyncSettings, SettingsError> {
    let (tag, manifest_override, update, flavor_override, channel_override) =
        if let Command::Sync(args) = &cli.command {
            let tag = args
                .tag
                .as_deref()
                .map(Tag::new)
                .unwrap_or_else(|| config.defaults.install_tag.clone());
            (
                tag,
                args.manifest.clone(),
                args.update,
                args.flavor.clone(),
                args.channel.clone(),
            )
        } else {
            (config.defaults.install_tag.clone(), None, false, None, None)
        };

    let addons_path = addons_path(config, &tag)?;
    let manifest_path = manifest_override.unwrap_or_else(|| config_dir.join("manifest.toml"));
    let lock_path = lock_path(config_dir, &tag);

    let flavor = match flavor_override {
        Some(s) => s
            .parse::<Flavor>()
            .map_err(|_| SettingsError::UnknownFlavor { flavor: s })?,
        None => config.defaults.flavor.clone(),
    };
    let channel = match channel_override {
        Some(s) => s
            .parse::<Channel>()
            .map_err(|_| SettingsError::UnknownChannel { channel: s })?,
        None => config.defaults.channel.clone(),
    };

    let provider_config = libwau::providers::ProviderConfig {
        curseforge_api_key: config
            .providers
            .curseforge
            .as_ref()
            .map(|c| c.api_key.clone()),
        github_token: config
            .providers
            .github
            .as_ref()
            .and_then(|g| g.token.clone()),
    };

    Ok(SyncSettings {
        tag,
        flavor,
        channel,
        addons_path,
        cache_dir: config.paths.cache.clone(),
        manifest_path,
        lock_path,
        update,
        provider_config,
    })
}

fn build_remove(
    cli: &Cli,
    config: &Config,
    config_dir: &Path,
) -> Result<RemoveSettings, SettingsError> {
    let (tag, addons) = if let Command::Remove(args) = &cli.command {
        let tag = args
            .tag
            .as_deref()
            .map(Tag::new)
            .unwrap_or_else(|| config.defaults.install_tag.clone());
        (tag, args.addons.clone())
    } else {
        (config.defaults.install_tag.clone(), Vec::new())
    };

    let addons_path = addons_path(config, &tag)?;
    let lock_path = lock_path(config_dir, &tag);

    Ok(RemoveSettings {
        tag,
        flavor: config.defaults.flavor.clone(),
        addons_path,
        lock_path,
        addons,
    })
}

fn build_search(
    cli: &Cli,
    config: &Config,
    config_dir: &Path,
) -> Result<SearchSettings, SettingsError> {
    let (tag, query) = if let Command::Search(args) = &cli.command {
        let tag = args
            .tag
            .as_deref()
            .map(Tag::new)
            .unwrap_or_else(|| config.defaults.install_tag.clone());
        (tag, args.query.clone())
    } else {
        (config.defaults.install_tag.clone(), String::new())
    };

    let addons_path = addons_path(config, &tag)?;
    let manifest_path = config_dir.join("manifest.toml");

    Ok(SearchSettings {
        query,
        tag,
        addons_path,
        manifest_path,
    })
}

fn build_info(
    cli: &Cli,
    config: &Config,
    config_dir: &Path,
) -> Result<InfoSettings, SettingsError> {
    let (tag, addon_name) = if let Command::Info(args) = &cli.command {
        let tag = args
            .tag
            .as_deref()
            .map(Tag::new)
            .unwrap_or_else(|| config.defaults.install_tag.clone());
        (tag, args.addon.clone())
    } else {
        (config.defaults.install_tag.clone(), String::new())
    };

    // Info doesn't need addons_path, but we still validate the tag.
    if config.addons_path(&tag).is_none() {
        return Err(SettingsError::TagNotFound {
            tag: tag.to_string(),
        });
    }

    Ok(InfoSettings {
        addon_name,
        tag: tag.clone(),
        manifest_path: config_dir.join("manifest.toml"),
        lock_path: lock_path(config_dir, &tag),
    })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn addons_path(config: &Config, tag: &Tag) -> Result<PathBuf, SettingsError> {
    config
        .addons_path(tag)
        .ok_or_else(|| SettingsError::TagNotFound {
            tag: tag.to_string(),
        })
}

fn lock_path(config_dir: &Path, tag: &Tag) -> PathBuf {
    config_dir.join(format!("{}.lock.toml", tag.as_str()))
}
