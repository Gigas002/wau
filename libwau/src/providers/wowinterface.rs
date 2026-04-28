//! WoWInterface provider: resolves and downloads addon files via the MMOUI v4 API.
//!
//! No authentication is required; the MMOUI API is publicly accessible.

use std::path::Path;

use serde::Deserialize;

use crate::{
    Result,
    manifest::ManifestAddon,
    providers::{InstallContext, Provider, ResolvedArtifact},
};

const DEFAULT_BASE_URL: &str = "https://api.mmoui.com/v4/game/WOW";

pub struct WoWInterfaceProvider {
    client: reqwest::Client,
    base_url: String,
}

impl Default for WoWInterfaceProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl WoWInterfaceProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_BASE_URL.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_base_url(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }
}

#[async_trait::async_trait]
impl Provider for WoWInterfaceProvider {
    async fn resolve(
        &self,
        addon: &ManifestAddon,
        _ctx: &InstallContext,
    ) -> Result<ResolvedArtifact> {
        let wowi_id = addon.wowi_id.ok_or_else(|| crate::Error::MissingWowiId {
            name: addon.name.clone(),
        })?;

        let url = format!("{}/filedetails/{}.json", self.base_url, wowi_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| crate::Error::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(crate::Error::Http(format!("{} {}", resp.status(), url)));
        }

        let files: Vec<WowiFile> = resp
            .json()
            .await
            .map_err(|e| crate::Error::Http(e.to_string()))?;

        let file = files
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::NoRelease {
                name: addon.name.clone(),
            })?;

        Ok(ResolvedArtifact {
            version: file.ui_version,
            id: file.uid,
            url: file.ui_download,
            sha256: None,
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
struct WowiFile {
    #[serde(rename = "UID")]
    uid: String,
    #[serde(rename = "UIVersion")]
    ui_version: String,
    #[serde(rename = "UIDownload")]
    ui_download: String,
}
