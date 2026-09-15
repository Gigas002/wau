use super::*;

#[test]
#[cfg(all(
    feature = "github",
    feature = "curseforge",
    feature = "wowinterface",
    feature = "tukui",
    feature = "wago"
))]
fn default_sources_are_registered_in_priority_order() {
    let sources = default_sources(&SourceConfig::default());
    let ids: Vec<&str> = sources.iter().map(|s| s.metadata().id).collect();
    assert_eq!(ids, vec!["github", "curse", "wowi", "tukui", "wago"]);
}

#[test]
fn find_source_looks_up_by_id() {
    let sources = default_sources(&SourceConfig::default());
    if !sources.is_empty() {
        let id = sources[0].metadata().id;
        assert!(find_source(&sources, id).is_some());
    }
    assert!(find_source(&sources, "does-not-exist").is_none());
}
