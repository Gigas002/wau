use std::{fs, io::Write as _};

use super::*;
use crate::http::HttpClient;

fn make_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Cursor;
    use zip::{ZipWriter, write::FileOptions};

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
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
// find_archive_addon_tocs
// ---------------------------------------------------------------------------

#[test]
fn find_archive_addon_tocs_matches_wow_convention() {
    let names = ["Foo/Foo.toc", "Foo/Foo.lua"];
    let found = find_archive_addon_tocs(names);
    assert_eq!(found, vec![("Foo/Foo.toc".to_owned(), "Foo".to_owned())]);
}

#[test]
fn find_archive_addon_tocs_is_case_insensitive_on_extension() {
    let names = ["Foo/Foo.TOC"];
    let found = find_archive_addon_tocs(names);
    assert_eq!(found.len(), 1);
}

#[test]
fn find_archive_addon_tocs_skips_nested_files() {
    let names = ["Foo/Sub/Foo.toc"];
    assert!(find_archive_addon_tocs(names).is_empty());
}

#[test]
fn find_archive_addon_tocs_requires_filename_prefixed_by_folder() {
    let names = ["Foo/Bar.toc"];
    assert!(find_archive_addon_tocs(names).is_empty());
}

#[test]
fn find_archive_addon_tocs_ignores_top_level_files() {
    let names = ["README.md"];
    assert!(find_archive_addon_tocs(names).is_empty());
}

#[test]
fn find_archive_addon_tocs_finds_multiple_addon_folders() {
    let names = ["Foo/Foo.toc", "Bar/Bar.toc", "Foo/Foo.lua", ".github/x.yml"];
    let found = find_archive_addon_tocs(names);
    let mut heads: Vec<String> = found.into_iter().map(|(_, h)| h).collect();
    heads.sort();
    assert_eq!(heads, vec!["Bar".to_owned(), "Foo".to_owned()]);
}

// ---------------------------------------------------------------------------
// open_zip_archive / extract
// ---------------------------------------------------------------------------

#[test]
fn open_zip_archive_finds_top_level_folders() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("addon.zip");
    fs::write(
        &zip_path,
        make_test_zip(&[
            ("Foo/Foo.toc", b"## Interface: 110000"),
            ("Foo/Foo.lua", b""),
            ("Bar/Bar.toc", b"## Interface: 110000"),
        ]),
    )
    .unwrap();

    let archive = open_zip_archive(&zip_path).unwrap();
    let mut folders: Vec<&String> = archive.top_level_folders.iter().collect();
    folders.sort();
    assert_eq!(folders, vec!["Bar", "Foo"]);
}

#[test]
fn extract_writes_only_addon_folders_skipping_top_level_junk() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("addon.zip");
    fs::write(
        &zip_path,
        make_test_zip(&[
            ("Foo/Foo.toc", b"## Interface: 110000"),
            ("Foo/Foo.lua", b"-- lua"),
            ("README.md", b"not an addon"),
            (".github/workflows/ci.yml", b"junk"),
        ]),
    )
    .unwrap();

    let archive = open_zip_archive(&zip_path).unwrap();
    let dest = dir.path().join("extracted");
    fs::create_dir_all(&dest).unwrap();
    archive.extract(&dest).unwrap();

    assert!(dest.join("Foo").join("Foo.toc").is_file());
    assert!(dest.join("Foo").join("Foo.lua").is_file());
    assert!(!dest.join("README.md").exists());
    assert!(!dest.join(".github").exists());
}

#[test]
fn extract_preserves_file_contents() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("addon.zip");
    fs::write(
        &zip_path,
        make_test_zip(&[("Foo/Foo.toc", b"## Version: 1.2.3")]),
    )
    .unwrap();

    let archive = open_zip_archive(&zip_path).unwrap();
    let dest = dir.path().join("extracted");
    fs::create_dir_all(&dest).unwrap();
    archive.extract(&dest).unwrap();

    let content = fs::read_to_string(dest.join("Foo").join("Foo.toc")).unwrap();
    assert_eq!(content, "## Version: 1.2.3");
}

#[test]
fn extract_handles_addon_with_no_toc_by_extracting_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("addon.zip");
    fs::write(&zip_path, make_test_zip(&[("README.md", b"just a file")])).unwrap();

    let archive = open_zip_archive(&zip_path).unwrap();
    assert!(archive.top_level_folders.is_empty());

    let dest = dir.path().join("extracted");
    fs::create_dir_all(&dest).unwrap();
    archive.extract(&dest).unwrap();
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
}

// ---------------------------------------------------------------------------
// file:// URI helpers
// ---------------------------------------------------------------------------

#[test]
fn is_file_uri_true_for_file_scheme() {
    assert!(is_file_uri("file:///home/user/addon.zip"));
    assert!(!is_file_uri("https://example.invalid/addon.zip"));
    assert!(!is_file_uri("not a url"));
}

#[test]
fn file_uri_to_path_round_trips_absolute_path() {
    let path = file_uri_to_path("file:///tmp/addon.zip").unwrap();
    assert_eq!(path, std::path::PathBuf::from("/tmp/addon.zip"));
}

// ---------------------------------------------------------------------------
// download_pkg_archive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn download_pkg_archive_short_circuits_file_uri() {
    let client = HttpClient::new(None).unwrap();
    let locks = DownloadLocks::new();
    let dir = tempfile::tempdir().unwrap();

    let path = download_pkg_archive(
        &client,
        &locks,
        "file:///tmp/some-addon.zip",
        &[],
        dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(path, std::path::PathBuf::from("/tmp/some-addon.zip"));
}

#[tokio::test]
async fn download_pkg_archive_fetches_and_writes_body() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/addon.zip")
        .with_status(200)
        .with_body(b"zip-bytes" as &[u8])
        .create_async()
        .await;

    let client = HttpClient::new(None).unwrap();
    let locks = DownloadLocks::new();
    let dir = tempfile::tempdir().unwrap();

    let path = download_pkg_archive(
        &client,
        &locks,
        &format!("{}/addon.zip", server.url()),
        &[],
        dir.path(),
    )
    .await
    .unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"zip-bytes");
    mock.assert_async().await;
}

#[tokio::test]
async fn download_pkg_archive_errors_on_failure_status() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/missing.zip")
        .with_status(404)
        .create_async()
        .await;

    let client = HttpClient::new(None).unwrap();
    let locks = DownloadLocks::new();
    let dir = tempfile::tempdir().unwrap();

    let err = download_pkg_archive(
        &client,
        &locks,
        &format!("{}/missing.zip", server.url()),
        &[],
        dir.path(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DownloadError::Status { status: 404, .. }));
}

#[tokio::test]
async fn concurrent_downloads_of_same_url_both_succeed_without_corrupting_each_other() {
    // The per-URL lock's guarantee (matching instawow) is that concurrent
    // downloads of the same URL don't race on temp-file writes — not that
    // only one HTTP request is made (the shared cache may or may not save
    // the second caller a round trip depending on timing).
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addon.zip")
        .with_status(200)
        .with_body(b"zip-bytes" as &[u8])
        .expect_at_least(1)
        .create_async()
        .await;

    let client = HttpClient::new(None).unwrap();
    let locks = DownloadLocks::new();
    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/addon.zip", server.url());

    let (a, b) = tokio::join!(
        download_pkg_archive(&client, &locks, &url, &[], dir.path()),
        download_pkg_archive(&client, &locks, &url, &[], dir.path()),
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(fs::read(&a).unwrap(), b"zip-bytes");
    assert_eq!(fs::read(&b).unwrap(), b"zip-bytes");
}
