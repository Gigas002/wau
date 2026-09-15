use super::*;

fn defn(alias: &str) -> Defn {
    Defn::new("wago", alias)
}

fn resolver(server: &mockito::ServerGuard) -> WagoAddonsResolver {
    WagoAddonsResolver::new_with_api_url(Some(SecretString::new("tok")), server.url())
}

fn addon_json(releases: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": "abc",
        "slug": "some-addon",
        "display_name": "Some Addon",
        "summary": "a fine addon",
        "website_url": "https://addons.wago.io/addons/some-addon",
        "recent_release": releases,
    })
}

fn release(label: &str, created_at: &str) -> serde_json::Value {
    serde_json::json!({
        "label": label,
        "changelog": "fixed stuff",
        "created_at": created_at,
        "download_link": format!("https://example.invalid/{label}.zip"),
    })
}

#[tokio::test]
async fn resolve_prefers_stable_unless_any_release_type() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addons/some-addon")
        .match_query(mockito::Matcher::UrlEncoded(
            "game_version".into(),
            "retail".into(),
        ))
        .with_status(200)
        .with_body(
            addon_json(serde_json::json!({
                "stable": release("1.0.0", "2026-01-01T00:00:00Z"),
                "beta": release("1.1.0-beta", "2026-02-01T00:00:00Z"),
            }))
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let candidate = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::Mainline,
        &defn("some-addon"),
    )
    .await
    .unwrap();
    assert_eq!(candidate.version, "1.0.0");
}

#[tokio::test]
async fn resolve_any_release_type_picks_most_recent_channel() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addons/some-addon")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(
            addon_json(serde_json::json!({
                "stable": release("1.0.0", "2026-01-01T00:00:00Z"),
                "beta": release("1.1.0-beta", "2026-02-01T00:00:00Z"),
            }))
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let mut d = defn("some-addon");
    d.strategies.any_release_type = true;
    let candidate = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &d)
        .await
        .unwrap();
    assert_eq!(candidate.version, "1.1.0-beta");
}

#[tokio::test]
async fn resolve_empty_recent_release_is_files_not_matching() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addons/some-addon")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(addon_json(serde_json::json!({})).to_string())
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let err = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::Mainline,
        &defn("some-addon"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        Failure::Manager(ManagerError::PkgFilesNotMatching { .. })
    ));
}

#[tokio::test]
async fn resolve_404_is_nonexistent() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addons/nope")
        .match_query(mockito::Matcher::Any)
        .with_status(404)
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let err =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("nope"))
            .await
            .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

#[tokio::test]
async fn resolve_errors_for_flavour_with_no_wago_mapping() {
    let http = HttpClient::new(None).unwrap();
    let server = mockito::Server::new_async().await;
    let err = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::TbcClassic,
        &defn("some-addon"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, Failure::Internal(_)));
}

#[test]
fn disabled_reason_requires_token() {
    assert!(
        WagoAddonsResolver::new(None)
            .get_disabled_reason()
            .is_some()
    );
    assert!(
        WagoAddonsResolver::new(Some(SecretString::new("x")))
            .get_disabled_reason()
            .is_none()
    );
}

#[test]
fn headers_include_bearer_token_when_present() {
    let r = WagoAddonsResolver::new(Some(SecretString::new("tok")));
    assert_eq!(
        r.make_request_headers(HeadersIntent::Fetch),
        vec![("Authorization".to_owned(), "Bearer tok".to_owned())]
    );
    assert!(
        WagoAddonsResolver::new(None)
            .make_request_headers(HeadersIntent::Fetch)
            .is_empty()
    );
}

#[test]
fn get_alias_from_url_parses_addon_page() {
    let r = WagoAddonsResolver::new(None);
    assert_eq!(
        r.get_alias_from_url("https://addons.wago.io/addons/some-addon"),
        Some("some-addon".to_owned())
    );
    assert_eq!(
        r.get_alias_from_url("https://example.invalid/addons/some-addon"),
        None
    );
}
