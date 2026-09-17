use std::{collections::HashMap, io::Write as _, sync::Mutex};

use chrono::{TimeZone, Utc};

use super::*;
use crate::model::{ChangelogFormat, Strategy};

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

fn make_zip_file(dir: &Path, filename: &str, folder: &str, toc_content: &[u8]) -> String {
    use zip::{ZipWriter, write::FileOptions};

    let path = dir.join(filename);
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = ZipWriter::new(file);
    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(format!("{folder}/{folder}.toc"), opts)
        .unwrap();
    zip.write_all(toc_content).unwrap();
    zip.finish().unwrap();

    format!("file://{}", path.display())
}

/// A test-only in-memory `Resolver`: no network, resolves aliases from a
/// fixed table, and supports every strategy so pin/version_eq paths are
/// exercisable without a real source's own strategy restrictions getting in
/// the way of the orchestration logic under test.
struct TestResolver {
    id: &'static str,
    candidates: Mutex<HashMap<String, PkgCandidate>>,
}

impl TestResolver {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            candidates: Mutex::new(HashMap::new()),
        }
    }

    fn with_candidate(self, alias: &str, candidate: PkgCandidate) -> Self {
        self.candidates
            .lock()
            .unwrap()
            .insert(alias.to_owned(), candidate);
        self
    }
}

#[async_trait::async_trait]
impl Resolver for TestResolver {
    fn metadata(&self) -> crate::model::SourceMetadata {
        crate::model::SourceMetadata {
            id: self.id,
            name: "Test",
            strategies: &[
                Strategy::AnyFlavour,
                Strategy::AnyReleaseType,
                Strategy::VersionEq,
            ],
            changelog_format: ChangelogFormat::Raw,
            addon_toc_key: None,
        }
    }

    async fn resolve_one_impl(
        &self,
        _http: &HttpClient,
        _flavour: Flavour,
        defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        self.candidates
            .lock()
            .unwrap()
            .get(&defn.alias)
            .cloned()
            .ok_or_else(|| ManagerError::PkgNonexistent.into())
    }
}

/// A resolver with no strategy support, to exercise `pin`'s rejection path.
struct NoStrategiesResolver;

#[async_trait::async_trait]
impl Resolver for NoStrategiesResolver {
    fn metadata(&self) -> crate::model::SourceMetadata {
        crate::model::SourceMetadata {
            id: "nostrat",
            name: "NoStrategies",
            strategies: &[],
            changelog_format: ChangelogFormat::Raw,
            addon_toc_key: None,
        }
    }

    async fn resolve_one_impl(
        &self,
        _http: &HttpClient,
        _flavour: Flavour,
        _defn: &Defn,
    ) -> AnyOutcome<PkgCandidate> {
        Err(ManagerError::PkgNonexistent.into())
    }
}

fn candidate(id: &str, slug: &str, version: &str, download_url: String) -> PkgCandidate {
    PkgCandidate {
        id: id.to_owned(),
        slug: slug.to_owned(),
        name: format!("{slug} display"),
        description: "a fine addon".to_owned(),
        url: format!("https://example.invalid/{slug}"),
        download_url,
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: version.to_owned(),
        changelog_url: "data:,changelog".to_owned(),
        deps: Vec::new(),
    }
}

struct Harness {
    _tmp: tempfile::TempDir,
    addon_dir: PathBuf,
    cache_dir: PathBuf,
    lock: LockFile,
    http: HttpClient,
    locks: DownloadLocks,
    progress: ProgressBus,
    sources: Vec<Box<dyn Resolver>>,
}

impl Harness {
    fn new(sources: Vec<Box<dyn Resolver>>) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let addon_dir = tmp.path().join("AddOns");
        let cache_dir = tmp.path().join("cache");
        std::fs::create_dir_all(&addon_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        Self {
            _tmp: tmp,
            addon_dir,
            cache_dir,
            lock: LockFile::in_memory(),
            http: HttpClient::new().unwrap(),
            locks: DownloadLocks::new(),
            progress: ProgressBus::new(),
            sources,
        }
    }
}

