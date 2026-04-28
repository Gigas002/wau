use super::*;

// ---------------------------------------------------------------------------
// Provider trait types
// ---------------------------------------------------------------------------

#[test]
fn install_context_fields_accessible() {
    let ctx = InstallContext {
        tag: Tag::new("test"),
        flavor: Flavor::Retail,
        channel: Channel::Stable,
        addons_path: PathBuf::from("/wow/Interface/AddOns"),
        cache_dir: PathBuf::from("/cache/wau"),
    };
    assert_eq!(ctx.tag.as_str(), "test");
    assert_eq!(ctx.flavor, Flavor::Retail);
    assert_eq!(ctx.channel, Channel::Stable);
}

#[test]
fn resolved_artifact_fields_accessible() {
    let a = ResolvedArtifact {
        version: "1.0.0".into(),
        id: "local:/tmp/addon.zip".into(),
        url: "/tmp/addon.zip".into(),
        sha256: None,
    };
    assert_eq!(a.version, "1.0.0");
    assert!(a.sha256.is_none());
}

#[test]
#[cfg(not(feature = "local"))]
fn for_provider_returns_error_when_no_features() {
    let result = for_provider(&crate::model::Provider::Local, &ProviderConfig::default());
    assert!(matches!(
        result,
        Err(crate::Error::ProviderNotSupported { .. })
    ));
}

// ---------------------------------------------------------------------------
// CurseForge provider (gated on the "curseforge" feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "curseforge")]
mod curseforge_tests {
    use std::path::PathBuf;

    use crate::{
        manifest::ManifestAddon,
        model::{Channel, Flavor, Provider as ModelProvider, Tag},
        providers::{
            InstallContext, Provider, ResolvedArtifact,
            curseforge::{CurseForgeProvider, channel_to_release_type, flavor_matches},
        },
    };

    fn make_ctx() -> InstallContext {
        InstallContext {
            tag: Tag::new("test"),
            flavor: Flavor::Retail,
            channel: Channel::Stable,
            addons_path: PathBuf::from("/wow/Interface/AddOns"),
            cache_dir: PathBuf::from("/tmp/wau-cache"),
        }
    }

    fn make_addon(project_id: Option<u64>) -> ManifestAddon {
        ManifestAddon {
            name: "WeakAuras".into(),
            provider: ModelProvider::CurseForge,
            channel: None,
            flavors: None,
            pin: None,
            project_id,
            wowi_id: None,
            repo: None,
            asset_regex: None,
            git_ref: None,
            url: None,
        }
    }

