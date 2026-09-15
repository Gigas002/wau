//! On-disk response cache store, backed by a small `rusqlite` table.

use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, params};

use super::CacheTtl;

/// A stored (or freshly fetched) HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub struct Cache {
    conn: Mutex<Connection>,
}

impl Cache {
    pub fn open(parent_dir: &Path) -> rusqlite::Result<Self> {
        std::fs::create_dir_all(parent_dir).ok();
        let conn = Connection::open(parent_dir.join("http_cache.sqlite"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS response (
                key TEXT PRIMARY KEY,
                status INTEGER NOT NULL,
                headers TEXT NOT NULL,
                body BLOB NOT NULL,
                expires_at INTEGER
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory cache, for tests.
    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE response (
                key TEXT PRIMARY KEY,
                status INTEGER NOT NULL,
                headers TEXT NOT NULL,
                body BLOB NOT NULL,
                expires_at INTEGER
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Returns the cached response for `key`, evicting and returning `None`
    /// if it has expired.
    pub fn get(&self, key: &str) -> Option<CachedResponse> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let row: Option<(u16, String, Vec<u8>, Option<i64>)> = conn
            .query_row(
                "SELECT status, headers, body, expires_at FROM response WHERE key = ?1",
                params![key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .ok()
            .flatten();

        let (status, headers_json, body, expires_at) = row?;

        if let Some(expires_at) = expires_at
            && expires_at <= now_millis()
        {
            let _ = conn.execute("DELETE FROM response WHERE key = ?1", params![key]);
            return None;
        }

        let headers: Vec<(String, String)> = serde_json::from_str(&headers_json).ok()?;
        Some(CachedResponse {
            status,
            headers,
            body,
        })
    }

    /// Stores `response` under `key` per `ttl`. A `CacheTtl::Never` call is a no-op.
    pub fn put(&self, key: &str, response: &CachedResponse, ttl: CacheTtl) {
        let expires_at = match ttl {
            CacheTtl::Never => return,
            CacheTtl::Indefinite => None,
            CacheTtl::For(duration) => Some(now_millis() + duration.as_millis() as i64),
        };

        let Ok(headers_json) = serde_json::to_string(&response.headers) else {
            return;
        };

        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let _ = conn.execute(
            "INSERT OR REPLACE INTO response (key, status, headers, body, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                key,
                response.status,
                headers_json,
                response.body,
                expires_at
            ],
        );
    }

    /// Clears every cached response (`wau cache clear`).
    pub fn clear(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM response", [])?;
        Ok(())
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
