use super::*;

fn resolver(server: &mockito::ServerGuard) -> CurseForgeResolver {
    CurseForgeResolver::new(Some(SecretString::new("test-key")), Some(server.url()))
}

fn defn(alias: &str) -> Defn {
    Defn::new("curse", alias)
}

fn mod_json(id: u64, files: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "data": {
            "id": id,
            "slug": "some-addon",
            "name": "Some Addon",
            "summary": "a fine addon",
            "links": { "websiteUrl": "https://www.curseforge.com/wow/addons/some-addon" },
            "latestFiles": files,
            "allowModDistribution": true,
        }
    })
}

fn stable_file(id: u64, type_id: u32) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "displayName": format!("v{id}"),
        "releaseType": 1,
        "fileDate": "2026-01-01T00:00:00Z",
        "downloadUrl": format!("https://example.invalid/{id}.zip"),
        "sortableGameVersions": [{ "gameVersionTypeId": type_id }],
        "dependencies": [],
    })
}

// ---------------------------------------------------------------------------
// resolve_one — by numeric id / by slug
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_by_numeric_id_fetches_mod_directly() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/12345")
        .with_status(200)
        .with_body(mod_json(12345, serde_json::json!([stable_file(999, 517)])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let candidate =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("12345"))
            .await
            .unwrap();

    assert_eq!(candidate.id, "12345");
    assert_eq!(candidate.download_url, "https://example.invalid/999.zip");
    mock.assert_async().await;
}

#[tokio::test]
async fn resolve_by_numeric_id_404_is_nonexistent() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/12345")
        .with_status(404)
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let err =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("12345"))
            .await
            .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

#[tokio::test]
async fn resolve_by_slug_searches_and_takes_single_match() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/search")
        .match_query(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("gameId".into(), "1".into()),
            mockito::Matcher::UrlEncoded("slug".into(), "some-addon".into()),
        ]))
        .with_status(200)
        .with_body(
            serde_json::json!({ "data": [ {
                "id": 1, "slug": "some-addon", "name": "Some Addon", "summary": "x",
                "links": { "websiteUrl": "https://example.invalid" },
                "latestFiles": [stable_file(5, 517)],
                "allowModDistribution": true,
            } ] })
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let candidate = crate::sources::resolve_one(
        &resolver(&server),
        &http,
        Flavour::Mainline,
        &defn("some-addon"),
    )
    .await
    .unwrap();
    assert_eq!(candidate.slug, "some-addon");
    mock.assert_async().await;
}

#[tokio::test]
async fn resolve_by_slug_no_match_is_nonexistent() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/search")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_body(serde_json::json!({ "data": [] }).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let err =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("nope"))
            .await
            .unwrap_err();
    assert_eq!(err, ManagerError::PkgNonexistent.into());
}

// ---------------------------------------------------------------------------
// Flavour / release-type / exposeAsAlternative filtering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_picks_highest_id_among_matching_flavour_files() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(
            mod_json(
                1,
                serde_json::json!([
                    stable_file(10, 517),
                    stable_file(20, 517),
                    stable_file(99, 67408)
                ]),
            )
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let candidate =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
            .await
            .unwrap();
    // File 99 is VanillaClassic (67408), excluded for Mainline; highest matching id is 20.
    assert_eq!(candidate.version, "v20_20");
}

#[tokio::test]
async fn resolve_falls_back_to_any_flavour_when_strategy_set() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([stable_file(7, 67408)])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let mut d = defn("1");
    d.strategies.any_flavour = true;

    // No Mainline (517) file exists, but any_flavour allows the VanillaClassic one through.
    let candidate = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &d)
        .await
        .unwrap();
    assert_eq!(candidate.version, "v7_7");
}

#[tokio::test]
async fn resolve_without_any_flavour_errors_when_no_match() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([stable_file(7, 67408)])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let err = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::Manager(ManagerError::PkgFilesNotMatching { .. })
    ));
}

#[tokio::test]
async fn resolve_prefers_stable_over_beta_unless_any_release_type() {
    let mut server = mockito::Server::new_async().await;
    let mut beta = stable_file(50, 517);
    beta["releaseType"] = serde_json::json!(2);
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([stable_file(10, 517), beta])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let candidate =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
            .await
            .unwrap();
    assert_eq!(candidate.version, "v10_10");
}

#[tokio::test]
async fn resolve_allows_beta_when_no_stable_exists() {
    let mut server = mockito::Server::new_async().await;
    let mut beta = stable_file(50, 517);
    beta["releaseType"] = serde_json::json!(2);
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([beta])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let candidate =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
            .await
            .unwrap();
    assert_eq!(candidate.version, "v50_50");
}

#[tokio::test]
async fn resolve_excludes_expose_as_alternative_files() {
    let mut server = mockito::Server::new_async().await;
    let mut alt = stable_file(999, 517);
    alt["exposeAsAlternative"] = serde_json::json!(true);
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([stable_file(1, 517), alt])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let candidate =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
            .await
            .unwrap();
    assert_eq!(candidate.version, "v1_1");
}

// ---------------------------------------------------------------------------
// downloadUrl missing / distribution forbidden
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_errors_when_download_url_missing() {
    let mut server = mockito::Server::new_async().await;
    let mut file = stable_file(1, 517);
    file["downloadUrl"] = serde_json::Value::Null;
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([file])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let err = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ManagerError::PkgFilesMissing {
            reason: "no files are available for download".into()
        }
        .into()
    );
}

