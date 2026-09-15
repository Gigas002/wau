//! WoWInterface source — ports instawow's `_sources/wowi.py`.
//!
//! Documented upstream limitation, preserved as-is (not "fixed"): the
//! details API can't express classic-specific files for multi-file addons,
//! and the download link always points at the retail build regardless of
//! flavour — "the WoWI API is only good for installing retail add-ons."

#[cfg(test)]
mod tests;

use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use url::Url;

use crate::{
    http::{CacheTtl, HttpClient},
    model::{ChangelogFormat, Defn, Flavour, SourceMetadata},
    results::{AnyOutcome, Failure, InternalError, ManagerError},
    sources::{PkgCandidate, Resolver},
};

const DEFAULT_DETAILS_API_URL: &str = "https://api.mmoui.com/v3/game/WOW/filedetails";

#[derive(Debug, Clone, Deserialize)]
struct WowiDetailsItem {
    #[serde(rename = "UID")]
    uid: String,
    #[serde(rename = "UIName")]
    ui_name: String,
    #[serde(rename = "UIVersion")]
    ui_version: String,
    #[serde(rename = "UIDate")]
    ui_date: i64,
    #[serde(rename = "UIDownload")]
    ui_download: String,
    #[serde(rename = "UIPending")]
    ui_pending: String,
    #[serde(rename = "UIDescription")]
    ui_description: String,
    #[serde(rename = "UIChangeLog")]
    ui_changelog: String,
}

fn timestamp_to_datetime(millis: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(millis)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
}

/// instawow's `normalise_names('-')`: casefold and replace runs of
/// non-alphanumerics with a single separator.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_sep = true; // avoid a leading separator
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('-');
            last_was_sep = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

fn take_while_digits(s: &str) -> String {
    s.chars().take_while(|c| c.is_ascii_digit()).collect()
}

#[derive(Default)]
pub struct WowInterfaceResolver {
    #[cfg(test)]
    details_api_url: Option<String>,
}

impl WowInterfaceResolver {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn new_with_api_url(details_api_url: String) -> Self {
        Self {
            details_api_url: Some(details_api_url),
        }
    }

    fn details_api_base(&self) -> &str {
        #[cfg(test)]
        return self
            .details_api_url
            .as_deref()
            .unwrap_or(DEFAULT_DETAILS_API_URL);
        #[cfg(not(test))]
        return DEFAULT_DETAILS_API_URL;
    }

    async fn resolve_checked(
        &self,
        defn: &Defn,
        metadata: Option<WowiDetailsItem>,
    ) -> AnyOutcome<PkgCandidate> {
        let unsupported = super::extraneous_strategies(defn, Resolver::metadata(self).strategies);
        if !unsupported.is_empty() {
            return Err(ManagerError::PkgStrategiesUnsupported {
                strategies: unsupported,
            }
            .into());
        }
        resolve_with_metadata(metadata)
    }
}

fn resolve_with_metadata(metadata: Option<WowiDetailsItem>) -> AnyOutcome<PkgCandidate> {
    let Some(meta) = metadata else {
        return Err(ManagerError::PkgNonexistent.into());
    };
    if meta.ui_pending == "1" {
        return Err(ManagerError::PkgFilesMissing {
            reason: "file awaiting approval".to_owned(),
        }
        .into());
    }

    Ok(PkgCandidate {
        id: meta.uid.clone(),
        slug: slugify(&format!("{} {}", meta.uid, meta.ui_name)),
        name: meta.ui_name,
        description: meta.ui_description,
        url: format!("https://www.wowinterface.com/downloads/info{}", meta.uid),
        download_url: meta.ui_download,
        date_published: timestamp_to_datetime(meta.ui_date),
        version: meta.ui_version,
        changelog_url: format!("data:,{}", super::percent_encode(&meta.ui_changelog)),
        deps: Vec::new(),
    })
}

