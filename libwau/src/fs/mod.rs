//! Minimal filesystem safety helpers shared across `libwau`.
//!
//! Removed content is moved into a temp "trash" location rather than
//! hard-deleted. This is the only safety net for destructive operations —
//! there is no SavedVariables backup/restore feature.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
mod tests;

/// Moves `path` into a unique subdirectory of the system temp dir instead of
/// deleting it, returning the new location. `path` may be a file or directory.
///
/// No OS trash-can integration — every platform gets the same temp-dir
/// move. The system temp dir is frequently on a different filesystem than
/// `path` (e.g. the addon directory lives on a separate drive/mount), so a
/// plain `rename` can fail with a cross-device error; when that happens this
/// falls back to a recursive copy followed by removing the original, and
/// only ever removes the original after the copy has fully succeeded.
pub fn trash(path: &Path) -> std::io::Result<PathBuf> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_owned());

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dest_dir = std::env::temp_dir()
        .join("wauT")
        .join(format!("deleted-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dest_dir)?;

    let dest = dest_dir.join(&name);
    if let Err(err) = std::fs::rename(path, &dest) {
        if err.kind() != std::io::ErrorKind::CrossesDevices {
            return Err(err);
        }
        if path.is_dir() {
            copy_dir_recursive(path, &dest)?;
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::copy(path, &dest)?;
            std::fs::remove_file(path)?;
        }
    }
    Ok(dest)
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// Whether `path` is strictly inside `root` (not equal to it) once both are
/// resolved to their canonical form. Used as a last line of defence before a
/// destructive operation on a path derived from untrusted or
/// possibly-corrupted data (e.g. an addon folder name read back out of the
/// lock file) — it must never resolve to `root` itself or something outside
/// it.
pub fn is_within(root: &Path, path: &Path) -> bool {
    let (Ok(root), Ok(path)) = (root.canonicalize(), path.canonicalize()) else {
        return false;
    };
    path != root && path.starts_with(&root)
}
