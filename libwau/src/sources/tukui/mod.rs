//! Tukui source.

#[cfg(test)]
mod tests;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use url::Url;

use crate::{
    http::HttpClient,
    model::{ChangelogFormat, Defn, Flavour, SourceMetadata},
    results::{AnyOutcome, Failure, InternalError, ManagerError},
    sources::{PkgCandidate, Resolver},
};

const DEFAULT_API_URL: &str = "https://api.tukui.org/v1";

#[derive(Debug, Deserialize)]
struct TukuiAddon {
    id: u64,
    slug: String,
    name: String,
    url: String,
    version: String,
    changelog_url: String,
    patch: Vec<String>,
    last_update: String,
    web_url: String,
    small_desc: String,
}

fn parse_date_only(s: &str) -> Result<DateTime<Utc>, Failure> {
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(InternalError::new)?;
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| InternalError::new("invalid date"))?;
    Ok(naive.and_utc())
}

#[derive(Default)]
pub struct TukuiResolver {
    /// Overridable only in tests (mockito needs a local base URL); the real
    /// resolver always targets the actual Tukui API.
    #[cfg(test)]
    api_url: Option<String>,
}

impl TukuiResolver {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn new_with_api_url(api_url: String) -> Self {
        Self {
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
impl Resolver for TukuiResolver {
    fn metadata(&self) -> SourceMetadata {
        SourceMetadata {
            id: "tukui",
            name: "Tukui",
            // No strategies supported: always "whatever the API currently reports."
            strategies: &[],
            changelog_format: ChangelogFormat::Markdown,
            addon_toc_key: Some("X-Tukui-ProjectID"),
        }
    }

    async fn resolve_one_impl(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        let url = format!("{}/addon/{}", self.api_base(), defn.alias);
        let response = http.get(&url, &[]).await?;
        if response.status == 404 {
            return Err(ManagerError::PkgNonexistent.into());
        }
        if !(200..300).contains(&response.status) {
            return Err(InternalError::new(format!("HTTP {} for {url}", response.status)).into());
        }
        let ui: TukuiAddon = serde_json::from_slice(&response.body)?;

        let matches_flavour = ui
            .patch
            .iter()
            .any(|p| Flavour::from_version_string(p) == Some(flavour));
        if !matches_flavour {
            return Err(ManagerError::PkgFilesNotMatching {
                strategies: defn.strategies.clone(),
            }
            .into());
        }

        let date_published = parse_date_only(&ui.last_update)?;

        // The changelog URL isn't versioned upstream; add a fragment purely
        // so the changelog cache key varies per version.
        let mut changelog_url = Url::parse(&ui.changelog_url).map_err(InternalError::new)?;
        changelog_url.set_fragment(Some(&ui.version));

        Ok(PkgCandidate {
            id: ui.id.to_string(),
            slug: ui.slug,
            name: ui.name,
            description: ui.small_desc,
            url: ui.web_url,
            download_url: ui.url,
            date_published,
            version: ui.version,
            changelog_url: changelog_url.to_string(),
            deps: Vec::new(),
        })
    }
}
