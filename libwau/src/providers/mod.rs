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

#[cfg(feature = "curseforge")]
pub mod curseforge;

#[cfg(feature = "wowinterface")]
pub mod wowinterface;

#[cfg(feature = "github")]
pub mod github;

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
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Resolve the manifest row to a single latest artifact given the install context.
    async fn resolve(
        &self,
        addon: &ManifestAddon,
        ctx: &InstallContext,
    ) -> Result<ResolvedArtifact>;

    /// Download (or copy) the artifact to `dest`, which should not exist yet.
    async fn download(&self, artifact: &ResolvedArtifact, dest: &Path) -> Result<()>;
}

/// Runtime credentials and options passed when constructing a provider.
///
/// All fields are optional; providers that require a credential (e.g. CurseForge API key)
/// will return [`crate::Error::MissingApiKey`] from [`for_provider`] when absent.
#[derive(Debug, Clone, Default)]
pub struct ProviderConfig {
    pub curseforge_api_key: Option<String>,
    pub github_token: Option<String>,
}

/// Returns the provider implementation for `provider`, or an error if it is
/// not compiled into this build or required credentials are absent.
pub fn for_provider(
    provider: &crate::model::Provider,
    _config: &ProviderConfig,
) -> Result<Box<dyn Provider>> {
    #[allow(unreachable_patterns)]
    match provider {
        #[cfg(feature = "local")]
        crate::model::Provider::Local => Ok(Box::new(local::LocalProvider::new())),
        #[cfg(feature = "curseforge")]
        crate::model::Provider::CurseForge => {
            let api_key =
                _config
                    .curseforge_api_key
                    .clone()
                    .ok_or(crate::Error::MissingApiKey {
                        provider: crate::model::Provider::CurseForge,
                    })?;
            Ok(Box::new(curseforge::CurseForgeProvider::new(api_key)))
        }
        #[cfg(feature = "wowinterface")]
        crate::model::Provider::WoWInterface => {
            Ok(Box::new(wowinterface::WoWInterfaceProvider::new()))
        }
        #[cfg(feature = "github")]
        crate::model::Provider::GitHub => Ok(Box::new(github::GitHubProvider::new(
            _config.github_token.clone(),
        ))),
        _ => Err(crate::Error::ProviderNotSupported {
            provider: provider.clone(),
        }),
    }
}
