//! Core CRUD queries — ports the DB-touching functions of instawow's
//! `pkg_management.py` (`get_pkgs`, `_check_pkgs_not_exist`, `_insert_pkg`,
//! `_delete_pkg`, `_mutate_pin`, `get_pkg_logged_versions`, and the
//! folder-conflict checks from `_mutate_install`/`_mutate_update`).
//!
//! Orchestration (resolve/install/update/remove) lives in `pkg_management`
//! (a later phase); this module only owns direct table access.

use rusqlite::{Connection, Row, params, params_from_iter, types::Value};

use super::{
    DbError, format_datetime,
    models::{Pkg, PkgDep, PkgFolder, PkgLoggedVersion, PkgOptions},
    parse_datetime,
};
use crate::model::Defn;

/// The `pkg` table's own columns, before the `pkg_options`/`pkg_folder`/`pkg_dep`
/// follow-up queries that turn it into a full [`Pkg`].
struct BasePkg {
    source: String,
    id: String,
    slug: String,
    name: String,
    description: String,
    url: String,
    download_url: String,
    date_published: String,
    version: String,
    changelog_url: String,
}

const BASE_COLUMNS: &str = "pkg.source, pkg.id, pkg.slug, pkg.name, pkg.description, pkg.url, \
     pkg.download_url, pkg.date_published, pkg.version, pkg.changelog_url";

fn row_to_base(row: &Row<'_>) -> rusqlite::Result<BasePkg> {
    Ok(BasePkg {
        source: row.get(0)?,
        id: row.get(1)?,
        slug: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        url: row.get(5)?,
        download_url: row.get(6)?,
        date_published: row.get(7)?,
        version: row.get(8)?,
        changelog_url: row.get(9)?,
    })
}

/// Same as [`row_to_base`] but for a `LEFT JOIN` result where an unmatched
/// `defn` row means every `pkg.*` column is `NULL`.
fn row_to_optional_base(row: &Row<'_>) -> rusqlite::Result<Option<BasePkg>> {
    let source: Option<String> = row.get(0)?;
    let Some(source) = source else {
        return Ok(None);
    };
    Ok(Some(BasePkg {
        source,
        id: row.get(1)?,
        slug: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        url: row.get(5)?,
        download_url: row.get(6)?,
        date_published: row.get(7)?,
        version: row.get(8)?,
        changelog_url: row.get(9)?,
    }))
}

fn invalid_datetime(column: &'static str) -> DbError {
    DbError::Sqlite(rusqlite::Error::InvalidColumnType(
        0,
        column.to_owned(),
        rusqlite::types::Type::Text,
    ))
}

/// Fetches `pkg_options`/`pkg_folder`/`pkg_dep` for `base` and assembles a full [`Pkg`].
fn hydrate(conn: &Connection, base: BasePkg) -> Result<Pkg, DbError> {
    let options = conn.query_row(
        "SELECT any_flavour, any_release_type, version_eq \
         FROM pkg_options WHERE pkg_source = ?1 AND pkg_id = ?2",
        params![base.source, base.id],
        |row| {
            Ok(PkgOptions {
                any_flavour: row.get(0)?,
                any_release_type: row.get(1)?,
                version_eq: row.get(2)?,
            })
        },
    )?;

    let folders: Vec<PkgFolder> = {
        let mut stmt =
            conn.prepare("SELECT name FROM pkg_folder WHERE pkg_source = ?1 AND pkg_id = ?2")?;
        stmt.query_map(params![base.source, base.id], |row| {
            Ok(PkgFolder { name: row.get(0)? })
        })?
        .collect::<rusqlite::Result<_>>()?
    };

    let deps: Vec<PkgDep> = {
        let mut stmt =
            conn.prepare("SELECT id FROM pkg_dep WHERE pkg_source = ?1 AND pkg_id = ?2")?;
        stmt.query_map(params![base.source, base.id], |row| {
            Ok(PkgDep { id: row.get(0)? })
        })?
        .collect::<rusqlite::Result<_>>()?
    };

    let date_published =
        parse_datetime(&base.date_published).ok_or_else(|| invalid_datetime("date_published"))?;

    Ok(Pkg {
        source: base.source,
        id: base.id,
        slug: base.slug,
        name: base.name,
        description: base.description,
        url: base.url,
        download_url: base.download_url,
        date_published,
        version: base.version,
        changelog_url: base.changelog_url,
        options,
        folders,
        deps,
    })
}

fn in_placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

fn values_placeholders(n: usize) -> String {
    vec!["(?, ?, ?)"; n].join(", ")
}

