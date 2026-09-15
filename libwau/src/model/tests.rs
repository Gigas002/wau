use std::path::Path;

use super::*;

// ---------------------------------------------------------------------------
// Flavour
// ---------------------------------------------------------------------------

#[test]
fn flavour_round_trips_through_display_and_parse() {
    for f in Flavour::ALL {
        assert_eq!(Flavour::parse(f.as_str()), Some(f));
        assert_eq!(f.to_string(), f.as_str());
    }
}

#[test]
fn flavour_classic_alias_is_mists() {
    assert_eq!(Flavour::parse("classic"), Some(Flavour::MistsClassic));
    assert_eq!(Flavour::CLASSIC, Flavour::MistsClassic);
}

#[test]
fn flavour_retail_back_compat_alias_maps_to_mainline() {
    assert_eq!(Flavour::parse("retail"), Some(Flavour::Mainline));
}

#[test]
fn flavour_parse_rejects_unknown() {
    assert_eq!(Flavour::parse("bogus"), None);
    assert!("bogus".parse::<Flavour>().is_err());
}

#[test]
fn flavour_serde_round_trip() {
    let json = serde_json::to_string(&Flavour::TbcClassic).unwrap();
    assert_eq!(json, "\"tbc_classic\"");
    let back: Flavour = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Flavour::TbcClassic);
}

#[test]
fn flavour_serde_accepts_retail_alias() {
    let f: Flavour = serde_json::from_str("\"retail\"").unwrap();
    assert_eq!(f, Flavour::Mainline);
}

#[test]
fn flavour_from_build_number_matches_known_ranges() {
    assert_eq!(Flavour::from_build_number(1_12_01), Some(Flavour::Mainline));
    assert_eq!(
        Flavour::from_build_number(1_13_05),
        Some(Flavour::VanillaClassic)
    );
    assert_eq!(
        Flavour::from_build_number(2_05_09),
        Some(Flavour::TbcClassic)
    );
    assert_eq!(
        Flavour::from_build_number(3_04_02),
        Some(Flavour::WrathClassic)
    );
    assert_eq!(
        Flavour::from_build_number(3_08_00),
        Some(Flavour::TitanClassic)
    );
    assert_eq!(
        Flavour::from_build_number(4_04_00),
        Some(Flavour::CataClassic)
    );
    assert_eq!(
        Flavour::from_build_number(5_05_00),
        Some(Flavour::MistsClassic)
    );
    assert_eq!(
        Flavour::from_build_number(11_00_02),
        Some(Flavour::Mainline)
    );
    assert_eq!(Flavour::from_build_number(0), None);
}

#[test]
fn flavour_from_version_string_pads_missing_components() {
    assert_eq!(
        Flavour::from_version_string("1.13"),
        Some(Flavour::VanillaClassic)
    );
    assert_eq!(Flavour::from_version_string("11"), Some(Flavour::Mainline));
    assert_eq!(Flavour::from_version_string("not-a-version"), None);
}

#[test]
fn flavour_toc_suffixes_wrath_and_titan_share_table() {
    assert_eq!(
        Flavour::WrathClassic.toc_suffixes(),
        Flavour::TitanClassic.toc_suffixes()
    );
    assert_eq!(Flavour::Mainline.toc_suffixes(), &["Mainline"]);
    assert_eq!(
        Flavour::VanillaClassic.toc_suffixes(),
        &["Vanilla", "Classic"]
    );
}

// ---------------------------------------------------------------------------
// Product / install-dir detection
// ---------------------------------------------------------------------------

#[test]
fn extract_installation_dir_from_addon_dir_matches_interface_addons_tail() {
    let addon_dir = Path::new("/games/wow/_retail_/Interface/AddOns");
    let install_dir = extract_installation_dir_from_addon_dir(addon_dir).unwrap();
    assert_eq!(install_dir, Path::new("/games/wow/_retail_"));
}

#[test]
fn extract_installation_dir_from_addon_dir_is_case_insensitive() {
    let addon_dir = Path::new("/games/wow/_retail_/interface/addons");
    assert!(extract_installation_dir_from_addon_dir(addon_dir).is_some());
}

#[test]
fn extract_installation_dir_from_addon_dir_rejects_non_matching_tail() {
    let addon_dir = Path::new("/games/wow/_retail_/SomethingElse/AddOns");
    assert_eq!(extract_installation_dir_from_addon_dir(addon_dir), None);
}

