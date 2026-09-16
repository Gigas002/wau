//! Global and per-profile configuration.
//!
//! Two independent TOML documents: a **global config** (`config.toml`:
//! logging, cache path, provider API keys) and one **profile config** per WoW
//! installation (`profiles/<name>/profile.toml`, alongside its sibling
//! `profiles/<name>/lock.toml` — the installed-package lock file, see
//! [`crate::lockfile`]). No environment variables are ever read —
//! everything comes from these files or their built-in defaults; config/cache
//! base dirs are resolved via platform-conventional locations (the `dirs`
//! crate) by default. Both can be pointed at an explicit path instead
//! ([`GlobalConfig::read_from`], [`ProfileConfig::read_from_path`]) — the
//! CLI's `--config`/`--profile` flags use this, mainly so integration tests
//! can run against an isolated fixture location rather than the real
//! platform config dir.

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

    #[error("no profile configured")]
    NoProfilesConfigured,

    #[error("multiple profiles configured: {}", .available.join(", "))]
    AmbiguousProfile { available: Vec<String> },
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
/// resolved via the `dirs` crate; never an env var read in this crate's own
/// code. [`GlobalConfig::read_from`] can point at a different config file
/// entirely, in which case this default is never consulted.
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

/// How the `github` source makes its requests: either it throws them
/// directly (optionally bearing [`AccessTokens::github`] as a bearer
/// token, same as every other provider), or it shells out to the system
/// `gh` CLI, which carries its own auth (`gh auth login`) instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitHubHandler {
    #[default]
    Token,
    Gh,
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

/// `[providers.github]` — same `api_key` every provider has, plus
/// `handler` to opt into the `gh`-CLI request path instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GitHubProviderConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<SecretString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handler: Option<GitHubHandler>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProvidersConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    curseforge: Option<ProviderConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wago: Option<ProviderConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    github: Option<GitHubProviderConfigFile>,
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
    pub github_handler: GitHubHandler,
    pub dirs: Dirs,
    /// Set only by [`Self::read_from`] with an explicit path; makes
    /// [`Self::config_file_path`] return that exact path instead of
    /// reconstructing it from `dirs.config`.
    config_path_override: Option<PathBuf>,
}

impl GlobalConfig {
    /// Built-in defaults — no disk read.
    pub fn defaults() -> Self {
        Self::from_file(GlobalConfigFile::default())
    }

    fn from_file(file: GlobalConfigFile) -> Self {
        let log_level = file.logging.and_then(|l| l.level).unwrap_or_default();

        let cache_override = file.paths.and_then(|p| p.cache).map(expand_tilde);

        let providers = file.providers.unwrap_or_default();
        let github_provider = providers.github.unwrap_or_default();
        let access_tokens = AccessTokens {
            cfcore: providers.curseforge.and_then(|p| p.api_key),
            github: github_provider.api_key,
            wago_addons: providers.wago.and_then(|p| p.api_key),
        };
        let github_handler = github_provider.handler.unwrap_or_default();

        GlobalConfig {
            log_level,
            access_tokens,
            github_handler,
            dirs: Dirs::default_dirs(cache_override),
            config_path_override: None,
        }
    }

    pub fn config_file_path(&self) -> PathBuf {
        self.config_path_override
            .clone()
            .unwrap_or_else(|| self.dirs.config.join("config.toml"))
    }

    /// Reads `config.toml` from the platform-conventional config dir
    /// (built-in defaults if absent). Equivalent to `read_from(None)`.
    pub fn read() -> Result<Self, ConfigError> {
        Self::read_from(None)
    }

    /// Reads `config.toml`, optionally from an explicit `path` instead of
    /// the platform-conventional config dir (built-in defaults if absent
    /// either way). When `path` is given, `dirs.config` becomes its parent
    /// directory — so `profiles/` is resolved as its sibling — and
    /// [`Self::config_file_path`] returns `path` verbatim. Used by the CLI's
    /// `--config` flag, mainly to let integration tests point at an isolated
    /// fixture file rather than the real platform config dir.
    pub fn read_from(path: Option<&Path>) -> Result<Self, ConfigError> {
        let resolved_path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| config_dir().join("config.toml"));

        let file = match fs::read_to_string(&resolved_path) {
            Ok(s) => toml::from_str::<GlobalConfigFile>(&s)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => GlobalConfigFile::default(),
            Err(e) => return Err(e.into()),
        };

        let mut config = Self::from_file(file);
        if let Some(p) = path {
            config.dirs.config = p
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            config.config_path_override = Some(p.to_path_buf());
        }
        Ok(config)
    }

    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        self.dirs.ensure()?;
        Ok(())
    }

    pub fn write(&self) -> Result<(), ConfigError> {
        self.ensure_dirs()?;
        let file =
            GlobalConfigFile {
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
                    wago: self.access_tokens.wago_addons.clone().map(|api_key| {
                        ProviderConfigFile {
                            api_key: Some(api_key),
                        }
                    }),
                    github: {
                        let api_key = self.access_tokens.github.clone();
                        let handler = (self.github_handler != GitHubHandler::default())
                            .then_some(self.github_handler);
                        (api_key.is_some() || handler.is_some())
                            .then_some(GitHubProviderConfigFile { api_key, handler })
                    },
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

/// On-disk shape of `profiles/<name>/profile.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileConfigFile {
    profile: String,
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    flavour: Option<Flavour>,
}

/// One WoW installation's config: which profile, which `Interface/AddOns`
/// directory, and (implicitly) its own `lock.toml` (see [`ProfileConfig::lock_file_path`]).
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub global_config: GlobalConfig,
    pub profile: String,
    pub addon_dir: PathBuf,
    pub flavour_override: Option<Flavour>,
    pub product: InstalledProduct,
    /// Set only by [`Self::read_from_path`]; makes [`Self::config_file_path`]
    /// return that exact path and [`Self::lock_file_path`] a `lock.toml`
    /// sibling, instead of deriving both from `profile`/`profiles_dir_path`.
    config_path_override: Option<PathBuf>,
}

