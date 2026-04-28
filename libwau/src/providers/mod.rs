//! Provider abstraction: trait, install context, resolved artifact, and registry.
//!
//! Each concrete provider lives behind its own Cargo feature (§2.2).
//! The registry function [`for_provider`] compiles cleanly with `--no-default-features`
//! and returns [`crate::Error::ProviderNotSupported`] for any provider not compiled in.

use std::path::{Path, PathBuf};

use crate::{
    Result,
    manifest::ManifestAddon,
    model::{Channel, Flavor, Tag},
};

#[cfg(test)]
mod tests;

#[cfg(feature = "local")]
pub mod local;

/// Install-time context passed to every provider call.
#[derive(Debug, Clone)]
pub struct InstallContext {
    pub tag: Tag,
    pub flavor: Flavor,
    pub channel: Channel,
    /// `Interface/AddOns` directory for the target install.
    pub addons_path: PathBuf,
    /// Cache root for staging areas and download buffers.
    pub cache_dir: PathBuf,
}

/// A resolved artifact ready for download and extraction.
#[derive(Debug, Clone)]
pub struct ResolvedArtifact {
    /// Human-readable version string.
    pub version: String,
    /// Provider-scoped opaque identifier (file id, tag, commit sha, …).
    pub id: String,
    /// Download URL (or `file://` / filesystem path for local provider).
    pub url: String,
    /// Optional sha256 hex digest for integrity verification.
    pub sha256: Option<String>,
}

/// Minimum interface every provider must implement.
pub trait Provider: Send + Sync {
    /// Resolve the manifest row to a single latest artifact given the install context.
    fn resolve(&self, addon: &ManifestAddon, ctx: &InstallContext) -> Result<ResolvedArtifact>;

    /// Download (or copy) the artifact to `dest`, which should not exist yet.
    fn download(&self, artifact: &ResolvedArtifact, dest: &Path) -> Result<()>;
}

/// Returns the provider implementation for `provider`, or an error if it is
/// not compiled into this build.
pub fn for_provider(provider: &crate::model::Provider) -> Result<Box<dyn Provider>> {
    match provider {
        #[cfg(feature = "local")]
        crate::model::Provider::Local => Ok(Box::new(local::LocalProvider::new())),
        _ => Err(crate::Error::ProviderNotSupported {
            provider: provider.clone(),
        }),
    }
}