/// Builds a `Ctx` by directly projecting `$h`'s fields inline at the call
/// site, so the borrow checker sees disjoint borrows of `$h.http`/`sources`/…
/// alongside a separate `&mut $h.lock` argument in the same call — going
/// through a `&self` method here would borrow all of `$h` and conflict.
macro_rules! ctx {
    ($h:expr) => {
        Ctx {
            http: &$h.http,
            sources: &$h.sources,
            download_locks: &$h.locks,
            addon_dir: &$h.addon_dir,
            cache_dir: &$h.cache_dir,
            progress: &$h.progress,
            flavour: Flavour::Mainline,
        }
    };
}

fn defn(source: &str, alias: &str) -> Defn {
    Defn::new(source, alias)
}

// ---------------------------------------------------------------------------
// install
// ---------------------------------------------------------------------------

#[tokio::test]
async fn install_fresh_addon_writes_files_and_db_row() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);

    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let outcome = results.get(&defn("test", "foo")).unwrap();
    assert!(matches!(
        outcome,
        Ok(Outcome::PkgInstalled { dry_run: false, .. })
    ));
    assert!(h.addon_dir.join("Foo").join("Foo.toc").is_file());

    let pkgs = h.lock.get_all_pkgs();
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].slug, "foo");
}

#[tokio::test]
async fn install_already_installed_is_reported_without_reresolving() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);

    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;
    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Err(Failure::Manager(ManagerError::PkgAlreadyInstalled))
    ));
}

#[tokio::test]
async fn install_dry_run_makes_no_changes() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);

    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, true).await;
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Ok(Outcome::PkgInstalled { dry_run: true, .. })
    ));
    assert!(h.lock.get_all_pkgs().is_empty());
    assert!(!h.addon_dir.join("Foo").exists());
}

#[tokio::test]
async fn install_conflicts_with_installed_package() {
    let url_dir = tempfile::tempdir().unwrap();
    let url_a = make_zip_file(url_dir.path(), "a.zip", "Shared", b"## Interface: 110000");
    let url_b = make_zip_file(url_dir.path(), "b.zip", "Shared", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test")
            .with_candidate("a", candidate("1", "a", "1.0.0", url_a))
            .with_candidate("b", candidate("2", "b", "1.0.0", url_b)),
    )]);

    install(&mut h.lock, &ctx!(h), &[defn("test", "a")], false, false).await;
    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "b")], false, false).await;

    assert!(matches!(
        results.get(&defn("test", "b")).unwrap(),
        Err(Failure::Manager(
            ManagerError::PkgConflictsWithInstalled { .. }
        ))
    ));
}

#[tokio::test]
async fn install_conflicts_with_unreconciled_folder_unless_replace() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    std::fs::create_dir_all(h.addon_dir.join("Foo")).unwrap();
    std::fs::write(h.addon_dir.join("Foo").join("hand-placed.txt"), b"x").unwrap();

    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Err(Failure::Manager(
            ManagerError::PkgConflictsWithUnreconciled { .. }
        ))
    ));

    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], true, false).await;
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Ok(Outcome::PkgInstalled { .. })
    ));
    assert!(h.addon_dir.join("Foo").join("Foo.toc").is_file());
}

#[tokio::test]
async fn install_unknown_source_is_source_invalid() {
    let mut h = Harness::new(vec![]);
    let results = install(&mut h.lock, &ctx!(h), &[defn("bogus", "foo")], false, false).await;
    assert!(matches!(
        results.get(&defn("bogus", "foo")).unwrap(),
        Err(Failure::Manager(ManagerError::PkgSourceInvalid))
    ));
}

// ---------------------------------------------------------------------------
// update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_all_installs_new_version_when_changed() {
    let url_dir = tempfile::tempdir().unwrap();
    let url_v1 = make_zip_file(url_dir.path(), "v1.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url_v1)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let url_v2 = make_zip_file(
        url_dir.path(),
        "v2.zip",
        "Foo",
        b"## Interface: 110000\n## Version: 2",
    );
    h.sources = vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "2.0.0", url_v2)),
    )];
    let results = update(&mut h.lock, &ctx!(h), UpdateTarget::All, false).await;

    let outcome = results.values().next().unwrap();
    assert!(matches!(outcome, Ok(Outcome::PkgUpdated { .. })));
    let pkgs = h.lock.get_all_pkgs();
    assert_eq!(pkgs[0].version, "2.0.0");
}

