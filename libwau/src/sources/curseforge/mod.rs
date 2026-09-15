//! CurseForge source.

#[cfg(test)]
mod tests;

use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    config::SecretString,
    http::{CacheTtl, HttpClient},
    model::{ChangelogFormat, Defn, Flavour, HeadersIntent, SourceMetadata, Strategy},
    results::{AnyOutcome, Failure, InternalError, ManagerError},
    sources::{PkgCandidate, Resolver},
};

const DEFAULT_API_URL: &str = "https://api.curseforge.com/v1/mods";
const WOW_GAME_ID: u32 = 1;
const VERSION_SEP: char = '_';
const RELEASE_TYPE_RELEASE: u8 = 1;
const RELATION_TYPE_REQUIRED_DEPENDENCY: u8 = 3;

// ============================================================================
// API response shapes (only the fields this port actually uses)
// ============================================================================

#[derive(Debug, Deserialize)]
struct DataResponse<T> {
    data: T,
}

#[derive(Debug, Serialize)]
struct ModIdsRequest<'a> {
    #[serde(rename = "modIds")]
    mod_ids: &'a [u64],
}

#[derive(Debug, Clone, Deserialize)]
struct CfMod {
    id: u64,
    slug: String,
    name: String,
    summary: String,
    links: CfModLinks,
    #[serde(rename = "latestFiles", default)]
    latest_files: Vec<CfFile>,
    #[serde(rename = "allowModDistribution")]
    allow_mod_distribution: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct CfModLinks {
    #[serde(rename = "websiteUrl")]
    website_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CfFile {
    id: u64,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "releaseType")]
    release_type: u8,
    #[serde(rename = "fileDate")]
    file_date: String,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "sortableGameVersions", default)]
    sortable_game_versions: Vec<CfSortableGameVersion>,
    #[serde(default)]
    dependencies: Vec<CfFileDependency>,
    #[serde(rename = "exposeAsAlternative", default)]
    expose_as_alternative: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct CfSortableGameVersion {
    #[serde(rename = "gameVersionTypeId")]
    game_version_type_id: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct CfFileDependency {
    #[serde(rename = "modId")]
    mod_id: u64,
    #[serde(rename = "relationType")]
    relation_type: u8,
}

/// `_CfCoreSortableGameVersionTypeId`, extracted from
/// `https://api.curseforge.com/v1/games/1/version-types`.
fn flavour_type_id(flavour: Flavour) -> u32 {
    match flavour {
        Flavour::Mainline => 517,
        Flavour::VanillaClassic => 67408,
        Flavour::TbcClassic => 73246,
        Flavour::WrathClassic => 73713,
        Flavour::TitanClassic => 81212,
        Flavour::CataClassic => 77522,
        Flavour::MistsClassic => 79434,
    }
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn parse_iso_datetime(s: &str) -> Result<DateTime<Utc>, Failure> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| InternalError::new(e).into())
}

// ============================================================================
// Resolver
// ============================================================================

pub struct CurseForgeResolver {
    api_key: Option<SecretString>,
    /// Self-hosted proxy override; when set, the API key isn't required (the
    /// proxy is assumed to handle auth).
    api_url: Option<String>,
}

impl CurseForgeResolver {
    pub fn new(api_key: Option<SecretString>, api_url: Option<String>) -> Self {
        Self { api_key, api_url }
    }

    fn mod_api_url(&self) -> &str {
        self.api_url.as_deref().unwrap_or(DEFAULT_API_URL)
    }

    fn headers_vec(&self, intent: HeadersIntent) -> Vec<(String, String)> {
        Resolver::make_request_headers(self, intent)
    }

    async fn get_mod(&self, http: &HttpClient, defn: &Defn) -> AnyOutcome<CfMod> {
        let headers = self.headers_vec(HeadersIntent::Fetch);
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        if is_numeric(&defn.alias) {
            let url = format!("{}/{}", self.mod_api_url(), defn.alias);
            let response = http
                .get(&url, &headers, CacheTtl::For(Duration::from_secs(15 * 60)))
                .await?;
            if response.status == 404 {
                return Err(ManagerError::PkgNonexistent.into());
            }
            if !(200..300).contains(&response.status) {
                return Err(
                    InternalError::new(format!("HTTP {} for {url}", response.status)).into(),
                );
            }
            let parsed: DataResponse<CfMod> = serde_json::from_slice(&response.body)?;
            Ok(parsed.data)
        } else {
            let mut url = Url::parse(&format!("{}/search", self.mod_api_url()))
                .map_err(InternalError::new)?;
            url.query_pairs_mut()
                .append_pair("gameId", &WOW_GAME_ID.to_string())
                .append_pair("slug", &defn.alias);
            let response = http
                .get(
                    url.as_str(),
                    &headers,
                    CacheTtl::For(Duration::from_secs(15 * 60)),
                )
                .await?;
            if !(200..300).contains(&response.status) {
                return Err(
                    InternalError::new(format!("HTTP {} for {url}", response.status)).into(),
                );
            }
            let parsed: DataResponse<Vec<CfMod>> = serde_json::from_slice(&response.body)?;
            match parsed.data.as_slice() {
                [one] => Ok(one.clone()),
                _ => Err(ManagerError::PkgNonexistent.into()),
            }
        }
    }

