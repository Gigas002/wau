use std::path::PathBuf;

use libwau::fs::InstalledAddon;
use libwau::manifest::ManifestAddon;
use libwau::model::Provider;
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

fn make_manifest_addon(name: &str, provider: Provider) -> ManifestAddon {
    ManifestAddon {
        name: name.to_owned(),
        provider,
        channel: None,
        flavors: None,
        pin: None,
        project_id: None,
        wowi_id: None,
        repo: None,
        asset_regex: None,
        git_ref: None,
        url: None,
    }
}

// ── format_addon_row ─────────────────────────────────────────────────────────

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

// ── quiet suppression ────────────────────────────────────────────────────────

#[test]
fn print_installed_quiet_does_not_panic() {
    // just verify the function exists and runs without side effects
    print_installed("TestAddon", true);
}

#[test]
fn print_removed_quiet_does_not_panic() {
    print_removed("TestAddon", true);
}

#[test]
fn print_sync_summary_quiet_does_not_panic() {
    print_sync_summary(3, 1, true);
}

// ── sync plan ────────────────────────────────────────────────────────────────

#[test]
fn print_sync_plan_does_not_panic() {
    let a = make_manifest_addon("WeakAuras", Provider::CurseForge);
    let b = make_manifest_addon("Details", Provider::GitHub);
    print_sync_plan(&[&a, &b]);
}

// ── remove plan ──────────────────────────────────────────────────────────────

#[test]
fn print_remove_plan_does_not_panic() {
    let names = vec!["WeakAuras".to_owned(), "Bagnon".to_owned()];
    print_remove_plan(&names);
}

// ── search results ───────────────────────────────────────────────────────────

#[test]
fn print_search_results_no_matches_does_not_panic() {
    print_search_results("xyzzy", &[], &[]);
}

#[test]
fn print_search_results_with_matches_does_not_panic() {
    let m = make_manifest_addon("WeakAuras", Provider::CurseForge);
    let a = make_addon("WeakAuras", Some("WeakAuras"), Some("5.0.0"));
    print_search_results("aura", &[&m], &[&a]);
}
