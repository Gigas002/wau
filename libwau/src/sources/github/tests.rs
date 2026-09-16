use std::io::Write as _;

use super::*;
use crate::http::HttpClient;

fn make_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Cursor;
    use zip::{ZipWriter, write::FileOptions};

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn defn(alias: &str) -> Defn {
    Defn::new("github", alias)
}

fn resolver(server: &mockito::ServerGuard) -> GitHubResolver {
    GitHubResolver::new_with_api_url(None, server.url())
}

fn repo_json() -> serde_json::Value {
    serde_json::json!({
        "id": 42,
        "full_name": "Owner/Repo",
        "name": "Repo",
        "description": "a fine addon",
        "html_url": "https://github.com/Owner/Repo",
    })
}

fn release_json(tag: &str, assets: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "tag_name": tag,
        "published_at": "2026-01-01T00:00:00Z",
        "assets": assets,
        "body": "release notes",
        "draft": false,
        "prerelease": false,
    })
}

fn zip_asset(url: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "url": url,
        "name": name,
        "content_type": "application/zip",
        "state": "uploaded",
    })
}

// ---------------------------------------------------------------------------
// resolve_one — repo + release lookup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_by_alias_matches_zip_contents_and_returns_candidate() {
    let mut server = mockito::Server::new_async().await;
    let asset_url = format!("{}/download/Foo.zip", server.url());

    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            serde_json::json!([release_json(
                "v1.0.0",
                serde_json::json!([zip_asset(&asset_url, "Foo.zip")])
            )])
            .to_string(),
        )
        .create_async()
        .await;
    server
        .mock("GET", "/download/Foo.zip")
        .with_status(200)
        .with_body(make_test_zip(&[("Foo/Foo.toc", b"## Interface: 110000")]))
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap();
    assert_eq!(candidate.id, "42");
    assert_eq!(candidate.slug, "owner/repo");
    assert_eq!(candidate.version, "v1.0.0");
    assert_eq!(candidate.download_url, asset_url);
    assert_eq!(candidate.changelog_url, "data:,release%20notes");
}

#[tokio::test]
async fn resolve_by_numeric_id_uses_repositories_endpoint() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repositories/42")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/repositories/42/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            serde_json::json!([release_json(
                "v1.0.0",
                serde_json::json!([zip_asset(&format!("{}/a.zip", server.url()), "Foo.zip")])
            )])
            .to_string(),
        )
        .create_async()
        .await;
    server
        .mock("GET", "/a.zip")
        .with_status(200)
        .with_body(make_test_zip(&[("Foo/Foo.toc", b"## Interface: 110000")]))
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let mut d = defn("42");
    d.id = Some("42".to_owned());
    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &d)
        .await
        .unwrap();
    assert_eq!(candidate.id, "42");
}

#[tokio::test]
async fn resolve_404_repo_is_nonexistent() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(404)
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let err = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

#[tokio::test]
async fn resolve_404_releases_is_files_missing() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(404)
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let err = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ManagerError::PkgFilesMissing {
            reason: "no releases found".into()
        }
        .into()
    );
}

#[tokio::test]
async fn resolve_version_eq_uses_tags_endpoint() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/repos/Owner/Repo/releases/tags/v2.0.0")
        .with_status(200)
        .with_body(
            release_json(
                "v2.0.0",
                serde_json::json!([zip_asset(&format!("{}/a.zip", server.url()), "Foo.zip")]),
            )
            .to_string(),
        )
        .create_async()
        .await;
    server
        .mock("GET", "/a.zip")
        .with_status(200)
        .with_body(make_test_zip(&[("Foo/Foo.toc", b"## Interface: 110000")]))
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);
    let d = defn("Owner/Repo").with_version("v2.0.0");

    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &d)
        .await
        .unwrap();
    assert_eq!(candidate.version, "v2.0.0");
}

