//! Global and per-profile configuration.
//!
//! Two independent TOML documents: a **global config** (`config.toml`:
//! logging, cache path, provider API keys) and one **profile config** per WoW
//! installation (`profiles/<name>.toml`, alongside its sibling
//! `profiles/<name>.sqlite`). No environment variables are ever read —
//! everything comes from these files or their built-in defaults; config/cache
//! base dirs are resolved via platform-conventional locations (the `dirs`
//! crate), never overridable.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::model::{
    Flavour, ProductInfo, extract_installation_dir_from_addon_dir, infer_product_from_addon_dir,
};

#[cfg(test)]
mod tests;

const APP_NAME: &str = "wau";

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config not found at {path}")]
    NotFound { path: PathBuf },

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("profile name must not be empty")]
    EmptyProfileName,

    #[error("'{}' is not a writable directory", .path.display())]
    AddonDirNotWritable { path: PathBuf },

    #[error("game flavour cannot be detected for '{}'; set flavour", .path.display())]
    NoFlavourDetected { path: PathBuf },
}

// ============================================================================
// Secrets
// ============================================================================

/// A config value that should never be printed in the clear. Serializes
/// (file I/O) as plain text — tokens are stored unencrypted on disk — but
/// `Debug`/`Display` always redact, so an accidental `{:?}`/`{}` elsewhere
/// in the app can't leak it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString(\"{}\")", REDACTED)
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

const REDACTED: &str = "**********";

// ============================================================================
// Log level
// ============================================================================

/// `[logging].level` in `config.toml` — the sole source of log verbosity (no
/// `-v` CLI flag, no `$RUST_LOG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    Error,
    #[default]
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for LogLevel {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown log level '{s}'")))
    }
}

// ============================================================================
// Dirs
// ============================================================================

/// The base locations `GlobalConfig` resolves eagerly: `config` (fixed,
/// platform-conventional, holds `config.toml` + `profiles/`) and `cache`
/// (platform-conventional default, overridable via `[paths].cache`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dirs {
    pub cache: PathBuf,
    pub config: PathBuf,
}

impl Dirs {
    fn default_dirs(cache_override: Option<PathBuf>) -> Self {
        Self {
            cache: cache_override.unwrap_or_else(cache_dir),
            config: config_dir(),
        }
    }

    fn ensure(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.cache)?;
        fs::create_dir_all(&self.config)?;
        Ok(())
    }
}

/// Platform-conventional config base (e.g. `~/.config/wau` on Linux),
/// resolved via the `dirs` crate — never overridable, never an env var read
/// in this crate's own code.
fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join(APP_NAME))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Platform-conventional cache base (e.g. `~/.cache/wau` on Linux); the
/// default when `[paths].cache` isn't set.
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join(APP_NAME))
        .unwrap_or_else(|| PathBuf::from("."))
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

fn is_writable_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let probe = path.join(format!(".wau-write-probe-{}", std::process::id()));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

// ============================================================================
// GlobalConfig
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccessTokens {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cfcore: Option<SecretString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<SecretString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wago_addons: Option<SecretString>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LoggingConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    level: Option<LogLevel>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PathsConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProviderConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<SecretString>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProvidersConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    curseforge: Option<ProviderConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wago: Option<ProviderConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    github: Option<ProviderConfigFile>,
}

/// On-disk shape of `config.toml` — [`Dirs`] are computed, never persisted.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GlobalConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logging: Option<LoggingConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paths: Option<PathsConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    providers: Option<ProvidersConfigFile>,
}

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub log_level: LogLevel,
    pub access_tokens: AccessTokens,
    pub dirs: Dirs,
}

impl GlobalConfig {
    /// Built-in defaults — no disk read.
    pub fn defaults() -> Self {
        Self::from_file(GlobalConfigFile::default())
    }

    fn from_file(file: GlobalConfigFile) -> Self {
        let log_level = file
            .logging
            .and_then(|l| l.level)
            .unwrap_or_default();

        let cache_override = file
            .paths
            .and_then(|p| p.cache)
            .map(expand_tilde);

        let providers = file.providers.unwrap_or_default();
        let access_tokens = AccessTokens {
            cfcore: providers.curseforge.and_then(|p| p.api_key),
            github: providers.github.and_then(|p| p.api_key),
            wago_addons: providers.wago.and_then(|p| p.api_key),
        };

        GlobalConfig {
            log_level,
            access_tokens,
            dirs: Dirs::default_dirs(cache_override),
        }
    }

    pub fn config_file_path(&self) -> PathBuf {
        self.dirs.config.join("config.toml")
    }

    /// Reads `config.toml` (built-in defaults if absent).
    pub fn read() -> Result<Self, ConfigError> {
        let path = config_dir().join("config.toml");

        let file = match fs::read_to_string(&path) {
            Ok(s) => toml::from_str::<GlobalConfigFile>(&s)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => GlobalConfigFile::default(),
            Err(e) => return Err(e.into()),
        };

        Ok(Self::from_file(file))
    }

    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        self.dirs.ensure()?;
        Ok(())
    }

    pub fn write(&self) -> Result<(), ConfigError> {
        self.ensure_dirs()?;
        let file = GlobalConfigFile {
            logging: Some(LoggingConfigFile {
                level: Some(self.log_level),
            }),
            paths: Some(PathsConfigFile {
                cache: Some(self.dirs.cache.clone()),
            }),
            providers: Some(ProvidersConfigFile {
                curseforge: self.access_tokens.cfcore.clone().map(|api_key| {
                    ProviderConfigFile {
                        api_key: Some(api_key),
                    }
                }),
                github: self.access_tokens.github.clone().map(|api_key| {
                    ProviderConfigFile {
                        api_key: Some(api_key),
                    }
                }),
                wago: self.access_tokens.wago_addons.clone().map(|api_key| {
                    ProviderConfigFile {
                        api_key: Some(api_key),
                    }
                }),
            }),
        };
        fs::write(self.config_file_path(), toml::to_string_pretty(&file)?)?;
        Ok(())
    }
}

