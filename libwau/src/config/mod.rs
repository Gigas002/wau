//! Global and per-profile configuration — ports instawow's `config/*.py`.
//!
//! Two independent JSON documents: a **global config** (secrets, defaults,
//! computed dirs) and one **profile config** per WoW installation (each
//! profile owns its own `db.sqlite`, written next to its `config.json`). Env
//! vars (`WAU_*`) always win over file values, re-applied on every read.
//! `WAU_HOME` (instawow: `INSTAWOW_HOME`) overrides all three dirs at once.

use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::model::{
    Flavour, ProductInfo, extract_installation_dir_from_addon_dir, infer_product_from_addon_dir,
};

#[cfg(test)]
mod tests;

const APP_NAME: &str = "wau";
const HOME_ENV_VAR: &str = "WAU_HOME";

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config not found at {path}")]
    NotFound { path: PathBuf },

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("profile name must not be empty")]
    EmptyProfileName,

    #[error("'{}' is not a writable directory", .path.display())]
    AddonDirNotWritable { path: PathBuf },

    #[error("game flavour cannot be detected for '{}'; set flavour_override", .path.display())]
    NoFlavourDetected { path: PathBuf },
}

// ============================================================================
// Secrets
// ============================================================================

/// A config value that should never be printed in the clear. Serializes
/// (file I/O) as plain text — matching instawow, which stores tokens
/// unencrypted on disk — but `Debug`/`Display` always redact, so an accidental
/// `{:?}`/`{}` elsewhere in the app can't leak it.
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
// Dirs
// ============================================================================

/// The three XDG-style locations `GlobalConfig` resolves eagerly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub cache: PathBuf,
    pub config: PathBuf,
    pub state: PathBuf,
}

impl Dirs {
    fn default_dirs() -> Self {
        Self {
            cache: cache_dir(&[]),
            config: config_dir(&[]),
            state: state_dir(&[]),
        }
    }

    fn ensure(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.cache)?;
        fs::create_dir_all(&self.config)?;
        fs::create_dir_all(&self.state)?;
        Ok(())
    }
}

fn dirs_home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn join_parts(base: PathBuf, parts: &[&str]) -> PathBuf {
    parts.iter().fold(base, |acc, p| acc.join(p))
}

fn home_override(kind: &str) -> Option<PathBuf> {
    env::var_os(HOME_ENV_VAR).map(|home| PathBuf::from(home).join(kind))
}

/// `$WAU_CONFIG_HOME`-independent config dir: `$WAU_HOME/config`, else
/// `$XDG_CONFIG_HOME/wau`, else the platform default, else `~/.config/wau`.
pub fn config_dir(parts: &[&str]) -> PathBuf {
    if let Some(base) = home_override("config") {
        return join_parts(base, parts);
    }
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(platform_config_base)
        .or_else(|| dirs_home().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    join_parts(base.join(APP_NAME), parts)
}

/// `$WAU_HOME/cache`, else `$XDG_CACHE_HOME/wau`, else the platform default,
/// else `~/.cache/wau`.
pub fn cache_dir(parts: &[&str]) -> PathBuf {
    if let Some(base) = home_override("cache") {
        return join_parts(base, parts);
    }
    let base = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(platform_cache_base)
        .or_else(|| dirs_home().map(|h| h.join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    join_parts(base.join(APP_NAME), parts)
}

/// `$WAU_HOME/state`, else `$XDG_STATE_HOME/wau`, else `~/.local/state/wau` on
/// Linux/BSD, else collapses into [`config_dir`] (macOS/Windows have no XDG
/// state convention — matches instawow).
pub fn state_dir(parts: &[&str]) -> PathBuf {
    if let Some(base) = home_override("state") {
        return join_parts(base, parts);
    }
    if let Some(base) = env::var_os("XDG_STATE_HOME").map(PathBuf::from) {
        return join_parts(base.join(APP_NAME), parts);
    }
    if !cfg!(target_os = "macos")
        && !cfg!(target_os = "windows")
        && let Some(home) = dirs_home()
    {
        return join_parts(home.join(".local").join("state").join(APP_NAME), parts);
    }
    config_dir(parts)
}

fn platform_config_base() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        dirs_home().map(|h| h.join("Library").join("Application Support"))
    } else if cfg!(target_os = "windows") {
        env::var_os("APPDATA").map(PathBuf::from)
    } else {
        None
    }
}

fn platform_cache_base() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        dirs_home().map(|h| h.join("Library").join("Caches"))
    } else if cfg!(target_os = "windows") {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        None
    }
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy().into_owned();
    if s == "~" {
        return dirs_home().unwrap_or(path);
    }
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = dirs_home()
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

impl AccessTokens {
    fn is_empty(&self) -> bool {
        self.cfcore.is_none() && self.github.is_none() && self.wago_addons.is_none()
    }
}

/// On-disk shape of `config.json` — [`Dirs`] are computed, never persisted.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GlobalConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_update_check: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_tokens: Option<AccessTokens>,
}

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub auto_update_check: bool,
    pub access_tokens: AccessTokens,
    pub dirs: Dirs,
}

impl GlobalConfig {
    /// Defaults overlaid with env vars only — no disk read.
    pub fn from_env() -> Self {
        Self::from_file(GlobalConfigFile::default())
    }