#[async_trait::async_trait]
impl Resolver for WowInterfaceResolver {
    fn metadata(&self) -> SourceMetadata {
        SourceMetadata {
            id: "wowi",
            name: "WoWInterface",
            strategies: &[],
            changelog_format: ChangelogFormat::Raw,
            addon_toc_key: Some("X-WoWI-ID"),
        }
    }

    fn get_alias_from_url(&self, url: &str) -> Option<String> {
        let parsed = Url::parse(url).ok()?;
        let host = parsed.host_str()?;
        if host != "wowinterface.com" && host != "www.wowinterface.com" {
            return None;
        }
        let segments: Vec<&str> = parsed.path_segments()?.collect();
        if segments.len() != 2 || segments[0] != "downloads" {
            return None;
        }
        let name = segments[1];
        if name == "landing.php" {
            return parsed
                .query_pairs()
                .find(|(k, _)| k == "fileid")
                .map(|(_, v)| v.into_owned());
        }
        if name == "fileinfo.php" {
            return parsed
                .query_pairs()
                .find(|(k, _)| k == "id")
                .map(|(_, v)| v.into_owned());
        }
        for prefix in ["download", "info"] {
            if let Some(rest) = name.strip_prefix(prefix) {
                let digits = take_while_digits(rest);
                if !digits.is_empty() {
                    return Some(digits);
                }
            }
        }
        None
    }

    async fn resolve_one_impl(
        &self,
        http: &HttpClient,
        flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        // `resolve_one` is only ever meaningfully driven via the batched
        // `resolve()` in instawow (a direct call with no pre-fetched metadata
        // always raises `PkgNonexistent` there); routing through our own
        // `resolve()` override here achieves the same real-world result
        // while still resolving a genuine single item correctly.
        let results =
            <Self as Resolver>::resolve(self, http, flavour, std::slice::from_ref(defn)).await;
        results
            .into_iter()
            .next()
            .unwrap_or_else(|| Err(ManagerError::PkgNonexistent.into()))
    }

    async fn resolve(
        &self,
        http: &HttpClient,
        _flavour: Flavour,
        defns: &[Defn],
    ) -> Vec<AnyOutcome<PkgCandidate>> {
        let ids: Vec<String> = defns.iter().map(|d| take_while_digits(&d.alias)).collect();
        let mut unique_ids: Vec<String> = ids.iter().filter(|i| !i.is_empty()).cloned().collect();
        unique_ids.sort_unstable();
        unique_ids.dedup();

        if unique_ids.is_empty() {
            let futures = defns.iter().map(|d| self.resolve_checked(d, None));
            return futures_util::future::join_all(futures).await;
        }

        let url = format!("{}/{}.json", self.details_api_base(), unique_ids.join(","));
        let response = match http
            .get(&url, &[], CacheTtl::For(Duration::from_secs(15 * 60)))
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let err: Failure = e.into();
                return defns.iter().map(|_| Err(err.clone())).collect();
            }
        };

        if response.status == 404 {
            let futures = defns.iter().map(|d| self.resolve_checked(d, None));
            return futures_util::future::join_all(futures).await;
        }
        if !(200..300).contains(&response.status) {
            let err: Failure =
                InternalError::new(format!("HTTP {} for {url}", response.status)).into();
            return defns.iter().map(|_| Err(err.clone())).collect();
        }

        let items: Vec<WowiDetailsItem> = match serde_json::from_slice(&response.body) {
            Ok(v) => v,
            Err(e) => {
                let err: Failure = e.into();
                return defns.iter().map(|_| Err(err.clone())).collect();
            }
        };
        let by_uid: HashMap<String, WowiDetailsItem> =
            items.into_iter().map(|i| (i.uid.clone(), i)).collect();

        let futures = defns.iter().zip(ids.iter()).map(|(d, id)| {
            let metadata = by_uid.get(id).cloned();
            self.resolve_checked(d, metadata)
        });
        futures_util::future::join_all(futures).await
    }
}
