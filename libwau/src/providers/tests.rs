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
    let result = for_provider(&crate::model::Provider::Local);
    assert!(matches!(
        result,
        Err(crate::Error::ProviderNotSupported { .. })
    ));
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
            repo: None,
            asset_regex: None,
            git_ref: None,
            url: url.map(str::to_owned),
        }
    }

    #[test]
    fn resolve_returns_error_when_url_missing() {
        let provider = LocalProvider::new();
        let addon = make_addon(None);
        let result = provider.resolve(&addon, &make_ctx());
        assert!(matches!(result, Err(crate::Error::LocalMissingUrl { .. })));
    }

    #[test]
    fn resolve_plain_path() {
        let provider = LocalProvider::new();
        let addon = make_addon(Some("/tmp/addon.zip"));
        let artifact = provider.resolve(&addon, &make_ctx()).unwrap();
        assert_eq!(artifact.version, "local");
        assert!(artifact.id.contains("/tmp/addon.zip"));
        assert_eq!(artifact.url, "/tmp/addon.zip");
        assert!(artifact.sha256.is_none());
    }

    #[test]
    fn resolve_file_url() {
        let provider = LocalProvider::new();
        let addon = make_addon(Some("file:///tmp/addon.zip"));
        let artifact = provider.resolve(&addon, &make_ctx()).unwrap();
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

    #[test]
    fn download_copies_file() {
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
        provider.download(&artifact, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"test data");
    }
}