// ---------------------------------------------------------------------------
// draft / prerelease filtering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_skips_draft_releases() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;

    let mut draft = release_json("v0.9.0-draft", serde_json::json!([]));
    draft["draft"] = serde_json::json!(true);
    let stable = release_json(
        "v1.0.0",
        serde_json::json!([zip_asset(&format!("{}/a.zip", server.url()), "Foo.zip")]),
    );
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(serde_json::json!([draft, stable]).to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/a.zip")
        .with_status(200)
        .with_body(make_test_zip(&[("Foo/Foo.toc", b"## Interface: 110000")]))
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap();
    assert_eq!(candidate.version, "v1.0.0");
}

#[tokio::test]
async fn resolve_prefers_stable_over_prerelease_unless_any_release_type() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;

    let mut pre = release_json(
        "v1.1.0-beta",
        serde_json::json!([zip_asset(&format!("{}/beta.zip", server.url()), "Foo.zip")]),
    );
    pre["prerelease"] = serde_json::json!(true);
    let stable = release_json(
        "v1.0.0",
        serde_json::json!([zip_asset(
            &format!("{}/stable.zip", server.url()),
            "Foo.zip"
        )]),
    );
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(serde_json::json!([pre, stable]).to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/stable.zip")
        .with_status(200)
        .with_body(make_test_zip(&[("Foo/Foo.toc", b"## Interface: 110000")]))
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap();
    assert_eq!(candidate.version, "v1.0.0");
}

// ---------------------------------------------------------------------------
// asset filtering: -nolib
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_skips_nolib_assets() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            serde_json::json!([release_json(
                "v1.0.0",
                serde_json::json!([
                    zip_asset(&format!("{}/nolib.zip", server.url()), "Foo-nolib.zip"),
                    zip_asset(&format!("{}/full.zip", server.url()), "Foo.zip"),
                ])
            )])
            .to_string(),
        )
        .create_async()
        .await;
    server
        .mock("GET", "/full.zip")
        .with_status(200)
        .with_body(make_test_zip(&[("Foo/Foo.toc", b"## Interface: 110000")]))
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap();
    assert_eq!(candidate.download_url, format!("{}/full.zip", server.url()));
}

// ---------------------------------------------------------------------------
// release.json based matching
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_matches_via_release_json() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;

    let release_json_url = format!("{}/release.json", server.url());
    let zip_url = format!("{}/Foo.zip", server.url());
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            serde_json::json!([release_json(
                "v1.0.0",
                serde_json::json!([
                    { "url": release_json_url, "name": "release.json", "content_type": "application/octet-stream", "state": "uploaded" },
                    zip_asset(&zip_url, "Foo.zip"),
                ])
            )])
            .to_string(),
        )
        .create_async()
        .await;
    server
        .mock("GET", "/release.json")
        .with_status(200)
        .with_body(
            serde_json::json!({
                "releases": [
                    {
                        "filename": "Foo.zip",
                        "nolib": false,
                        "metadata": [ { "flavor": "mainline", "interface": 110000 } ],
                    }
                ]
            })
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap();
    assert_eq!(candidate.download_url, zip_url);
}

#[tokio::test]
async fn resolve_release_json_skips_nolib_subrelease() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;

    let release_json_url = format!("{}/release.json", server.url());
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            serde_json::json!([release_json(
                "v1.0.0",
                serde_json::json!([
                    { "url": release_json_url, "name": "release.json", "content_type": "application/octet-stream", "state": "uploaded" },
                ])
            )])
            .to_string(),
        )
        .create_async()
        .await;
    server
        .mock("GET", "/release.json")
        .with_status(200)
        .with_body(
            serde_json::json!({
                "releases": [
                    {
                        "filename": "Foo-nolib.zip",
                        "nolib": true,
                        "metadata": [ { "flavor": "mainline", "interface": 110000 } ],
                    }
                ]
            })
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let err = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::Manager(ManagerError::PkgFilesNotMatching { .. })
    ));
}

