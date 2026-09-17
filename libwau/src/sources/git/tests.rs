use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use super::*;
use crate::{
    model::Strategy,
    pkg_archives::open_zip_archive,
    results::{Failure, ManagerError},
    sources::resolve_one,
};

const SUCCESS_SCRIPT: &str = "#!/bin/sh\n\
set -e\n\
echo x >> \"$startdir/build-count\"\n\
mkdir -p \"$pkgdir/$pkgname\"\n\
printf '## Interface: 110000\\n' > \"$pkgdir/$pkgname/$pkgname.toc\"\n";

const FAILING_SCRIPT: &str = "#!/bin/sh\necho boom >&2\nexit 1\n";

fn defn(alias: &str) -> Defn {
    Defn::new("git", alias)
}

async fn git_cmd(dir: &Path, args: &[&str]) {
    let status = tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .await
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

async fn init_source_repo(dir: &Path) {
    tokio::fs::create_dir_all(dir).await.unwrap();
    git_cmd(dir, &["init", "--quiet", "--initial-branch", "main"]).await;
    git_cmd(dir, &["config", "user.email", "test@example.invalid"]).await;
    git_cmd(dir, &["config", "user.name", "test"]).await;
    tokio::fs::write(dir.join("addon.lua"), "-- v1")
        .await
        .unwrap();
    git_cmd(dir, &["add", "."]).await;
    git_cmd(dir, &["commit", "--quiet", "-m", "v1"]).await;
}

async fn commit_more(dir: &Path, contents: &str) {
    tokio::fs::write(dir.join("addon.lua"), contents)
        .await
        .unwrap();
    git_cmd(dir, &["add", "."]).await;
    git_cmd(dir, &["commit", "--quiet", "-m", "more"]).await;
}

/// Writes `<addbuilds_dir>/<addname>/{addbuild.toml,build.sh}`, `chmod +x`ing
/// the script, and returns the addbuild directory.
fn write_addbuild(
    addbuilds_dir: &Path,
    addname: &str,
    url: &str,
    extra: &str,
    script: &str,
) -> PathBuf {
    let dir = addbuilds_dir.join(addname);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("addbuild.toml"),
        format!("addname = \"{addname}\"\nurl = \"{url}\"\nbuild = \"build.sh\"\n{extra}"),
    )
    .unwrap();
    let script_path = dir.join("build.sh");
    std::fs::write(&script_path, script).unwrap();
    let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms).unwrap();
    dir
}

#[test]
fn git_declares_any_flavour_and_version_eq_strategies() {
    let r = GitResolver::new(PathBuf::new(), PathBuf::new());
    let meta = Resolver::metadata(&r);
    assert_eq!(meta.id, "git");
    assert!(meta.strategies.contains(&Strategy::AnyFlavour));
    assert!(meta.strategies.contains(&Strategy::VersionEq));
    assert!(meta.addon_toc_key.is_none());
}

#[tokio::test]
async fn resolve_builds_and_packages_a_fresh_repo() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(addbuilds.path(), "foo", &url, "", SUCCESS_SCRIPT);

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    let candidate = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();

    assert_eq!(candidate.id, "foo");
    assert_eq!(candidate.slug, "foo");
    assert!(candidate.version.starts_with("r1."));

    let path = crate::pkg_archives::file_uri_to_path(&candidate.download_url).unwrap();
    let archive = open_zip_archive(&path).unwrap();
    assert!(archive.top_level_folders.contains("foo"));
}

#[tokio::test]
async fn resolve_missing_addbuild_is_nonexistent() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();

    let err = resolve_one(&resolver, &http, Flavour::Mainline, &defn("nope"))
        .await
        .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

#[tokio::test]
async fn resolve_addname_mismatch_is_internal_error() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let dir = addbuilds.path().join("foo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("addbuild.toml"),
        "addname = \"bar\"\nurl = \"x\"\nbuild = \"build.sh\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("build.sh"), SUCCESS_SCRIPT).unwrap();

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    let err = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap_err();
    assert!(matches!(err, Failure::Internal(_)));
}

#[tokio::test]
async fn resolve_flavour_not_supported_is_rejected_and_any_flavour_bypasses() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(
        addbuilds.path(),
        "foo",
        &url,
        "flavors = [\"mainline\"]\n",
        SUCCESS_SCRIPT,
    );

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();

    let err = resolve_one(&resolver, &http, Flavour::VanillaClassic, &defn("foo"))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::Manager(ManagerError::PkgFilesNotMatching { .. })
    ));

    let mut any_flavour = defn("foo");
    any_flavour.strategies.any_flavour = true;
    let ok = resolve_one(&resolver, &http, Flavour::VanillaClassic, &any_flavour).await;
    assert!(ok.is_ok(), "{ok:?}");
}

#[tokio::test]
async fn resolve_missing_makedepends_is_internal_error() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(
        addbuilds.path(),
        "foo",
        &url,
        "makedepends = [\"definitely-not-a-real-binary-xyz\"]\n",
        SUCCESS_SCRIPT,
    );

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    let err = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Failure::Internal(e) if e.0.contains("definitely-not-a-real-binary-xyz"))
    );
}

#[tokio::test]
async fn resolve_malformed_depends_is_internal_error() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(
        addbuilds.path(),
        "foo",
        &url,
        "depends = [\"not-a-valid-uri\"]\n",
        SUCCESS_SCRIPT,
    );

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    let err = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap_err();
    assert!(matches!(&err, Failure::Internal(e) if e.0.contains("not-a-valid-uri")));
}