    async fn get_files(
        &self,
        http: &HttpClient,
        defn: &Defn,
        metadata: &CfMod,
    ) -> AnyOutcome<Vec<CfFile>> {
        let headers = self.headers_vec(HeadersIntent::Fetch);
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let version_eq = defn.strategies.version_eq.as_deref().unwrap_or_default();

        let mut files_url = format!("{}/{}/files", self.mod_api_url(), metadata.id);
        let mut fetched_single = false;

        if let Some((_, file_id_str)) = version_eq.rsplit_once(VERSION_SEP)
            && !file_id_str.is_empty()
            && let Ok(file_id) = file_id_str.parse::<u64>()
        {
            if let Some(file) = metadata.latest_files.iter().find(|f| f.id == file_id) {
                return Ok(vec![file.clone()]);
            }
            files_url = format!("{files_url}/{file_id}");
            fetched_single = true;
        }

        let response = http
            .get(
                &files_url,
                &headers,
                CacheTtl::For(Duration::from_secs(3600)),
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(
                InternalError::new(format!("HTTP {} for {files_url}", response.status)).into(),
            );
        }

        if fetched_single {
            let parsed: DataResponse<CfFile> = serde_json::from_slice(&response.body)?;
            Ok(vec![parsed.data])
        } else {
            let parsed: DataResponse<Vec<CfFile>> = serde_json::from_slice(&response.body)?;
            Ok(parsed
                .data
                .into_iter()
                .filter(|f| f.display_name == version_eq)
                .take(1)
                .collect())
        }
    }

    async fn fetch_mods_by_id(
        &self,
        http: &HttpClient,
        numeric_ids: &[String],
    ) -> AnyOutcome<HashMap<String, CfMod>> {
        let headers = self.headers_vec(HeadersIntent::Fetch);
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let ids: Vec<u64> = numeric_ids.iter().filter_map(|s| s.parse().ok()).collect();
        let body = serde_json::to_vec(&ModIdsRequest { mod_ids: &ids }).unwrap_or_default();

        let response = http
            .post(
                self.mod_api_url(),
                &headers,
                body,
                CacheTtl::For(Duration::from_secs(5 * 60)),
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(InternalError::new(format!(
                "HTTP {} for {}",
                response.status,
                self.mod_api_url()
            ))
            .into());
        }
        let parsed: DataResponse<Vec<CfMod>> = serde_json::from_slice(&response.body)?;
        Ok(parsed
            .data
            .into_iter()
            .map(|m| (m.id.to_string(), m))
            .collect())
    }

    async fn resolve_checked(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
        metadata: Option<CfMod>,
    ) -> AnyOutcome<PkgCandidate> {
        let unsupported = super::extraneous_strategies(defn, Resolver::metadata(self).strategies);
        if !unsupported.is_empty() {
            return Err(ManagerError::PkgStrategiesUnsupported {
                strategies: unsupported,
            }
            .into());
        }
        self.resolve_one_with_metadata(http, flavour, defn, metadata)
            .await
    }

