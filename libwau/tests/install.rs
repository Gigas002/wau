//! Integration tests for the install/remove pipeline with local zip fixtures.
//! Gated on the `local` feature — skipped entirely when built with `--no-default-features`.
#![cfg(feature = "local")]

use std::{fs, io::Write as _, path::PathBuf};

use libwau::{
    lock::{self, Lock},
    manifest::ManifestAddon,
    model::{Channel, Flavor, Provider, Tag},
    ops,
    providers::{InstallContext, local::LocalProvider},
};

/// Creates a zip archive in memory from `(path, content)` pairs.
/// Paths ending with `/` become directory entries.
fn make_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Cursor;
    let buf = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let opts =
        zip::write::FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        if name.ends_with('/') {
            zip.add_directory(*name, opts).unwrap();
        } else {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_ctx(addons_path: PathBuf, cache_dir: PathBuf) -> InstallContext {
    InstallContext {
        tag: Tag::new("test"),
        flavor: Flavor::Retail,
        channel: Channel::Stable,
        addons_path,
        cache_dir,
    }
}

fn make_addon(name: &str, zip_path: &std::path::Path) -> ManifestAddon {
    ManifestAddon {
        name: name.into(),
        provider: Provider::Local,
        channel: Some(Channel::Stable),
        flavors: Some(vec![Flavor::Retail]),
        pin: None,
        project_id: None,
        wowi_id: None,
        repo: None,
        asset_regex: None,
        git_ref: None,
        url: Some(zip_path.to_str().unwrap().to_owned()),
    }
}

/// Writes a test zip containing the given `entries` and returns its path.
fn write_zip(dir: &std::path::Path, filename: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let bytes = make_test_zip(entries);
    let path = dir.join(filename);
    fs::write(&path, bytes).unwrap();
    path
}

// ---------------------------------------------------------------------------
// Basic install / remove cycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn install_and_remove_single_addon() {
    let dir = tempfile::tempdir().unwrap();
    let zip = write_zip(
        dir.path(),
        "WeakAuras.zip",
        &[
            ("WeakAuras/", &[]),
            (
                "WeakAuras/WeakAuras.toc",
                b"## Interface: 110200\n## Title: WeakAuras\n## Version: 5.0.0\n",
            ),
            ("WeakAuras/WeakAuras.lua", b"-- lua"),
        ],
    );

    let addons_dir = dir.path().join("AddOns");
    let cache_dir = dir.path().join("cache");
    let lock_path = dir.path().join("test.lock.toml");

    let provider = LocalProvider::new();
    let addon = make_addon("WeakAuras", &zip);
    let ctx = make_ctx(addons_dir.clone(), cache_dir);
    let mut lock = Lock::new(Tag::new("test"));

    // Install
    ops::install(&provider, &addon, &ctx, &mut lock)
        .await
        .unwrap();

    assert_eq!(lock.addon.len(), 1);
    let entry = &lock.addon[0];
    assert_eq!(entry.name, "WeakAuras");
    assert_eq!(entry.installed_dirs, vec!["WeakAuras"]);
    assert_eq!(entry.resolved_version, "local");
    assert!(addons_dir.join("WeakAuras").join("WeakAuras.toc").exists());

    lock::save(&lock, &lock_path).unwrap();

    // Reload lock and verify
    let reloaded = lock::load(&lock_path).unwrap();
    assert_eq!(reloaded.addon[0].name, "WeakAuras");

    // Remove
    ops::remove("WeakAuras", &ctx, &mut lock).await.unwrap();

    assert!(lock.addon.is_empty());
    assert!(!addons_dir.join("WeakAuras").exists());
}

// ---------------------------------------------------------------------------
// Multi-folder addon zip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn install_multi_folder_addon() {
    let dir = tempfile::tempdir().unwrap();
    let zip = write_zip(
        dir.path(),
        "Bagnon.zip",
        &[
            ("Bagnon/", &[]),
            (
                "Bagnon/Bagnon.toc",
                b"## Interface: 110200\n## Title: Bagnon\n",
            ),
            ("Bagnon_Config/", &[]),
            (
                "Bagnon_Config/Bagnon_Config.toc",
                b"## Interface: 110200\n## Title: Bagnon Config\n",
            ),
            ("Bagnon_Bags/", &[]),
            (
                "Bagnon_Bags/Bagnon_Bags.toc",
                b"## Interface: 110200\n## Title: Bagnon Bags\n",
            ),
        ],
    );

    let addons_dir = dir.path().join("AddOns");
    let cache_dir = dir.path().join("cache");

    let provider = LocalProvider::new();
    let addon = make_addon("Bagnon", &zip);
    let ctx = make_ctx(addons_dir.clone(), cache_dir);
    let mut lock = Lock::new(Tag::new("test"));

    ops::install(&provider, &addon, &ctx, &mut lock)
        .await
        .unwrap();

    let entry = &lock.addon[0];
    assert_eq!(entry.installed_dirs.len(), 3);
    assert!(addons_dir.join("Bagnon").exists());
    assert!(addons_dir.join("Bagnon_Config").exists());
    assert!(addons_dir.join("Bagnon_Bags").exists());
}