    fn from_file(mut file: GlobalConfigFile) -> Self {
        if let Some(v) = env_bool("WAU_AUTO_UPDATE_CHECK") {
            file.auto_update_check = Some(v);
        }

        let mut access_tokens = file.access_tokens.unwrap_or_default();
        if let Ok(v) = env::var("WAU_ACCESS_TOKENS_CFCORE") {
            access_tokens.cfcore = Some(SecretString::new(v));
        }
        if let Ok(v) = env::var("WAU_ACCESS_TOKENS_GITHUB") {
            access_tokens.github = Some(SecretString::new(v));
        }
        if let Ok(v) = env::var("WAU_ACCESS_TOKENS_WAGO_ADDONS") {
            access_tokens.wago_addons = Some(SecretString::new(v));
        }

        GlobalConfig {
            auto_update_check: file.auto_update_check.unwrap_or(true),
            access_tokens,
            dirs: Dirs::default_dirs(),
        }
    }

    pub fn config_file_path(&self) -> PathBuf {
        self.dirs.config.join("config.json")
    }

    fn access_tokens_file_path(&self) -> PathBuf {
        self.dirs.config.join("config.access_tokens.json")
    }

    /// Reads `config.json` (defaults if absent), overlays
    /// `config.access_tokens.json` if present and non-empty, then overlays env
    /// vars (env always wins).
    pub fn read() -> Result<Self, ConfigError> {
        let base = Self::from_env();

        let mut file = match fs::read(base.config_file_path()) {
            Ok(bytes) => serde_json::from_slice::<GlobalConfigFile>(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => GlobalConfigFile::default(),
            Err(e) => return Err(e.into()),
        };

        if let Ok(bytes) = fs::read(base.access_tokens_file_path())
            && let Ok(tokens) = serde_json::from_slice::<AccessTokens>(&bytes)
            && !tokens.is_empty()
        {
            file.access_tokens = Some(tokens);
        }

        Ok(Self::from_file(file))
    }

    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        self.dirs.ensure()?;
        Ok(())
    }

    pub fn write(&self) -> Result<(), ConfigError> {
        self.ensure_dirs()?;
        let file = GlobalConfigFile {
            auto_update_check: Some(self.auto_update_check),
            access_tokens: Some(self.access_tokens.clone()),
        };
        fs::write(
            self.config_file_path(),
            serde_json::to_string_pretty(&file)?,
        )?;
        Ok(())
    }
}

fn env_bool(key: &str) -> Option<bool> {
    match env::var(key).ok()?.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

// ============================================================================
// ProfileConfig
// ============================================================================

/// The WoW client a profile targets: either auto-detected from `addon_dir`'s
/// install-directory subfolder name, or forced via `flavour_override`
/// (instawow's `Product` / `_NullProduct` union, collapsed to an enum).
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileConfigFile {
    profile: String,
    addon_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    flavour_override: Option<Flavour>,
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

fn profile_config_file_path(global_config: &GlobalConfig, profile: &str) -> PathBuf {
    global_config
        .dirs
        .config
        .join("profiles")
        .join(profile)
        .join("config.json")
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
        let bytes = fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ConfigError::NotFound { path: path.clone() }
            } else {
                ConfigError::Io(e)
            }
        })?;
        let file: ProfileConfigFile = serde_json::from_slice(&bytes)?;
        Self::new(
            global_config,
            file.profile,
            file.addon_dir,
            file.flavour_override,
        )
    }

    /// Every configured profile name (`profiles/*/config.json`), sorted.
    pub fn iter_profiles(global_config: &GlobalConfig) -> Vec<String> {
        let Ok(entries) = fs::read_dir(global_config.dirs.config.join("profiles")) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().join("config.json").is_file())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect();
        names.sort();
        names
    }

    /// Installation directories already covered by a configured profile
    /// (best-effort: reads just the raw `addon_dir` value from each profile's
    /// JSON, unexpanded — matches instawow's `iter_profile_installations`).
    pub fn iter_profile_installations(global_config: &GlobalConfig) -> Vec<PathBuf> {
        Self::iter_profiles(global_config)
            .into_iter()
            .filter_map(|name| {
                let path = profile_config_file_path(global_config, &name);
                let bytes = fs::read(&path).ok()?;
                let file: ProfileConfigFile = serde_json::from_slice(&bytes).ok()?;
                extract_installation_dir_from_addon_dir(&file.addon_dir)
            })
            .collect()
    }

    pub fn config_file_path(&self) -> PathBuf {
        profile_config_file_path(&self.global_config, &self.profile)
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_file_path()
            .parent()
            .expect("profile config file path always has a parent")
            .to_path_buf()
    }

    pub fn db_file_path(&self) -> PathBuf {
        self.config_path().join("db.sqlite")
    }

    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        fs::create_dir_all(self.config_path())?;
        Ok(())
    }

    pub fn write(&self) -> Result<(), ConfigError> {
        self.ensure_dirs()?;
        let file = ProfileConfigFile {
            profile: self.profile.clone(),
            addon_dir: self.addon_dir.clone(),
            flavour_override: self.flavour_override,
        };
        fs::write(
            self.config_file_path(),
            serde_json::to_string_pretty(&file)?,
        )?;
        Ok(())
    }

    /// Trashes the profile's whole config directory (config.json + db.sqlite).
    pub fn delete(&self) -> Result<(), ConfigError> {
        crate::fs::trash(&self.config_path())?;
        Ok(())
    }
}
