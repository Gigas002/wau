use std::{collections::HashMap, io::Write as _, sync::Mutex};

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
        date_published: "2026-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
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
    trash: PathBuf,
}

impl Harness {
    /// How many things have been moved to the trash.
    fn trashed(&self) -> usize {
        std::fs::read_dir(&self.trash).map_or(0, |d| d.count())
    }

    fn new(sources: Vec<Box<dyn Resolver>>) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let addon_dir = tmp.path().join("AddOns");
        let cache_dir = tmp.path().join("cache");
        std::fs::create_dir_all(&addon_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        crate::fs::set_test_trash_root(&tmp.path().join("trash"));

        Self {
            trash: tmp.path().join("trash"),
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
        ctx!($h, crate::fs::Removal::Trash)
    };
    ($h:expr, $removal:expr) => {
        Ctx {
            http: &$h.http,
            sources: &$h.sources,
            download_locks: &$h.locks,
            addon_dir: &$h.addon_dir,
            cache_dir: &$h.cache_dir,
            progress: &$h.progress,
            flavour: Flavour::Mainline,
            removal: $removal,
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
async fn install_refuses_an_archive_with_no_addon_folder() {
    // A source-code zip nests the addon one level down (`repo-main/Foo/Foo.toc`). It used to be
    // reported as installed with nothing extracted and no folders recorded.
    use zip::{ZipWriter, write::FileOptions};
    let url_dir = tempfile::tempdir().unwrap();
    let path = url_dir.path().join("nested.zip");
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("repo-main/Foo/Foo.toc", FileOptions::<()>::default())
        .unwrap();
    zip.write_all(b"## Interface: 110000").unwrap();
    zip.finish().unwrap();
    let url = format!("file://{}", path.display());
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);

    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    assert!(
        matches!(
            results.get(&defn("test", "foo")).unwrap(),
            Err(Failure::Manager(ManagerError::PkgFilesMissing { .. }))
        ),
        "{results:?}"
    );
    assert!(h.lock.get_all_pkgs().is_empty());
}

#[tokio::test]
async fn downloads_are_removed_from_staging_after_install_and_update() {
    // Every download was written to <cache>/staging/download-<pid>-<nanos> and never deleted,
    // on top of the HTTP cache's own copy, so staging grew by one archive per install/update.
    let zip_dir = tempfile::tempdir().unwrap();
    let read_zip = |name: &str, version: &[u8]| {
        let url = make_zip_file(zip_dir.path(), name, "Foo", version);
        std::fs::read(pkg_archives::file_uri_to_path(&url).unwrap()).unwrap()
    };
    let mut server = mockito::Server::new_async().await;
    let _v1 = server
        .mock("GET", "/foo-1.zip")
        .with_body(read_zip("1.zip", b"## Version: 1"))
        .create_async()
        .await;
    let _v2 = server
        .mock("GET", "/foo-2.zip")
        .with_body(read_zip("2.zip", b"## Version: 2"))
        .create_async()
        .await;
    let local = make_zip_file(zip_dir.path(), "bar.zip", "Bar", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test")
            .with_candidate(
                "foo",
                candidate("1", "foo", "1.0.0", format!("{}/foo-1.zip", server.url())),
            )
            .with_candidate("bar", candidate("2", "bar", "1.0.0", local.clone())),
    )]);
    let staged = |h: &Harness| -> usize {
        std::fs::read_dir(h.cache_dir.join("staging")).map_or(0, |d| d.count())
    };

    install(
        &mut h.lock,
        &ctx!(h),
        &[defn("test", "foo"), defn("test", "bar")],
        false,
        false,
    )
    .await;
    assert_eq!(h.lock.get_all_pkgs().len(), 2);
    assert_eq!(staged(&h), 0);
    // The user's own file:// archive is never deleted.
    assert!(pkg_archives::file_uri_to_path(&local).unwrap().is_file());

    h.sources = vec![Box::new(TestResolver::new("test").with_candidate(
        "foo",
        candidate("1", "foo", "2.0.0", format!("{}/foo-2.zip", server.url())),
    ))];
    let results = update(
        &mut h.lock,
        &ctx!(h),
        UpdateTarget::Specific(vec![defn("test", "foo")]),
        false,
    )
    .await;
    assert!(
        matches!(
            results.get(&defn("test", "foo")).unwrap(),
            Ok(Outcome::PkgUpdated { .. })
        ),
        "{results:?}"
    );
    assert_eq!(
        std::fs::read(h.addon_dir.join("Foo/Foo.toc")).unwrap(),
        b"## Version: 2"
    );
    assert_eq!(staged(&h), 0);
}

/// `make_zip_file`, with the `.toc`'s stored bytes changed after the CRC was written: the
/// archive opens and lists fine, but extracting it fails partway.
fn make_corrupt_zip_file(dir: &Path, filename: &str, folder: &str) -> String {
    let url = make_zip_file(dir, filename, folder, b"## Version: corrupt-me");
    let path = pkg_archives::file_uri_to_path(&url).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes.windows(10).position(|w| w == b"corrupt-me").unwrap();
    bytes[at..at + 10].copy_from_slice(b"CORRUPT-ME");
    std::fs::write(&path, bytes).unwrap();
    url
}

#[tokio::test]
async fn update_with_a_corrupt_archive_keeps_the_old_folders() {
    // The old folders were trashed before extracting, so a CRC error left a half-written
    // folder in place of a working addon, with the lock still naming the old version.
    let url_dir = tempfile::tempdir().unwrap();
    let v1 = make_zip_file(url_dir.path(), "v1.zip", "Foo", b"## Version: 1");
    let v2 = make_corrupt_zip_file(url_dir.path(), "v2.zip", "Foo");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", v1)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;
    h.sources = vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "2.0.0", v2)),
    )];

    let results = update(
        &mut h.lock,
        &ctx!(h),
        UpdateTarget::Specific(vec![defn("test", "foo")]),
        false,
    )
    .await;

    assert!(
        matches!(
            results.get(&defn("test", "foo")).unwrap(),
            Err(Failure::Internal(_))
        ),
        "{results:?}"
    );
    assert_eq!(
        std::fs::read(h.addon_dir.join("Foo/Foo.toc")).unwrap(),
        b"## Version: 1"
    );
    assert_eq!(h.lock.get_all_pkgs()[0].version, "1.0.0");
    let names: Vec<String> = std::fs::read_dir(&h.addon_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, vec!["Foo".to_owned()]);
}

