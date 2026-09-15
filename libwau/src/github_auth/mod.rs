//! GitHub device-code OAuth flow — ports instawow's `_github_auth.py`.
//!
//! Stores nothing itself: callers persist the returned access token wherever
//! they see fit (`config::GlobalConfig::access_tokens.github`).

use std::{collections::HashMap, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    http::{CacheTtl, HttpClient},
    results::{Failure, InternalError},
};

#[cfg(test)]
mod tests;

const CLIENT_ID: &str = "aa178904bdf5143e93ec";

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize)]
struct DeviceCodeRequest {
    client_id: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct AccessTokenRequest<'a> {
    client_id: &'static str,
    device_code: &'a str,
    grant_type: &'static str,
}

/// GitHub's device-flow token-poll response: either a success payload or an
/// `{"error": "..."}` payload (`authorization_pending` while the user hasn't
/// approved yet, some other error code otherwise).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum AccessTokenResponse {
    Success { access_token: String },
    Error { error: String },
}

/// One category's usage/limit counters in GitHub's `/rate_limit` response.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RateLimitResource {
    pub limit: u64,
    pub remaining: u64,
    pub reset: u64,
    #[serde(default)]
    pub used: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RateLimitStatus {
    #[serde(default)]
    pub resources: HashMap<String, RateLimitResource>,
    #[serde(default)]
    pub rate: Option<RateLimitResource>,
}

#[derive(Default)]
pub struct GitHubAuth {
    /// Overridable only in tests (mockito needs a local base URL); the real
    /// flow always targets the actual GitHub hosts.
    #[cfg(test)]
    base_url: Option<String>,
    #[cfg(test)]
    api_base_url: Option<String>,
}

impl GitHubAuth {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn new_with_urls(base_url: String, api_base_url: String) -> Self {
        Self {
            base_url: Some(base_url),
            api_base_url: Some(api_base_url),
        }
    }

    fn base(&self) -> &str {
        #[cfg(test)]
        return self.base_url.as_deref().unwrap_or("https://github.com");
        #[cfg(not(test))]
        return "https://github.com";
    }

    fn api_base(&self) -> &str {
        #[cfg(test)]
        return self
            .api_base_url
            .as_deref()
            .unwrap_or("https://api.github.com");
        #[cfg(not(test))]
        return "https://api.github.com";
    }

    /// Requests a device/user code pair to start the flow. Show `user_code`
    /// and `verification_uri` to the user, then call [`Self::poll_for_access_token`].
    pub async fn get_codes(&self, http: &HttpClient) -> Result<DeviceCodeResponse, Failure> {
        let body = serde_json::to_vec(&DeviceCodeRequest {
            client_id: CLIENT_ID,
        })
        .unwrap_or_default();
        let response = http
            .post(
                &format!("{}/login/device/code", self.base()),
                &[("Accept", "application/json")],
                body,
                CacheTtl::Never,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(InternalError::new(format!(
                "HTTP {} for device code request",
                response.status
            ))
            .into());
        }
        Ok(serde_json::from_slice(&response.body)?)
    }

    /// Polls until the user authorises the device (or the flow errors out),
    /// sleeping `polling_interval` between attempts on "authorization_pending".
    pub async fn poll_for_access_token(
        &self,
        http: &HttpClient,
        device_code: &str,
        polling_interval: Duration,
    ) -> Result<String, Failure> {
        loop {
            let body = serde_json::to_vec(&AccessTokenRequest {
                client_id: CLIENT_ID,
                device_code,
                grant_type: "urn:ietf:params:oauth:grant-type:device_code",
            })
            .unwrap_or_default();
            let response = http
                .post(
                    &format!("{}/login/oauth/access_token", self.base()),
                    &[("Accept", "application/json")],
                    body,
                    CacheTtl::Never,
                )
                .await?;
            if !(200..300).contains(&response.status) {
                return Err(InternalError::new(format!(
                    "HTTP {} for access token poll",
                    response.status
                ))
                .into());
            }

            match serde_json::from_slice(&response.body)? {
                AccessTokenResponse::Success { access_token } => return Ok(access_token),
                AccessTokenResponse::Error { error } if error == "authorization_pending" => {
                    tokio::time::sleep(polling_interval).await;
                }
                AccessTokenResponse::Error { error } => {
                    return Err(InternalError::new(format!("authorization failed: {error}")).into());
                }
            }
        }
    }

    /// Fetches GitHub's current API rate-limit status (inspection only — not
    /// used to enforce client-side rate limiting anywhere in this port,
    /// matching instawow).
    pub async fn get_rate_limit_status(
        &self,
        http: &HttpClient,
        access_token: Option<&str>,
    ) -> Result<RateLimitStatus, Failure> {
        let mut headers = vec![
            ("Accept", "application/vnd.github+json"),
            ("X-GitHub-Api-Version", "2022-11-28"),
        ];
        let auth_header = access_token.map(|token| format!("Bearer {token}"));
        if let Some(header) = &auth_header {
            headers.push(("Authorization", header));
        }

        let response = http
            .get(
                &format!("{}/rate_limit", self.api_base()),
                &headers,
                CacheTtl::Never,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(InternalError::new(format!(
                "HTTP {} for rate limit status",
                response.status
            ))
            .into());
        }
        Ok(serde_json::from_slice(&response.body)?)
    }
}
