//! CurseForge provider: resolves and downloads addon files via the CurseForge v1 API.
//!
//! Requires a CurseForge API key, configured via `[providers.curseforge] api_key` in
//! `config.toml` and carried through [`crate::providers::ProviderConfig`].

use std::path::Path;

use serde::Deserialize;

use crate::{
    Result,
    manifest::ManifestAddon,
    model::{Channel, Flavor},
    providers::{InstallContext, Provider, ResolvedArtifact},
};

const DEFAULT_BASE_URL: &str = "https://api.curseforge.com/v1";

pub struct CurseForgeProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl CurseForgeProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: DEFAULT_BASE_URL.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url,
        }
    }
}

#[async_trait::async_trait]
impl Provider for CurseForgeProvider {
    async fn resolve(
        &self,
        addon: &ManifestAddon,
        ctx: &InstallContext,
    ) -> Result<ResolvedArtifact> {
        let project_id = addon
            .project_id
            .ok_or_else(|| crate::Error::MissingProjectId {
                name: addon.name.clone(),
            })?;

        let channel = addon.channel.as_ref().unwrap_or(&ctx.channel);
        let release_type = channel_to_release_type(channel);

        let url = format!("{}/mods/{}/files", self.base_url, project_id);
        let resp = self
            .client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .query(&[
                ("gameId", "1"),
                ("releaseType", release_type),
                ("pageSize", "50"),
            ])
            .send()
            .await
            .map_err(|e| crate::Error::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(crate::Error::Http(format!("{} {}", resp.status(), url)));
        }

        let body: FilesResponse = resp
            .json()
            .await
            .map_err(|e| crate::Error::Http(e.to_string()))?;

        let file = body
            .data
            .into_iter()
            .find(|f| flavor_matches(&ctx.flavor, &f.game_versions))
            .ok_or_else(|| crate::Error::NoRelease {
                name: addon.name.clone(),
            })?;

        let sha256 = file
            .hashes
            .iter()
            .find(|h| h.algo == 2)
            .map(|h| h.value.clone());

        Ok(ResolvedArtifact {
            version: file.display_name,
            id: file.id.to_string(),
            url: file.download_url,
            sha256,
        })
    }

    async fn download(&self, artifact: &ResolvedArtifact, dest: &Path) -> Result<()> {
        let resp = self
            .client
            .get(&artifact.url)
            .send()
            .await
            .map_err(|e| crate::Error::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(crate::Error::Http(format!(
                "{} {}",
                resp.status(),
                artifact.url
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| crate::Error::Http(e.to_string()))?;
        std::fs::write(dest, bytes)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FilesResponse {
    data: Vec<CfFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFile {
    id: i64,
    display_name: String,
    download_url: String,
    game_versions: Vec<String>,
    hashes: Vec<CfHash>,
}

#[derive(Deserialize)]
struct CfHash {
    value: String,
    algo: u8,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn channel_to_release_type(channel: &Channel) -> &'static str {
    match channel {
        Channel::Stable => "1",
        Channel::Beta => "2",
        Channel::Alpha => "3",
    }
}

/// Returns `true` if any version string in `versions` matches the major-version
/// prefix expected for `flavor`. Returns `true` when `versions` is empty (no
/// flavor metadata available from the provider — accept the file).
pub(crate) fn flavor_matches(flavor: &Flavor, versions: &[String]) -> bool {
    if versions.is_empty() {
        return true;
    }
    let prefix = flavor_version_prefix(flavor);
    versions.iter().any(|v| v.starts_with(prefix))
}

fn flavor_version_prefix(flavor: &Flavor) -> &'static str {
    match flavor {
        // Retail tracks the current mainline client. Midnight launched at 12.x.
        Flavor::Retail => "12.",
        Flavor::Tww => "11.",
        Flavor::Dragonflight => "10.",
        Flavor::Shadowlands => "9.",
        Flavor::Bfa => "8.",
        Flavor::Legion => "7.",
        Flavor::Wod => "6.",
        Flavor::Mop => "5.",
        Flavor::Cata => "4.",
        Flavor::Wrath => "3.",
        Flavor::Tbc => "2.",
        Flavor::Era => "1.",
    }
}
