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
/// move.
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
    std::fs::rename(path, &dest)?;
    Ok(dest)
}
