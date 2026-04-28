use super::*;
use crate::{
    lock::Lock,
    manifest::ManifestAddon,
    model::{Channel, Flavor, Provider as ModelProvider, Tag},
    providers::{InstallContext, Provider, ResolvedArtifact},
};
use std::path::{Path, PathBuf};

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

fn make_addon(name: &str, url: &str) -> ManifestAddon {
    ManifestAddon {
        name: name.into(),
        provider: ModelProvider::Local,
        channel: None,
        flavors: None,
        pin: None,
        project_id: None,
        repo: None,
        asset_regex: None,
        git_ref: None,
        url: Some(url.to_owned()),
    }
}

/// A minimal provider stub that serves a pre-built zip from a given path.
struct ZipFileProvider {
    zip_path: PathBuf,
}

impl Provider for ZipFileProvider {
    fn resolve(
        &self,
        _addon: &ManifestAddon,
        _ctx: &InstallContext,
    ) -> crate::Result<ResolvedArtifact> {
        Ok(ResolvedArtifact {
            version: "test".into(),
            id: "test:1".into(),
            url: self.zip_path.to_str().unwrap().to_owned(),
            sha256: None,
        })
    }

    fn download(&self, _artifact: &ResolvedArtifact, dest: &Path) -> crate::Result<()> {
        fs::copy(&self.zip_path, dest)?;
        Ok(())
    }
}

fn make_addon_zip(dir: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
    let zip_bytes = crate::fs::make_test_zip(entries);
    let path = dir.join("addon.zip");
    fs::write(&path, zip_bytes).unwrap();
    path
}

// ---------------------------------------------------------------------------
// install
// ---------------------------------------------------------------------------

#[test]
fn install_updates_lock_and_copies_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = make_addon_zip(
        dir.path(),
        &[
            ("WeakAuras/", &[]),
            (
                "WeakAuras/WeakAuras.toc",
                b"## Interface: 110200\n## Title: WeakAuras\n",
            ),
            ("WeakAuras/WeakAuras.lua", b""),
        ],
    );

    let addons_dir = dir.path().join("AddOns");
    let cache_dir = dir.path().join("cache");

    let provider = ZipFileProvider { zip_path };
    let addon = make_addon("WeakAuras", "placeholder");
    let ctx = make_ctx(addons_dir.clone(), cache_dir);
    let mut lock = Lock::new(Tag::new("test"));

    install(&provider, &addon, &ctx, &mut lock).unwrap();

    assert_eq!(lock.addon.len(), 1);
    let entry = &lock.addon[0];
    assert_eq!(entry.name, "WeakAuras");
    assert_eq!(entry.installed_dirs, vec!["WeakAuras"]);
    assert!(addons_dir.join("WeakAuras").exists());
    assert!(addons_dir.join("WeakAuras").join("WeakAuras.toc").exists());
}

#[test]
fn install_twice_replaces_lock_entry() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = make_addon_zip(
        dir.path(),
        &[
            ("TestAddon/", &[]),
            ("TestAddon/TestAddon.toc", b"## Interface: 110200\n"),
        ],
    );

    let addons_dir = dir.path().join("AddOns");
    let cache_dir = dir.path().join("cache");

    let provider = ZipFileProvider { zip_path };
    let addon = make_addon("TestAddon", "placeholder");
    let ctx = make_ctx(addons_dir.clone(), cache_dir);
    let mut lock = Lock::new(Tag::new("test"));

    install(&provider, &addon, &ctx, &mut lock).unwrap();
    install(&provider, &addon, &ctx, &mut lock).unwrap();

    assert_eq!(
        lock.addon.len(),
        1,
        "second install should replace, not append"
    );
}

#[test]
fn install_fails_when_zip_has_no_addon_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = make_addon_zip(dir.path(), &[("README.md", b"no addons here")]);

    let addons_dir = dir.path().join("AddOns");
    let cache_dir = dir.path().join("cache");

    let provider = ZipFileProvider { zip_path };
    let addon = make_addon("Empty", "placeholder");
    let ctx = make_ctx(addons_dir, cache_dir);
    let mut lock = Lock::new(Tag::new("test"));

    let result = install(&provider, &addon, &ctx, &mut lock);
    assert!(matches!(
        result,
        Err(crate::Error::NoInstallableDirs { .. })
    ));
    assert!(lock.addon.is_empty());
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

#[test]
fn remove_deletes_dirs_and_updates_lock() {
    let dir = tempfile::tempdir().unwrap();
    let addons_dir = dir.path().join("AddOns");
    fs::create_dir_all(&addons_dir).unwrap();

    // Simulate an already-installed addon.
    let addon_dir = addons_dir.join("Questie");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(addon_dir.join("Questie.toc"), b"## Interface: 11508\n").unwrap();

    let mut lock = Lock::new(Tag::new("test"));
    lock.addon.push(crate::lock::LockedAddon {
        name: "Questie".into(),
        provider: ModelProvider::GitHub,
        flavor: Flavor::Era,
        channel: Channel::Stable,
        project_id: None,
        resolved_version: "6.5.0".into(),
        resolved_id: "v6.5.0".into(),
        download_url: "https://example.invalid/Questie.zip".into(),
        sha256: None,
        installed_dirs: vec!["Questie".into()],
        installed_at: chrono::Utc::now(),
    });

    let ctx = InstallContext {
        tag: Tag::new("test"),
        flavor: Flavor::Era,
        channel: Channel::Stable,
        addons_path: addons_dir.clone(),
        cache_dir: dir.path().join("cache"),
    };

    remove("Questie", &ctx, &mut lock).unwrap();

    assert!(lock.addon.is_empty());
    assert!(!addons_dir.join("Questie").exists());
}

#[test]
fn remove_returns_error_when_not_in_lock() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = make_ctx(dir.path().join("AddOns"), dir.path().join("cache"));
    let mut lock = Lock::new(Tag::new("test"));

    let result = remove("NoSuchAddon", &ctx, &mut lock);
    assert!(matches!(result, Err(crate::Error::AddonNotInLock { .. })));
}

#[test]
fn remove_tolerates_missing_dir() {
    let dir = tempfile::tempdir().unwrap();
    let addons_dir = dir.path().join("AddOns");
    fs::create_dir_all(&addons_dir).unwrap();

    let mut lock = Lock::new(Tag::new("test"));
    lock.addon.push(crate::lock::LockedAddon {
        name: "Gone".into(),
        provider: ModelProvider::Local,
        flavor: Flavor::Retail,
        channel: Channel::Stable,
        project_id: None,
        resolved_version: "1.0".into(),
        resolved_id: "local:/tmp/gone.zip".into(),
        download_url: "/tmp/gone.zip".into(),
        sha256: None,
        installed_dirs: vec!["Gone".into()],
        installed_at: chrono::Utc::now(),
    });

    let ctx = make_ctx(addons_dir, dir.path().join("cache"));
    // Should not error even though the dir is already absent.
    remove("Gone", &ctx, &mut lock).unwrap();
    assert!(lock.addon.is_empty());
}