// ---------------------------------------------------------------------------
// Ranged partial read (206 + Content-Range)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_uses_ranged_partial_zip_when_server_honours_range() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;

    // Flavour-suffixed filename so the match is decided from the archive
    // listing alone (covered by the ranged tail read) without needing a
    // second request to inspect file *content* — that fallback path is
    // exercised separately by the non-ranged zip-contents tests above.
    let full_zip = make_test_zip(&[("Foo/Foo-Mainline.toc", b"## Interface: 110000")]);
    let zip_url = format!("{}/Foo.zip", server.url());
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            serde_json::json!([release_json(
                "v1.0.0",
                serde_json::json!([zip_asset(&zip_url, "Foo.zip")])
            )])
            .to_string(),
        )
        .create_async()
        .await;
    // Whole file fits well under 25000 bytes, so the entire body is the "tail".
    server
        .mock("GET", "/Foo.zip")
        .match_header("range", "bytes=-25000")
        .with_status(206)
        .with_header(
            "Content-Range",
            &format!("bytes 0-{}/{}", full_zip.len() - 1, full_zip.len()),
        )
        .with_body(full_zip)
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &defn("Owner/Repo"))
        .await
        .unwrap();
    assert_eq!(candidate.download_url, zip_url);
}

// ---------------------------------------------------------------------------
// any_flavour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_any_flavour_matches_non_current_flavour_toc() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/repos/Owner/Repo")
        .with_status(200)
        .with_body(repo_json().to_string())
        .create_async()
        .await;
    let zip_url = format!("{}/Foo.zip", server.url());
    server
        .mock("GET", "/repos/Owner/Repo/releases")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            serde_json::json!([release_json(
                "v1.0.0",
                serde_json::json!([zip_asset(&zip_url, "Foo.zip")])
            )])
            .to_string(),
        )
        .create_async()
        .await;
    // Only a Vanilla-flavoured TOC — won't match Mainline unless any_flavour is set.
    server
        .mock("GET", "/Foo.zip")
        .with_status(200)
        .with_body(make_test_zip(&[(
            "Foo/Foo-Vanilla.toc",
            b"## Interface: 11400",
        )]))
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);

    let mut d = defn("Owner/Repo");
    d.strategies.any_flavour = true;
    let candidate = crate::sources::resolve_one(&r, &http, Flavour::Mainline, &d)
        .await
        .unwrap();
    assert_eq!(candidate.download_url, zip_url);
}

// ---------------------------------------------------------------------------
// Misc: headers, alias-from-url
// ---------------------------------------------------------------------------

#[test]
fn headers_differ_by_intent_and_include_token() {
    let r = GitHubResolver::new(Some(SecretString::new("tok")));
    let fetch = r.make_request_headers(HeadersIntent::Fetch);
    assert!(fetch.contains(&(
        "Accept".to_owned(),
        "application/vnd.github+json".to_owned()
    )));
    assert!(fetch.contains(&("Authorization".to_owned(), "token tok".to_owned())));

    let download = r.make_request_headers(HeadersIntent::Download);
    assert!(download.contains(&("Accept".to_owned(), "application/octet-stream".to_owned())));
}

#[test]
fn headers_omit_authorization_without_token() {
    let r = GitHubResolver::new(None);
    let headers = r.make_request_headers(HeadersIntent::Fetch);
    assert!(!headers.iter().any(|(k, _)| k == "Authorization"));
}

#[test]
fn get_alias_from_url_parses_owner_repo() {
    let r = GitHubResolver::new(None);
    assert_eq!(
        r.get_alias_from_url("https://github.com/Owner/Repo"),
        Some("Owner/Repo".to_owned())
    );
    assert_eq!(
        r.get_alias_from_url("https://example.invalid/Owner/Repo"),
        None
    );
}

#[test]
fn percent_encode_and_decode_round_trip() {
    let text = "hello, world! / 100%";
    let encoded = super::super::percent_encode(text);
    let decoded = super::super::percent_decode(&encoded);
    assert_eq!(decoded, text);
}
