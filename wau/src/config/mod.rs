// Config is Phase 1 scaffolding; consumed by `settings` (Phase 5) rather than directly from main.
#![allow(dead_code)]

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use libwau::model::{Channel, Flavor, LogLevel, Tag};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema: u32,
    pub defaults: Defaults,
    #[serde(default)]
    pub logging: Logging,
    pub paths: Paths,
    #[serde(default)]
    pub providers: Providers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    pub flavor: Flavor,
    pub channel: Channel,
    pub install_tag: Tag,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default)]
    pub level: LogLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    pub cache: PathBuf,
    #[serde(default)]
    pub installs: Vec<Install>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Install {
    pub tag: Tag,
    pub flavor: Flavor,
    pub wow_root: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Providers {
    pub curseforge: Option<CurseForgeProvider>,
    pub github: Option<GitHubProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeProvider {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubProvider {
    /// Personal access token for higher API rate limits (optional).
    pub token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config not found at {path}")]
    NotFound { path: PathBuf },

    #[error("IO: {0}")]
    Io(#[from] io::Error),

    #[error("TOML deserialize: {0}")]
    TomlDeserialize(#[from] toml::de::Error),
}

/// Returns the default config path using XDG conventions.
/// On Linux: `$XDG_CONFIG_HOME/wau/config.toml` (fallback `~/.config/wau/config.toml`).
pub fn default_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".config")
        })
        .join("wau")
        .join("config.toml")
}

/// Returns `override_path` if provided, otherwise the XDG default.
pub fn resolved_path(override_path: Option<&Path>) -> PathBuf {
    override_path
        .map(Path::to_path_buf)
        .unwrap_or_else(default_path)
}

/// Loads and parses config from `path`, expanding `~` in path fields.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ConfigError::NotFound {
                path: path.to_path_buf(),
            }
        } else {
            ConfigError::Io(e)
        }
    })?;
    parse(&content)
}

/// Parses config from a TOML string and expands `~` in path fields.
pub fn parse(s: &str) -> Result<Config, ConfigError> {
    let mut config: Config = toml::from_str(s)?;
    config.expand_paths();
    Ok(config)
}

impl Config {
    /// Returns the `Interface/AddOns` path for `tag`, if that tag is configured.
    pub fn addons_path(&self, tag: &Tag) -> Option<PathBuf> {
        self.paths
            .installs
            .iter()
            .find(|i| &i.tag == tag)
            .map(|i| i.wow_root.join("Interface").join("AddOns"))
    }

    fn expand_paths(&mut self) {
        self.paths.cache = expand_tilde(self.paths.cache.clone());
        for install in &mut self.paths.installs {
            install.wow_root = expand_tilde(install.wow_root.clone());
        }
    }
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy().into_owned();
    if s == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    path
}
