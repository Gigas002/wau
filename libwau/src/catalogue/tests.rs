use super::*;

fn raw_entry(
    source: &str,
    id: &str,
    name: &str,
    downloads: u64,
    same_as: Vec<(&str, &str)>,
) -> serde_json::Value {
    serde_json::json!({
        "source": source,
        "id": id,
        "slug": format!("{name}-slug"),
        "name": name,
        "url": format!("https://example.invalid/{name}"),
        "game_flavours": ["mainline"],
        "download_count": downloads,
        "last_updated": "2026-01-01T00:00:00Z",
        "folders": [[name]],
        "same_as": same_as.into_iter().map(|(s,i)| serde_json::json!({"source": s, "id": i})).collect::<Vec<_>>(),
    })
}

fn parse(entries: Vec<serde_json::Value>) -> ComputedCatalogue {
    let raw: RawCatalogue =
        serde_json::from_value(serde_json::json!({ "entries": entries })).unwrap();
    ComputedCatalogue::from_raw(raw)
}

#[test]
fn normalise_name_strips_non_alphanumeric_and_casefolds() {
    assert_eq!(normalise_name("WeakAuras 2!"), "weakauras2");
    assert_eq!(normalise_name("Foo-Bar_Baz"), "foobarbaz");
}

#[test]
fn derived_download_score_normalises_within_source() {
    let catalogue = parse(vec![
        raw_entry("curse", "1", "Foo", 100, vec![]),
        raw_entry("curse", "2", "Bar", 50, vec![]),
        raw_entry("wowi", "9", "Baz", 10, vec![]),
    ]);
    let foo = catalogue.entries.iter().find(|e| e.id == "1").unwrap();
    let bar = catalogue.entries.iter().find(|e| e.id == "2").unwrap();
    let baz = catalogue.entries.iter().find(|e| e.id == "9").unwrap();
    assert_eq!(foo.derived_download_score, 1.0);
    assert_eq!(bar.derived_download_score, 0.5);
    // Different source's own max (10) normalises independently to 1.0.
    assert_eq!(baz.derived_download_score, 1.0);
}

#[test]
fn same_as_backfilled_from_github_entry() {
    let catalogue = parse(vec![
        raw_entry(
            "github",
            "1",
            "Foo",
            10,
            vec![("curse", "100"), ("wowi", "200")],
        ),
        raw_entry("curse", "100", "Foo", 5, vec![]),
    ]);
    let curse_entry = catalogue
        .entries
        .iter()
        .find(|e| e.source == "curse")
        .unwrap();
    // Backfilled with the github entry itself, plus the OTHER cross-ref (wowi),
    // excluding the one matching curse itself.
    assert!(
        curse_entry
            .same_as
            .iter()
            .any(|k| k.source == "github" && k.id == "1")
    );
    assert!(
        curse_entry
            .same_as
            .iter()
            .any(|k| k.source == "wowi" && k.id == "200")
    );
    assert!(!curse_entry.same_as.iter().any(|k| k.source == "curse"));
}

#[test]
fn github_entry_keeps_its_own_same_as_unchanged() {
    let catalogue = parse(vec![raw_entry(
        "github",
        "1",
        "Foo",
        10,
        vec![("curse", "100")],
    )]);
    let gh = &catalogue.entries[0];
    assert_eq!(
        gh.same_as,
        vec![AddonKey {
            source: "curse".into(),
            id: "100".into()
        }]
    );
}

#[test]
fn entry_without_github_backfill_keeps_own_same_as() {
    let catalogue = parse(vec![raw_entry(
        "curse",
        "1",
        "Foo",
        10,
        vec![("wowi", "9")],
    )]);
    let entry = &catalogue.entries[0];
    assert_eq!(
        entry.same_as,
        vec![AddonKey {
            source: "wowi".into(),
            id: "9".into()
        }]
    );
}

#[test]
fn keyed_entries_looks_up_by_source_and_id() {
    let catalogue = parse(vec![raw_entry("curse", "1", "Foo", 10, vec![])]);
    let keyed = catalogue.keyed_entries();
    assert!(keyed.contains_key(&("curse", "1")));
    assert!(!keyed.contains_key(&("curse", "2")));
}

#[test]
fn unparsable_last_updated_drops_the_entry() {
    let mut entry = raw_entry("curse", "1", "Foo", 10, vec![]);
    entry["last_updated"] = serde_json::json!("not-a-date");
    let catalogue = parse(vec![entry]);
    assert!(catalogue.entries.is_empty());
}

mod git_cache {
    use std::path::Path;

    use crate::catalogue::git_cache::resolve;

    async fn init_source_repo(dir: &Path, branch: &str, filename: &str, contents: &str) {
        let git = |args: &[&str]| {
            let dir = dir.to_owned();
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            async move {
                let status = tokio::process::Command::new("git")
                    .args(&args)
                    .current_dir(&dir)
                    .status()
                    .await
                    .unwrap();
                assert!(status.success(), "git {args:?} failed");
            }
        };
        tokio::fs::create_dir_all(dir).await.unwrap();
        git(&["init", "--quiet", "--initial-branch", branch]).await;
        git(&["config", "user.email", "test@example.invalid"]).await;
        git(&["config", "user.name", "test"]).await;
        tokio::fs::write(dir.join(filename), contents)
            .await
            .unwrap();
        git(&["add", filename]).await;
        git(&["commit", "--quiet", "-m", "data"]).await;
    }

    #[tokio::test]
    async fn clones_then_updates_on_change() {
        let src = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        init_source_repo(src.path(), "data", "catalogue.json", "v1").await;

        let repo_url = src.path().to_string_lossy().into_owned();
        let path = resolve(&repo_url, "data", cache.path(), "catalogue.json")
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "v1");

        init_source_repo(src.path(), "data", "catalogue.json", "v2").await;
        let path = resolve(&repo_url, "data", cache.path(), "catalogue.json")
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "v2");
    }

    #[tokio::test]
    async fn falls_back_to_stale_copy_when_update_fails() {
        let src = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        init_source_repo(src.path(), "data", "catalogue.json", "v1").await;

        let repo_url = src.path().to_string_lossy().into_owned();
        resolve(&repo_url, "data", cache.path(), "catalogue.json")
            .await
            .unwrap();

        // The remote's gone, but the local clone is intact.
        drop(src);
        let path = resolve(&repo_url, "data", cache.path(), "catalogue.json")
            .await
            .unwrap();
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "v1");
    }

    #[tokio::test]
    async fn errors_when_first_clone_fails() {
        let cache = tempfile::tempdir().unwrap();
        let err = resolve(
            "/nonexistent/instawow-data",
            "data",
            cache.path(),
            "catalogue.json",
        )
        .await;
        assert!(err.is_err());
    }
}