#[tokio::test]
async fn resolve_reports_forbidden_distribution_reason() {
    let mut server = mockito::Server::new_async().await;
    let mut file = stable_file(1, 517);
    file["downloadUrl"] = serde_json::Value::Null;
    let mut body = mod_json(1, serde_json::json!([file]));
    body["data"]["allowModDistribution"] = serde_json::json!(false);
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(body.to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let err = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        ManagerError::PkgFilesMissing {
            reason: "package distribution is forbidden".into()
        }
        .into()
    );
}

// ---------------------------------------------------------------------------
// version_eq
// ---------------------------------------------------------------------------

#[tokio::test]
async fn version_eq_uses_latest_files_when_file_id_present() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(
            mod_json(
                1,
                serde_json::json!([stable_file(5, 517), stable_file(9, 517)]),
            )
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let d = defn("1").with_version("v5_5");
    let candidate = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &d)
        .await
        .unwrap();
    // Picked directly from latestFiles — no extra HTTP call needed.
    assert_eq!(candidate.version, "v5_5");
}

#[tokio::test]
async fn version_eq_fetches_specific_file_when_not_in_latest_files() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([stable_file(5, 517)])).to_string())
        .create_async()
        .await;
    server
        .mock("GET", "/1/files/77")
        .with_status(200)
        .with_body(serde_json::json!({ "data": stable_file(77, 517) }).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let d = defn("1").with_version("vOld_77");
    let candidate = crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &d)
        .await
        .unwrap();
    assert_eq!(candidate.version, "v77_77");
}

// ---------------------------------------------------------------------------
// deps
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_includes_only_required_dependencies() {
    let mut server = mockito::Server::new_async().await;
    let mut file = stable_file(1, 517);
    file["dependencies"] = serde_json::json!([
        { "modId": 111, "relationType": 3 },
        { "modId": 222, "relationType": 1 },
    ]);
    server
        .mock("GET", "/1")
        .with_status(200)
        .with_body(mod_json(1, serde_json::json!([file])).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let candidate =
        crate::sources::resolve_one(&resolver(&server), &http, Flavour::Mainline, &defn("1"))
            .await
            .unwrap();
    assert_eq!(candidate.deps, vec!["111".to_owned()]);
}

// ---------------------------------------------------------------------------
// Batch resolve
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_batches_numeric_ids_into_one_post() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/")
        .with_status(200)
        .with_body(
            serde_json::json!({ "data": [
                { "id": 1, "slug": "a", "name": "A", "summary": "x",
                  "links": { "websiteUrl": "https://example.invalid/a" },
                  "latestFiles": [stable_file(1, 517)], "allowModDistribution": true },
                { "id": 2, "slug": "b", "name": "B", "summary": "x",
                  "links": { "websiteUrl": "https://example.invalid/b" },
                  "latestFiles": [stable_file(2, 517)], "allowModDistribution": true },
            ] })
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);
    let defns = vec![defn("1"), defn("2")];
    let results = Resolver::resolve(&r, &http, Flavour::Mainline, &defns).await;

    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
    mock.assert_async().await;
}

#[tokio::test]
async fn resolve_batch_failure_applies_same_error_to_every_defn() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/")
        .with_status(500)
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let r = resolver(&server);
    let defns = vec![defn("1"), defn("2")];
    let results = Resolver::resolve(&r, &http, Flavour::Mainline, &defns).await;

    assert_eq!(results.len(), 2);
    assert!(results[0].is_err());
    assert_eq!(results[0], results[1]);
}

// ---------------------------------------------------------------------------
// Misc: headers, disabled reason, alias-from-url, strategies
// ---------------------------------------------------------------------------

#[test]
fn headers_include_api_key_for_fetch_but_not_download() {
    let r = CurseForgeResolver::new(Some(SecretString::new("secret-key")), None);
    let fetch = r.make_request_headers(HeadersIntent::Fetch);
    assert_eq!(
        fetch,
        vec![("x-api-key".to_owned(), "secret-key".to_owned())]
    );
    assert!(r.make_request_headers(HeadersIntent::Download).is_empty());
}

#[test]
fn headers_omit_api_key_when_alternative_url_configured() {
    let r = CurseForgeResolver::new(
        Some(SecretString::new("secret-key")),
        Some("https://proxy.invalid".to_owned()),
    );
    assert!(r.make_request_headers(HeadersIntent::Fetch).is_empty());
}

#[test]
fn disabled_reason_requires_token_unless_alternative_url_set() {
    assert!(
        CurseForgeResolver::new(None, None)
            .get_disabled_reason()
            .is_some()
    );
    assert!(
        CurseForgeResolver::new(None, Some("https://proxy.invalid".into()))
            .get_disabled_reason()
            .is_none()
    );
    assert!(
        CurseForgeResolver::new(Some(SecretString::new("x")), None)
            .get_disabled_reason()
            .is_none()
    );
}

#[test]
fn get_alias_from_url_parses_curseforge_addon_page() {
    let r = CurseForgeResolver::new(None, None);
    assert_eq!(
        r.get_alias_from_url("https://www.curseforge.com/wow/addons/weakauras"),
        Some("WeakAuras".to_lowercase())
    );
    assert_eq!(
        r.get_alias_from_url("https://example.invalid/not-curse"),
        None
    );
}

#[test]
fn curseforge_declares_support_for_every_strategy() {
    // CF is the one source that supports all three strategies — the
    // "strategy unsupported" rejection path is exercised generically for
    // sources/mod.rs's `resolve_one`, and per-source for Tukui/WoWInterface
    // (which support none) instead of duplicating it here.
    let r = CurseForgeResolver::new(None, None);
    let strategies = Resolver::metadata(&r).strategies;
    assert!(strategies.contains(&Strategy::AnyFlavour));
    assert!(strategies.contains(&Strategy::AnyReleaseType));
    assert!(strategies.contains(&Strategy::VersionEq));
}