const PROFILE_FILE_NAME: &str = "profile.toml";
const LOCK_FILE_NAME: &str = "lock.toml";

fn profiles_dir_path(global_config: &GlobalConfig) -> PathBuf {
    global_config.dirs.config.join("profiles")
}

/// Each profile owns a subdirectory named after it (`profiles/<name>/`), so
/// unrelated per-profile files (config, lock file, ...) stay grouped instead
/// of colliding by filename in a single flat `profiles/` dir.
fn profile_dir_path(global_config: &GlobalConfig, profile: &str) -> PathBuf {
    profiles_dir_path(global_config).join(profile)
}

fn profile_config_file_path(global_config: &GlobalConfig, profile: &str) -> PathBuf {
    profile_dir_path(global_config, profile).join(PROFILE_FILE_NAME)
}

fn profile_lock_file_path(global_config: &GlobalConfig, profile: &str) -> PathBuf {
    profile_dir_path(global_config, profile).join(LOCK_FILE_NAME)
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
            config_path_override: None,
        })
    }

    /// Makes [`Self::config_file_path`] resolve to `path` and
    /// [`Self::lock_file_path`] to a `lock.toml` sibling of it, instead of
    /// the name-based `profiles/<profile>/*` layout. Used when bootstrapping
    /// a new profile from a `--profile` value that looked like a path (see
    /// `wau::ctx::new_profile`).
    pub fn with_path_override(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_path_override = Some(path.into());
        self
    }

    /// Reads `profiles/<profile>/profile.toml` by name inside
    /// `global_config`'s config dir.
    pub fn read(global_config: GlobalConfig, profile: &str) -> Result<Self, ConfigError> {
        let path = profile_config_file_path(&global_config, profile);
        Self::read_toml_at(global_config, &path, None)
    }

    /// Reads the one configured profile — for callers with no explicit name
    /// to resolve against (no implicit "default"-named profile). Errors with
    /// [`ConfigError::NoProfilesConfigured`]/[`ConfigError::AmbiguousProfile`]
    /// if zero or multiple profiles exist; callers that already know which
    /// profile they want should go through [`Self::read`]/
    /// [`Self::read_from_path`] instead.
    pub fn read_sole(global_config: GlobalConfig) -> Result<Self, ConfigError> {
        let names = Self::iter_profiles(&global_config);
        match names.as_slice() {
            [] => Err(ConfigError::NoProfilesConfigured),
            [only] => Self::read(global_config, only),
            _ => Err(ConfigError::AmbiguousProfile { available: names }),
        }
    }

    /// Reads a profile config directly from `path`, bypassing name-based
    /// lookup inside `global_config`'s `profiles/` dir entirely. Used by the
    /// CLI's `--profile` flag when given a path instead of a name, mainly so
    /// integration tests can point at a fixture file in an arbitrary
    /// location. [`Self::lock_file_path`] resolves to a `lock.toml` sibling
    /// of `path`, colocating the lock file with the fixture rather than
    /// assuming a `profiles/` layout.
    pub fn read_from_path(
        global_config: GlobalConfig,
        path: impl Into<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let path = path.into();
        Self::read_toml_at(global_config, &path, Some(path.clone()))
    }

    fn read_toml_at(
        global_config: GlobalConfig,
        path: &Path,
        path_override: Option<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let s = fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound {
                    path: path.to_path_buf(),
                }
            } else {
                ConfigError::Io(e)
            }
        })?;
        let file: ProfileConfigFile = toml::from_str(&s)?;
        let mut profile = Self::new(global_config, file.profile, file.path, file.flavour)?;
        profile.config_path_override = path_override;
        Ok(profile)
    }

    /// Every configured profile name (`profiles/<name>/profile.toml`), sorted.
    pub fn iter_profiles(global_config: &GlobalConfig) -> Vec<String> {
        let Ok(entries) = fs::read_dir(profiles_dir_path(global_config)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().join(PROFILE_FILE_NAME).is_file())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
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
        self.config_path_override
            .clone()
            .unwrap_or_else(|| profile_config_file_path(&self.global_config, &self.profile))
    }

    /// The installed-package lock file (see [`crate::lockfile`]) — always a
    /// `lock.toml` sibling of [`Self::config_file_path`], whether that's the
    /// name-based `profiles/<profile>/profile.toml` or a `--profile` path
    /// override.
    pub fn lock_file_path(&self) -> PathBuf {
        match &self.config_path_override {
            Some(path) => path.with_file_name(LOCK_FILE_NAME),
            None => profile_lock_file_path(&self.global_config, &self.profile),
        }
    }

    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        let dir = match &self.config_path_override {
            Some(path) => path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(".")),
            None => profile_dir_path(&self.global_config, &self.profile),
        };
        fs::create_dir_all(dir)?;
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

    /// Trashes the profile's config file and its sibling `lock.toml`, if any.
    pub fn delete(&self) -> Result<(), ConfigError> {
        crate::fs::trash(&self.config_file_path())?;
        let lock_path = self.lock_file_path();
        if lock_path.exists() {
            crate::fs::trash(&lock_path)?;
        }
        Ok(())
    }
}
