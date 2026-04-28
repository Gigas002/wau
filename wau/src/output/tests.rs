use std::path::PathBuf;

use libwau::fs::InstalledAddon;
use libwau::toc::TocFile;

use super::*;

fn make_addon(folder: &str, title: Option<&str>, version: Option<&str>) -> InstalledAddon {
    InstalledAddon {
        folder: folder.to_owned(),
        toc_files: vec![TocFile {
            path: PathBuf::from(format!("{folder}/{folder}.toc")),
            title: title.map(str::to_owned),
            version: version.map(str::to_owned),
            interface: vec![110200],
            ..Default::default()
        }],
    }
}

#[test]
fn format_row_with_title_and_version() {
    let addon = make_addon("WeakAuras", Some("WeakAuras"), Some("5.0.0"));
    let row = format_addon_row(&addon);
    assert!(row.contains("WeakAuras"));
    assert!(row.contains("5.0.0"));
}

#[test]
fn format_row_no_version_shows_dash() {
    let addon = make_addon("SomeAddon", Some("SomeAddon"), None);
    let row = format_addon_row(&addon);
    assert!(row.ends_with("  -"));
}

#[test]
fn format_row_no_title_uses_folder() {
    let addon = make_addon("FolderName", None, Some("1.0"));
    let row = format_addon_row(&addon);
    assert!(row.starts_with("FolderName"));
}
