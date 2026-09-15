//! Result reporting and `list` formatting — pure string/JSON formatting, no
//! I/O.

use std::collections::HashMap;

use libwau::{
    db::Pkg,
    model::Defn,
    pkg_management::Outcome,
    results::{AnyOutcome, Failure},
};

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
pub fn format_result(defn: &Defn, outcome: &AnyOutcome<Outcome>) -> String {
    let heading = format!("{} {}", symbol_for(outcome), defn.as_uri(false, false));
    let detail = match outcome {
        Ok(o) => o.to_string(),
        Err(e) => e.to_string(),
    };
    format!("{heading}\n  {detail}")
}

/// Renders a whole result batch, sorted by URI for stable output — results
/// come from a concurrently-bucketed resolve, so their natural order isn't
/// stable across runs; sorting keeps this reproducible.
pub fn format_results(results: &HashMap<Defn, AnyOutcome<Outcome>>) -> String {
    let mut defns: Vec<&Defn> = results.keys().collect();
    defns.sort_by_key(|d| d.as_uri(false, false));
    defns
        .into_iter()
        .map(|d| format_result(d, &results[d]))
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

/// One bare `source:slug` URI per line.
pub fn format_list_simple(pkgs: &[&Pkg]) -> String {
    pkgs.iter()
        .map(|p| Defn::new(p.source.clone(), p.slug.clone()).as_uri(false, false))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A multi-line definition-list block per package.
pub fn format_list_detailed(pkgs: &[&Pkg]) -> String {
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
            format!(
                "{}\n  name: {}\n  description: {}\n  url: {}\n  version: {}\n  date published: {}\n  folders: {}\n  dependencies: {}\n  options: any_flavour={}; any_release_type={}; version_eq={}",
                p.to_defn().as_uri(false, false),
                p.name,
                p.description,
                p.url,
                p.version,
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
