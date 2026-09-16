//! HTTP client, backed by `reqwest`.

use std::time::Duration;

#[cfg(test)]
mod tests;

/// A fetched HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

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
}

impl HttpClient {
    pub fn new() -> Result<Self, HttpError> {
        Ok(Self {
            default: build_client(false)?,
            cloudflare_compat: build_client(true)?,
        })
    }

    pub async fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<HttpResponse, HttpError> {
        self.request(reqwest::Method::GET, url, headers, None).await
    }

    pub async fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: Vec<u8>,
    ) -> Result<HttpResponse, HttpError> {
        self.request(reqwest::Method::POST, url, headers, Some(body))
            .await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, HttpError> {
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

        Ok(response)
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
