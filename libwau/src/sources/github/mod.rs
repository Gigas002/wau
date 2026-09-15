//! GitHub source — ports instawow's `_sources/github.py`.
//!
//! The asset-selection heuristic here is simplified in one way for
//! tractability: when release.json is absent and the flavour can't be
//! determined from a `.toc` filename suffix alone, instawow does a second
//! *targeted* ranged read for just the main `.toc` file's bytes (using the
//! zip central directory's recorded header offsets) to inspect `##
//! Interface:`. This port instead falls back to a full asset download at
//! that point. Both paths inspect the exact same bytes and make the exact
//! same match/no-match decision — the simplification only trades bandwidth
//! for implementation complexity in the (rare, for typical addon-sized
//! zips) case where the initial 25 KB tail read isn't already the whole
//! archive.

#[cfg(test)]
mod tests;

use std::{io::Cursor, time::Duration};

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

const API_URL: &str = "https://api.github.com";

// ============================================================================
// API response shapes
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
struct GhRepo {
    id: u64,
    full_name: String,
    name: String,
    description: Option<String>,
    html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GhRelease {
    tag_name: String,
    published_at: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
    #[serde(default)]
    body: String,
    draft: bool,
    prerelease: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GhAsset {
    url: String,
    name: String,
    content_type: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct PackagerReleaseJson {
    releases: Vec<PackagerRelease>,
}

#[derive(Debug, Deserialize)]
struct PackagerRelease {
    filename: String,
    nolib: bool,
    metadata: Vec<PackagerMetadata>,
}

#[derive(Debug, Deserialize)]
struct PackagerMetadata {
    flavor: String,
    interface: u32,
}

/// `_PackagerReleaseJsonFlavor`.
fn packager_flavor(flavour: Flavour) -> &'static str {
    match flavour {
        Flavour::Mainline => "mainline",
        Flavour::VanillaClassic => "classic",
        Flavour::TbcClassic => "bcc",
        Flavour::WrathClassic => "wrath",
        Flavour::TitanClassic => "titan",
        Flavour::CataClassic => "cata",
        Flavour::MistsClassic => "mists",
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

/// `{sep}{suffix}.toc` (lowercased) for every TOC suffix a flavour accepts —
/// `matchers.NORMALISED_FLAVOUR_TOC_EXTENSIONS`'s entry for one flavour,
/// derived here from `Flavour::toc_suffixes` (built in an earlier phase)
/// rather than waiting on the `matchers` module.
fn normalised_toc_extensions(flavour: Flavour) -> Vec<String> {
    flavour
        .toc_suffixes()
        .iter()
        .flat_map(|suffix| ['-', '_'].map(|sep| format!("{sep}{suffix}.toc").to_lowercase()))
        .collect()
}

fn all_toc_extensions() -> Vec<String> {
    Flavour::ALL
        .iter()
        .flat_map(|f| normalised_toc_extensions(*f))
        .collect()
}

fn content_range_total(headers: &[(String, String)]) -> Option<u64> {
    let (_, value) = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-range"))?;
    value.rsplit('/').next()?.trim().parse().ok()
}

// ============================================================================
// Resolver
// ============================================================================

pub struct GitHubResolver {
    token: Option<SecretString>,
    /// Overridable only in tests (mockito needs a local base URL); the real
    /// resolver always targets the actual GitHub API.
    #[cfg(test)]
    api_url: String,
}

impl GitHubResolver {
    pub fn new(token: Option<SecretString>) -> Self {
        Self {
            token,
            #[cfg(test)]
            api_url: API_URL.to_owned(),
        }
    }

    #[cfg(test)]
    fn new_with_api_url(token: Option<SecretString>, api_url: String) -> Self {
        Self { token, api_url }
    }

    fn api_base(&self) -> &str {
        #[cfg(test)]
        return &self.api_url;
        #[cfg(not(test))]
        return API_URL;
    }

    fn headers(&self, intent: HeadersIntent) -> Vec<(String, String)> {
        Resolver::make_request_headers(self, intent)
    }

    /// Ranged tail fetch (`bytes=-25000`) for the archive's central
    /// directory, falling back to a full download when the server doesn't
    /// honour the range or the tail alone doesn't parse as a valid zip.
    /// Returns `(bytes, is_complete)`.
    async fn fetch_tail_or_full(
        &self,
        http: &HttpClient,
        headers: &[(&str, &str)],
        url: &str,
    ) -> AnyOutcome<Option<(Vec<u8>, bool)>> {
        let mut range_headers = headers.to_vec();
        range_headers.push(("Range", "bytes=-25000"));

        let response = http.get(url, &range_headers, CacheTtl::Indefinite).await?;

        if response.status == 200 {
            return Ok(Some((response.body, true)));
        }

        if response.status == 206
            && let Some(total) = content_range_total(&response.headers)
        {
            let start = (total as usize).saturating_sub(response.body.len());
            let mut buffer = vec![0u8; total as usize];
            let end = (start + response.body.len()).min(buffer.len());
            buffer[start..end].copy_from_slice(&response.body[..end - start]);
            if zip::ZipArchive::new(Cursor::new(buffer.clone())).is_ok() {
                return Ok(Some((buffer, false)));
            }
        }

        // 416/501 (GitHub mislabels out-of-range as 501) or an unparsable
        // partial read: fall back to a full download.
        let full = http.get(url, headers, CacheTtl::Indefinite).await?;
        if !(200..300).contains(&full.status) {
            return Ok(None);
        }
        Ok(Some((full.body, true)))
    }

    async fn find_match_from_zip_contents(
        &self,
        http: &HttpClient,
        assets: &[GhAsset],
        desired_flavour: Option<Flavour>,
    ) -> AnyOutcome<Option<GhAsset>> {
        let candidates: Vec<&GhAsset> = assets
            .iter()
            .filter(|a| {
                a.state == "uploaded"
                    && matches!(
                        a.content_type.as_str(),
                        "application/zip" | "application/x-zip-compressed"
                    )
                    && a.name.ends_with(".zip")
                    && !a.name.contains("-nolib")
            })
            .collect();
        if candidates.is_empty() {
            return Ok(None);
        }

        let download_headers = self.headers(HeadersIntent::Download);
        let download_headers: Vec<(&str, &str)> = download_headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let all_extensions = all_toc_extensions();
        let desired_extensions = match desired_flavour {
            Some(f) => normalised_toc_extensions(f),
            None => all_extensions.clone(),
        };

        for candidate in candidates {
            let Some((bytes, is_complete)) = self
                .fetch_tail_or_full(http, &download_headers, &candidate.url)
                .await?
            else {
                continue;
            };

            let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
                continue;
            };
            let names: Vec<String> = archive.file_names().map(str::to_owned).collect();

            let toc_filenames: Vec<String> =
                crate::pkg_archives::find_archive_addon_tocs(names.iter().map(String::as_str))
                    .into_iter()
                    .filter(|(_, head)| candidate.name.contains(head.as_str()))
                    .map(|(name, _)| name)
                    .collect();
            if toc_filenames.is_empty() {
                continue;
            }

            if desired_flavour.is_none() {
                return Ok(Some(candidate.clone()));
            }

            let matches_by_filename = toc_filenames.iter().any(|n| {
                let lower = n.to_lowercase();
                desired_extensions
                    .iter()
                    .any(|ext| lower.ends_with(ext.as_str()))
            });
            if matches_by_filename {
                return Ok(Some(candidate.clone()));
            }

            let Some(main_toc) = toc_filenames
                .iter()
                .filter(|n| {
                    let lower = n.to_lowercase();
                    !all_extensions
                        .iter()
                        .any(|ext| lower.ends_with(ext.as_str()))
                })
                .min_by_key(|n| n.len())
                .cloned()
            else {
                continue;
            };

            let toc_bytes: Vec<u8> = if is_complete {
                let Ok(mut entry) = archive.by_name(&main_toc) else {
                    continue;
                };
                let mut buf = Vec::new();
                if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
                    continue;
                }
                buf
            } else {
                // Need the file's actual content and don't have the whole
                // zip yet — fall back to a full download (see module docs).
                let Some((full_bytes, _)) = self
                    .fetch_tail_or_full_force_complete(http, &download_headers, &candidate.url)
                    .await?
                else {
                    continue;
                };
                let Ok(mut full_archive) = zip::ZipArchive::new(Cursor::new(full_bytes)) else {
                    continue;
                };
                let Ok(mut entry) = full_archive.by_name(&main_toc) else {
                    continue;
                };
                let mut buf = Vec::new();
                if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
                    continue;
                }
                buf
            };

            let toc = crate::toc::parse_str(&String::from_utf8_lossy(&toc_bytes));
            let matches_by_interface = toc
                .interface
                .iter()
                .any(|i| desired_flavour == Flavour::from_build_number(*i));
            if matches_by_interface {
                return Ok(Some(candidate.clone()));
            }
        }

        Ok(None)
    }

    async fn fetch_tail_or_full_force_complete(
        &self,
        http: &HttpClient,
        headers: &[(&str, &str)],
        url: &str,
    ) -> AnyOutcome<Option<(Vec<u8>, bool)>> {
        let full = http.get(url, headers, CacheTtl::Indefinite).await?;
        if !(200..300).contains(&full.status) {
            return Ok(None);
        }
        Ok(Some((full.body, true)))
    }

    async fn find_match_from_release_json(
        &self,
        http: &HttpClient,
        assets: &[GhAsset],
        release_json_asset: &GhAsset,
        desired_flavour: Option<Flavour>,
    ) -> AnyOutcome<Option<GhAsset>> {
        let download_headers = self.headers(HeadersIntent::Download);
        let download_headers: Vec<(&str, &str)> = download_headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let response = http
            .get(
                &release_json_asset.url,
                &download_headers,
                CacheTtl::For(Duration::from_secs(86_400)),
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(InternalError::new(format!(
                "HTTP {} for {}",
                response.status, release_json_asset.url
            ))
            .into());
        }
        let packager: PackagerReleaseJson = serde_json::from_slice(&response.body)?;
        if packager.releases.is_empty() {
            return Ok(None);
        }

        let is_compatible = |release: &PackagerRelease| -> bool {
            match desired_flavour {
                Some(flavour) => {
                    let want = packager_flavor(flavour);
                    release.metadata.iter().any(|m| {
                        m.flavor == want && Flavour::from_build_number(m.interface) == Some(flavour)
                    })
                }
                None => true,
            }
        };

        let Some(matching_release) = packager
            .releases
            .iter()
            .find(|r| !r.nolib && is_compatible(r))
        else {
            return Ok(None);
        };

        Ok(assets
            .iter()
            .find(|a| a.name == matching_release.filename && a.state == "uploaded")
            .cloned())
    }

    async fn find_match(
        &self,
        http: &HttpClient,
        release: &GhRelease,
        desired_flavours: &[Option<Flavour>],
    ) -> AnyOutcome<Option<GhAsset>> {
        let release_json_asset = release
            .assets
            .iter()
            .find(|a| a.name == "release.json" && a.state == "uploaded");

        for &desired in desired_flavours {
            let matched = match release_json_asset {
                Some(rj) => {
                    self.find_match_from_release_json(http, &release.assets, rj, desired)
                        .await?
                }
                None => {
                    self.find_match_from_zip_contents(http, &release.assets, desired)
                        .await?
                }
            };
            if matched.is_some() {
                return Ok(matched);
            }
        }
        Ok(None)
    }
}

#[async_trait::async_trait]
impl Resolver for GitHubResolver {
    fn metadata(&self) -> SourceMetadata {
        SourceMetadata {
            id: "github",
            name: "GitHub",
            strategies: &[
                Strategy::AnyFlavour,
                Strategy::AnyReleaseType,
                Strategy::VersionEq,
            ],
            changelog_format: ChangelogFormat::Markdown,
            addon_toc_key: None,
        }
    }