#[tokio::test]
async fn update_reports_up_to_date_when_version_unchanged() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let results = update(&mut h.lock, &ctx!(h), UpdateTarget::All, false).await;
    let outcome = results.values().next().unwrap();
    assert!(matches!(
        outcome,
        Err(Failure::Manager(ManagerError::PkgUpToDate {
            is_pinned: false
        }))
    ));
}

#[tokio::test]
async fn update_reinstalls_when_folder_missing_even_if_version_unchanged() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    std::fs::remove_dir_all(h.addon_dir.join("Foo")).unwrap();

    let results = update(&mut h.lock, &ctx!(h), UpdateTarget::All, false).await;
    let outcome = results.values().next().unwrap();
    assert!(matches!(outcome, Ok(Outcome::PkgUpdated { .. })));
    assert!(h.addon_dir.join("Foo").join("Foo.toc").is_file());
}

#[tokio::test]
async fn update_specific_defn_not_installed_reports_not_installed() {
    let mut h = Harness::new(vec![Box::new(TestResolver::new("test"))]);
    let results = update(
        &mut h.lock,
        &ctx!(h),
        UpdateTarget::Specific(vec![defn("test", "foo")]),
        false,
    )
    .await;
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Err(Failure::Manager(ManagerError::PkgNotInstalled))
    ));
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

#[tokio::test]
async fn remove_deletes_db_row_and_trashes_folder_by_default() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let results = remove(&mut h.lock, &h.addon_dir, &[defn("test", "foo")], false);
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Ok(Outcome::PkgRemoved { .. })
    ));
    assert!(h.lock.get_all_pkgs().is_empty());
    assert!(!h.addon_dir.join("Foo").exists());
}

#[tokio::test]
async fn remove_keep_folders_leaves_files_on_disk() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    remove(&mut h.lock, &h.addon_dir, &[defn("test", "foo")], true);
    assert!(h.addon_dir.join("Foo").join("Foo.toc").is_file());
}

#[tokio::test]
async fn remove_refuses_to_trash_a_folder_name_that_escapes_addon_dir() {
    // Simulates a lock file row corrupted (or crafted, pre-fix) with a
    // folder name of ".." — `addon_dir.join("..")` resolves to addon_dir's
    // *parent*. `remove` must refuse to trash that instead of deleting
    // everything alongside the addon directory.
    let mut h = Harness::new(vec![]);
    h.lock.insert_pkg(Pkg {
        source: "test".into(),
        id: "1".into(),
        slug: "foo".into(),
        name: "Foo".into(),
        description: "".into(),
        url: "".into(),
        download_url: "".into(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1.0.0".into(),
        changelog_url: "".into(),
        options: PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![PkgFolder { name: "..".into() }],
        deps: vec![],
    });

    let results = remove(&mut h.lock, &h.addon_dir, &[defn("test", "foo")], false);
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Err(Failure::Internal(_))
    ));
    // The lock row must survive the refusal — losing track of the package
    // while leaving its (unremoved) folder behind would be worse.
    assert_eq!(h.lock.get_all_pkgs().len(), 1);
    // The sibling `cache` dir, one level up from `addon_dir`, must be intact.
    assert!(h.cache_dir.exists());
}

#[tokio::test]
async fn remove_not_installed_is_reported() {
    let mut h = Harness::new(vec![]);
    let results = remove(&mut h.lock, &h.addon_dir, &[defn("test", "foo")], false);
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Err(Failure::Manager(ManagerError::PkgNotInstalled))
    ));
}

// ---------------------------------------------------------------------------
// pin
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pin_flips_version_eq_when_matching_installed_version() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let d = defn("test", "foo").with_version("1.0.0");
    let results = pin(&mut h.lock, &h.sources, std::slice::from_ref(&d));
    assert!(matches!(
        results.get(&d).unwrap(),
        Ok(Outcome::PkgInstalled { .. })
    ));

    let pkgs = h.lock.get_all_pkgs();
    assert!(pkgs[0].options.version_eq);
}

#[tokio::test]
async fn pin_errors_when_requested_version_does_not_match_installed() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let d = defn("test", "foo").with_version("9.9.9");
    let results = pin(&mut h.lock, &h.sources, std::slice::from_ref(&d));
    assert!(matches!(
        results.get(&d).unwrap(),
        Err(Failure::Manager(ManagerError::PkgFilesNotMatching { .. }))
    ));
}

