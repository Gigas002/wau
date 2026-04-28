//! Resolved runtime settings — merges CLI flags over config.
//!
//! All downstream code (app, output) receives only `Settings` variants; it must
//! not reach back into `cli` or `config` directly.

use std::path::PathBuf;

use libwau::model::{Channel, Flavor, Tag};

use crate::{
    cli::{Cli, Command},
    config::{self, ConfigError},
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("{0}")]
    Config(#[from] ConfigError),

    #[error("install tag '{tag}' not found in config; check the [paths.installs] section")]
    TagNotFound { tag: String },
}

// ---------------------------------------------------------------------------
// Per-command settings structs
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

impl ListSettings {
    pub fn for_list(cli: &Cli) -> Result<Self, SettingsError> {
        let config_path = config::resolved_path(cli.config.as_deref());
        let config = config::load(&config_path)?;

        let tag = if let Command::List(args) = &cli.command
            && let Some(t) = &args.tag
        {
            Tag::new(t)
        } else {
            config.defaults.install_tag.clone()
        };

        let addons_path = config
            .addons_path(&tag)
            .ok_or_else(|| SettingsError::TagNotFound {
                tag: tag.to_string(),
            })?;

        Ok(ListSettings { tag, addons_path })
    }
}

impl SyncSettings {
    pub fn for_sync(cli: &Cli) -> Result<Self, SettingsError> {
        let config_path = config::resolved_path(cli.config.as_deref());
        let config = config::load(&config_path)?;

        let (tag, manifest_override, update) = if let Command::Sync(args) = &cli.command {
            let tag = args
                .tag
                .as_deref()
                .map(Tag::new)
                .unwrap_or_else(|| config.defaults.install_tag.clone());
            (tag, args.manifest.clone(), args.update)
        } else {
            (config.defaults.install_tag.clone(), None, false)
        };

        let addons_path = config
            .addons_path(&tag)
            .ok_or_else(|| SettingsError::TagNotFound {
                tag: tag.to_string(),
            })?;

        let config_dir = config_path.parent().unwrap_or(&config_path).to_path_buf();
        let manifest_path = manifest_override.unwrap_or_else(|| config_dir.join("manifest.toml"));
        let lock_path = config_dir.join(format!("{}.lock.toml", tag.as_str()));

        Ok(SyncSettings {
            tag,
            flavor: config.defaults.flavor,
            channel: config.defaults.channel,
            addons_path,
            cache_dir: config.paths.cache,
            manifest_path,
            lock_path,
            update,
        })
    }
}

impl RemoveSettings {
    pub fn for_remove(cli: &Cli) -> Result<Self, SettingsError> {
        let config_path = config::resolved_path(cli.config.as_deref());
        let config = config::load(&config_path)?;

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

        let addons_path = config
            .addons_path(&tag)
            .ok_or_else(|| SettingsError::TagNotFound {
                tag: tag.to_string(),
            })?;

        let config_dir = config_path.parent().unwrap_or(&config_path).to_path_buf();
        let lock_path = config_dir.join(format!("{}.lock.toml", tag.as_str()));

        Ok(RemoveSettings {
            tag,
            flavor: config.defaults.flavor,
            addons_path,
            lock_path,
            addons,
        })
    }
}