#[tokio::test]
async fn install_with_a_corrupt_archive_leaves_nothing_behind() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_corrupt_zip_file(url_dir.path(), "foo.zip", "Foo");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);

    let results = install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    assert!(results.get(&defn("test", "foo")).unwrap().is_err());
    assert!(h.lock.get_all_pkgs().is_empty());
    assert_eq!(std::fs::read_dir(&h.addon_dir).unwrap().count(), 0);
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
    assert_eq!(h.trashed(), 1);
}

#[tokio::test]
async fn update_with_removal_delete_deletes_the_old_folders_without_trashing_them() {
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
    let results = update(
        &mut h.lock,
        &ctx!(h, Removal::Delete),
        UpdateTarget::All,
        false,
    )
    .await;

    let outcome = results.values().next().unwrap();
    assert!(matches!(outcome, Ok(Outcome::PkgUpdated { .. })));
    assert_eq!(
        std::fs::read(h.addon_dir.join("Foo").join("Foo.toc")).unwrap(),
        b"## Interface: 110000\n## Version: 2"
    );
    assert_eq!(h.trashed(), 0);
}

#[tokio::test]
async fn update_refuses_a_folder_another_package_of_the_same_source_owns() {
    // curse:1's new version also ships `Bar`, which curse:2 owns. With `Bar` missing from disk
    // the update went through, leaving both packages owning `Bar`.
    let url_dir = tempfile::tempdir().unwrap();
    let foo_v1 = make_zip_file(url_dir.path(), "foo1.zip", "Foo", b"## Interface: 110000");
    let bar = make_zip_file(url_dir.path(), "bar.zip", "Bar", b"## Interface: 110000");
    let foo_v2 = {
        use zip::{ZipWriter, write::FileOptions};
        let path = url_dir.path().join("foo2.zip");
        let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
        for folder in ["Foo", "Bar"] {
            zip.start_file(
                format!("{folder}/{folder}.toc"),
                FileOptions::<()>::default(),
            )
            .unwrap();
            zip.write_all(b"## Interface: 110000").unwrap();
        }
        zip.finish().unwrap();
        format!("file://{}", path.display())
    };
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test")
            .with_candidate("foo", candidate("1", "foo", "1.0.0", foo_v1))
            .with_candidate("bar", candidate("2", "bar", "1.0.0", bar)),
    )]);
    install(
        &mut h.lock,
        &ctx!(h),
        &[defn("test", "foo"), defn("test", "bar")],
        false,
        false,
    )
    .await;
    std::fs::remove_dir_all(h.addon_dir.join("Bar")).unwrap();
    h.sources = vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "2.0.0", foo_v2)),
    )];

    let results = update(
        &mut h.lock,
        &ctx!(h),
        UpdateTarget::Specific(vec![defn("test", "foo")]),
        false,
    )
    .await;

    assert!(
        matches!(
            results.get(&defn("test", "foo")).unwrap(),
            Err(Failure::Manager(
                ManagerError::PkgConflictsWithInstalled { .. }
            ))
        ),
        "{results:?}"
    );
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
// plan_update / apply_update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn plan_update_previews_without_mutating_disk_or_lock() {
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

    let plan = plan_update(&h.lock, &ctx!(h), UpdateTarget::All).await;
    assert!(!plan.is_empty());
    let preview = plan.preview();
    assert!(matches!(
        preview.values().next().unwrap(),
        Outcome::PkgUpdated { old, new, dry_run: true } if old.version == "1.0.0" && new.version == "2.0.0"
    ));

    // Nothing on disk or in the lock file has changed yet.
    assert_eq!(h.lock.get_all_pkgs()[0].version, "1.0.0");
    assert_eq!(
        std::fs::read_to_string(h.addon_dir.join("Foo").join("Foo.toc")).unwrap(),
        "## Interface: 110000"
    );
}

