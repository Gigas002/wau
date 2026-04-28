//! Local provider: installs from a filesystem path or `file://` URL.
//!
//! Manifest rows must supply a `url` field with either an absolute path
//! (`/path/to/addon.zip`) or a `file://` URL (`file:///path/to/addon.zip`).
//! This provider is the reference implementation for end-to-end tests without
//! external network access.

use std::{fs, path::Path};

use crate::{
    Result,
    manifest::ManifestAddon,
    providers::{InstallContext, Provider, ResolvedArtifact},
};

pub struct LocalProvider;

impl Default for LocalProvider {
    fn default() -> Self {
        Self
    }
}

impl LocalProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for LocalProvider {
    fn resolve(&self, addon: &ManifestAddon, _ctx: &InstallContext) -> Result<ResolvedArtifact> {
        let url = addon
            .url
            .as_deref()
            .ok_or_else(|| crate::Error::LocalMissingUrl {
                name: addon.name.clone(),
            })?;

        let path = url_to_path(url);
        let id = format!("local:{}", path.display());

        Ok(ResolvedArtifact {
            version: "local".into(),
            id,
            url: url.to_owned(),
            sha256: None,
        })
    }

    fn download(&self, artifact: &ResolvedArtifact, dest: &Path) -> Result<()> {
        let src = url_to_path(&artifact.url);
        fs::copy(&src, dest)?;
        Ok(())
    }
}

/// Converts a `file://`-prefixed URL or a plain filesystem path to a `PathBuf`.
pub(crate) fn url_to_path(url: &str) -> std::path::PathBuf {
    if let Some(rest) = url.strip_prefix("file://") {
        std::path::PathBuf::from(rest)
    } else {
        std::path::PathBuf::from(url)
    }
}
