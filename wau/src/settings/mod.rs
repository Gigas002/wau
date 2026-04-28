//! Resolved runtime settings — merges CLI flags over config.
//!
//! All downstream code (app, output) receives only `Settings`; it must not reach
//! back into `cli` or `config` directly.

use std::path::PathBuf;

use libwau::model::Tag;

use crate::{
    cli::{Cli, Command},
    config::{self, ConfigError},
};

#[cfg(test)]
mod tests;

/// Fully-resolved runtime settings for a single command invocation.
#[derive(Debug)]
pub struct Settings {
    pub tag: Tag,
    pub addons_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("{0}")]
    Config(#[from] ConfigError),

    #[error("install tag '{tag}' not found in config; check the [paths.installs] section")]
    TagNotFound { tag: String },
}

impl Settings {
    /// Resolves `Settings` for `wau list`.
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

        Ok(Settings { tag, addons_path })
    }
}