#[tokio::test]
async fn apply_update_installs_the_version_a_plan_previewed() {
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

    let plan = plan_update(&h.lock, &ctx!(h), UpdateTarget::All).await;
    let results = apply_update(&mut h.lock, &ctx!(h), plan).await;

    let outcome = results.values().next().unwrap();
    assert!(matches!(outcome, Ok(Outcome::PkgUpdated { .. })));
    assert_eq!(h.lock.get_all_pkgs()[0].version, "2.0.0");
}

#[tokio::test]
async fn update_plan_is_empty_when_nothing_needs_updating() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let plan = plan_update(&h.lock, &ctx!(h), UpdateTarget::All).await;
    assert!(plan.is_empty());
    assert!(plan.preview().is_empty());
    assert!(matches!(
        plan.results.values().next().unwrap(),
        Err(Failure::Manager(ManagerError::PkgUpToDate {
            is_pinned: false
        }))
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

    let results = remove(
        &mut h.lock,
        &h.addon_dir,
        &[defn("test", "foo")],
        false,
        Removal::Trash,
    );
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Ok(Outcome::PkgRemoved { .. })
    ));
    assert!(h.lock.get_all_pkgs().is_empty());
    assert!(!h.addon_dir.join("Foo").exists());
    assert_eq!(h.trashed(), 1);
}

#[tokio::test]
async fn remove_with_removal_delete_deletes_the_folder_without_trashing_it() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    let results = remove(
        &mut h.lock,
        &h.addon_dir,
        &[defn("test", "foo")],
        false,
        Removal::Delete,
    );
    assert!(matches!(
        results.get(&defn("test", "foo")).unwrap(),
        Ok(Outcome::PkgRemoved { .. })
    ));
    assert!(h.lock.get_all_pkgs().is_empty());
    assert!(!h.addon_dir.join("Foo").exists());
    assert_eq!(h.trashed(), 0);
}

