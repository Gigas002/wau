//! Result reporting and `list` formatting — pure string/JSON formatting, no
//! I/O. Every renderer that isn't machine-readable (`_json` excepted) takes
//! an explicit `color: bool` — see [`crate::style`] for why that's a plain
//! parameter rather than the functions detecting a terminal themselves.

use std::collections::{HashMap, HashSet};

use libwau::{
    catalogue::CatalogueEntry,
    lockfile::Pkg,
    model::Defn,
    pkg_management::Outcome,
    results::{AnyOutcome, Failure},
};

use crate::style;

#[cfg(test)]
mod tests;

/// Per-result symbol: plain glyphs, no terminal color dependency.
pub fn symbol_for(outcome: &AnyOutcome<Outcome>) -> &'static str {
    match outcome {
        Ok(_) => "✓",
        Err(Failure::Manager(_)) => "✗",
        Err(Failure::Internal(_)) => "!",
    }
}

/// Renders one `(Defn, outcome)` pair: a symbol-prefixed heading followed
/// by an indented message line.
pub fn format_result(defn: &Defn, outcome: &AnyOutcome<Outcome>, color: bool) -> String {
    let symbol = symbol_for(outcome);
    let symbol = match outcome {
        Ok(_) => style::success(color, symbol),
        Err(Failure::Manager(_)) => style::failure(color, symbol),
        Err(Failure::Internal(_)) => style::warn(color, symbol),
    };
    let uri = style::name(color, &defn.as_uri(false, false));
    let detail = match outcome {
        Ok(o) => o.to_string(),
        Err(e) => e.to_string(),
    };
    format!("{symbol} {uri}\n  {}", style::dim(color, &detail))
}

/// Renders a whole result batch, sorted by URI for stable output — results
/// come from a concurrently-bucketed resolve, so their natural order isn't
/// stable across runs; sorting keeps this reproducible.
pub fn format_results(results: &HashMap<Defn, AnyOutcome<Outcome>>, color: bool) -> String {
    let mut defns: Vec<&Defn> = results.keys().collect();
    defns.sort_by_key(|d| d.as_uri(false, false));
    defns
        .into_iter()
        .map(|d| format_result(d, &results[d], color))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether any result in the batch is a failure — used for the process exit code.
pub fn any_errors(results: &HashMap<Defn, AnyOutcome<Outcome>>) -> bool {
    results.values().any(|r| r.is_err())
}

// ============================================================================
// `list` formats
// ============================================================================

/// One `source:slug` URI per line, source and slug colored separately.
pub fn format_list_simple(pkgs: &[&Pkg], color: bool) -> String {
    pkgs.iter()
        .map(|p| {
            format!(
                "{}:{}",
                style::source(color, &p.source),
                style::name(color, &p.slug)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A multi-line definition-list block per package.
pub fn format_list_detailed(pkgs: &[&Pkg], color: bool) -> String {
    pkgs.iter()
        .map(|p| {
            let folders = p
                .folders
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let deps = p
                .deps
                .iter()
                .map(|d| Defn::new(p.source.clone(), d.id.clone()).as_uri(false, false))
                .collect::<Vec<_>>()
                .join(", ");
            let heading = format!(
                "{}:{}",
                style::source(color, &p.source),
                style::name(color, &p.slug)
            );
            format!(
                "{heading}\n  name: {}\n  description: {}\n  url: {}\n  version: {}\n  date published: {}\n  folders: {}\n  dependencies: {}\n  options: any_flavour={}; any_release_type={}; version_eq={}",
                p.name,
                style::dim(color, &p.description),
                p.url,
                style::version(color, &p.version),
                p.date_published.to_rfc3339(),
                folders,
                deps,
                p.options.any_flavour,
                p.options.any_release_type,
                p.options.version_eq,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Renders search results paru-style: numbered from `entries.len()` (top,
/// least relevant shown) down to `1` (bottom, most relevant, right above the
/// input prompt so the best pick is the easiest one to type) — `entries` is
/// expected already sorted best-first, as [`catalogue::search::search`]
/// returns it. Each result gets a `source/slug [downloads]` heading (an
/// `[Installed]` tag appended when applicable) and an indented name line —
/// the catalogue doesn't carry a version or description to show, unlike a
/// real package repository, so those stand in for paru's version/popularity
/// and description lines.
///
/// [`catalogue::search::search`]: libwau::catalogue::search::search
pub fn format_search_results(
    entries: &[&CatalogueEntry],
    installed_keys: &HashSet<(String, String)>,
    color: bool,
) -> String {
    (0..entries.len())
        .rev()
        .map(|i| {
            let entry = entries[i];
            let slug = if entry.slug.is_empty() {
                &entry.id
            } else {
                &entry.slug
            };
            let installed = installed_keys.contains(&(entry.source.clone(), entry.id.clone()));
            let heading = format!(
                "{} {}/{} [{} \u{2193}]{}",
                style::number(color, i + 1),
                style::source(color, &entry.source),
                style::name(color, slug),
                entry.download_count,
                if installed {
                    format!(" {}", style::installed_tag(color))
                } else {
                    String::new()
                },
            );
            format!("{heading}\n    {}", style::dim(color, &entry.name))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, serde::Serialize)]
struct PkgOptionsJson {
    any_flavour: bool,
    any_release_type: bool,
    version_eq: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PkgJson {
    source: String,
    id: String,
    slug: String,
    name: String,
    description: String,
    url: String,
    download_url: String,
    date_published: String,
    version: String,
    changelog_url: String,
    folders: Vec<String>,
    deps: Vec<String>,
    options: PkgOptionsJson,
}

impl From<&Pkg> for PkgJson {
    fn from(pkg: &Pkg) -> Self {
        PkgJson {
            source: pkg.source.clone(),
            id: pkg.id.clone(),
            slug: pkg.slug.clone(),
            name: pkg.name.clone(),
            description: pkg.description.clone(),
            url: pkg.url.clone(),
            download_url: pkg.download_url.clone(),
            date_published: pkg.date_published.to_rfc3339(),
            version: pkg.version.clone(),
            changelog_url: pkg.changelog_url.clone(),
            folders: pkg.folders.iter().map(|f| f.name.clone()).collect(),
            deps: pkg.deps.iter().map(|d| d.id.clone()).collect(),
            options: PkgOptionsJson {
                any_flavour: pkg.options.any_flavour,
                any_release_type: pkg.options.any_release_type,
                version_eq: pkg.options.version_eq,
            },
        }
    }
}

pub fn pkg_to_json(pkg: &Pkg) -> serde_json::Value {
    serde_json::to_value(PkgJson::from(pkg)).unwrap_or(serde_json::Value::Null)
}

/// An array of full package objects.
pub fn format_list_json(pkgs: &[&Pkg]) -> String {
    let values: Vec<_> = pkgs.iter().map(|p| pkg_to_json(p)).collect();
    serde_json::to_string_pretty(&values).unwrap_or_default()
}
