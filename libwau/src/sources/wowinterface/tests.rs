use super::*;

fn defn(alias: &str) -> Defn {
    Defn::new("wowi", alias)
}

fn resolver(server: &mockito::ServerGuard) -> WowInterfaceResolver {
    WowInterfaceResolver::new_with_api_url(server.url())
}

fn detail_item(uid: &str, pending: &str) -> serde_json::Value {
    serde_json::json!({
        "UID": uid,
        "UIName": "WeakAuras",
        "UIVersion": "5.0.0",
        "UIDate": 1_700_000_000_000i64,
        "UIDownload": "https://example.invalid/weakauras.zip",
        "UIPending": pending,
        "UIDescription": "a fine addon",
        "UIChangeLog": "fixed stuff",
    })
}

#[tokio::test]
async fn resolve_batches_and_matches_by_uid() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/12345.json")
        .with_status(200)
        .with_body(serde_json::json!([detail_item("12345", "0")]).to_string())
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let candidate = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::Mainline,
        &defn("12345-WeakAuras"),
    )
    .await
    .unwrap();

    assert_eq!(candidate.id, "12345");
    assert_eq!(candidate.slug, "12345-weakauras");
    assert_eq!(candidate.changelog_url, "data:,fixed%20stuff");
}

#[tokio::test]
async fn resolve_pending_file_is_files_missing() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/12345.json")
        .with_status(200)
        .with_body(serde_json::json!([detail_item("12345", "1")]).to_string())
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let err = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::Mainline,
        &defn("12345-WeakAuras"),
    )
    .await
    .unwrap_err();
    assert_eq!(
        err,
        ManagerError::PkgFilesMissing {
            reason: "file awaiting approval".into()
        }
        .into()
    );
}

#[tokio::test]
async fn resolve_unknown_uid_in_batch_response_is_nonexistent() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/99999.json")
        .with_status(200)
        .with_body(serde_json::json!([]).to_string())
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let err =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("99999"))
            .await
            .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

#[tokio::test]
async fn resolve_404_batch_falls_back_to_nonexistent_for_all() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/12345.json")
        .with_status(404)
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let err =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("12345"))
            .await
            .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

#[tokio::test]
async fn resolve_no_numeric_alias_is_nonexistent_without_request() {
    let http = HttpClient::new(None).unwrap();
    let server = mockito::Server::new_async().await;
    let err = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::Mainline,
        &defn("not-numeric"),
    )
    .await
    .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

#[test]
fn get_alias_from_url_handles_landing_fileinfo_and_download_forms() {
    let r = WowInterfaceResolver::new();
    assert_eq!(
        r.get_alias_from_url("https://www.wowinterface.com/downloads/landing.php?fileid=12345"),
        Some("12345".to_owned())
    );
    assert_eq!(
        r.get_alias_from_url("https://www.wowinterface.com/downloads/fileinfo.php?id=6789"),
        Some("6789".to_owned())
    );
    assert_eq!(
        r.get_alias_from_url("https://www.wowinterface.com/downloads/info24608-WeakAuras.html"),
        Some("24608".to_owned())
    );
    assert_eq!(
        r.get_alias_from_url("https://www.wowinterface.com/downloads/download24608-WeakAuras"),
        Some("24608".to_owned())
    );
    assert_eq!(
        r.get_alias_from_url("https://example.invalid/downloads/info123"),
        None
    );
}

#[test]
fn slugify_normalises_spaces_and_case() {
    assert_eq!(slugify("12345 WeakAuras"), "12345-weakauras");
    assert_eq!(slugify("Foo  Bar!!Baz"), "foo-bar-baz");
}

#[test]
fn wowinterface_supports_no_strategies() {
    let r = WowInterfaceResolver::new();
    assert!(Resolver::metadata(&r).strategies.is_empty());
}
