//! Schema version history.
//!
//! Versioning uses SQLite's `PRAGMA user_version`, not a separate migrations
//! table. A fresh database is created with the current schema already
//! including the indexes added below; the migrations here only matter for
//! upgrading/downgrading an existing database created at an older version.

use rusqlite::Connection;

/// The schema version a fresh database is created at.
pub const CURRENT_VERSION: u32 = 1;

/// One schema version's forward/backward SQL.
struct Migration {
    upgrade: &'static [&'static str],
    downgrade: &'static [&'static str],
}

/// 1-indexed: `MIGRATIONS[i]` moves a database from version `i - 1` to `i`
/// (upgrade) or back (downgrade). This first migration adds four
/// non-critical indexes that a fresh schema now creates inline.
const MIGRATIONS: &[Migration] = &[Migration {
    upgrade: &[
        "CREATE UNIQUE INDEX pkg_options_fk ON pkg_options (pkg_source, pkg_id)",
        "CREATE INDEX pkg_folder_fk ON pkg_folder (pkg_source, pkg_id)",
        "CREATE INDEX pkg_dep_fk ON pkg_dep (pkg_source, pkg_id)",
        "CREATE INDEX pkg_version_log_faux_fk ON pkg_version_log (pkg_source, pkg_id)",
    ],
    downgrade: &[
        "DROP INDEX pkg_options_fk",
        "DROP INDEX pkg_folder_fk",
        "DROP INDEX pkg_dep_fk",
        "DROP INDEX pkg_version_log_faux_fk",
    ],
}];

/// Migrates `conn` from `current_version` to `new_version`, running each
/// intermediate version's `upgrade` (ascending) or `downgrade` (descending)
/// statements inside one transaction.
///
/// The `foreign_keys` pragma is a no-op once a transaction is open (SQLite
/// only honours it outside any pending `BEGIN`), so it's toggled off on the
/// plain connection before opening the transaction, and always restored
/// afterwards even on error.
pub(super) fn migrate(
    conn: &mut Connection,
    current_version: u32,
    new_version: u32,
) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = OFF")?;

    let result = (|| -> rusqlite::Result<()> {
        let tx = conn.transaction()?;

        if new_version > current_version {
            for version in (current_version + 1)..=new_version {
                for stmt in MIGRATIONS[(version - 1) as usize].upgrade {
                    tx.execute_batch(stmt)?;
                }
            }
        } else {
            for version in ((new_version + 1)..=current_version).rev() {
                for stmt in MIGRATIONS[(version - 1) as usize].downgrade {
                    tx.execute_batch(stmt)?;
                }
            }
        }

        tx.execute_batch("PRAGMA foreign_key_check")?;
        tx.pragma_update(None, "user_version", new_version)?;
        tx.commit()
    })();

    conn.execute_batch("PRAGMA foreign_keys = ON")?;
    result
}
