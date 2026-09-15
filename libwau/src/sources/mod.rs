//! Provider abstraction — ports instawow's `resolvers.py` (the `Resolver`/
//! `BaseResolver` protocol) and `_sources/__init__.py` (`DEFAULT_RESOLVERS`).
//!
//! Each concrete source lives behind its own Cargo feature (§2.2 in the
//! plan) and its own submodule; this module only holds the shared trait,
//! candidate types, and the registry.

use chrono::{DateTime, Utc};

use crate::{
    config::SecretString,
    http::HttpError,
    model::{Defn, Flavour, HeadersIntent, SourceMetadata, Strategy},
    pkg_archives::DownloadError,
    results::{AnyOutcome, Failure, InternalError, ManagerError},
};

#[cfg(test)]
mod tests;

#[cfg(feature = "curseforge")]
pub mod curseforge;
#[cfg(feature = "github")]
pub mod github;
#[cfg(feature = "tukui")]
pub mod tukui;
#[cfg(feature = "wago")]
pub mod wago;
#[cfg(feature = "wowinterface")]
pub mod wowinterface;

/// A resolved addon, not yet persisted — instawow's `PkgCandidate` TypedDict.
#[derive(Debug, Clone, PartialEq)]
pub struct PkgCandidate {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub download_url: String,
    pub date_published: DateTime<Utc>,
    pub version: String,
    pub changelog_url: String,
    /// Dependency ids, within the same source.
    pub deps: Vec<String>,
}

/// Infra-level failures a source can hit converted into `Failure::Internal` —
/// mirrors `resultify` wrapping any unclassified Python exception.
impl From<HttpError> for Failure {
    fn from(e: HttpError) -> Self {
        Failure::Internal(InternalError::new(e))
    }
}
impl From<DownloadError> for Failure {
    fn from(e: DownloadError) -> Self {
        Failure::Internal(InternalError::new(e))
    }
}
impl From<serde_json::Error> for Failure {
    fn from(e: serde_json::Error) -> Self {
        Failure::Internal(InternalError::new(e))
    }
}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Failure::Internal(InternalError::new(e))
    }
}

/// A remote addon source: search/resolve metadata + artifact retrieval.
/// Ports instawow's `Resolver`/`BaseResolver`.
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    fn metadata(&self) -> SourceMetadata;

    /// Why this source is unusable right now (e.g. a required token is
    /// missing), if it is.
    fn get_disabled_reason(&self) -> Option<String> {
        None
    }

    /// Extracts a `Defn` alias from a source website URL, if `url` is one.
    fn get_alias_from_url(&self, _url: &str) -> Option<String> {
        None
    }

    /// Headers for this source's HTTP requests; sources needing a different
    /// header set for downloads (e.g. GitHub's octet-stream `Accept`)
    /// override this per [`HeadersIntent`].
    fn make_request_headers(&self, _intent: HeadersIntent) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Per-source resolve logic for one `Defn`. Callers should go through the
    /// free function [`resolve_one`] (or [`Resolver::resolve`]), not this
    /// directly — strategy support isn't checked here. Ported as a separate
    /// method because instawow's `__init_subclass__`-based wrapping (which
    /// makes the check unconditional for *every* subclass, even ones
    /// overriding `resolve_one`) has no equivalent overridable-trait-method
    /// mechanism in Rust; a free function fills the same role instead.
    async fn resolve_one_impl(
        &self,
        http: &crate::http::HttpClient,
        flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate>;

    /// Batch resolve; default runs [`resolve_one`] concurrently per `Defn`.
    /// Sources that can resolve many `Defn`s in one request (e.g. CurseForge's
    /// batched `/mods` lookup) override this.
    async fn resolve(
        &self,
        http: &crate::http::HttpClient,
        flavour: Flavour,
        defns: &[Defn],
    ) -> Vec<AnyOutcome<PkgCandidate>> {
        let futures = defns.iter().map(|d| resolve_one(self, http, flavour, d));
        futures_util::future::join_all(futures).await
    }

    /// Fetches a changelog from `url`. The default handles `data:,<text>`
    /// (inline), `file://` (local read), and `http(s)://` (cached
    /// indefinitely) schemes — matches `BaseResolver.get_changelog`.
    async fn get_changelog(
        &self,
        http: &crate::http::HttpClient,
        url: &str,
    ) -> Result<String, Failure> {
        default_get_changelog(self, http, url).await
    }
}

/// Strategies `defn` requests that `supported` doesn't declare — ported from
/// instawow's `defn.strategies.filled.keys() - self.metadata.strategies` check.
fn extraneous_strategies(defn: &Defn, supported: &[Strategy]) -> Vec<Strategy> {
    let mut extraneous = Vec::new();
    if defn.strategies.any_flavour && !supported.contains(&Strategy::AnyFlavour) {
        extraneous.push(Strategy::AnyFlavour);
    }
    if defn.strategies.any_release_type && !supported.contains(&Strategy::AnyReleaseType) {
        extraneous.push(Strategy::AnyReleaseType);
    }
    if defn.strategies.version_eq.is_some() && !supported.contains(&Strategy::VersionEq) {
        extraneous.push(Strategy::VersionEq);
    }
    extraneous
}

