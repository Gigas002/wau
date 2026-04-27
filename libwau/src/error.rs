use std::path::PathBuf;

use thiserror::Error;

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
}
