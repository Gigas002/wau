//! Addon zip inspection and extraction.

use std::{collections::HashSet, fs, io, path::Path};

#[cfg(test)]
mod tests;

mod download;

pub use download::{
    DownloadError, DownloadLocks, download_pkg_archive, file_uri_to_path, is_file_uri,
};

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("IO: {0}")]
    Io(#[from] io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// An opened addon archive: the top-level folders it contains, and an
/// [`OpenArchive::extract`] method that unpacks only those folders (nothing
/// else — READMEs, `.github/`, and similar top-level junk are skipped).
pub struct OpenArchive {
    pub top_level_folders: HashSet<String>,
    archive_path: std::path::PathBuf,
}

/// Finds top-level addon folders in a flat list of archive member paths: a
/// member counts when it sits exactly one level deep (`Folder/File.toc`, not
/// nested further) and its filename starts with the folder name and ends in
/// `.toc` (case-insensitive) — matching the WoW convention `MyAddon/MyAddon.toc`.
pub fn find_archive_addon_tocs<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Vec<(String, String)> {
    names
        .into_iter()
        .filter_map(|name| {
            if name.matches('/').count() != 1 {
                return None;
            }
            let (head, tail) = name.split_once('/')?;
            let is_toc = tail.len() >= 4 && tail[tail.len() - 4..].eq_ignore_ascii_case(".toc");
            if is_toc && tail.starts_with(head) {
                Some((name.to_owned(), head.to_owned()))
            } else {
                None
            }
        })
        .collect()
}

/// Opens `archive_path` and determines its top-level addon folders, without
/// extracting anything yet.
pub fn open_zip_archive(archive_path: &Path) -> Result<OpenArchive, ArchiveError> {
    let file = fs::File::open(archive_path)?;
    let zip = zip::ZipArchive::new(file)?;
    let top_level_folders = find_archive_addon_tocs(zip.file_names())
        .into_iter()
        .map(|(_, head)| head)
        .collect();
    Ok(OpenArchive {
        top_level_folders,
        archive_path: archive_path.to_owned(),
    })
}

impl OpenArchive {
    /// Extracts every member under [`Self::top_level_folders`] into `dest`,
    /// skipping anything else in the archive. Uses `enclosed_name()` for the
    /// extraction path (rather than the raw member string) to reject
    /// zip-slip path traversal; for well-formed archives this makes no
    /// behavioural difference.
    pub fn extract(&self, dest: &Path) -> Result<(), ArchiveError> {
        let file = fs::File::open(&self.archive_path)?;
        let mut zip = zip::ZipArchive::new(file)?;

        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(enclosed) = entry.enclosed_name() else {
                continue;
            };
            let Some(head) = enclosed.components().next() else {
                continue;
            };
            let head = head.as_os_str().to_string_lossy();
            if !self.top_level_folders.contains(head.as_ref()) {
                continue;
            }

            let out_path = dest.join(&enclosed);
            if entry.is_dir() {
                fs::create_dir_all(&out_path)?;
            } else {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out_file = fs::File::create(&out_path)?;
                io::copy(&mut entry, &mut out_file)?;
            }
        }

        Ok(())
    }
}
