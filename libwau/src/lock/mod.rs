use std::{fs, path::Path};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    Result,
    model::{Channel, Flavor, Provider, Tag},
};

#[cfg(test)]
mod tests;

pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lock {
    pub schema: u32,
    pub generated_at: DateTime<Utc>,
    pub install_tag: Tag,
    #[serde(default)]
    pub addon: Vec<LockedAddon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedAddon {
    pub name: String,
    pub provider: Provider,
    pub flavor: Flavor,
    pub channel: Channel,
    pub project_id: Option<u64>,
    pub resolved_version: String,
    pub resolved_id: String,
    pub download_url: String,
    pub sha256: Option<String>,
    pub installed_dirs: Vec<String>,
    pub installed_at: DateTime<Utc>,
}

impl Lock {
    pub fn new(install_tag: Tag) -> Self {
        Self {
            schema: SUPPORTED_SCHEMA,
            generated_at: Utc::now(),
            install_tag,
            addon: Vec::new(),
        }
    }
}

/// Loads a lock from `path`.
pub fn load(path: &Path) -> Result<Lock> {
    let content = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::Error::LockNotFound {
                path: path.to_path_buf(),
            }
        } else {
            crate::Error::Io(e)
        }
    })?;
    parse(&content)
}

/// Parses a lock from a TOML string.
pub fn parse(s: &str) -> Result<Lock> {
    Ok(toml::from_str(s)?)
}

/// Writes a lock to `path`. The parent directory must already exist.
pub fn save(lock: &Lock, path: &Path) -> Result<()> {
    let content = toml::to_string_pretty(lock)?;
    fs::write(path, content)?;
    Ok(())
}