#[tokio::test]
async fn remove_keep_folders_leaves_files_on_disk() {
    let url_dir = tempfile::tempdir().unwrap();
    let url = make_zip_file(url_dir.path(), "foo.zip", "Foo", b"## Interface: 110000");
    let mut h = Harness::new(vec![Box::new(
        TestResolver::new("test").with_candidate("foo", candidate("1", "foo", "1.0.0", url)),
    )]);
    install(&mut h.lock, &ctx!(h), &[defn("test", "foo")], false, false).await;

    remove(
        &mut h.lock,
        &h.addon_dir,
        &[defn("test", "foo")],
        true,
        Removal::Trash,
    );
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
        date_published: "2026-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
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

    let results = remove(
        &mut h.lock,
        &h.addon_dir,
        &[defn("test", "foo")],
        false,
        Removal::Trash,
    );
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
    let results = remove(
        &mut h.lock,
        &h.addon_dir,
        &[defn("test", "foo")],
        false,
        Removal::Trash,
    );
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
async fn resolve_follows_a_cross_source_dependency_string() {
    // `git`'s `addbuild.toml` `depends` are cross-source `source:alias`
    // strings (unlike every other source's same-source numeric dep ids) —
    // `parse_dep_defn` tells the two apart by whether the id contains a `:`.
    let mut cand = candidate("1", "foo", "1.0.0", String::new());
    cand.deps = vec!["other:bar".to_owned()];
    let h = Harness::new(vec![
        Box::new(TestResolver::new("test").with_candidate("foo", cand)),
        Box::new(
            TestResolver::new("other")
                .with_candidate("bar", candidate("2", "bar", "1.0.0", String::new())),
        ),
    ]);

    let results = resolve(&ctx!(h), &[defn("test", "foo")], true).await;
    assert_eq!(results.len(), 2);
    let dep_entry = results
        .iter()
        .find(|(d, _)| d.source == "other" && d.alias == "bar");
    assert!(dep_entry.is_some(), "{results:?}");
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

#[tokio::test]
async fn replace_installs_the_new_packages_dependencies() {
    // Dependencies are resolved along with the new package but have no old package to
    // replace; replace used to panic on them ("every candidate has an inverse old defn"),
    // possibly after the old package had already been removed.
    let url_dir = tempfile::tempdir().unwrap();
    let url_old = make_zip_file(url_dir.path(), "old.zip", "Foo", b"## Interface: 110000");
    let url_new = make_zip_file(url_dir.path(), "new.zip", "Foo", b"## Interface: 110000");
    let url_dep = make_zip_file(url_dir.path(), "dep.zip", "Lib", b"## Interface: 110000");
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
    let mut new = candidate("9", "foo", "2.0.0", url_new);
    new.deps = vec!["7".to_owned()];
    h.sources = vec![Box::new(
        TestResolver::new("new-source")
            .with_candidate("foo", new)
            .with_candidate("7", candidate("7", "lib", "1.0.0", url_dep)),
    )];

    let pairs = vec![(defn("old-source", "foo"), defn("new-source", "foo"))];
    let results = replace(&mut h.lock, &ctx!(h), &pairs).await.unwrap();

    assert!(results.values().all(Result::is_ok), "{results:?}");
    let mut installed: Vec<(String, String)> = h
        .lock
        .get_all_pkgs()
        .into_iter()
        .map(|p| (p.source, p.id))
        .collect();
    installed.sort();
    assert_eq!(
        installed,
        vec![
            ("new-source".to_owned(), "7".to_owned()),
            ("new-source".to_owned(), "9".to_owned())
        ]
    );
    assert!(h.addon_dir.join("Foo").is_dir());
    assert!(h.addon_dir.join("Lib").is_dir());
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
        date_published: "2026-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
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
