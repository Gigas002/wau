use super::*;
use crate::toc::TocFile;

// ---------------------------------------------------------------------------
// extract_addon_zip
// ---------------------------------------------------------------------------

fn write_zip(dir: &std::path::Path, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
    let zip_bytes = make_test_zip(entries);
    let path = dir.join("test.zip");
    std::fs::write(&path, zip_bytes).unwrap();
    path
}

#[test]
fn extract_addon_zip_finds_addon_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = write_zip(
        dir.path(),
        &[
            ("MyAddon/", &[]),
            ("MyAddon/MyAddon.toc", b"## Interface: 110200\n"),
            ("MyAddon/main.lua", b""),
            ("MyAddonHelper/", &[]),
            ("MyAddonHelper/MyAddonHelper.toc", b"## Interface: 110200\n"),
        ],
    );
    let extract_dir = dir.path().join("extracted");
    std::fs::create_dir_all(&extract_dir).unwrap();
    let addon_dirs = extract_addon_zip(&zip_path, &extract_dir).unwrap();
    let names: Vec<&str> = addon_dirs
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["MyAddon", "MyAddonHelper"]);
}

#[test]
fn extract_addon_zip_skips_dirs_without_toc() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = write_zip(
        dir.path(),
        &[
            ("README.md", b"# hello"),
            ("MyAddon/", &[]),
            ("MyAddon/MyAddon.toc", b"## Interface: 110200\n"),
        ],
    );
    let extract_dir = dir.path().join("extracted");
    std::fs::create_dir_all(&extract_dir).unwrap();
    let addon_dirs = extract_addon_zip(&zip_path, &extract_dir).unwrap();
    assert_eq!(addon_dirs.len(), 1);
}

// ---------------------------------------------------------------------------
// install_addon_dirs / remove_addon_dirs
// ---------------------------------------------------------------------------

#[test]
fn install_addon_dirs_copies_and_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let addons = dir.path().join("AddOns");

    // Create a source addon dir.
    let addon_src = src.join("TestAddon");
    std::fs::create_dir_all(&addon_src).unwrap();
    std::fs::write(addon_src.join("TestAddon.toc"), b"## Interface: 110200\n").unwrap();

    let installed = install_addon_dirs(std::slice::from_ref(&addon_src), &addons).unwrap();
    assert_eq!(installed, vec!["TestAddon"]);
    assert!(addons.join("TestAddon").join("TestAddon.toc").exists());

    // Second call replaces the existing dir.
    std::fs::write(addon_src.join("new.lua"), b"").unwrap();
    install_addon_dirs(std::slice::from_ref(&addon_src), &addons).unwrap();
    assert!(addons.join("TestAddon").join("new.lua").exists());
}

#[test]
fn remove_addon_dirs_deletes_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let addons = dir.path().join("AddOns");
    std::fs::create_dir_all(addons.join("TestAddon")).unwrap();

    remove_addon_dirs(&["TestAddon".to_owned()], &addons).unwrap();
    assert!(!addons.join("TestAddon").exists());
}

#[test]
fn remove_addon_dirs_tolerates_missing() {
    let dir = tempfile::tempdir().unwrap();
    let addons = dir.path().join("AddOns");
    std::fs::create_dir_all(&addons).unwrap();

    // Should not error even though the dir doesn't exist.
    remove_addon_dirs(&["Ghost".to_owned()], &addons).unwrap();
}

// ---------------------------------------------------------------------------
// InstalledAddon helpers
// ---------------------------------------------------------------------------

fn make_toc(title: Option<&str>, version: Option<&str>, interface: Vec<u32>) -> TocFile {
    TocFile {
        title: title.map(str::to_owned),
        version: version.map(str::to_owned),
        interface,
        ..Default::default()
    }
}

#[test]
fn display_title_uses_first_toc_with_title() {
    let addon = InstalledAddon {
        folder: "MyFolder".into(),
        toc_files: vec![
            make_toc(None, None, vec![]),
            make_toc(Some("Real Title"), Some("2.0"), vec![110200]),
        ],
    };
    assert_eq!(addon.display_title(), "Real Title");
}

