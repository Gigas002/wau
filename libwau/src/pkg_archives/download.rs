//! Package archive download.
//!
//! No checksum verification is performed anywhere in this path: despite
//! CurseForge exposing file hashes in its API, the downloaded bytes are
//! never checked against them.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
    time::{SystemTime, UNIX_EPOCH},
};

use tokio::sync::Mutex as AsyncMutex;
use url::Url;

use crate::http::{CacheTtl, HttpClient, HttpError};

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("{0}")]
    Http(#[from] HttpError),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid file:// URI '{0}'")]
    InvalidFileUri(String),
    #[error("download failed with status {status} for {url}")]
    Status { status: u16, url: String },
}

/// `true` for `file://` URLs, which are short-circuited to a direct
/// filesystem path instead of an HTTP request (used by local/test sources).
pub fn is_file_uri(url: &str) -> bool {
    Url::parse(url)
        .map(|u| u.scheme() == "file")
        .unwrap_or(false)
}

/// Converts a `file://` URL to a filesystem path.
pub fn file_uri_to_path(url: &str) -> Option<PathBuf> {
    Url::parse(url).ok()?.to_file_path().ok()
}

/// Per-URL download locks, so concurrent requests for the same archive share
/// one download instead of racing.
#[derive(Default)]
pub struct DownloadLocks {
    locks: StdMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl DownloadLocks {
    pub fn new() -> Self {
        Self::default()
    }

    async fn acquire(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
            locks
                .entry(key.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }
}

/// Downloads `download_url` to a unique file under `temp_dir`, returning its
/// path. `file://` URLs are returned directly without any network I/O.
/// Downloads are cached indefinitely, which relies on resolvers returning
/// version-specific/immutable URLs.
pub async fn download_pkg_archive(
    client: &HttpClient,
    locks: &DownloadLocks,
    download_url: &str,
    headers: &[(&str, &str)],
    temp_dir: &Path,
) -> Result<PathBuf, DownloadError> {
    if is_file_uri(download_url) {
        return file_uri_to_path(download_url)
            .ok_or_else(|| DownloadError::InvalidFileUri(download_url.to_owned()));
    }

    let _guard = locks.acquire(download_url).await;

    let response = client
        .get(download_url, headers, CacheTtl::Indefinite)
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(DownloadError::Status {
            status: response.status,
            url: download_url.to_owned(),
        });
    }

    tokio::fs::create_dir_all(temp_dir).await?;
    let temp_path = temp_dir.join(format!("download-{}", unique_suffix()));
    tokio::fs::write(&temp_path, &response.body).await?;
    Ok(temp_path)
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}