    fn one_stable_retail_file() -> &'static str {
        r#"{
            "data": [{
                "id": 4922788,
                "displayName": "WeakAuras-4.5.0.zip",
                "downloadUrl": "https://edge.forgecdn.net/files/WeakAuras-4.5.0.zip",
                "releaseType": 1,
                "gameVersions": ["12.0.5"],
                "hashes": [
                    {"value": "abc123sha1", "algo": 1},
                    {"value": "def456sha256", "algo": 2}
                ]
            }],
            "pagination": {"index":0,"pageSize":50,"resultCount":1,"totalCount":1}
        }"#
    }

    // resolve

    #[tokio::test]
    async fn resolve_returns_latest_stable_file() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/mods/90003/files")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(one_stable_retail_file())
            .create_async()
            .await;

        let provider =
            CurseForgeProvider::with_base_url("test-key".into(), format!("{}/v1", server.url()));
        let addon = make_addon(Some(90003));
        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();

        assert_eq!(artifact.version, "WeakAuras-4.5.0.zip");
        assert_eq!(
            artifact.url,
            "https://edge.forgecdn.net/files/WeakAuras-4.5.0.zip"
        );
        assert_eq!(artifact.id, "4922788");
        assert_eq!(artifact.sha256.as_deref(), Some("def456sha256"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_error_when_project_id_missing() {
        let server = mockito::Server::new_async().await;
        let provider =
            CurseForgeProvider::with_base_url("key".into(), format!("{}/v1", server.url()));
        let addon = make_addon(None);
        let result = provider.resolve(&addon, &make_ctx()).await;
        assert!(matches!(result, Err(crate::Error::MissingProjectId { .. })));
    }

    #[tokio::test]
    async fn resolve_error_when_data_empty() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/mods/90003/files")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data":[],"pagination":{"index":0,"pageSize":50,"resultCount":0,"totalCount":0}}"#,
            )
            .create_async()
            .await;

        let provider =
            CurseForgeProvider::with_base_url("key".into(), format!("{}/v1", server.url()));
        let result = provider
            .resolve(&make_addon(Some(90003)), &make_ctx())
            .await;
        assert!(matches!(result, Err(crate::Error::NoRelease { .. })));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_uses_addon_channel_over_context_channel() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/mods/90003/files")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "data": [{
                    "id": 1001,
                    "displayName": "WeakAuras-beta.zip",
                    "downloadUrl": "https://cdn/beta.zip",
                    "releaseType": 2,
                    "gameVersions": ["12.0.5"],
                    "hashes": []
                }],
                "pagination": {"index":0,"pageSize":50,"resultCount":1,"totalCount":1}
            }"#,
            )
            .create_async()
            .await;

        let provider =
            CurseForgeProvider::with_base_url("key".into(), format!("{}/v1", server.url()));
        let mut addon = make_addon(Some(90003));
        addon.channel = Some(Channel::Beta);

        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();
        assert_eq!(artifact.version, "WeakAuras-beta.zip");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_skips_wrong_flavor_picks_matching() {
        let mut server = mockito::Server::new_async().await;
        // First file is era-only; second is retail (Midnight) — context is retail, picks second.
        let mock = server
            .mock("GET", "/v1/mods/90003/files")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "data": [
                    {
                        "id": 2001, "displayName": "WeakAuras-era.zip",
                        "downloadUrl": "https://cdn/era.zip",
                        "releaseType": 1, "gameVersions": ["1.15.4"], "hashes": []
                    },
                    {
                        "id": 2002, "displayName": "WeakAuras-retail.zip",
                        "downloadUrl": "https://cdn/retail.zip",
                        "releaseType": 1, "gameVersions": ["12.0.5"], "hashes": []
                    }
                ],
                "pagination": {"index":0,"pageSize":50,"resultCount":2,"totalCount":2}
            }"#,
            )
            .create_async()
            .await;

        let provider =
            CurseForgeProvider::with_base_url("key".into(), format!("{}/v1", server.url()));
        let artifact = provider
            .resolve(&make_addon(Some(90003)), &make_ctx())
            .await
            .unwrap();
        assert_eq!(artifact.version, "WeakAuras-retail.zip");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_error_on_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/mods/90003/files")
            .match_query(mockito::Matcher::Any)
            .with_status(403)
            .create_async()
            .await;

        let provider =
            CurseForgeProvider::with_base_url("bad-key".into(), format!("{}/v1", server.url()));
        let result = provider
            .resolve(&make_addon(Some(90003)), &make_ctx())
            .await;
        assert!(matches!(result, Err(crate::Error::Http(_))));
        mock.assert_async().await;
    }

    // download

    #[tokio::test]
    async fn download_writes_bytes_to_dest() {
        let mut server = mockito::Server::new_async().await;
        let fake_zip = b"PK\x03\x04fake zip content";
        let mock = server
            .mock("GET", "/files/WeakAuras.zip")
            .with_status(200)
            .with_body(fake_zip.as_slice())
            .create_async()
            .await;

        let provider =
            CurseForgeProvider::with_base_url("key".into(), format!("{}/v1", server.url()));
        let artifact = ResolvedArtifact {
            version: "4.5.0".into(),
            id: "4922788".into(),
            url: format!("{}/files/WeakAuras.zip", server.url()),
            sha256: None,
        };

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.zip");
        provider.download(&artifact, &dest).await.unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), fake_zip);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn download_error_on_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/files/bad.zip")
            .with_status(404)
            .create_async()
            .await;

        let provider =
            CurseForgeProvider::with_base_url("key".into(), format!("{}/v1", server.url()));
        let artifact = ResolvedArtifact {
            version: "1.0".into(),
            id: "1".into(),
            url: format!("{}/files/bad.zip", server.url()),
            sha256: None,
        };

        let dir = tempfile::tempdir().unwrap();
        let result = provider
            .download(&artifact, &dir.path().join("out.zip"))
            .await;
        assert!(matches!(result, Err(crate::Error::Http(_))));
        mock.assert_async().await;
    }

    // flavor_matches

    #[test]
    fn flavor_matches_retail_accepts_12x() {
        assert!(flavor_matches(&Flavor::Retail, &["12.0.5".into()]));
    }

    #[test]
    fn flavor_matches_retail_rejects_tww() {
        assert!(!flavor_matches(&Flavor::Retail, &["11.0.7".into()]));
    }

    #[test]
    fn flavor_matches_retail_rejects_era() {
        assert!(!flavor_matches(&Flavor::Retail, &["1.15.4".into()]));
    }

    #[test]
    fn flavor_matches_tww_accepts_11x() {
        assert!(flavor_matches(&Flavor::Tww, &["11.0.7".into()]));
    }

    #[test]
    fn flavor_matches_tww_rejects_retail() {
        assert!(!flavor_matches(&Flavor::Tww, &["12.0.5".into()]));
    }

    #[test]
    fn flavor_matches_era_accepts_1x() {
        assert!(flavor_matches(&Flavor::Era, &["1.15.4".into()]));
    }

    #[test]
    fn flavor_matches_era_rejects_retail() {
        assert!(!flavor_matches(&Flavor::Era, &["12.0.5".into()]));
    }

    #[test]
    fn flavor_matches_empty_accepts_any() {
        assert!(flavor_matches(&Flavor::Retail, &[]));
        assert!(flavor_matches(&Flavor::Era, &[]));
    }

    // channel_to_release_type

    #[test]
    fn channel_release_type_values() {
        assert_eq!(channel_to_release_type(&Channel::Stable), "1");
        assert_eq!(channel_to_release_type(&Channel::Beta), "2");
        assert_eq!(channel_to_release_type(&Channel::Alpha), "3");
    }
}

