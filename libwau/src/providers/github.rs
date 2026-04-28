//! GitHub provider: resolves addon files via the GitHub REST API v3.
//!
//! Two modes, selected by which manifest fields are present:
//!   - **Release asset** (`asset_regex` is set): latest release whose assets contain a
//!     file matching the regex, filtered by channel (stable excludes pre-releases).
//!   - **Git-ref tip** (`git_ref` is set, `asset_regex` is absent): HEAD commit SHA of
//!     the ref; the zipball URL is stored so the lock records the exact commit.
//!
//! An optional `github_token` in [`crate::providers::ProviderConfig`] raises the
//! unauthenticated rate limit from 60 to 5 000 requests / hour.

use std::path::Path;

use regex::Regex;
use serde::Deserialize;

use crate::{
    Result,
    manifest::ManifestAddon,
    model::Channel,
    providers::{InstallContext, Provider, ResolvedArtifact},
};

const DEFAULT_BASE_URL: &str = "https://api.github.com";

pub struct GitHubProvider {
    client: reqwest::Client,
    token: Option<String>,
    base_url: String,
}

impl GitHubProvider {
    pub fn new(token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            token,
            base_url: DEFAULT_BASE_URL.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_base_url(token: Option<String>, base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            token,
            base_url,
        }
    }

    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .client
            .get(url)
            .header("User-Agent", "wau")
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        req
    }
}

#[async_trait::async_trait]
impl Provider for GitHubProvider {
    async fn resolve(
        &self,
        addon: &ManifestAddon,
        ctx: &InstallContext,
    ) -> Result<ResolvedArtifact> {
        let repo = addon
            .repo
            .as_deref()
            .ok_or_else(|| crate::Error::MissingRepo {
                name: addon.name.clone(),
            })?;

        let channel = addon.channel.as_ref().unwrap_or(&ctx.channel);

        if let Some(pattern) = &addon.asset_regex {
            resolve_release_asset(self, addon, repo, pattern, channel).await
        } else if let Some(git_ref) = &addon.git_ref {
            resolve_git_ref(self, addon, repo, git_ref).await
        } else {
            Err(crate::Error::NoRelease {
                name: addon.name.clone(),
            })
        }
    }

    async fn download(&self, artifact: &ResolvedArtifact, dest: &Path) -> Result<()> {
        let resp = self
            .client
            .get(&artifact.url)
            .header("User-Agent", "wau")
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

async fn resolve_release_asset(
    provider: &GitHubProvider,
    addon: &ManifestAddon,
    repo: &str,
    pattern: &str,
    channel: &Channel,
) -> Result<ResolvedArtifact> {
    let re =
        Regex::new(pattern).map_err(|e| crate::Error::Http(format!("invalid asset_regex: {e}")))?;

    let url = format!("{}/repos/{}/releases", provider.base_url, repo);
    let resp = provider
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::Error::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(crate::Error::Http(format!("{} {}", resp.status(), url)));
    }

    let releases: Vec<GhRelease> = resp
        .json()
        .await
        .map_err(|e| crate::Error::Http(e.to_string()))?;

    if releases.is_empty() {
        return Err(crate::Error::NoRelease {
            name: addon.name.clone(),
        });
    }

    let include_prerelease = !matches!(channel, Channel::Stable);

    for release in releases
        .iter()
        .filter(|r| !r.draft && (include_prerelease || !r.prerelease))
    {
        if let Some(asset) = release.assets.iter().find(|a| re.is_match(&a.name)) {
            return Ok(ResolvedArtifact {
                version: release.tag_name.clone(),
                id: asset.id.to_string(),
                url: asset.browser_download_url.clone(),
                sha256: None,
            });
        }
    }

    Err(crate::Error::NoMatchingAsset {
        name: addon.name.clone(),
        tag: releases
            .first()
            .map(|r| r.tag_name.as_str())
            .unwrap_or("?")
            .to_owned(),
        pattern: pattern.to_owned(),
    })
}

async fn resolve_git_ref(
    provider: &GitHubProvider,
    _addon: &ManifestAddon,
    repo: &str,
    git_ref: &str,
) -> Result<ResolvedArtifact> {
    let url = format!("{}/repos/{}/commits/{}", provider.base_url, repo, git_ref);
    let resp = provider
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::Error::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(crate::Error::Http(format!("{} {}", resp.status(), url)));
    }

    let commit: GhCommit = resp
        .json()
        .await
        .map_err(|e| crate::Error::Http(e.to_string()))?;

    let sha = commit.sha;
    let short = sha[..sha.len().min(7)].to_owned();
    let zipball_url = format!("{}/repos/{}/zipball/{}", provider.base_url, repo, sha);

    Ok(ResolvedArtifact {
        version: short,
        id: sha,
        url: zipball_url,
        sha256: None,
    })
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    prerelease: bool,
    draft: bool,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    id: u64,
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhCommit {
    sha: String,
}