    async fn resolve_one_with_metadata(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
        metadata: Option<CfMod>,
    ) -> AnyOutcome<PkgCandidate> {
        let metadata = match metadata {
            Some(m) => m,
            None => self.get_mod(http, defn).await?,
        };

        let mut files = if defn.strategies.version_eq.is_some() {
            self.get_files(http, defn, &metadata).await?
        } else {
            metadata.latest_files.clone()
        };

        if files.is_empty() {
            return Err(ManagerError::files_missing().into());
        }

        files.retain(|f| !f.expose_as_alternative.unwrap_or(false));

        // Allow pre-releases only if no stable releases exist or explicitly opted into.
        if !defn.strategies.any_release_type
            && files.iter().any(|f| f.release_type == RELEASE_TYPE_RELEASE)
        {
            files.retain(|f| f.release_type == RELEASE_TYPE_RELEASE);
        }

        let mut desired_flavours = vec![Some(flavour)];
        if defn.strategies.any_flavour {
            desired_flavours.push(None);
        }

        let mut chosen: Option<&CfFile> = None;
        for desired in &desired_flavours {
            let shortlisted: Vec<&CfFile> = match desired {
                Some(f) => {
                    let type_id = flavour_type_id(*f);
                    files
                        .iter()
                        .filter(|file| {
                            file.sortable_game_versions
                                .iter()
                                .any(|s| s.game_version_type_id == type_id)
                        })
                        .collect()
                }
                None => files.iter().collect(),
            };
            // The `id` is just a counter, so max-by-id picks "latest" without dates.
            if let Some(file) = shortlisted.into_iter().max_by_key(|f| f.id) {
                chosen = Some(file);
                break;
            }
        }

        let Some(file) = chosen else {
            return Err(ManagerError::PkgFilesNotMatching {
                strategies: defn.strategies.clone(),
            }
            .into());
        };

        let Some(download_url) = &file.download_url else {
            let reason = if metadata.allow_mod_distribution == Some(false) {
                "package distribution is forbidden".to_owned()
            } else {
                "no files are available for download".to_owned()
            };
            return Err(ManagerError::PkgFilesMissing { reason }.into());
        };

        let date_published = parse_iso_datetime(&file.file_date)?;

        Ok(PkgCandidate {
            id: metadata.id.to_string(),
            slug: metadata.slug.clone(),
            name: metadata.name.clone(),
            description: metadata.summary.clone(),
            url: metadata.links.website_url.clone(),
            download_url: download_url.clone(),
            date_published,
            version: format!("{}{VERSION_SEP}{}", file.display_name, file.id),
            changelog_url: format!(
                "{}/{}/files/{}/changelog",
                self.mod_api_url(),
                metadata.id,
                file.id
            ),
            deps: file
                .dependencies
                .iter()
                .filter(|d| d.relation_type == RELATION_TYPE_REQUIRED_DEPENDENCY)
                .map(|d| d.mod_id.to_string())
                .collect(),
        })
    }
}

#[async_trait::async_trait]
impl Resolver for CurseForgeResolver {
    fn metadata(&self) -> SourceMetadata {
        SourceMetadata {
            id: "curse",
            name: "CurseForge",
            strategies: &[
                Strategy::AnyFlavour,
                Strategy::AnyReleaseType,
                Strategy::VersionEq,
            ],
            changelog_format: ChangelogFormat::Html,
            addon_toc_key: Some("X-Curse-Project-ID"),
        }
    }

    fn get_disabled_reason(&self) -> Option<String> {
        if self.api_url.is_none() && self.api_key.is_none() {
            Some("access token missing".to_owned())
        } else {
            None
        }
    }

    fn get_alias_from_url(&self, url: &str) -> Option<String> {
        let parsed = Url::parse(url).ok()?;
        if parsed.host_str() != Some("www.curseforge.com") {
            return None;
        }
        let segments: Vec<&str> = parsed.path_segments()?.collect();
        if segments.len() > 2 && segments[0] == "wow" && segments[1] == "addons" {
            Some(segments[2].to_lowercase())
        } else {
            None
        }
    }

    fn make_request_headers(&self, intent: HeadersIntent) -> Vec<(String, String)> {
        if self.api_url.is_none()
            && intent != HeadersIntent::Download
            && let Some(token) = &self.api_key
        {
            return vec![("x-api-key".to_owned(), token.expose().to_owned())];
        }
        Vec::new()
    }

    async fn resolve_one_impl(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        self.resolve_one_with_metadata(http, flavour, defn, None)
            .await
    }

    /// Batches numeric-id lookups into one `POST /mods` request. A batch
    /// failure applies the same error to every `Defn` in the batch, since a
    /// single failed request doesn't distinguish which id(s) caused it.
    async fn resolve(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defns: &[Defn],
    ) -> Vec<AnyOutcome<PkgCandidate>> {
        let ids: Vec<String> = defns
            .iter()
            .map(|d| d.id.clone().unwrap_or_else(|| d.alias.clone()))
            .collect();
        let mut numeric_ids: Vec<String> = ids.iter().filter(|i| is_numeric(i)).cloned().collect();
        numeric_ids.sort_unstable();
        numeric_ids.dedup();

        if numeric_ids.is_empty() {
            let futures = defns
                .iter()
                .map(|d| self.resolve_checked(http, flavour, d, None));
            return futures_util::future::join_all(futures).await;
        }

        match self.fetch_mods_by_id(http, &numeric_ids).await {
            Ok(addons_by_id) => {
                let futures = defns.iter().zip(ids.iter()).map(|(d, id)| {
                    let metadata = addons_by_id.get(id).cloned();
                    self.resolve_checked(http, flavour, d, metadata)
                });
                futures_util::future::join_all(futures).await
            }
            Err(err) => defns.iter().map(|_| Err(err.clone())).collect(),
        }
    }

    async fn get_changelog(&self, http: &HttpClient, url: &str) -> Result<String, Failure> {
        let headers = self.headers_vec(HeadersIntent::Fetch);
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let response = http.get(url, &headers, CacheTtl::Indefinite).await?;
        if !(200..300).contains(&response.status) {
            return Err(InternalError::new(format!("HTTP {} for {url}", response.status)).into());
        }
        let parsed: DataResponse<String> = serde_json::from_slice(&response.body)?;
        Ok(parsed.data)
    }
}