// ---------------------------------------------------------------------------
// Reinstall (update) replaces existing files + lock entry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reinstall_replaces_existing_dirs_and_lock_entry() {
    let dir = tempfile::tempdir().unwrap();
    let zip = write_zip(
        dir.path(),
        "MyAddon.zip",
        &[
            ("MyAddon/", &[]),
            (
                "MyAddon/MyAddon.toc",
                b"## Interface: 110200\n## Version: 2.0.0\n",
            ),
        ],
    );

    let addons_dir = dir.path().join("AddOns");
    let cache_dir = dir.path().join("cache");
    let provider = LocalProvider::new();
    let addon = make_addon("MyAddon", &zip);
    let ctx = make_ctx(addons_dir.clone(), cache_dir);
    let mut lock = Lock::new(Tag::new("test"));

    ops::install(&provider, &addon, &ctx, &mut lock)
        .await
        .unwrap();
    ops::install(&provider, &addon, &ctx, &mut lock)
        .await
        .unwrap();

    assert_eq!(lock.addon.len(), 1, "should not grow on reinstall");
    assert!(addons_dir.join("MyAddon").exists());
}

// ---------------------------------------------------------------------------
// file:// URL support
// ---------------------------------------------------------------------------

#[tokio::test]
async fn install_via_file_url() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = write_zip(
        dir.path(),
        "addon.zip",
        &[
            ("TestAddon/", &[]),
            ("TestAddon/TestAddon.toc", b"## Interface: 110200\n"),
        ],
    );

    let file_url = format!("file://{}", zip_path.to_str().unwrap());
    let mut addon = make_addon("TestAddon", &zip_path);
    addon.url = Some(file_url);

    let addons_dir = dir.path().join("AddOns");
    let provider = LocalProvider::new();
    let ctx = make_ctx(addons_dir.clone(), dir.path().join("cache"));
    let mut lock = Lock::new(Tag::new("test"));

    ops::install(&provider, &addon, &ctx, &mut lock)
        .await
        .unwrap();
    assert!(addons_dir.join("TestAddon").exists());
}

// ---------------------------------------------------------------------------
// Error paths
// ---------------------------------------------------------------------------

#[tokio::test]
async fn install_fails_when_url_missing() {
    let dir = tempfile::tempdir().unwrap();
    let provider = LocalProvider::new();
    let mut addon = make_addon("NoUrl", std::path::Path::new("/nonexistent.zip"));
    addon.url = None;

    let ctx = make_ctx(dir.path().join("AddOns"), dir.path().join("cache"));
    let mut lock = Lock::new(Tag::new("test"));

    let result = ops::install(&provider, &addon, &ctx, &mut lock).await;
    assert!(matches!(result, Err(libwau::Error::LocalMissingUrl { .. })));
}

#[tokio::test]
async fn install_fails_when_zip_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let provider = LocalProvider::new();
    let addon = make_addon("Missing", std::path::Path::new("/nonexistent/missing.zip"));

    let ctx = make_ctx(dir.path().join("AddOns"), dir.path().join("cache"));
    let mut lock = Lock::new(Tag::new("test"));

    let result = ops::install(&provider, &addon, &ctx, &mut lock).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn remove_fails_when_addon_not_in_lock() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = make_ctx(dir.path().join("AddOns"), dir.path().join("cache"));
    let mut lock = Lock::new(Tag::new("test"));

    let result = ops::remove("NotInstalled", &ctx, &mut lock).await;
    assert!(matches!(result, Err(libwau::Error::AddonNotInLock { .. })));
}

// ---------------------------------------------------------------------------
// Lock persistence across install + remove
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lock_round_trip_after_install() {
    let dir = tempfile::tempdir().unwrap();
    let zip = write_zip(
        dir.path(),
        "Questie.zip",
        &[
            ("Questie/", &[]),
            (
                "Questie/Questie.toc",
                b"## Interface: 11508\n## Title: Questie\n",
            ),
        ],
    );

    let addons_dir = dir.path().join("AddOns");
    let lock_path = dir.path().join("test.lock.toml");

    let provider = LocalProvider::new();
    let mut addon = make_addon("Questie", &zip);
    addon.flavors = Some(vec![Flavor::Era]);

    let ctx = InstallContext {
        tag: Tag::new("era"),
        flavor: Flavor::Era,
        channel: Channel::Stable,
        addons_path: addons_dir,
        cache_dir: dir.path().join("cache"),
    };
    let mut lock = Lock::new(Tag::new("era"));

    ops::install(&provider, &addon, &ctx, &mut lock)
        .await
        .unwrap();
    lock::save(&lock, &lock_path).unwrap();

    let reloaded = lock::load(&lock_path).unwrap();
    let entry = reloaded
        .addon
        .iter()
        .find(|a| a.name == "Questie")
        .expect("Questie must be in lock after install");

    assert_eq!(entry.flavor, Flavor::Era);
    assert_eq!(entry.installed_dirs, vec!["Questie"]);
    assert!(!entry.download_url.is_empty());
}
