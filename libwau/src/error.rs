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

    #[error("local provider addon '{name}' has no url field")]
    LocalMissingUrl { name: String },

    #[error("zip extraction failed: {0}")]
    ZipExtract(#[from] zip::result::ZipError),

    #[error("no installable addon directories found in zip for '{name}'")]
    NoInstallableDirs { name: String },

    #[error("addon '{name}' not found in lock")]
    AddonNotInLock { name: String },
}