// ---------------------------------------------------------------------------
// LocalProvider (gated on the "local" feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "local")]
mod local_tests {
    use std::path::PathBuf;

    use crate::{
        manifest::ManifestAddon,
        model::{Channel, Flavor, Provider as ModelProvider, Tag},
        providers::{
            InstallContext, Provider, ResolvedArtifact,
            local::{LocalProvider, url_to_path},
        },
    };

    fn make_ctx() -> InstallContext {
        InstallContext {
            tag: Tag::new("test"),
            flavor: Flavor::Retail,
            channel: Channel::Stable,
            addons_path: PathBuf::from("/wow/Interface/AddOns"),
            cache_dir: PathBuf::from("/cache/wau"),
        }
    }

    fn make_addon(url: Option<&str>) -> ManifestAddon {
        ManifestAddon {
            name: "TestAddon".into(),
            provider: ModelProvider::Local,
            channel: None,
            flavors: None,
            pin: None,
            project_id: None,
            wowi_id: None,
            repo: None,
            asset_regex: None,
            git_ref: None,
            url: url.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn resolve_returns_error_when_url_missing() {
        let provider = LocalProvider::new();
        let addon = make_addon(None);
        let result = provider.resolve(&addon, &make_ctx()).await;
        assert!(matches!(result, Err(crate::Error::LocalMissingUrl { .. })));
    }

    #[tokio::test]
    async fn resolve_plain_path() {
        let provider = LocalProvider::new();
        let addon = make_addon(Some("/tmp/addon.zip"));
        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();
        assert_eq!(artifact.version, "local");
        assert!(artifact.id.contains("/tmp/addon.zip"));
        assert_eq!(artifact.url, "/tmp/addon.zip");
        assert!(artifact.sha256.is_none());
    }

    #[tokio::test]
    async fn resolve_file_url() {
        let provider = LocalProvider::new();
        let addon = make_addon(Some("file:///tmp/addon.zip"));
        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();
        assert!(artifact.id.contains("/tmp/addon.zip"));
        assert_eq!(artifact.url, "file:///tmp/addon.zip");
    }

    #[test]
    fn url_to_path_plain() {
        assert_eq!(url_to_path("/tmp/x.zip"), PathBuf::from("/tmp/x.zip"));
    }

    #[test]
    fn url_to_path_file_scheme() {
        assert_eq!(
            url_to_path("file:///tmp/x.zip"),
            PathBuf::from("/tmp/x.zip")
        );
    }

    #[tokio::test]
    async fn download_copies_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.zip");
        std::fs::write(&src, b"test data").unwrap();

        let dest = dir.path().join("dest.zip");
        let provider = LocalProvider::new();
        let artifact = ResolvedArtifact {
            version: "local".into(),
            id: "local:/tmp".into(),
            url: src.to_str().unwrap().to_owned(),
            sha256: None,
        };
        provider.download(&artifact, &dest).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"test data");
    }
}