/// Resolves one `Defn`, enforcing that every requested [`Strategy`] is one
/// `resolver` declares support for first.
pub async fn resolve_one<R: Resolver + ?Sized>(
    resolver: &R,
    http: &crate::http::HttpClient,
    flavour: Flavour,
    defn: &Defn,
) -> AnyOutcome<PkgCandidate> {
    let unsupported = extraneous_strategies(defn, resolver.metadata().strategies);
    if !unsupported.is_empty() {
        return Err(ManagerError::PkgStrategiesUnsupported {
            strategies: unsupported,
        }
        .into());
    }
    resolver.resolve_one_impl(http, flavour, defn).await
}

async fn default_get_changelog<R: Resolver + ?Sized>(
    resolver: &R,
    http: &crate::http::HttpClient,
    url: &str,
) -> Result<String, Failure> {
    if let Some(data) = url.strip_prefix("data:,") {
        return Ok(percent_decode(data));
    }
    if let Some(path) = crate::pkg_archives::file_uri_to_path(url) {
        return tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| InternalError::new(e).into());
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let headers = resolver.make_request_headers(HeadersIntent::Fetch);
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let response = http
            .get(url, &headers, crate::http::CacheTtl::Indefinite)
            .await?;
        return String::from_utf8(response.body).map_err(|e| InternalError::new(e).into());
    }
    Err(InternalError::new(format!("unsupported changelog URL scheme: {url}")).into())
}

pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = bytes.get(i + 1..i + 3)
            && let Ok(hex_str) = std::str::from_utf8(hex)
            && let Ok(byte) = u8::from_str_radix(hex_str, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encodes `s`, matching Python's `urllib.parse.quote` default safe
/// set (`/` left unescaped) — the inverse of [`percent_decode`]. Used to
/// build `data:,<urlencoded>` changelog URLs (GitHub, Wago Addons,
/// WoWInterface all embed their changelog text this way rather than
/// fetching it separately). Unused (dead code) under `--no-default-features`,
/// where none of those sources are compiled in.
#[allow(dead_code)]
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        let c = *byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '/') {
            out.push(c);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ============================================================================
// Registry
// ============================================================================

/// Credentials for sources that need them. All optional; a source requiring
/// one reports [`Resolver::get_disabled_reason`] when it's absent.
#[derive(Debug, Clone, Default)]
pub struct SourceConfig {
    pub cfcore_api_key: Option<SecretString>,
    /// Self-hosted CFCore-compatible proxy URL, overriding the default
    /// `https://api.curseforge.com/v1`. instawow's `INSTAWOW_CF_API_URL`.
    pub cfcore_api_url: Option<String>,
    pub github_token: Option<SecretString>,
    pub wago_addons_token: Option<SecretString>,
}

/// Every compiled-in source, in instawow's `DEFAULT_RESOLVERS` priority order
/// (GitHub, CurseForge, WoWInterface, Tukui, Wago Addons — used for
/// reconciliation tie-breaking; the `instawow`/WeakAuras Companion source is
/// out of scope, see `docs/WAU_RS_PLAN.md`).
// `vec![]` can't express the per-source `#[cfg(feature = ...)]` gating below;
// both `config` and `mut` go unused under `--no-default-features`, where no
// source is compiled in.
#[allow(clippy::vec_init_then_push, unused_variables, unused_mut)]
pub fn default_sources(config: &SourceConfig) -> Vec<Box<dyn Resolver>> {
    let mut sources: Vec<Box<dyn Resolver>> = Vec::new();

    #[cfg(feature = "github")]
    sources.push(Box::new(github::GitHubResolver::new(
        config.github_token.clone(),
    )));
    #[cfg(feature = "curseforge")]
    sources.push(Box::new(curseforge::CurseForgeResolver::new(
        config.cfcore_api_key.clone(),
        config.cfcore_api_url.clone(),
    )));
    #[cfg(feature = "wowinterface")]
    sources.push(Box::new(wowinterface::WowInterfaceResolver::new()));
    #[cfg(feature = "tukui")]
    sources.push(Box::new(tukui::TukuiResolver::new()));
    #[cfg(feature = "wago")]
    sources.push(Box::new(wago::WagoAddonsResolver::new(
        config.wago_addons_token.clone(),
    )));

    sources
}

/// Looks up a source by its [`SourceMetadata::id`].
pub fn find_source<'a>(sources: &'a [Box<dyn Resolver>], id: &str) -> Option<&'a dyn Resolver> {
    sources
        .iter()
        .find(|r| r.metadata().id == id)
        .map(|r| r.as_ref())
}