fn bind_defns(defns: &[Defn]) -> Vec<Value> {
    let mut bind = Vec::with_capacity(defns.len() * 3);
    for d in defns {
        bind.push(Value::Text(d.source.clone()));
        bind.push(Value::Text(d.alias.clone()));
        bind.push(d.id.clone().map(Value::Text).unwrap_or(Value::Null));
    }
    bind
}

/// The alias/id/slug join `pkg_management.get_pkgs` and `_check_pkgs_not_exist`
/// share: a `defn` matches a `pkg` row when `pkg.id` equals the defn's `alias`
/// *or* its `id`, or `pkg.slug` case-insensitively equals the defn's `alias`.
const DEFN_JOIN_PREDICATE: &str =
    "pkg.id = defn.alias OR pkg.id = defn.id OR lower(pkg.slug) = lower(defn.alias)";

/// Looks up one `Pkg` per `defn`, in the same order, `None` where no package
/// is installed for that definition (matches by source + alias/id/slug).
pub fn get_pkgs(conn: &Connection, defns: &[Defn]) -> Result<Vec<Option<Pkg>>, DbError> {
    if defns.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "WITH defn (source, alias, id) AS (VALUES {})
         SELECT {BASE_COLUMNS}
         FROM defn
         LEFT JOIN pkg ON pkg.source = defn.source AND ({DEFN_JOIN_PREDICATE})",
        values_placeholders(defns.len()),
    );

    let bind = bind_defns(defns);
    let mut stmt = conn.prepare(&sql)?;
    let bases: Vec<Option<BasePkg>> = stmt
        .query_map(params_from_iter(bind.iter()), row_to_optional_base)?
        .collect::<rusqlite::Result<_>>()?;

    bases
        .into_iter()
        .map(|maybe_base| maybe_base.map(|base| hydrate(conn, base)).transpose())
        .collect()
}

/// Every installed package.
pub fn get_all_pkgs(conn: &Connection) -> Result<Vec<Pkg>, DbError> {
    let sql = format!("SELECT {BASE_COLUMNS} FROM pkg");
    let mut stmt = conn.prepare(&sql)?;
    let bases: Vec<BasePkg> = stmt
        .query_map([], row_to_base)?
        .collect::<rusqlite::Result<_>>()?;
    bases.into_iter().map(|b| hydrate(conn, b)).collect()
}

/// For each `defn`, `true` if no package is currently installed for it.
pub fn check_pkgs_not_exist(conn: &Connection, defns: &[Defn]) -> Result<Vec<bool>, DbError> {
    if defns.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "WITH defn (source, alias, id) AS (VALUES {})
         SELECT NOT EXISTS (
             SELECT 1 FROM pkg WHERE pkg.source = defn.source AND ({DEFN_JOIN_PREDICATE})
         )
         FROM defn",
        values_placeholders(defns.len()),
    );

    let bind = bind_defns(defns);
    let mut stmt = conn.prepare(&sql)?;
    let result: Vec<bool> = stmt
        .query_map(params_from_iter(bind.iter()), |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(result)
}

/// Last 10 logged versions for a package, most recent first.
pub fn get_pkg_logged_versions(
    conn: &Connection,
    source: &str,
    id: &str,
) -> Result<Vec<PkgLoggedVersion>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT version, install_time FROM pkg_version_log
         WHERE pkg_source = ?1 AND pkg_id = ?2
         ORDER BY install_time DESC
         LIMIT 10",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![source, id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    rows.into_iter()
        .map(|(version, install_time)| {
            parse_datetime(&install_time)
                .map(|install_time| PkgLoggedVersion {
                    version,
                    install_time,
                })
                .ok_or_else(|| invalid_datetime("install_time"))
        })
        .collect()
}