#[test]
fn get_addon_dir_from_installation_dir_appends_interface_addons() {
    let install_dir = Path::new("/games/wow/_retail_");
    assert_eq!(
        get_addon_dir_from_installation_dir(install_dir),
        Path::new("/games/wow/_retail_/Interface/AddOns")
    );
}

#[test]
fn infer_product_from_addon_dir_recognises_known_subfolder() {
    let addon_dir = Path::new("/games/wow/_classic_era_/Interface/AddOns");
    let product = infer_product_from_addon_dir(addon_dir).unwrap();
    assert_eq!(product.code, "wow_classic_era");
    assert_eq!(product.flavour, Flavour::VanillaClassic);
}

#[test]
fn infer_product_from_addon_dir_returns_none_for_custom_subfolder() {
    let addon_dir = Path::new("/games/wow/my-private-server/Interface/AddOns");
    assert_eq!(infer_product_from_addon_dir(addon_dir), None);
}

#[test]
fn products_table_has_no_duplicate_codes() {
    let mut codes: Vec<&str> = PRODUCTS.iter().map(|p| p.code).collect();
    codes.sort_unstable();
    let mut deduped = codes.clone();
    deduped.dedup();
    assert_eq!(codes, deduped);
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

#[test]
fn strategies_default_is_empty() {
    assert!(Strategies::default().is_empty());
}

#[test]
fn strategies_uri_tokens_only_include_set_fields() {
    let s = Strategies {
        any_flavour: true,
        any_release_type: false,
        version_eq: Some("1.2.3".to_owned()),
    };
    assert_eq!(s.uri_tokens(), vec!["any_flavour", "version_eq=1.2.3"]);
}

// ---------------------------------------------------------------------------
// Defn
// ---------------------------------------------------------------------------

const KNOWN_SOURCES: &[&str] = &["curse", "github", "wowi", "tukui", "wago"];

#[test]
fn defn_from_uri_parses_source_and_alias() {
    let defn = Defn::from_uri("curse:some-addon", KNOWN_SOURCES, false).unwrap();
    assert_eq!(defn.source, "curse");
    assert_eq!(defn.alias, "some-addon");
    assert!(defn.strategies.is_empty());
}

#[test]
fn defn_from_uri_parses_strategies_fragment() {
    let defn = Defn::from_uri(
        "curse:some-addon#any_flavour,version_eq=1.2.3",
        KNOWN_SOURCES,
        false,
    )
    .unwrap();
    assert!(defn.strategies.any_flavour);
    assert!(!defn.strategies.any_release_type);
    assert_eq!(defn.strategies.version_eq.as_deref(), Some("1.2.3"));
}

#[test]
fn defn_from_uri_bare_equals_fragment_is_explicit_default() {
    let defn = Defn::from_uri("curse:some-addon#=", KNOWN_SOURCES, false).unwrap();
    assert!(defn.strategies.is_empty());
}

#[test]
fn defn_from_uri_rejects_unknown_strategy() {
    let err = Defn::from_uri("curse:some-addon#bogus", KNOWN_SOURCES, false).unwrap_err();
    assert!(matches!(err, DefnParseError::UnknownStrategies(_)));
}

#[test]
fn defn_from_uri_unknown_source_becomes_bare_alias_when_not_retained() {
    let defn = Defn::from_uri(
        "https://www.curseforge.com/wow/addons/foo",
        KNOWN_SOURCES,
        false,
    )
    .unwrap();
    assert_eq!(defn.source, "");
    assert_eq!(defn.alias, "https://www.curseforge.com/wow/addons/foo");
}

#[test]
fn defn_from_uri_unknown_source_retained_when_requested() {
    let defn = Defn::from_uri("wago:some-addon", KNOWN_SOURCES, true).unwrap();
    assert_eq!(defn.source, "wago");
    assert_eq!(defn.alias, "some-addon");
}

#[test]
fn defn_as_uri_round_trips_alias_and_strategies() {
    let defn = Defn::new("curse", "some-addon").with_version("1.2.3");
    assert_eq!(defn.as_uri(false, false), "curse:some-addon");
    assert_eq!(
        defn.as_uri(false, true),
        "curse:some-addon#version_eq=1.2.3"
    );
}

#[test]
fn defn_as_uri_alias_is_id_uses_id_when_present() {
    let mut defn = Defn::new("curse", "some-addon");
    defn.id = Some("12345".to_owned());
    assert_eq!(defn.as_uri(true, false), "curse:12345");
}

#[test]
fn defn_as_uri_alias_is_id_falls_back_to_alias_without_id() {
    let defn = Defn::new("curse", "some-addon");
    assert_eq!(defn.as_uri(true, false), "curse:some-addon");
}