// ---------------------------------------------------------------------------
// WoWInterface provider (gated on the "wowinterface" feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "wowinterface")]
mod wowinterface_tests {
    use std::path::PathBuf;

    use crate::{
        manifest::ManifestAddon,
        model::{Channel, Flavor, Provider as ModelProvider, Tag},
        providers::{
            InstallContext, Provider, ResolvedArtifact, wowinterface::WoWInterfaceProvider,
        },
    };

    fn make_ctx() -> InstallContext {
        InstallContext {
            tag: Tag::new("test"),
            flavor: Flavor::Retail,
            channel: Channel::Stable,
            addons_path: PathBuf::from("/wow/Interface/AddOns"),
            cache_dir: PathBuf::from("/tmp/wau-cache"),
        }
    }

    fn make_addon(wowi_id: Option<u64>) -> ManifestAddon {
        ManifestAddon {
            name: "WeakAuras".into(),
            provider: ModelProvider::WoWInterface,
            channel: None,
            flavors: None,
            pin: None,
            project_id: None,
            wowi_id,
            repo: None,
            asset_regex: None,
            git_ref: None,
            url: None,
        }
    }

    fn file_details_response() -> &'static str {
        r#"[{
            "UID": "12345",
            "UIName": "WeakAuras",
            "UIVersion": "4.5.0",
            "UIDownload": "https://cdn.wowinterface.com/downloads/file12345/WeakAuras-4.5.0.zip",
            "UIFileName": "WeakAuras-4.5.0.zip",
            "UIDate": "04-01-26 12:00 PM"
        }]"#
    }

    // resolve

    #[tokio::test]
    async fn resolve_returns_file_details() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/filedetails/12345.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(file_details_response())
            .create_async()
            .await;

        let provider = WoWInterfaceProvider::with_base_url(server.url());
        let artifact = provider
            .resolve(&make_addon(Some(12345)), &make_ctx())
            .await
            .unwrap();

        assert_eq!(artifact.version, "4.5.0");
        assert_eq!(artifact.id, "12345");
        assert_eq!(
            artifact.url,
            "https://cdn.wowinterface.com/downloads/file12345/WeakAuras-4.5.0.zip"
        );
        assert!(artifact.sha256.is_none());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_error_when_wowi_id_missing() {
        let server = mockito::Server::new_async().await;
        let provider = WoWInterfaceProvider::with_base_url(server.url());
        let result = provider.resolve(&make_addon(None), &make_ctx()).await;
        assert!(matches!(result, Err(crate::Error::MissingWowiId { .. })));
    }

    #[tokio::test]
    async fn resolve_error_when_data_empty() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/filedetails/12345.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;

        let provider = WoWInterfaceProvider::with_base_url(server.url());
        let result = provider
            .resolve(&make_addon(Some(12345)), &make_ctx())
            .await;
        assert!(matches!(result, Err(crate::Error::NoRelease { .. })));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_error_on_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/filedetails/12345.json")
            .with_status(404)
            .create_async()
            .await;

        let provider = WoWInterfaceProvider::with_base_url(server.url());
        let result = provider
            .resolve(&make_addon(Some(12345)), &make_ctx())
            .await;
        assert!(matches!(result, Err(crate::Error::Http(_))));
        mock.assert_async().await;
    }

    // download

    #[tokio::test]
    async fn download_writes_bytes_to_dest() {
        let mut server = mockito::Server::new_async().await;
        let fake_zip = b"PK\x03\x04fake zip content";
        let mock = server
            .mock("GET", "/files/WeakAuras.zip")
            .with_status(200)
            .with_body(fake_zip.as_slice())
            .create_async()
            .await;

        let provider = WoWInterfaceProvider::with_base_url(server.url());
        let artifact = ResolvedArtifact {
            version: "4.5.0".into(),
            id: "12345".into(),
            url: format!("{}/files/WeakAuras.zip", server.url()),
            sha256: None,
        };

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.zip");
        provider.download(&artifact, &dest).await.unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), fake_zip);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn download_error_on_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/files/bad.zip")
            .with_status(404)
            .create_async()
            .await;

        let provider = WoWInterfaceProvider::with_base_url(server.url());
        let artifact = ResolvedArtifact {
            version: "1.0".into(),
            id: "1".into(),
            url: format!("{}/files/bad.zip", server.url()),
            sha256: None,
        };

        let dir = tempfile::tempdir().unwrap();
        let result = provider
            .download(&artifact, &dir.path().join("out.zip"))
            .await;
        assert!(matches!(result, Err(crate::Error::Http(_))));
        mock.assert_async().await;
    }
}

