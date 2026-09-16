//! Wago Addons source.

#[cfg(test)]
mod tests;

use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use url::Url;

use crate::{
    config::SecretString,
    http::{CacheTtl, HttpClient},
    model::{ChangelogFormat, Defn, Flavour, HeadersIntent, SourceMetadata, Strategy},
    results::{AnyOutcome, Failure, InternalError, ManagerError},
    sources::{PkgCandidate, Resolver},
};

const DEFAULT_API_URL: &str = "https://addons.wago.io/api/external";

#[derive(Debug, Deserialize)]
struct WagoAddon {
    id: String,
    slug: String,
    display_name: String,
    summary: String,
    website_url: String,
    #[serde(default)]
    recent_release: HashMap<String, WagoRelease>,
}

#[derive(Debug, Deserialize)]
struct WagoRelease {
    label: String,
    changelog: String,
    created_at: String,
    download_link: String,
}

fn parse_iso_datetime(s: &str) -> Result<DateTime<Utc>, Failure> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| InternalError::new(e).into())
}

/// TBC/Titan classic have no Wago game-version mapping; callers surface
/// `None` as an internal error rather than silently treating it as
/// unsupported.
fn game_version_param(flavour: Flavour) -> Option<&'static str> {
    match flavour {
        Flavour::Mainline => Some("retail"),
        Flavour::VanillaClassic => Some("classic"),
        Flavour::WrathClassic => Some("wotlk"),
        Flavour::CataClassic => Some("cata"),
        Flavour::MistsClassic => Some("mop"),
        Flavour::TbcClassic | Flavour::TitanClassic => None,
    }
}

pub struct WagoAddonsResolver {
    token: Option<SecretString>,
    #[cfg(test)]
    api_url: Option<String>,
}

impl WagoAddonsResolver {
    pub fn new(token: Option<SecretString>) -> Self {
        Self {
            token,
            #[cfg(test)]
            api_url: None,
        }
    }

    #[cfg(test)]
    fn new_with_api_url(token: Option<SecretString>, api_url: String) -> Self {
        Self {
            token,
            api_url: Some(api_url),
        }
    }

    fn api_base(&self) -> &str {
        #[cfg(test)]
        return self.api_url.as_deref().unwrap_or(DEFAULT_API_URL);
        #[cfg(not(test))]
        return DEFAULT_API_URL;
    }
}

#[async_trait::async_trait]
impl Resolver for WagoAddonsResolver {
    fn metadata(&self) -> SourceMetadata {
        SourceMetadata {
            id: "wago",
            name: "Wago Addons",
            strategies: &[Strategy::AnyReleaseType],
            changelog_format: ChangelogFormat::Markdown,
            addon_toc_key: Some("X-Wago-ID"),
        }
    }

    fn get_disabled_reason(&self) -> Option<String> {
        if self.token.is_none() {
            Some("access token missing".to_owned())
        } else {
            None
        }
    }

    fn get_alias_from_url(&self, url: &str) -> Option<String> {
        let parsed = Url::parse(url).ok()?;
        if parsed.host_str() != Some("addons.wago.io") {
            return None;
        }
        let segments: Vec<&str> = parsed.path_segments()?.collect();
        if segments.len() > 1 && segments[0] == "addons" {
            Some(segments[1].to_owned())
        } else {
            None
        }
    }

    /// Requires a bearer token to be configured; when it's missing, this
    /// returns no `Authorization` header at all rather than erroring, since
    /// a missing-token source is expected to be filtered out by
    /// `get_disabled_reason` before any request is attempted.
    fn make_request_headers(&self, _intent: HeadersIntent) -> Vec<(String, String)> {
        match &self.token {
            Some(t) => vec![("Authorization".to_owned(), format!("Bearer {}", t.expose()))],
            None => Vec::new(),
        }
    }

    async fn resolve_one_impl(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        let Some(game_version) = game_version_param(flavour) else {
            return Err(InternalError::new(format!(
                "Wago has no game_version mapping for flavour '{flavour}'"
            ))
            .into());
        };

        let headers = Resolver::make_request_headers(self, HeadersIntent::Fetch);
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let mut url = Url::parse(&format!("{}/addons/{}", self.api_base(), defn.alias))
            .map_err(InternalError::new)?;
        url.query_pairs_mut()
            .append_pair("game_version", game_version);

        let response = http
            .get(
                url.as_str(),
                &headers,
                CacheTtl::For(Duration::from_secs(5 * 60)),
            )
            .await?;
        if response.status == 404 {
            return Err(ManagerError::PkgNonexistent.into());
        }
        if !(200..300).contains(&response.status) {
            return Err(InternalError::new(format!("HTTP {} for {url}", response.status)).into());
        }

        let addon: WagoAddon = serde_json::from_slice(&response.body)?;

        let mut releases: Vec<(String, WagoRelease)> = addon.recent_release.into_iter().collect();
        if !defn.strategies.any_release_type && releases.iter().any(|(k, _)| k == "stable") {
            releases.retain(|(k, _)| k == "stable");
        }

        let chosen = releases
            .into_iter()
            .filter_map(|(_, r)| parse_iso_datetime(&r.created_at).ok().map(|dt| (dt, r)))
            .max_by_key(|(dt, _)| *dt);

        let Some((date_published, file)) = chosen else {
            return Err(ManagerError::PkgFilesNotMatching {
                strategies: defn.strategies.clone(),
            }
            .into());
        };

        Ok(PkgCandidate {
            id: addon.id,
            slug: addon.slug,
            name: addon.display_name,
            description: addon.summary,
            url: addon.website_url,
            download_url: file.download_link,
            date_published,
            version: file.label,
            changelog_url: format!("data:,{}", super::percent_encode(&file.changelog)),
            deps: Vec::new(),
        })
    }
}
