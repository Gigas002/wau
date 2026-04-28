use std::path::PathBuf;

use thiserror::Error;

use crate::model::Provider;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML deserialize: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("TOML serialize: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("manifest not found at {path}")]
    ManifestNotFound { path: PathBuf },

    #[error("lock not found at {path}")]
    LockNotFound { path: PathBuf },

    #[error("provider '{provider}' is not supported in this build")]
    ProviderNotSupported { provider: Provider },

    #[error("no API key configured for provider '{provider}'")]
    MissingApiKey { provider: Provider },

    #[error("local provider addon '{name}' has no url field")]
    LocalMissingUrl { name: String },

    #[error("CurseForge addon '{name}' has no project_id")]
    MissingProjectId { name: String },

    #[error("WoWInterface addon '{name}' has no wowi_id")]
    MissingWowiId { name: String },

    #[error("GitHub addon '{name}' has no repo")]
    MissingRepo { name: String },

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("no release found for addon '{name}'")]
    NoRelease { name: String },

    #[error("no asset matching '{pattern}' in release '{tag}' for addon '{name}'")]
    NoMatchingAsset {
        name: String,
        tag: String,
        pattern: String,
    },

    #[error("zip extraction failed: {0}")]
    ZipExtract(#[from] zip::result::ZipError),

    #[error("no installable addon directories found in zip for '{name}'")]
    NoInstallableDirs { name: String },

    #[error("addon '{name}' not found in lock")]
    AddonNotInLock { name: String },
}
