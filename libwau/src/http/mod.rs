//! HTTP client, backed by `reqwest`, with an optional on-disk response cache.

use std::{path::Path, sync::Arc, time::Duration};

mod cache;
#[cfg(test)]
mod tests;

/// A fetched HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Every call site picks a caching policy explicitly — nothing is cached
/// unless the caller passes one, and nothing is cached at all unless the
/// client was built with [`HttpClient::with_cache_dir`].
#[derive(Debug, Clone, Copy)]
pub enum CacheTtl {
    /// Bypass the cache entirely for this request.
    Never,
    /// Cache for a fixed duration.
    For(Duration),
    /// Cache forever once fetched — relies on the URL being
    /// version-specific/immutable (used for downloads and changelogs).
    Indefinite,
}

/// Status codes worth caching: 200 (OK), 206 (Partial Content — GitHub ranged
/// reads), 501 (Not Implemented — GitHub mislabels out-of-range as this).
const ALLOWED_CACHE_STATUSES: [u16; 3] = [200, 206, 501];

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("request: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("HTTP status {status} for {url}")]
    Status { status: u16, url: String },
}

fn user_agent() -> String {
    format!(
        "wau (+https://github.com/Gigas002/wau) v{}",
        env!("CARGO_PKG_VERSION")
    )
}

/// Shared HTTP client: a default `reqwest::Client` plus a "Cloudflare
/// compatibility" variant (forces TLS 1.3) retried into on a `403` bearing a
/// `CF-RAY` header — see `init_web_client`/`get_ssl_context` in
/// `http/__init__.py` and the CF workaround in `pkg_archives/_download.py`.
pub struct HttpClient {
    default: reqwest::Client,
    cloudflare_compat: reqwest::Client,
    cache: Option<Arc<cache::Cache>>,
}

impl HttpClient {
    /// No on-disk cache — every request hits the network regardless of the
    /// `ttl` passed to [`Self::get`]/[`Self::post`].
    pub fn new() -> Result<Self, HttpError> {
        Ok(Self {
            default: build_client(false)?,
            cloudflare_compat: build_client(true)?,
            cache: None,
        })
    }

    /// Caches responses under `cache_dir` per call site's requested
    /// [`CacheTtl`].
    pub fn with_cache_dir(cache_dir: &Path) -> Result<Self, HttpError> {
        Ok(Self {
            default: build_client(false)?,
            cloudflare_compat: build_client(true)?,
            cache: Some(Arc::new(cache::Cache::new(&cache_dir.join("http")))),
        })
    }

    pub async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        ttl: CacheTtl,
    ) -> Result<HttpResponse, HttpError> {
        self.request(reqwest::Method::GET, url, headers, None, ttl)
            .await
    }

    pub async fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: Vec<u8>,
        ttl: CacheTtl,
    ) -> Result<HttpResponse, HttpError> {
        self.request(reqwest::Method::POST, url, headers, Some(body), ttl)
            .await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
        ttl: CacheTtl,
    ) -> Result<HttpResponse, HttpError> {
        let key = cache_key(method.as_str(), url, headers);

        if let Some(cached) = self.cache_get(&ttl, &key).await {
            return Ok(cached);
        }

        let response = self
            .send(&self.default, &method, url, headers, body.as_deref())
            .await?;

        // CloudFlare rejects the default TLS 1.2/1.3 combination on some
        // fronted hosts; retry once forcing TLS 1.3.
        let response = if response.status == 403
            && response
                .headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("cf-ray"))
        {
            self.send(
                &self.cloudflare_compat,
                &method,
                url,
                headers,
                body.as_deref(),
            )
            .await?
        } else {
            response
        };

        if ALLOWED_CACHE_STATUSES.contains(&response.status) {
            self.cache_put(&key, &response, ttl).await;
        }

        Ok(response)
    }

    async fn cache_get(&self, ttl: &CacheTtl, key: &str) -> Option<HttpResponse> {
        if matches!(ttl, CacheTtl::Never) {
            return None;
        }
        self.cache.as_ref()?.get(key).await
    }

    async fn cache_put(&self, key: &str, response: &HttpResponse, ttl: CacheTtl) {
        let Some(cache) = &self.cache else {
            return;
        };
        cache.put(key, response, ttl).await;
    }

    async fn send(
        &self,
        client: &reqwest::Client,
        method: &reqwest::Method,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<HttpResponse, HttpError> {
        let mut builder = client.request(method.clone(), url);
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        if let Some(body) = body {
            builder = builder.body(body.to_vec());
        }

        let response = builder.send().await?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_owned()))
            .collect();
        let body = response.bytes().await?.to_vec();

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn cache_key(method: &str, url: &str, headers: &[(&str, &str)]) -> String {
    let mut sorted: Vec<(&str, &str)> = headers.to_vec();
    sorted.sort_unstable();
    let headers_str: String = sorted
        .iter()
        .map(|(k, v)| format!("{k}:{v}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{method} {url}\n{headers_str}")
}

fn build_client(cloudflare_compat: bool) -> Result<reqwest::Client, HttpError> {
    let mut builder = reqwest::Client::builder()
        .user_agent(user_agent())
        .connect_timeout(Duration::from_secs(60))
        .timeout(Duration::from_secs(80))
        .gzip(true);
    if cloudflare_compat {
        builder = builder.min_tls_version(reqwest::tls::Version::TLS_1_3);
    }
    Ok(builder.build()?)
}