#[tokio::test]
async fn resolve_valid_depends_are_carried_into_the_candidate() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(
        addbuilds.path(),
        "foo",
        &url,
        "depends = [\"wowi:1234\"]\n",
        SUCCESS_SCRIPT,
    );

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    let candidate = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();
    assert_eq!(candidate.deps, vec!["wowi:1234".to_owned()]);
}

#[tokio::test]
async fn resolve_non_executable_build_script_is_internal_error() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    let dir = addbuilds.path().join("foo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("addbuild.toml"),
        format!("addname = \"foo\"\nurl = \"{url}\"\nbuild = \"build.sh\"\n"),
    )
    .unwrap();
    std::fs::write(dir.join("build.sh"), SUCCESS_SCRIPT).unwrap();
    // deliberately not chmod +x

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    let err = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap_err();
    assert!(matches!(&err, Failure::Internal(e) if e.0.contains("not executable")));
}

#[tokio::test]
async fn resolve_failing_build_script_is_internal_error() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(addbuilds.path(), "foo", &url, "", FAILING_SCRIPT);

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    let err = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap_err();
    assert!(matches!(&err, Failure::Internal(e) if e.0.contains("boom")));
}

#[tokio::test]
async fn resolve_reuses_cached_build_until_the_repo_changes() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    let addbuild_dir = write_addbuild(addbuilds.path(), "foo", &url, "", SUCCESS_SCRIPT);

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();

    let c1 = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();
    let c2 = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();
    assert_eq!(c1.version, c2.version);
    let count = std::fs::read_to_string(addbuild_dir.join("build-count")).unwrap();
    assert_eq!(count.lines().count(), 1, "build should only run once");

    commit_more(src.path(), "-- v2").await;
    let c3 = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();
    assert_ne!(c1.version, c3.version);
    let count = std::fs::read_to_string(addbuild_dir.join("build-count")).unwrap();
    assert_eq!(
        count.lines().count(),
        2,
        "a new commit should trigger a rebuild"
    );
}

#[tokio::test]
async fn resolve_reclones_when_the_addbuild_url_changes() {
    // A plain `git fetch` never repoints an existing `origin` remote, so if
    // `addbuild.toml`'s `url` is edited after a checkout already exists
    // (e.g. fixing a wrong URL), a naive fetch-and-reset would silently keep
    // pulling from the *old* repo forever. `clone_or_update` must notice the
    // mismatch and re-clone instead.
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let old_src = tempfile::tempdir().unwrap();
    let new_src = tempfile::tempdir().unwrap();
    init_source_repo(old_src.path()).await;
    tokio::fs::write(new_src.path().join("marker"), "new-repo")
        .await
        .unwrap();
    init_source_repo(new_src.path()).await;

    let old_url = old_src.path().to_string_lossy().into_owned();
    write_addbuild(addbuilds.path(), "foo", &old_url, "", SUCCESS_SCRIPT);

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();
    resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();

    let new_url = new_src.path().to_string_lossy().into_owned();
    write_addbuild(addbuilds.path(), "foo", &new_url, "", SUCCESS_SCRIPT);
    resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();

    let checkout = cache.path().join("git-src").join("foo");
    let remote = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&checkout)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&remote.stdout).trim(),
        new_url,
        "origin should now point at the new url, not the old one"
    );
    assert!(checkout.join("marker").is_file());
}

#[tokio::test]
async fn resolve_version_eq_pins_to_a_historical_commit() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(addbuilds.path(), "foo", &url, "", SUCCESS_SCRIPT);

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();

    let old = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();
    commit_more(src.path(), "-- v2").await;
    let latest = resolve_one(&resolver, &http, Flavour::Mainline, &defn("foo"))
        .await
        .unwrap();
    assert_ne!(old.version, latest.version);

    let pinned_defn = defn("foo").with_version(old.version.clone());
    let pinned = resolve_one(&resolver, &http, Flavour::Mainline, &pinned_defn)
        .await
        .unwrap();
    assert_eq!(pinned.version, old.version);
}

#[tokio::test]
async fn resolve_version_eq_unknown_hash_is_files_not_matching() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    init_source_repo(src.path()).await;
    let url = src.path().to_string_lossy().into_owned();
    write_addbuild(addbuilds.path(), "foo", &url, "", SUCCESS_SCRIPT);

    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();

    let d = defn("foo").with_version("r1.deadbeef");
    let err = resolve_one(&resolver, &http, Flavour::Mainline, &d)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::Manager(ManagerError::PkgFilesNotMatching { .. })
    ));
}

#[tokio::test]
async fn any_release_type_strategy_is_rejected() {
    let addbuilds = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let resolver = GitResolver::new(addbuilds.path().to_owned(), cache.path().to_owned());
    let http = HttpClient::new().unwrap();

    let mut d = defn("foo");
    d.strategies.any_release_type = true;
    let err = resolve_one(&resolver, &http, Flavour::Mainline, &d)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::Manager(ManagerError::PkgStrategiesUnsupported { .. })
    ));
}

#[test]
fn parse_pinned_hash_extracts_the_commit() {
    assert_eq!(parse_pinned_hash("r42.abc1234"), Some("abc1234"));
    assert_eq!(parse_pinned_hash("not-a-version"), None);
    assert_eq!(parse_pinned_hash("r42."), None);
}