/// Inserts `pkg`'s `pkg`/`pkg_options`/`pkg_folder`/`pkg_dep` rows, and logs its
/// version (`INSERT OR IGNORE`, since re-installing the same version shouldn't
/// duplicate/refresh the log entry). Callers wrap this in a transaction.
pub fn insert_pkg(conn: &Connection, pkg: &Pkg) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO pkg (
            source, id, slug, name, description, url, download_url, date_published, version, changelog_url
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            pkg.source,
            pkg.id,
            pkg.slug,
            pkg.name,
            pkg.description,
            pkg.url,
            pkg.download_url,
            format_datetime(&pkg.date_published),
            pkg.version,
            pkg.changelog_url,
        ],
    )?;

    conn.execute(
        "INSERT INTO pkg_options (any_flavour, any_release_type, version_eq, pkg_source, pkg_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            pkg.options.any_flavour,
            pkg.options.any_release_type,
            pkg.options.version_eq,
            pkg.source,
            pkg.id,
        ],
    )?;

    {
        let mut stmt =
            conn.prepare("INSERT INTO pkg_folder (name, pkg_source, pkg_id) VALUES (?1, ?2, ?3)")?;
        for folder in &pkg.folders {
            stmt.execute(params![folder.name, pkg.source, pkg.id])?;
        }
    }

    if !pkg.deps.is_empty() {
        let mut stmt =
            conn.prepare("INSERT INTO pkg_dep (id, pkg_source, pkg_id) VALUES (?1, ?2, ?3)")?;
        for dep in &pkg.deps {
            stmt.execute(params![dep.id, pkg.source, pkg.id])?;
        }
    }

    conn.execute(
        "INSERT OR IGNORE INTO pkg_version_log (version, pkg_source, pkg_id) VALUES (?1, ?2, ?3)",
        params![pkg.version, pkg.source, pkg.id],
    )?;

    Ok(())
}

/// `DELETE FROM pkg WHERE source = ? AND id = ?` — cascades to `pkg_options`,
/// `pkg_folder`, `pkg_dep`; leaves `pkg_version_log` intact (no FK, deliberately
/// survives deletion so reinstall history is preserved).
pub fn delete_pkg(conn: &Connection, source: &str, id: &str) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM pkg WHERE source = ?1 AND id = ?2",
        params![source, id],
    )?;
    Ok(())
}

/// Flips `pkg_options.version_eq` for an installed package, returning the new
/// value. instawow "does not have true pinning" — this only sets the flag;
/// the pinned version is whatever `pkg.version` already says.
pub fn pin_pkg(
    conn: &Connection,
    source: &str,
    id: &str,
    version_eq: bool,
) -> Result<bool, DbError> {
    let result: bool = conn.query_row(
        "UPDATE pkg_options SET version_eq = ?1 WHERE pkg_source = ?2 AND pkg_id = ?3 \
         RETURNING version_eq",
        params![version_eq, source, id],
        |row| row.get(0),
    )?;
    Ok(result)
}

/// Installed packages that own any of `folder_names` (the `_mutate_install`
/// conflict check: a new archive can't claim a folder another package tracks).
pub fn find_pkgs_owning_folders(
    conn: &Connection,
    folder_names: &[String],
) -> Result<Vec<Pkg>, DbError> {
    if folder_names.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT DISTINCT {BASE_COLUMNS}
         FROM pkg
         JOIN pkg_folder ON pkg_folder.pkg_source = pkg.source AND pkg_folder.pkg_id = pkg.id
         WHERE pkg_folder.name IN ({})",
        in_placeholders(folder_names.len()),
    );

    let bind: Vec<Value> = folder_names.iter().cloned().map(Value::Text).collect();
    let mut stmt = conn.prepare(&sql)?;
    let bases: Vec<BasePkg> = stmt
        .query_map(params_from_iter(bind.iter()), row_to_base)?
        .collect::<rusqlite::Result<_>>()?;
    bases.into_iter().map(|b| hydrate(conn, b)).collect()
}

/// Same conflict check as [`find_pkgs_owning_folders`], for the `_mutate_update`
/// path, excluding the package being updated. Ported verbatim including its
/// `AND` (not `OR`) exclusion clause: a *different* package that happens to
/// share `exclude_source` (with a different id) is also excluded from the
/// results — matches instawow exactly, not "fixed."
pub fn find_pkgs_owning_folders_excluding(
    conn: &Connection,
    folder_names: &[String],
    exclude_source: &str,
    exclude_id: &str,
) -> Result<Vec<Pkg>, DbError> {
    if folder_names.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT DISTINCT {BASE_COLUMNS}
         FROM pkg
         JOIN pkg_folder ON pkg_folder.pkg_source = pkg.source AND pkg_folder.pkg_id = pkg.id
         WHERE (pkg_folder.pkg_source != ?1 AND pkg_folder.pkg_id != ?2)
           AND (pkg_folder.name IN ({}))",
        in_placeholders(folder_names.len()),
    );

    let mut bind: Vec<Value> = vec![
        Value::Text(exclude_source.to_owned()),
        Value::Text(exclude_id.to_owned()),
    ];
    bind.extend(folder_names.iter().cloned().map(Value::Text));

    let mut stmt = conn.prepare(&sql)?;
    let bases: Vec<BasePkg> = stmt
        .query_map(params_from_iter(bind.iter()), row_to_base)?
        .collect::<rusqlite::Result<_>>()?;
    bases.into_iter().map(|b| hydrate(conn, b)).collect()
}
