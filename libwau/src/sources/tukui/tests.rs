use super::*;

fn defn(alias: &str) -> Defn {
    Defn::new("tukui", alias)
}

fn resolver(server: &mockito::ServerGuard) -> TukuiResolver {
    TukuiResolver::new_with_api_url(server.url())
}

fn addon_json() -> serde_json::Value {
    serde_json::json!({
        "id": 42,
        "slug": "elvui",
        "name": "ElvUI",
        "url": "https://example.invalid/elvui.zip",
        "version": "13.0",
        "changelog_url": "https://example.invalid/elvui/changelog",
        "patch": ["11.0.5"],
        "last_update": "2026-01-15",
        "web_url": "https://www.tukui.org/addons.php?id=1",
        "small_desc": "a fine addon",
    })
}

#[tokio::test]
async fn resolve_matches_flavour_via_patch_list() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addon/elvui")
        .with_status(200)
        .with_body(addon_json().to_string())
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let candidate =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("elvui"))
            .await
            .unwrap();

    assert_eq!(candidate.id, "42");
    assert_eq!(candidate.version, "13.0");
    assert_eq!(
        candidate.changelog_url,
        "https://example.invalid/elvui/changelog#13.0"
    );
}

#[tokio::test]
async fn resolve_errors_when_flavour_not_in_patch_list() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addon/elvui")
        .with_status(200)
        .with_body(addon_json().to_string())
        .create_async()
        .await;

    let http = HttpClient::new(None).unwrap();
    let err = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::VanillaClassic,
        &defn("elvui"),
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
        .mock("GET", "/addon/nope")
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
async fn unsupported_strategy_is_rejected() {
    let http = HttpClient::new(None).unwrap();
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/addon/elvui")
        .with_status(200)
        .with_body(addon_json().to_string())
        .create_async()
        .await;
    let mut d = defn("elvui");
    d.strategies.any_flavour = true;

    let err = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &d)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::Manager(ManagerError::PkgStrategiesUnsupported { .. })
    ));
}

#[test]
fn tukui_supports_no_strategies() {
    let r = TukuiResolver::new();
    assert!(Resolver::metadata(&r).strategies.is_empty());
}
