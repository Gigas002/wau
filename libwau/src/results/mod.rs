//! Per-package operation outcomes.
//!
//! Failure modes a package operation can hit in the *expected* course of
//! business (already installed, source disabled, no matching release, …) are
//! modelled as [`ManagerError`] values; anything unclassified (a network
//! error, an unexpected API response shape, …) becomes an [`InternalError`].
//! The point of both is the same: one failing `Defn` in a batch
//! (`resolve`/`install`/`update`/…) is reported per-item rather than
//! aborting the whole operation.
//!
//! The success side isn't defined here, to avoid this module depending on
//! the database layer.

use crate::model::{Strategies, Strategy};

#[cfg(test)]
mod tests;

/// Result of a per-`Defn` operation — either success, a recognized business
/// failure, or an unclassified one.
pub type AnyOutcome<T> = Result<T, Failure>;

/// Either a recognized business failure or an unclassified one.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum Failure {
    #[error(transparent)]
    Manager(#[from] ManagerError),
    #[error(transparent)]
    Internal(#[from] InternalError),
}

/// Catch-all for unclassified failures surfaced per-`Defn`, wrapping
/// anything a resolver/operation doesn't explicitly raise as a
/// [`ManagerError`]. Sources convert their own error types into this via
/// `?` rather than aborting the whole batch resolve/install/update.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("internal error: \"{0}\"")]
pub struct InternalError(pub String);

impl InternalError {
    pub fn new(err: impl std::fmt::Display) -> Self {
        Self(err.to_string())
    }
}

/// Lightweight package identity used in error messages where the full DB row
/// isn't needed (keeps `results` independent of `db`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgRef {
    pub source: String,
    pub id: String,
    pub name: String,
}

/// Expected, "business" failure modes for a package operation.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ManagerError {
    #[error("package already installed")]
    PkgAlreadyInstalled,

    #[error("{}", format_conflicts_with_installed(conflicting))]
    PkgConflictsWithInstalled { conflicting: Vec<PkgRef> },

    #[error("{}", format_conflicts_with_unreconciled(folders))]
    PkgConflictsWithUnreconciled { folders: Vec<String> },

    #[error("package does not exist")]
    PkgNonexistent,

    #[error("{reason}")]
    PkgFilesMissing { reason: String },

    #[error("{}", format_files_not_matching(strategies))]
    PkgFilesNotMatching { strategies: Strategies },

    #[error("package is not installed")]
    PkgNotInstalled,

    #[error("package source is invalid")]
    PkgSourceInvalid,

    #[error("{}", format_source_disabled(reason))]
    PkgSourceDisabled { reason: Option<String> },

    #[error("{}", format_up_to_date(*is_pinned))]
    PkgUpToDate { is_pinned: bool },

    #[error("{}", format_strategies_unsupported(strategies))]
    PkgStrategiesUnsupported { strategies: Vec<Strategy> },
}

impl ManagerError {
    /// Convenience constructor for the default "no files available" reason.
    pub fn files_missing() -> Self {
        Self::PkgFilesMissing {
            reason: "no files are available for download".to_owned(),
        }
    }
}

fn format_conflicts_with_installed(conflicting: &[PkgRef]) -> String {
    let plural = if conflicting.len() > 1 { "s" } else { "" };
    let list = conflicting
        .iter()
        .map(|c| format!("{} ({}:{})", c.name, c.source, c.id))
        .collect::<Vec<_>>()
        .join(", ");
    format!("package folders conflict with installed package{plural} {list}")
}

fn format_conflicts_with_unreconciled(folders: &[String]) -> String {
    let list = folders
        .iter()
        .map(|f| format!("'{f}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("package folders conflict with {list}")
}

fn format_files_not_matching(strategies: &Strategies) -> String {
    format!("no files found for: {}", format_strategies(strategies))
}

fn format_strategies(strategies: &Strategies) -> String {
    let mut parts = Vec::new();
    if strategies.any_flavour {
        parts.push("any_flavour=True".to_owned());
    }
    if strategies.any_release_type {
        parts.push("any_release_type=True".to_owned());
    }
    if let Some(v) = &strategies.version_eq {
        parts.push(format!("version_eq={v:?}"));
    }
    parts.join("; ")
}

fn format_source_disabled(reason: &Option<String>) -> String {
    match reason {
        Some(r) => format!("package source is disabled: {r}"),
        None => "package source is disabled".to_owned(),
    }
}

fn format_up_to_date(is_pinned: bool) -> String {
    format!(
        "package is {}",
        if is_pinned { "pinned" } else { "up to date" }
    )
}

fn format_strategies_unsupported(strategies: &[Strategy]) -> String {
    let mut sorted: Vec<&str> = strategies.iter().map(|s| s.as_str()).collect();
    sorted.sort_unstable();
    format!("strategies are not valid for source: {}", sorted.join(", "))
}