    fn get_alias_from_url(&self, url: &str) -> Option<String> {
        let parsed = Url::parse(url).ok()?;
        if parsed.host_str() != Some("github.com") {
            return None;
        }
        let segments: Vec<&str> = parsed.path_segments()?.collect();
        if segments.len() >= 2 {
            Some(format!("{}/{}", segments[0], segments[1]))
        } else {
            None
        }
    }

    fn make_request_headers(&self, intent: HeadersIntent) -> Vec<(String, String)> {
        let mut headers = vec![("X-GitHub-Api-Version".to_owned(), "2022-11-28".to_owned())];
        let accept = if intent == HeadersIntent::Download {
            "application/octet-stream"
        } else {
            "application/vnd.github+json"
        };
        headers.push(("Accept".to_owned(), accept.to_owned()));
        if let Some(token) = &self.token {
            headers.push((
                "Authorization".to_owned(),
                format!("token {}", token.expose()),
            ));
        }
        headers
    }

    async fn resolve_one_impl(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        let headers = self.headers(HeadersIntent::Fetch);
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let id_or_alias = defn.id.clone().unwrap_or_else(|| defn.alias.clone());
        let repo_url = if is_numeric(&id_or_alias) {
            format!("{}/repositories/{id_or_alias}", self.api_base())
        } else {
            format!("{}/repos/{}", self.api_base(), defn.alias)
        };

        let project_response = http
            .get(
                &repo_url,
                &headers,
                CacheTtl::For(Duration::from_secs(3600)),
            )
            .await?;
        if project_response.status == 404 {
            return Err(ManagerError::PkgNonexistent.into());
        }
        if !(200..300).contains(&project_response.status) {
            return Err(InternalError::new(format!(
                "HTTP {} for {repo_url}",
                project_response.status
            ))
            .into());
        }
        let project: GhRepo = serde_json::from_slice(&project_response.body)?;

        let version_eq = defn.strategies.version_eq.as_deref();
        let release_url = match version_eq {
            Some(v) => format!("{repo_url}/releases/tags/{v}"),
            None => format!("{repo_url}/releases?per_page=10"),
        };

        let releases_response = http
            .get(
                &release_url,
                &headers,
                CacheTtl::For(Duration::from_secs(5 * 60)),
            )
            .await?;
        if releases_response.status == 404 {
            return Err(ManagerError::PkgFilesMissing {
                reason: "no releases found".to_owned(),
            }
            .into());
        }
        if !(200..300).contains(&releases_response.status) {
            return Err(InternalError::new(format!(
                "HTTP {} for {release_url}",
                releases_response.status
            ))
            .into());
        }

        let json: serde_json::Value = serde_json::from_slice(&releases_response.body)?;
        let mut releases: Vec<GhRelease> = if json.is_array() {
            serde_json::from_value(json)?
        } else {
            vec![serde_json::from_value(json)?]
        };

        // Only users with push access get draft releases, but filter just in case.
        releases.retain(|r| !r.draft);

        // Allow pre-releases only if no stable releases exist or explicitly opted into.
        if !defn.strategies.any_release_type && releases.iter().any(|r| !r.prerelease) {
            releases.retain(|r| !r.prerelease);
        }

        if releases.is_empty() {
            return Err(ManagerError::PkgFilesNotMatching {
                strategies: defn.strategies.clone(),
            }
            .into());
        }

        let mut desired_flavours = vec![Some(flavour)];
        if defn.strategies.any_flavour {
            desired_flavours.push(None);
        }

        let mut found: Option<(GhRelease, GhAsset)> = None;
        for release in &releases {
            if let Some(asset) = self.find_match(http, release, &desired_flavours).await? {
                found = Some((release.clone(), asset));
                break;
            }
        }

        let Some((release, asset)) = found else {
            return Err(ManagerError::PkgFilesNotMatching {
                strategies: defn.strategies.clone(),
            }
            .into());
        };

        let date_published = parse_iso_datetime(&release.published_at)?;

        Ok(PkgCandidate {
            id: project.id.to_string(),
            slug: project.full_name.to_lowercase(),
            name: project.name,
            description: project.description.unwrap_or_default(),
            url: project.html_url,
            download_url: asset.url,
            date_published,
            version: release.tag_name,
            changelog_url: format!("data:,{}", super::percent_encode(&release.body)),
            deps: Vec::new(),
        })
    }
}
