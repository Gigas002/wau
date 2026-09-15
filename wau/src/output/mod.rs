//! Result reporting and `list` formatting — pure string/JSON formatting, no
//! I/O. Ports the presentation half of instawow's `cli/__init__.py`
//! (`report_results`, `list_installed`'s three `_ListFormat` branches).

use std::collections::HashMap;

use libwau::{
    db::Pkg,
    model::Defn,
    pkg_management::Outcome,
    results::{AnyOutcome, Failure},
};

#[cfg(test)]
mod tests;

/// `report_results`'s per-result symbol: green check / red cross / blue bang
/// in instawow, collapsed to plain glyphs here (no terminal color dependency).
pub fn symbol_for(outcome: &AnyOutcome<Outcome>) -> &'static str {
    match outcome {
        Ok(_) => "✓",
        Err(Failure::Manager(_)) => "✗",
        Err(Failure::Internal(_)) => "!",
    }
}

/// Renders one `(Defn, outcome)` pair the way `report_results` does: a
/// symbol-prefixed heading followed by an indented message line.
pub fn format_result(defn: &Defn, outcome: &AnyOutcome<Outcome>) -> String {
    let heading = format!("{} {}", symbol_for(outcome), defn.as_uri(false, false));
    let detail = match outcome {
        Ok(o) => o.to_string(),
        Err(e) => e.to_string(),
    };
    format!("{heading}\n  {detail}")
}

/// Renders a whole result batch, sorted by URI for stable output — instawow
/// iterates results in resolve order, which isn't itself stable across our
/// concurrent-bucketed `resolve`, so sorting keeps this reproducible.
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

/// `_ListFormat.Simple`: one bare `source:slug` URI per line.
pub fn format_list_simple(pkgs: &[&Pkg]) -> String {
    pkgs.iter()
        .map(|p| Defn::new(p.source.clone(), p.slug.clone()).as_uri(false, false))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `_ListFormat.Detailed`: a multi-line definition-list block per package.
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

pub fn pkg_to_json(pkg: &Pkg) -> serde_json::Value {
    serde_json::json!({
        "source": pkg.source,
        "id": pkg.id,
        "slug": pkg.slug,
        "name": pkg.name,
        "description": pkg.description,
        "url": pkg.url,
        "download_url": pkg.download_url,
        "date_published": pkg.date_published.to_rfc3339(),
        "version": pkg.version,
        "changelog_url": pkg.changelog_url,
        "folders": pkg.folders.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
        "deps": pkg.deps.iter().map(|d| d.id.clone()).collect::<Vec<_>>(),
        "options": {
            "any_flavour": pkg.options.any_flavour,
            "any_release_type": pkg.options.any_release_type,
            "version_eq": pkg.options.version_eq,
        },
    })
}

/// `_ListFormat.Json`: an array of full package objects.
pub fn format_list_json(pkgs: &[&Pkg]) -> String {
    let values: Vec<_> = pkgs.iter().map(|p| pkg_to_json(p)).collect();
    serde_json::to_string_pretty(&values).unwrap_or_default()
}
