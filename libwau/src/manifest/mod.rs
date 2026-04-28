//! `manifest.toml` schema, loading, and validation.
//!
//! **Pin policy**: `pin` fields are parsed for forward compatibility but the resolver
//! must not honour them until Phase 8. Manifests containing pins are accepted; a
//! `tracing::warn` is emitted per pinned addon at load time so the limitation is
//! visible without being fatal.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    Result,
    model::{Channel, Flavor, Provider},
};

#[cfg(test)]
mod tests;

pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    #[serde(default)]
    pub addon: Vec<ManifestAddon>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestAddon {
    pub name: String,
    pub provider: Provider,
    pub channel: Option<Channel>,
    pub flavors: Option<Vec<Flavor>>,
    pub pin: Option<Pin>,
    // CurseForge
    pub project_id: Option<u64>,
    // WoWInterface
    pub wowi_id: Option<u64>,
    // GitHub
    pub repo: Option<String>,
    pub asset_regex: Option<String>,
    pub git_ref: Option<String>,
    // Local: file:// path or absolute filesystem path to a zip
    pub url: Option<String>,
}

/// Version pin — parsed for forward compatibility, ignored by the resolver until Phase 8.
///
/// Variants are ordered from most specific to least specific for untagged serde matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Pin {
    TagWithSha { tag: String, sha256: String },
    Tag { tag: String },
    Version { version: String },
    FileId { file_id: u64 },
    Commit { commit: String },
}

/// Loads and validates a manifest from `path`.
pub fn load(path: &Path) -> Result<Manifest> {
    let content = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::Error::ManifestNotFound {
                path: path.to_path_buf(),
            }
        } else {
            crate::Error::Io(e)
        }
    })?;
    parse(&content)
}

/// Parses a manifest from a TOML string and emits warnings for unsupported features.
pub fn parse(s: &str) -> Result<Manifest> {
    let manifest: Manifest = toml::from_str(s)?;
    warn_pins(&manifest);
    Ok(manifest)
}

fn warn_pins(manifest: &Manifest) {
    for addon in &manifest.addon {
        if addon.pin.is_some() {
            tracing::warn!(
                addon = %addon.name,
                "pin is unsupported until Phase 8 and will be ignored by the resolver"
            );
        }
    }
}
