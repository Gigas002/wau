//! SQLite persistence for installed packages — ports instawow's `pkg_db/*.py`.
//!
//! Raw SQL via `rusqlite` (no ORM), mirroring instawow's own approach —
//! both use hand-written SQL rather than fighting an ORM abstraction. Schema
//! versioning uses `PRAGMA user_version` + an ordered migration list, not a
//! separate migrations table (see the `migrations` submodule).

use std::path::Path;

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::Connection;

#[cfg(test)]
mod tests;

mod migrations;
mod models;
mod queries;

pub use migrations::CURRENT_VERSION;
pub use models::{Pkg, PkgDep, PkgFolder, PkgLoggedVersion, PkgOptions};
pub use queries::{
    check_pkgs_not_exist, delete_pkg, find_pkgs_owning_folders, find_pkgs_owning_folders_excluding,
    get_all_pkgs, get_pkg_logged_versions, get_pkgs, insert_pkg, pin_pkg,
};

/// Schema for a brand-new database — already includes the indexes instawow's
/// `_Migration_1` adds to an *older* (pre-index) database; a fresh install
/// never runs that migration in practice, only upgrades from it.
const SCHEMA: &str = "
CREATE TABLE pkg (
    source VARCHAR NOT NULL,
    id VARCHAR NOT NULL,
    slug VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    description VARCHAR NOT NULL,
    url VARCHAR NOT NULL,
    download_url VARCHAR NOT NULL,
    date_published DATETIME NOT NULL,
    version VARCHAR NOT NULL,
    changelog_url VARCHAR NOT NULL,
    PRIMARY KEY (source, id)
);

CREATE TABLE pkg_version_log (
    version VARCHAR NOT NULL,
    install_time DATETIME DEFAULT (CURRENT_TIMESTAMP) NOT NULL,
    pkg_source VARCHAR NOT NULL,
    pkg_id VARCHAR NOT NULL,
    PRIMARY KEY (version, pkg_source, pkg_id)
);
CREATE INDEX pkg_version_log_faux_fk ON pkg_version_log (pkg_source, pkg_id);

CREATE TABLE pkg_options (
    any_flavour BOOLEAN NOT NULL,
    any_release_type BOOLEAN NOT NULL,
    version_eq BOOLEAN NOT NULL,
    pkg_source VARCHAR NOT NULL,
    pkg_id VARCHAR NOT NULL,
    PRIMARY KEY (pkg_source, pkg_id),
    CONSTRAINT fk_pkg_options_pkg_source_and_id
        FOREIGN KEY (pkg_source, pkg_id)
        REFERENCES pkg (source, id)
        ON DELETE CASCADE
);
CREATE UNIQUE INDEX pkg_options_fk ON pkg_options (pkg_source, pkg_id);

CREATE TABLE pkg_folder (
    name VARCHAR NOT NULL,
    pkg_source VARCHAR NOT NULL,
    pkg_id VARCHAR NOT NULL,
    PRIMARY KEY (name),
    CONSTRAINT fk_pkg_folder_pkg_source_and_id
        FOREIGN KEY (pkg_source, pkg_id)
        REFERENCES pkg (source, id)
        ON DELETE CASCADE
);
CREATE INDEX pkg_folder_fk ON pkg_folder (pkg_source, pkg_id);

CREATE TABLE pkg_dep (
    id VARCHAR NOT NULL,
    pkg_source VARCHAR NOT NULL,
    pkg_id VARCHAR NOT NULL,
    PRIMARY KEY (id, pkg_source, pkg_id),
    CONSTRAINT fk_pkg_dep_pkg_source_and_id
        FOREIGN KEY (pkg_source, pkg_id)
        REFERENCES pkg (source, id)
        ON DELETE CASCADE
);
CREATE INDEX pkg_dep_fk ON pkg_dep (pkg_source, pkg_id);
";

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Opens (creating and/or migrating as needed) the package database at `path`.
pub fn prepare_database(path: &Path) -> Result<Connection, DbError> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;

    match get_version(&conn)? {
        None => create(&conn)?,
        Some(v) if v != CURRENT_VERSION => migrations::migrate(&mut conn, v, CURRENT_VERSION)?,
        Some(_) => {}
    }

    Ok(conn)
}

/// Opens an in-memory database, primarily for tests.
pub fn prepare_in_memory() -> Result<Connection, DbError> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    create(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;",
    )
}

fn get_version(conn: &Connection) -> rusqlite::Result<Option<u32>> {
    let has_pkg_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'pkg')",
        [],
        |row| row.get(0),
    )?;
    if !has_pkg_table {
        return Ok(None);
    }
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map(Some)
}

fn create(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)?;
    conn.pragma_update(None, "user_version", CURRENT_VERSION)
}

// ============================================================================
// Datetime storage format
// ============================================================================
//
// instawow stores UTC datetimes as `isoformat(' ')` on a tz-naive value: a
// space (not `T`) separator, no offset, and no fractional-second suffix when
// the microseconds are zero. Matched exactly here (not via rusqlite's
// `chrono` feature, whose default format differs) so a raw `ORDER BY
// install_time` or hand inspection of the DB behaves identically.

pub(crate) fn format_datetime(dt: &DateTime<Utc>) -> String {
    if dt.timestamp_subsec_micros() == 0 {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        dt.format("%Y-%m-%d %H:%M:%S%.6f").to_string()
    }
}

pub(crate) fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .map(|naive| naive.and_utc())
}
