//! Catalogue fuzzy search.
//!
//! Uses `frizbee`'s Smith-Waterman-based fuzzy matcher: the query's
//! characters must appear, in order, somewhere in a candidate's normalised
//! name (with bonuses for prefix and exact matches). That score is
//! normalised to 0.0-1.0 and fed into a blend-with-download-popularity
//! ranking formula. Unlike a plain edit-distance ratio, this also matches
//! abbreviation-style queries (e.g. "dbm" matching "Deadly Boss Mods").

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use frizbee::{Config, Matcher};

use super::CatalogueEntry;
use crate::model::Flavour;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterInstalled {
    /// No filtering by install state.
    Ident,
    /// Only entries matching an installed package.
    IncludeOnly,
    /// Exclude entries matching an installed package.
    Exclude,
    /// Exclude entries matching an installed package, plus any of its
    /// cross-source `same_as` equivalents.
    ExcludeFromAllSources,
}

pub struct SearchOptions<'a> {
    pub limit: usize,
    pub sources: &'a [String],
    pub prefer_source: Option<&'a str>,
    pub start_date: Option<DateTime<Utc>>,
    pub filter_installed: FilterInstalled,
}

impl Default for SearchOptions<'_> {
    fn default() -> Self {
        Self {
            limit: 10,
            sources: &[],
            prefer_source: None,
            start_date: None,
            filter_installed: FilterInstalled::Ident,
        }
    }
}

const EDIT_WEIGHT: f64 = 0.5;
const DOWNLOAD_WEIGHT: f64 = 0.5;

/// Fuzzy-searches `entries` for `search_terms`, ranked by a blend of string
/// similarity and download popularity. `installed_keys` is every
/// `(source, id)` pair currently installed (used by `FilterInstalled`).
pub fn search<'a>(
    entries: &'a [CatalogueEntry],
    search_terms: &str,
    flavour: Flavour,
    installed_keys: &HashSet<(String, String)>,
    options: &SearchOptions<'_>,
) -> Vec<&'a CatalogueEntry> {
    let mut excluded_keys: HashSet<(String, String)> = HashSet::new();
    if options.filter_installed == FilterInstalled::ExcludeFromAllSources {
        let by_key: std::collections::HashMap<(&str, &str), &CatalogueEntry> = entries
            .iter()
            .map(|e| ((e.source.as_str(), e.id.as_str()), e))
            .collect();
        for key in installed_keys {
            excluded_keys.insert(key.clone());
            if let Some(entry) = by_key.get(&(key.0.as_str(), key.1.as_str())) {
                for s in &entry.same_as {
                    excluded_keys.insert((s.source.clone(), s.id.clone()));
                }
            }
        }
    } else if options.filter_installed == FilterInstalled::Exclude {
        excluded_keys = installed_keys.clone();
    }

    let mut candidates: Vec<&CatalogueEntry> = entries
        .iter()
        .filter(|e| e.game_flavours.contains(&flavour))
        .filter(|e| options.sources.is_empty() || options.sources.iter().any(|s| s == &e.source))
        .filter(|e| {
            options
                .start_date
                .map(|d| e.last_updated >= d)
                .unwrap_or(true)
        })
        .filter(|e| {
            !matches!(
                options.filter_installed,
                FilterInstalled::Exclude | FilterInstalled::ExcludeFromAllSources
            ) || !excluded_keys.contains(&(e.source.clone(), e.id.clone()))
        })
        .filter(|e| match options.prefer_source {
            Some(prefer) => !e.same_as.iter().any(|s| s.source == prefer),
            None => true,
        })
        .collect();

    if options.filter_installed == FilterInstalled::IncludeOnly {
        candidates.retain(|e| installed_keys.contains(&(e.source.clone(), e.id.clone())));
    }

    let normalised_query = super::normalise_name(search_terms);

    let mut scored: Vec<(f64, &CatalogueEntry)> = if search_terms == "*" {
        candidates.into_iter().map(|e| (0.0, e)).collect()
    } else {
        let mut matcher = Matcher::new(normalised_query.as_str(), &Config::default());
        // frizbee's score is an unbounded, bonus-laden u16 rather than a
        // 0.0-1.0 ratio. Normalise against the query's own best-possible
        // match against itself, which is always achievable and always the
        // maximum for that query, regardless of scoring-constant tuning.
        let max_score = matcher
            .match_list(&[normalised_query.as_str()])
            .first()
            .map_or(1, |m| m.score.max(1));

        let haystacks: Vec<&str> = candidates
            .iter()
            .map(|e| e.normalised_name.as_str())
            .collect();
        matcher
            .match_list(&haystacks)
            .into_iter()
            .map(|m| {
                let score = (f64::from(m.score) / f64::from(max_score)).min(1.0);
                (score, candidates[m.index as usize])
            })
            .collect()
    };

    scored.sort_by(|(score_a, entry_a), (score_b, entry_b)| {
        let key_a = score_a * EDIT_WEIGHT + entry_a.derived_download_score * DOWNLOAD_WEIGHT;
        let key_b = score_b * EDIT_WEIGHT + entry_b.derived_download_score * DOWNLOAD_WEIGHT;
        key_b
            .partial_cmp(&key_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    scored
        .into_iter()
        .take(options.limit)
        .map(|(_, e)| e)
        .collect()
}
