//! Aggregate addon catalogue: fetches the published catalogue data and
//! computes derived per-entry fields (name normalisation, popularity,
//! cross-source `same_as` links).
//!
//! Generating the catalogue data itself is out of scope here — this only
//! consumes the already-published, pre-built catalogue JSON.

use std::{collections::HashMap, path::Path};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{
    model::Flavour,
    results::{Failure, InternalError},
};

mod git_cache;
#[cfg(test)]
mod tests;

pub mod search;

const CATALOGUE_VERSION: u32 = 8;
const GITHUB_SOURCE_ID: &str = "github";
const DATA_REPO_URL: &str = "https://github.com/layday/instawow-data.git";
const DATA_BRANCH: &str = "data";

fn catalogue_filename() -> String {
    format!("base-catalogue-v{CATALOGUE_VERSION}.compact.json")
}

/// Strips non-ASCII-alphanumeric characters and casefolds.
pub(crate) fn normalise_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AddonKey {
    pub source: String,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct CatalogueEntry {
    pub source: String,
    pub id: String,
    pub slug: String,
    pub name: String,
    pub url: String,
    pub game_flavours: Vec<Flavour>,
    pub download_count: u64,
    pub last_updated: DateTime<Utc>,
    /// Each entry is one combination of co-installed folder names expected
    /// for this addon. Kept as `Vec<String>` (small, order-preserving)
    /// rather than a `HashSet` — lookups are "does this name appear here",
    /// for which scanning a handful of names is simpler than hashing.
    pub folders: Vec<Vec<String>>,
    pub same_as: Vec<AddonKey>,
    pub normalised_name: String,
    pub derived_download_score: f64,
}

#[derive(Debug, Deserialize)]
struct RawAddonKey {
    source: String,
    id: String,
}

#[derive(Debug, Deserialize)]
struct RawCatalogueEntry {
    source: String,
    id: String,
    #[serde(default)]
    slug: String,
    name: String,
    url: String,
    #[serde(default)]
    game_flavours: Vec<String>,
    download_count: u64,
    last_updated: String,
    #[serde(default)]
    folders: Vec<Vec<String>>,
    #[serde(default)]
    same_as: Vec<RawAddonKey>,
}

#[derive(Debug, Deserialize)]
struct RawCatalogue {
    entries: Vec<RawCatalogueEntry>,
}

/// The catalogue once loaded and post-processed: `same_as` cross-references
/// backfilled from GitHub entries, plus derived name-normalisation and
/// download-popularity fields.
pub struct ComputedCatalogue {
    pub entries: Vec<CatalogueEntry>,
}

impl ComputedCatalogue {
    fn from_raw(raw: RawCatalogue) -> Self {
        let mut most_downloads: HashMap<String, u64> = HashMap::new();
        for e in &raw.entries {
            let slot = most_downloads.entry(e.source.clone()).or_insert(0);
            if e.download_count > *slot {
                *slot = e.download_count;
            }
        }

        // GitHub's generated catalogue CSV carries cross-source ids; backfill
        // non-GitHub entries' `same_as` from whatever GitHub entry references them.
        let mut same_as_from_github: HashMap<(String, String), Vec<AddonKey>> = HashMap::new();
        for e in &raw.entries {
            if e.source == GITHUB_SOURCE_ID && !e.same_as.is_empty() {
                for s in &e.same_as {
                    let mut list = vec![AddonKey {
                        source: e.source.clone(),
                        id: e.id.clone(),
                    }];
                    list.extend(e.same_as.iter().filter(|i| i.source != s.source).map(|i| {
                        AddonKey {
                            source: i.source.clone(),
                            id: i.id.clone(),
                        }
                    }));
                    same_as_from_github.insert((s.source.clone(), s.id.clone()), list);
                }
            }
        }

        let entries = raw
            .entries
            .into_iter()
            .filter_map(|e| {
                let last_updated = DateTime::parse_from_rfc3339(&e.last_updated)
                    .ok()?
                    .with_timezone(&Utc);
                let same_as = if e.source == GITHUB_SOURCE_ID {
                    e.same_as
                        .iter()
                        .map(|i| AddonKey {
                            source: i.source.clone(),
                            id: i.id.clone(),
                        })
                        .collect()
                } else {
                    same_as_from_github
                        .get(&(e.source.clone(), e.id.clone()))
                        .cloned()
                        .unwrap_or_else(|| {
                            e.same_as
                                .iter()
                                .map(|i| AddonKey {
                                    source: i.source.clone(),
                                    id: i.id.clone(),
                                })
                                .collect()
                        })
                };
                let max_downloads = *most_downloads.get(&e.source).unwrap_or(&0);
                let derived_download_score = if max_downloads == 0 {
                    e.download_count as f64
                } else {
                    e.download_count as f64 / max_downloads as f64
                };

                Some(CatalogueEntry {
                    normalised_name: normalise_name(&e.name),
                    source: e.source,
                    id: e.id,
                    slug: e.slug,
                    name: e.name,
                    url: e.url,
                    game_flavours: e
                        .game_flavours
                        .iter()
                        .filter_map(|s| Flavour::parse(s))
                        .collect(),
                    download_count: e.download_count,
                    last_updated,
                    folders: e.folders,
                    same_as,
                    derived_download_score,
                })
            })
            .collect();

        Self { entries }
    }

    pub fn keyed_entries(&self) -> HashMap<(&str, &str), &CatalogueEntry> {
        self.entries
            .iter()
            .map(|e| ((e.source.as_str(), e.id.as_str()), e))
            .collect()
    }
}

/// Resolves and parses the published aggregate catalogue from a local mirror
/// of `instawow-data`'s `data` branch under `cache_dir`, cloning or updating
/// it as needed — see `git_cache` (private submodule).
pub async fn synchronise(cache_dir: &Path) -> Result<ComputedCatalogue, Failure> {
    let filename = catalogue_filename();
    let path = git_cache::resolve(DATA_REPO_URL, DATA_BRANCH, cache_dir, &filename).await?;
    let bytes = tokio::fs::read(&path).await.map_err(InternalError::new)?;
    let raw: RawCatalogue = serde_json::from_slice(&bytes)?;
    Ok(ComputedCatalogue::from_raw(raw))
}