#[test]
fn display_title_falls_back_to_folder() {
    let addon = InstalledAddon {
        folder: "FallbackFolder".into(),
        toc_files: vec![make_toc(None, None, vec![110200])],
    };
    assert_eq!(addon.display_title(), "FallbackFolder");
}

#[test]
fn version_returns_first_available() {
    let addon = InstalledAddon {
        folder: "x".into(),
        toc_files: vec![
            make_toc(None, None, vec![]),
            make_toc(None, Some("3.1.4"), vec![]),
        ],
    };
    assert_eq!(addon.version(), Some("3.1.4"));
}

#[test]
fn all_interface_versions_deduped_sorted() {
    let addon = InstalledAddon {
        folder: "x".into(),
        toc_files: vec![
            make_toc(None, None, vec![110200, 50503]),
            make_toc(None, None, vec![50503, 11508]),
        ],
    };
    assert_eq!(addon.all_interface_versions(), vec![11508, 50503, 110200]);
}

// ---------------------------------------------------------------------------
// scan
// ---------------------------------------------------------------------------

#[test]
fn scan_empty_dir_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let result = scan(dir.path()).unwrap();
    assert!(result.is_empty());
}

#[test]
fn scan_skips_dirs_without_toc() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = dir.path().join("NotAnAddon");
    std::fs::create_dir(&addon_dir).unwrap();
    std::fs::write(addon_dir.join("main.lua"), "").unwrap();

    let result = scan(dir.path()).unwrap();
    assert!(result.is_empty());
}

#[test]
fn scan_skips_files_at_top_level() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("stray.toc"), "").unwrap();

    let result = scan(dir.path()).unwrap();
    assert!(result.is_empty());
}

#[test]
fn scan_finds_addon_with_toc() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = dir.path().join("WeakAuras");
    std::fs::create_dir(&addon_dir).unwrap();
    std::fs::write(
        addon_dir.join("WeakAuras.toc"),
        "## Interface: 110200\n## Title: WeakAuras\n## Version: 5.0.0\n",
    )
    .unwrap();

    let result = scan(dir.path()).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].folder, "WeakAuras");
    assert_eq!(result[0].display_title(), "WeakAuras");
    assert_eq!(result[0].version(), Some("5.0.0"));
}

#[test]
fn scan_multiple_toc_files_in_one_dir() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = dir.path().join("BGE");
    std::fs::create_dir(&addon_dir).unwrap();
    std::fs::write(
        addon_dir.join("BGE_Mainline.toc"),
        "## Interface: 110200\n## Title: BGE\n## Version: 11.0\n",
    )
    .unwrap();
    std::fs::write(
        addon_dir.join("BGE_Vanilla.toc"),
        "## Interface: 11508\n## Title: BGE\n## Version: 11.0\n",
    )
    .unwrap();

    let result = scan(dir.path()).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].toc_files.len(), 2);
    assert_eq!(result[0].all_interface_versions(), vec![11508, 110200]);
}

#[test]
fn scan_results_sorted_alphabetically() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["Zebra", "Alpha", "Middle"] {
        let addon_dir = dir.path().join(name);
        std::fs::create_dir(&addon_dir).unwrap();
        std::fs::write(
            addon_dir.join(format!("{name}.toc")),
            "## Interface: 110200\n",
        )
        .unwrap();
    }

    let result = scan(dir.path()).unwrap();
    let names: Vec<&str> = result.iter().map(|a| a.folder.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Middle", "Zebra"]);
}

#[test]
fn scan_toc_contains_full_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let addon_dir = dir.path().join("Curse");
    std::fs::create_dir(&addon_dir).unwrap();
    std::fs::write(
        addon_dir.join("Curse.toc"),
        "## Interface: 110200\n## Title: Curse\n## X-Curse-Project-ID: 9999\n",
    )
    .unwrap();

    let result = scan(dir.path()).unwrap();
    let toc = &result[0].toc_files[0];
    assert_eq!(
        toc.x_fields.get("X-Curse-Project-ID").map(String::as_str),
        Some("9999")
    );
}
