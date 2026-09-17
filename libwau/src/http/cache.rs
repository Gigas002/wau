//! On-disk HTTP response cache: one small JSON sidecar (status + headers +
//! expiry) plus one raw body file per entry, keyed by a hash of the request.
//! Flat files rather than the `rusqlite` store this replaced — no extra
//! dependency, and a response body never needs anything fancier than "read
//! these bytes".

use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use super::{CacheTtl, HttpResponse};

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    status: u16,
    headers: Vec<(String, String)>,
    /// `None` means [`CacheTtl::Indefinite`].
    expires_at: Option<i64>,
}

pub(super) struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub(super) fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
        }
    }

    fn entry_paths(&self, key: &str) -> (PathBuf, PathBuf) {
        let hash = hash_key(key);
        (
            self.dir.join(format!("{hash}.meta")),
            self.dir.join(format!("{hash}.body")),
        )
    }

    pub(super) async fn get(&self, key: &str) -> Option<HttpResponse> {
        let (meta_path, body_path) = self.entry_paths(key);
        let meta_bytes = tokio::fs::read(&meta_path).await.ok()?;
        let meta: Meta = serde_json::from_slice(&meta_bytes).ok()?;

        if let Some(expires_at) = meta.expires_at
            && expires_at <= now_millis()
        {
            let _ = tokio::fs::remove_file(&meta_path).await;
            let _ = tokio::fs::remove_file(&body_path).await;
            return None;
        }

        let body = tokio::fs::read(&body_path).await.ok()?;
        Some(HttpResponse {
            status: meta.status,
            headers: meta.headers,
            body,
        })
    }

    pub(super) async fn put(&self, key: &str, response: &HttpResponse, ttl: CacheTtl) {
        let expires_at = match ttl {
            CacheTtl::Never => return,
            CacheTtl::Indefinite => None,
            CacheTtl::For(duration) => Some(now_millis() + duration.as_millis() as i64),
        };
        let Ok(meta_bytes) = serde_json::to_vec(&Meta {
            status: response.status,
            headers: response.headers.clone(),
            expires_at,
        }) else {
            return;
        };

        if tokio::fs::create_dir_all(&self.dir).await.is_err() {
            return;
        }
        let (meta_path, body_path) = self.entry_paths(key);
        // Body before meta: a reader only trusts an entry once its meta file
        // exists, so a write interrupted between the two never serves a
        // truncated body as a "complete" cache hit.
        let _ = tokio::fs::write(&body_path, &response.body).await;
        let _ = tokio::fs::write(&meta_path, meta_bytes).await;
    }
}

fn hash_key(key: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