// ============================================================================
// ProfileConfig
// ============================================================================

/// The WoW client a profile targets: either auto-detected from `addon_dir`'s
/// install-directory subfolder name, or forced via `flavour_override`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstalledProduct {
    Known(&'static ProductInfo),
    Overridden(Flavour),
}

impl InstalledProduct {
    pub fn flavour(self) -> Flavour {
        match self {
            Self::Known(p) => p.flavour,
            Self::Overridden(f) => f,
        }
    }
}

/// On-disk shape of `profiles/<name>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileConfigFile {
    profile: String,
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    flavour: Option<Flavour>,
}

/// One WoW installation's config: which profile, which `Interface/AddOns`
/// directory, and (implicitly) its own `db.sqlite` (see [`ProfileConfig::db_file_path`]).
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub global_config: GlobalConfig,
    pub profile: String,
    pub addon_dir: PathBuf,
    pub flavour_override: Option<Flavour>,
    pub product: InstalledProduct,
}

fn profiles_dir_path(global_config: &GlobalConfig) -> PathBuf {
    global_config.dirs.config.join("profiles")
}

fn profile_config_file_path(global_config: &GlobalConfig, profile: &str) -> PathBuf {
    profiles_dir_path(global_config).join(format!("{profile}.toml"))
}

fn profile_db_file_path(global_config: &GlobalConfig, profile: &str) -> PathBuf {
    profiles_dir_path(global_config).join(format!("{profile}.sqlite"))
}

impl ProfileConfig {
    pub fn new(
        global_config: GlobalConfig,
        profile: impl Into<String>,
        addon_dir: impl Into<PathBuf>,
        flavour_override: Option<Flavour>,
    ) -> Result<Self, ConfigError> {
        let profile = profile.into().trim().to_owned();
        if profile.is_empty() {
            return Err(ConfigError::EmptyProfileName);
        }

        let addon_dir = expand_tilde(addon_dir.into());
        if !is_writable_dir(&addon_dir) {
            return Err(ConfigError::AddonDirNotWritable { path: addon_dir });
        }

        let product = match flavour_override {
            Some(f) => InstalledProduct::Overridden(f),
            None => {
                let info = infer_product_from_addon_dir(&addon_dir).ok_or_else(|| {
                    ConfigError::NoFlavourDetected {
                        path: addon_dir.clone(),
                    }
                })?;
                InstalledProduct::Known(info)
            }
        };

        Ok(ProfileConfig {
            global_config,
            profile,
            addon_dir,
            flavour_override,
            product,
        })
    }

    pub fn read(global_config: GlobalConfig, profile: &str) -> Result<Self, ConfigError> {
        let path = profile_config_file_path(&global_config, profile);
        let s = fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound { path: path.clone() }
            } else {
                ConfigError::Io(e)
            }
        })?;
        let file: ProfileConfigFile = toml::from_str(&s)?;
        Self::new(global_config, file.profile, file.path, file.flavour)
    }

    /// Every configured profile name (`profiles/*.toml`), sorted.
    pub fn iter_profiles(global_config: &GlobalConfig) -> Vec<String> {
        let Ok(entries) = fs::read_dir(profiles_dir_path(global_config)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_owned)
            })
            .collect();
        names.sort();
        names
    }

    /// Installation directories already covered by a configured profile
    /// (best-effort: reads just the raw `path` value from each profile's
    /// TOML, unexpanded).
    pub fn iter_profile_installations(global_config: &GlobalConfig) -> Vec<PathBuf> {
        Self::iter_profiles(global_config)
            .into_iter()
            .filter_map(|name| {
                let path = profile_config_file_path(global_config, &name);
                let s = fs::read_to_string(&path).ok()?;
                let file: ProfileConfigFile = toml::from_str(&s).ok()?;
                extract_installation_dir_from_addon_dir(&file.path)
            })
            .collect()
    }

    pub fn config_file_path(&self) -> PathBuf {
        profile_config_file_path(&self.global_config, &self.profile)
    }

    pub fn db_file_path(&self) -> PathBuf {
        profile_db_file_path(&self.global_config, &self.profile)
    }

    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        fs::create_dir_all(profiles_dir_path(&self.global_config))?;
        Ok(())
    }

    pub fn write(&self) -> Result<(), ConfigError> {
        self.ensure_dirs()?;
        let file = ProfileConfigFile {
            profile: self.profile.clone(),
            path: self.addon_dir.clone(),
            flavour: self.flavour_override,
        };
        fs::write(self.config_file_path(), toml::to_string_pretty(&file)?)?;
        Ok(())
    }

    /// Trashes the profile's config file and its sibling `db.sqlite`, if any.
    pub fn delete(&self) -> Result<(), ConfigError> {
        crate::fs::trash(&self.config_file_path())?;
        let db_path = self.db_file_path();
        if db_path.exists() {
            crate::fs::trash(&db_path)?;
        }
        Ok(())
    }
}
