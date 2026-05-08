//! `manifest.toml` schema, loading, validation, and bootstrap generation.
//!
//! **Pin policy**: `pin` fields are parsed for forward compatibility but the resolver
//! must not honour them until Phase 8. Manifests containing pins are accepted; a
//! `tracing::warn` is emitted per pinned addon at load time so the limitation is
//! visible without being fatal.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use std::collections::{HashMap, HashSet};

use crate::{
    Result,
    fs::InstalledAddon,
    model::{Channel, Flavor, Provider},
};

#[cfg(test)]
mod tests;

pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addon: Vec<ManifestAddon>,
}

impl Manifest {
    pub fn new() -> Self {
        Self {
            schema: SUPPORTED_SCHEMA,
            addon: Vec::new(),
        }
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
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

/// Writes a manifest to `path`. The parent directory must already exist.
pub fn save(manifest: &Manifest, path: &Path) -> Result<()> {
    let content = toml::to_string_pretty(manifest)?;
    fs::write(path, content)?;
    Ok(())
}

/// Parses a manifest from a TOML string and emits warnings for unsupported features.
pub fn parse(s: &str) -> Result<Manifest> {
    let manifest: Manifest = toml::from_str(s)?;
    warn_pins(&manifest);
    Ok(manifest)
}

/// Builds a [`Manifest`] by detecting provider IDs from installed addons' `.toc` files.
///
/// Each addon whose `.toc` contains a recognised `X-*` provider field becomes one
/// `[[addon]]` entry using `flavor` as the flavor constraint and `stable` as the default
/// channel. Addons with no detectable provider are silently skipped.
///
/// Deduplication is performed by provider-scoped ID (e.g. two folders sharing the same
/// `X-Curse-Project-ID` produce a single manifest entry).
pub fn from_installed(addons: &[InstalledAddon], flavor: &Flavor) -> Manifest {
    let mut manifest = Manifest::new();
    let mut seen_cf: HashSet<u64> = HashSet::new();
    let mut seen_wowi: HashSet<u64> = HashSet::new();

    for addon in addons {
        if let Some(entry) = addon_from_installed(addon, flavor) {
            let is_new = match &entry.provider {
                Provider::CurseForge => entry.project_id.is_none_or(|id| seen_cf.insert(id)),
                Provider::WoWInterface => entry.wowi_id.is_none_or(|id| seen_wowi.insert(id)),
                _ => true,
            };
            if is_new {
                manifest.addon.push(entry);
            }
        }
    }

    manifest
}

/// Attempts to build a single [`ManifestAddon`] from one installed addon by scanning
/// its `.toc` files for recognised provider `X-*` fields.
///
/// Priority: CurseForge (`X-Curse-Project-ID`) → WoWInterface (`X-WoWI-ID`,
/// `X-WoWInterface-ID`). Returns `None` when no provider can be detected.
fn addon_from_installed(addon: &InstalledAddon, flavor: &Flavor) -> Option<ManifestAddon> {
    for toc in &addon.toc_files {
        let x = &toc.x_fields;

        // CurseForge: X-Curse-Project-ID
        if let Some(id) = x_field_u64(x, "x-curse-project-id") {
            return Some(ManifestAddon {
                name: addon.display_title().to_owned(),
                provider: Provider::CurseForge,
                channel: Some(Channel::Stable),
                flavors: Some(vec![flavor.clone()]),
                pin: None,
                project_id: Some(id),
                wowi_id: None,
                repo: None,
                asset_regex: None,
                git_ref: None,
                url: None,
            });
        }

        // WoWInterface: X-WoWI-ID or X-WoWInterface-ID
        for key in ["x-wowi-id", "x-wowinterface-id"] {
            if let Some(id) = x_field_u64(x, key) {
                return Some(ManifestAddon {
                    name: addon.display_title().to_owned(),
                    provider: Provider::WoWInterface,
                    channel: Some(Channel::Stable),
                    flavors: Some(vec![flavor.clone()]),
                    pin: None,
                    project_id: None,
                    wowi_id: Some(id),
                    repo: None,
                    asset_regex: None,
                    git_ref: None,
                    url: None,
                });
            }
        }
    }

    None
}

/// Case-insensitive lookup of an `X-*` field returning a parsed `u64`.
fn x_field_u64(fields: &HashMap<String, String>, key: &str) -> Option<u64> {
    fields.iter().find_map(|(k, v)| {
        if k.to_ascii_lowercase() == key {
            v.trim().parse::<u64>().ok()
        } else {
            None
        }
    })
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