#[tokio::test]
async fn pin_unsupported_source_strategy_is_rejected() {
    let mut h = Harness::new(vec![Box::new(NoStrategiesResolver)]);
    let results = pin(&mut h.lock, &h.sources, &[defn("nostrat", "foo")]);
    assert!(matches!(
        results.get(&defn("nostrat", "foo")).unwrap(),
        Err(Failure::Manager(
            ManagerError::PkgStrategiesUnsupported { .. }
        ))
    ));
}

#[tokio::test]
async fn pin_not_installed_is_reported() {
    let mut h = Harness::new(vec![Box::new(TestResolver::new("test"))]);
    let results = pin(&mut h.lock, &h.sources, &[defn("test", "foo")]);
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Err(Failure::Manager(ManagerError::PkgNotInstalled))
    ));
}

// ---------------------------------------------------------------------------
// resolve (with_deps) / replace
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_follows_one_level_of_dependencies() {
    let mut cand = candidate("1", "foo", "1.0.0", String::new());
    cand.deps = vec!["2".to_owned()];
    let h = Harness::new(vec![Box::new(
        TestResolver::new("test")
            .with_candidate("foo", cand)
            .with_candidate("2", candidate("2", "bar", "1.0.0", String::new())),
    )]);

    let results = resolve(&ctx!(h), &[defn("test", "foo")], true).await;
    // The primary defn plus its one dependency.
    assert_eq!(results.len(), 2);
    let dep_entry = results.iter().find(|(d, _)| d.alias == "bar");
    assert!(dep_entry.is_some());
}

#[tokio::test]
async fn replace_switches_installed_package_to_new_source() {
    let url_dir = tempfile::tempdir().unwrap();
    let url_old = make_zip_file(url_dir.path(), "old.zip", "Foo", b"## Interface: 110000");
    let url_new = make_zip_file(url_dir.path(), "new.zip", "Foo", b"## Interface: 110000");

    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("old-source")
            .with_candidate("foo", candidate("1", "foo", "1.0.0", url_old)),
    )]);
    install(
        &mut h.lock,
        &ctx!(h),
        &[defn("old-source", "foo")],
        false,
        false,
    )
    .await;

    h.sources = vec![Box::new(
        TestResolver::new("new-source")
            .with_candidate("foo", candidate("9", "foo", "2.0.0", url_new)),
    )];

    let pairs = vec![(defn("old-source", "foo"), defn("new-source", "foo"))];
    let results = replace(&mut h.lock, &ctx!(h), &pairs).await.unwrap();

    assert!(matches!(
        results.get(&defn("old-source", "foo")).unwrap(),
        Ok(Outcome::PkgRemoved { .. })
    ));
    assert!(matches!(
        results.get(&defn("new-source", "foo")).unwrap(),
        Ok(Outcome::PkgInstalled { .. })
    ));

    let pkgs = h.lock.get_all_pkgs();
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].source, "new-source");
}

// ---------------------------------------------------------------------------
// Outcome::Display
// ---------------------------------------------------------------------------

#[test]
fn outcome_display_messages() {
    let pkg = crate::lockfile::Pkg {
        source: "test".into(),
        id: "1".into(),
        slug: "foo".into(),
        name: "Foo".into(),
        description: "".into(),
        url: "".into(),
        download_url: "".into(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1.0.0".into(),
        changelog_url: "".into(),
        options: PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![],
        deps: vec![],
    };
    assert_eq!(
        Outcome::PkgInstalled {
            pkg: pkg.clone(),
            dry_run: false
        }
        .to_string(),
        "installed 1.0.0"
    );
    assert_eq!(
        Outcome::PkgInstalled {
            pkg: pkg.clone(),
            dry_run: true
        }
        .to_string(),
        "would have installed 1.0.0"
    );
    assert_eq!(
        Outcome::PkgRemoved { pkg: pkg.clone() }.to_string(),
        "removed"
    );

    let mut new_pkg = pkg.clone();
    new_pkg.version = "2.0.0".into();
    assert_eq!(
        Outcome::PkgUpdated {
            old: pkg,
            new: Box::new(new_pkg),
            dry_run: false
        }
        .to_string(),
        "updated 1.0.0 to 2.0.0"
    );
}