// ---------------------------------------------------------------------------
// GitHub provider (gated on the "github" feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "github")]
mod github_tests {
    use std::path::PathBuf;

    use crate::{
        manifest::ManifestAddon,
        model::{Channel, Flavor, Provider as ModelProvider, Tag},
        providers::{InstallContext, Provider, ResolvedArtifact, github::GitHubProvider},
    };

    fn make_ctx() -> InstallContext {
        InstallContext {
            tag: Tag::new("test"),
            flavor: Flavor::Retail,
            channel: Channel::Stable,
            addons_path: PathBuf::from("/wow/Interface/AddOns"),
            cache_dir: PathBuf::from("/tmp/wau-cache"),
        }
    }

    fn make_addon(
        repo: Option<&str>,
        asset_regex: Option<&str>,
        git_ref: Option<&str>,
    ) -> ManifestAddon {
        ManifestAddon {
            name: "TestAddon".into(),
            provider: ModelProvider::GitHub,
            channel: None,
            flavors: None,
            pin: None,
            project_id: None,
            wowi_id: None,
            repo: repo.map(str::to_owned),
            asset_regex: asset_regex.map(str::to_owned),
            git_ref: git_ref.map(str::to_owned),
            url: None,
        }
    }

    fn releases_response(prerelease: bool) -> String {
        format!(
            r#"[{{
                "tag_name": "v4.5.0",
                "prerelease": {prerelease},
                "draft": false,
                "assets": [{{
                    "id": 11111,
                    "name": "TestAddon-4.5.0.zip",
                    "browser_download_url": "https://github.com/owner/repo/releases/download/v4.5.0/TestAddon-4.5.0.zip"
                }}]
            }}]"#
        )
    }

    // resolve — release asset mode

    #[tokio::test]
    async fn resolve_release_asset_returns_matching_asset() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/repos/owner/repo/releases")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(releases_response(false))
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(Some("owner/repo"), Some(r"TestAddon-.*\.zip"), None);
        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();

        assert_eq!(artifact.version, "v4.5.0");
        assert_eq!(artifact.id, "11111");
        assert_eq!(
            artifact.url,
            "https://github.com/owner/repo/releases/download/v4.5.0/TestAddon-4.5.0.zip"
        );
        assert!(artifact.sha256.is_none());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_release_asset_skips_prerelease_for_stable() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/repos/owner/repo/releases")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[
                {"tag_name":"v5.0.0-beta","prerelease":true,"draft":false,"assets":[
                    {"id":2,"name":"TestAddon-5.0.0-beta.zip","browser_download_url":"https://cdn/beta.zip"}
                ]},
                {"tag_name":"v4.5.0","prerelease":false,"draft":false,"assets":[
                    {"id":1,"name":"TestAddon-4.5.0.zip","browser_download_url":"https://cdn/stable.zip"}
                ]}
            ]"#)
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(Some("owner/repo"), Some(r"TestAddon-.*\.zip"), None);
        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();

        assert_eq!(artifact.version, "v4.5.0");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_release_asset_includes_prerelease_for_beta() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/repos/owner/repo/releases")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(releases_response(true))
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let mut addon = make_addon(Some("owner/repo"), Some(r"TestAddon-.*\.zip"), None);
        addon.channel = Some(Channel::Beta);

        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();
        assert_eq!(artifact.version, "v4.5.0");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_release_asset_error_when_no_matching_asset() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/repos/owner/repo/releases")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(releases_response(false))
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(Some("owner/repo"), Some(r"OtherAddon-.*\.zip"), None);
        let result = provider.resolve(&addon, &make_ctx()).await;

        assert!(matches!(result, Err(crate::Error::NoMatchingAsset { .. })));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_release_asset_error_on_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/repos/owner/repo/releases")
            .with_status(403)
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(Some("owner/repo"), Some(r"TestAddon-.*\.zip"), None);
        let result = provider.resolve(&addon, &make_ctx()).await;

        assert!(matches!(result, Err(crate::Error::Http(_))));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_error_when_repo_missing() {
        let server = mockito::Server::new_async().await;
        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(None, Some(r"TestAddon-.*\.zip"), None);
        let result = provider.resolve(&addon, &make_ctx()).await;
        assert!(matches!(result, Err(crate::Error::MissingRepo { .. })));
    }

    #[tokio::test]
    async fn resolve_error_when_neither_asset_regex_nor_git_ref() {
        let server = mockito::Server::new_async().await;
        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(Some("owner/repo"), None, None);
        let result = provider.resolve(&addon, &make_ctx()).await;
        assert!(matches!(result, Err(crate::Error::NoRelease { .. })));
    }

    // resolve — git-ref mode

    #[tokio::test]
    async fn resolve_git_ref_returns_commit_sha() {
        let mut server = mockito::Server::new_async().await;
        let sha = "deadbeef0123456789abcdef01234567deadbeef";
        let mock = server
            .mock("GET", "/repos/owner/repo/commits/main")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(r#"{{"sha":"{sha}"}}"#))
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(Some("owner/repo"), None, Some("main"));
        let artifact = provider.resolve(&addon, &make_ctx()).await.unwrap();

        assert_eq!(artifact.version, &sha[..7]);
        assert_eq!(artifact.id, sha);
        assert!(artifact.url.contains(sha));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_git_ref_error_on_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/repos/owner/repo/commits/main")
            .with_status(404)
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let addon = make_addon(Some("owner/repo"), None, Some("main"));
        let result = provider.resolve(&addon, &make_ctx()).await;

        assert!(matches!(result, Err(crate::Error::Http(_))));
        mock.assert_async().await;
    }

    // download

    #[tokio::test]
    async fn download_writes_bytes_to_dest() {
        let mut server = mockito::Server::new_async().await;
        let fake_zip = b"PK\x03\x04fake zip content";
        let mock = server
            .mock("GET", "/files/TestAddon.zip")
            .with_status(200)
            .with_body(fake_zip.as_slice())
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let artifact = ResolvedArtifact {
            version: "v4.5.0".into(),
            id: "11111".into(),
            url: format!("{}/files/TestAddon.zip", server.url()),
            sha256: None,
        };

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.zip");
        provider.download(&artifact, &dest).await.unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), fake_zip);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn download_error_on_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/files/bad.zip")
            .with_status(404)
            .create_async()
            .await;

        let provider = GitHubProvider::with_base_url(None, server.url());
        let artifact = ResolvedArtifact {
            version: "v1.0".into(),
            id: "1".into(),
            url: format!("{}/files/bad.zip", server.url()),
            sha256: None,
        };

        let dir = tempfile::tempdir().unwrap();
        let result = provider
            .download(&artifact, &dir.path().join("out.zip"))
            .await;
        assert!(matches!(result, Err(crate::Error::Http(_))));
        mock.assert_async().await;
    }
}
