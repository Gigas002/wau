use super::*;

// Fixture files embedded at compile time so tests run without filesystem I/O.
const FIXTURE_SIMPLE: &str = include_str!("../../../examples/toc/simple.toc");
const FIXTURE_MULTI_CLIENT: &str = include_str!("../../../examples/toc/multi-client.toc");
const FIXTURE_FLAVOR_SPECIFIC: &str = include_str!("../../../examples/toc/flavor-specific.toc");
const FIXTURE_CURSEFORGE: &str = include_str!("../../../examples/toc/curseforge.toc");
const FIXTURE_COLORFUL: &str = include_str!("../../../examples/toc/colorful.toc");
const FIXTURE_DEPS: &str = include_str!("../../../examples/toc/dependencies.toc");

// ---------------------------------------------------------------------------
// strip_color_codes
// ---------------------------------------------------------------------------

#[test]
fn strip_color_plain_string_unchanged() {
    assert_eq!(strip_color_codes("Hello"), "Hello");
}

#[test]
fn strip_color_single_colored_word() {
    assert_eq!(strip_color_codes("|cffffe00aDBM|r"), "DBM");
}

#[test]
fn strip_color_complex_dbm_title() {
    let raw = "|cffffe00a<|r|cffff7d0aDBM|r|cffffe00a>|r |cff10ff10Core|r";
    assert_eq!(strip_color_codes(raw), "<DBM> Core");
}

#[test]
fn strip_color_empty_string() {
    assert_eq!(strip_color_codes(""), "");
}

#[test]
fn strip_color_only_codes_becomes_empty() {
    assert_eq!(strip_color_codes("|cffffe00a|r"), "");
}

#[test]
fn strip_color_pipe_without_color_code_preserved() {
    // A bare '|' not followed by 'c' or 'r' is kept as-is.
    assert_eq!(strip_color_codes("a|b"), "a|b");
}

// ---------------------------------------------------------------------------
// parse_csv / parse_csv_u32 (via parse_str)
// ---------------------------------------------------------------------------

#[test]
fn csv_single_value() {
    let toc = parse_str("## SavedVariables: MyDB\n");
    assert_eq!(toc.saved_variables, vec!["MyDB"]);
}

#[test]
fn csv_multiple_values_with_spaces() {
    let toc = parse_str("## OptionalDeps: Ace3, LibStub, LibSharedMedia-3.0\n");
    assert_eq!(
        toc.optional_deps,
        vec!["Ace3", "LibStub", "LibSharedMedia-3.0"]
    );
}

#[test]
fn csv_trailing_comma_ignored() {
    let toc = parse_str("## Dependencies: A, B,\n");
    assert_eq!(toc.dependencies, vec!["A", "B"]);
}

// ---------------------------------------------------------------------------
// parse_bool (via parse_str)
// ---------------------------------------------------------------------------

#[test]
fn bool_value_one_is_true() {
    let toc = parse_str("## LoadOnDemand: 1\n");
    assert!(toc.load_on_demand);
}

#[test]
fn bool_value_true_is_true() {
    let toc = parse_str("## LoadOnDemand: true\n");
    assert!(toc.load_on_demand);
}

#[test]
fn bool_value_yes_is_true() {
    let toc = parse_str("## LoadOnDemand: yes\n");
    assert!(toc.load_on_demand);
}

#[test]
fn bool_value_zero_is_false() {
    let toc = parse_str("## LoadOnDemand: 0\n");
    assert!(!toc.load_on_demand);
}

#[test]
fn bool_absent_is_false() {
    let toc = parse_str("## Interface: 110200\n");
    assert!(!toc.load_on_demand);
}

// ---------------------------------------------------------------------------
// Interface parsing
// ---------------------------------------------------------------------------

#[test]
fn interface_single_version() {
    let toc = parse_str("## Interface: 110200\n");
    assert_eq!(toc.interface, vec![110200]);
}

#[test]
fn interface_multiple_versions_comma_separated() {
    let toc = parse_str("## Interface: 110207, 50503, 38000\n");
    assert_eq!(toc.interface, vec![110207, 50503, 38000]);
}

#[test]
fn interface_flavor_specific_keys() {
    let content =
        "## Interface-Mainline: 120001\n## Interface-Vanilla: 11508\n## Interface-TBC: 20505\n";
    let toc = parse_str(content);
    assert!(
        toc.interface.is_empty(),
        "generic Interface should be empty"
    );
    assert_eq!(toc.interface_flavor.get("Mainline"), Some(&vec![120001]));
    assert_eq!(toc.interface_flavor.get("Vanilla"), Some(&vec![11508]));
    assert_eq!(toc.interface_flavor.get("TBC"), Some(&vec![20505]));
}

#[test]
fn interface_flavor_capitalisation_preserved() {
    let toc = parse_str("## Interface-Mainline: 120001\n");
    assert!(toc.interface_flavor.contains_key("Mainline"));
    assert!(!toc.interface_flavor.contains_key("mainline"));
}

#[test]
fn interface_non_numeric_skipped() {
    let toc = parse_str("## Interface: 110200, bad, 50503\n");
    assert_eq!(toc.interface, vec![110200, 50503]);
}

// ---------------------------------------------------------------------------
// Title / Notes
// ---------------------------------------------------------------------------

#[test]
fn title_plain_string() {
    let toc = parse_str("## Title: SimpleAddon\n");
    assert_eq!(toc.title.as_deref(), Some("SimpleAddon"));
    assert_eq!(toc.title_raw.as_deref(), Some("SimpleAddon"));
}

#[test]
fn title_color_codes_stripped() {
    let toc = parse_str("## Title: |cffffe00a<|r|cffff7d0aDBM|r|cffffe00a>|r |cff10ff10Core|r\n");
    assert_eq!(toc.title.as_deref(), Some("<DBM> Core"));
}

#[test]
fn title_raw_preserves_color_codes() {
    let raw = "|cffffe00aDBM|r";
    let toc = parse_str(&format!("## Title: {raw}\n"));
    assert_eq!(toc.title.as_deref(), Some("DBM"));
    assert_eq!(toc.title_raw.as_deref(), Some(raw));
}

#[test]
fn title_locale_suffix() {
    let content = "## Title: English\n## Title-frFR: Français\n## Title-deDE: Deutsch\n";
    let toc = parse_str(content);
    assert_eq!(toc.title.as_deref(), Some("English"));
    // Locale keys are stored lowercase.
    assert_eq!(
        toc.title_locale.get("frfr").map(String::as_str),
        Some("Français")
    );
    assert_eq!(
        toc.title_locale.get("dede").map(String::as_str),
        Some("Deutsch")
    );
}

#[test]
fn notes_plain() {
    let toc = parse_str("## Notes: A short description.\n");
    assert_eq!(toc.notes.as_deref(), Some("A short description."));
}

#[test]
fn notes_locale_suffix() {
    let toc = parse_str("## Notes: EN notes\n## Notes-deDE: DE Notizen\n");
    assert_eq!(
        toc.notes_locale.get("dede").map(String::as_str),
        Some("DE Notizen")
    );
}

// ---------------------------------------------------------------------------
// Other standard fields
// ---------------------------------------------------------------------------

#[test]
fn author_parsed() {
    let toc = parse_str("## Author: SomeDev\n");
    assert_eq!(toc.author.as_deref(), Some("SomeDev"));
}

#[test]
fn category_parsed() {
    let toc = parse_str("## Category: DBM\n");
    assert_eq!(toc.category.as_deref(), Some("DBM"));
}

#[test]
fn group_parsed() {
    let toc = parse_str("## Group: MySuite\n");
    assert_eq!(toc.group.as_deref(), Some("MySuite"));
}

#[test]
fn icon_texture_parsed() {
    let toc = parse_str("## IconTexture: Interface\\AddOns\\MyAddon\\icon.tga\n");
    assert_eq!(
        toc.icon_texture.as_deref(),
        Some("Interface\\AddOns\\MyAddon\\icon.tga")
    );
}

#[test]
fn icon_atlas_parsed() {
    let toc = parse_str("## IconAtlas: MyAtlasEntry\n");
    assert_eq!(toc.icon_atlas.as_deref(), Some("MyAtlasEntry"));
}

// ---------------------------------------------------------------------------
// Dependencies
// ---------------------------------------------------------------------------

#[test]
fn dependencies_key() {
    let toc = parse_str("## Dependencies: CoreAddon, SharedLib\n");
    assert_eq!(toc.dependencies, vec!["CoreAddon", "SharedLib"]);
}

#[test]
fn requireddeps_alias() {
    let toc = parse_str("## RequiredDeps: CoreAddon\n");
    assert_eq!(toc.dependencies, vec!["CoreAddon"]);
}

#[test]
fn dep_star_alias() {
    // Any key starting with "Dep" (case-insensitive) is treated as required deps.
    let toc = parse_str("## Deps: CoreAddon, Lib\n");
    assert_eq!(toc.dependencies, vec!["CoreAddon", "Lib"]);
}

#[test]
fn optional_deps_parsed() {
    let toc = parse_str("## OptionalDeps: Ace3, LibStub\n");
    assert_eq!(toc.optional_deps, vec!["Ace3", "LibStub"]);
}

#[test]
fn load_with_parsed() {
    let toc = parse_str("## LoadWith: OtherAddon\n");
    assert_eq!(toc.load_with, vec!["OtherAddon"]);
}

#[test]
fn load_managers_parsed() {
    let toc = parse_str("## LoadManagers: AddonLoader\n");
    assert_eq!(toc.load_managers, vec!["AddonLoader"]);
}

// ---------------------------------------------------------------------------
// Load behaviour
// ---------------------------------------------------------------------------

#[test]
fn default_state_parsed() {
    let toc = parse_str("## DefaultState: enabled\n");
    assert_eq!(toc.default_state.as_deref(), Some("enabled"));
}

#[test]
fn load_first_true() {
    let toc = parse_str("## LoadFirst: 1\n");
    assert!(toc.load_first);
}

#[test]
fn allow_load_game_type_parsed() {
    let toc = parse_str("## AllowLoadGameType: mainline, classic\n");
    assert_eq!(toc.allow_load_game_type, vec!["mainline", "classic"]);
}

// ---------------------------------------------------------------------------
// Saved variables
// ---------------------------------------------------------------------------

#[test]
fn saved_variables_parsed() {
    let toc = parse_str("## SavedVariables: DB1, DB2\n");
    assert_eq!(toc.saved_variables, vec!["DB1", "DB2"]);
}

#[test]
fn saved_variables_per_character_parsed() {
    let toc = parse_str("## SavedVariablesPerCharacter: CharDB\n");
    assert_eq!(toc.saved_variables_per_character, vec!["CharDB"]);
}

// ---------------------------------------------------------------------------
// X-* custom fields
// ---------------------------------------------------------------------------

#[test]
fn x_field_stored_with_prefix() {
    let toc = parse_str("## X-Curse-Project-ID: 12345\n");
    assert_eq!(
        toc.x_fields.get("X-Curse-Project-ID").map(String::as_str),
        Some("12345")
    );
}

#[test]
fn x_field_case_preserved() {
    let toc = parse_str("## X-Wago-ID: aAbBcCdD\n");
    assert_eq!(
        toc.x_fields.get("X-Wago-ID").map(String::as_str),
        Some("aAbBcCdD")
    );
}

#[test]
fn multiple_x_fields_all_stored() {
    let content = "## X-Curse-Project-ID: 274066\n## X-Wago-ID: nQN5aoNB\n## X-DBM-Mod: 1\n";
    let toc = parse_str(content);
    assert_eq!(toc.x_fields.len(), 3);
}

// ---------------------------------------------------------------------------
// Case insensitivity
// ---------------------------------------------------------------------------

#[test]
fn keys_are_case_insensitive() {
    let toc = parse_str("## INTERFACE: 110200\n## TITLE: Foo\n## VERSION: 1.0\n");
    assert_eq!(toc.interface, vec![110200]);
    assert_eq!(toc.title.as_deref(), Some("Foo"));
    assert_eq!(toc.version.as_deref(), Some("1.0"));
}

#[test]
fn mixed_case_key() {
    let toc = parse_str("## SavedVariables: DB\n## savedvariablespercharacter: CharDB\n");
    assert_eq!(toc.saved_variables, vec!["DB"]);
    assert_eq!(toc.saved_variables_per_character, vec!["CharDB"]);
}

// ---------------------------------------------------------------------------
// Comments and blank lines
// ---------------------------------------------------------------------------

#[test]
fn single_hash_comment_ignored() {
    let toc = parse_str("# this is a comment\n## Title: Real\n");
    assert_eq!(toc.title.as_deref(), Some("Real"));
}

#[test]
fn blank_lines_ignored() {
    let toc = parse_str("\n## Title: Real\n\n## Version: 1.0\n\n");
    assert_eq!(toc.title.as_deref(), Some("Real"));
    assert_eq!(toc.version.as_deref(), Some("1.0"));
}

#[test]
fn double_hash_without_space_treated_as_comment() {
    // "##Key: Value" (no space after ##) — not a directive per spec.
    let toc = parse_str("##Title: ShouldNotParse\n## Title: Correct\n");
    assert_eq!(toc.title.as_deref(), Some("Correct"));
}

// ---------------------------------------------------------------------------
// File entries
// ---------------------------------------------------------------------------

#[test]
fn file_entries_collected() {
    let toc = parse_str("## Title: T\nCore.lua\nUtils.lua\nLocales\\enUS.lua\n");
    assert_eq!(
        toc.files,
        vec!["Core.lua", "Utils.lua", "Locales\\enUS.lua"]
    );
}

#[test]
fn file_entry_with_conditional_directive_stored_verbatim() {
    let toc = parse_str("[AllowLoadGameType mainline] Core.lua\n");
    assert_eq!(toc.files, vec!["[AllowLoadGameType mainline] Core.lua"]);
}

#[test]
fn empty_content_gives_default_toc() {
    let toc = parse_str("");
    assert!(toc.interface.is_empty());
    assert!(toc.title.is_none());
    assert!(toc.files.is_empty());
}

// ---------------------------------------------------------------------------
// Value parsing edge cases
// ---------------------------------------------------------------------------

#[test]
fn value_with_colon_preserved() {
    // Only split on the first colon — values may contain colons.
    let toc = parse_str("## Notes: See https://example.com for more.\n");
    assert_eq!(
        toc.notes.as_deref(),
        Some("See https://example.com for more.")
    );
}

#[test]
fn value_leading_trailing_whitespace_trimmed() {
    let toc = parse_str("## Version:   2.0.0   \n");
    assert_eq!(toc.version.as_deref(), Some("2.0.0"));
}

#[test]
fn unknown_directive_does_not_panic() {
    let toc =
        parse_str("## X-Some-Future-Tag: whatever\n## CompletelyUnknown: ignored\n## Title: OK\n");
    assert_eq!(toc.title.as_deref(), Some("OK"));
}

// ---------------------------------------------------------------------------
// Fixture: simple.toc
// ---------------------------------------------------------------------------

#[test]
fn fixture_simple_interface() {
    let toc = parse_str(FIXTURE_SIMPLE);
    assert_eq!(toc.interface, vec![120001]);
}

#[test]
fn fixture_simple_title() {
    let toc = parse_str(FIXTURE_SIMPLE);
    assert_eq!(toc.title.as_deref(), Some("SimpleAddon"));
}

#[test]
fn fixture_simple_version() {
    let toc = parse_str(FIXTURE_SIMPLE);
    assert_eq!(toc.version.as_deref(), Some("1.0.0"));
}

#[test]
fn fixture_simple_author() {
    let toc = parse_str(FIXTURE_SIMPLE);
    assert_eq!(toc.author.as_deref(), Some("Anon"));
}

#[test]
fn fixture_simple_file_count() {
    let toc = parse_str(FIXTURE_SIMPLE);
    assert_eq!(toc.files, vec!["SimpleAddon.lua"]);
}

// ---------------------------------------------------------------------------
// Fixture: multi-client.toc
// ---------------------------------------------------------------------------

#[test]
fn fixture_multi_client_interface_count() {
    let toc = parse_str(FIXTURE_MULTI_CLIENT);
    assert_eq!(toc.interface.len(), 10);
}

#[test]
fn fixture_multi_client_contains_retail_and_era() {
    let toc = parse_str(FIXTURE_MULTI_CLIENT);
    assert!(toc.interface.contains(&120001)); // Retail/Midnight
    assert!(toc.interface.contains(&11508)); // Classic Era
}

#[test]
fn fixture_multi_client_x_curse_id() {
    let toc = parse_str(FIXTURE_MULTI_CLIENT);
    assert_eq!(
        toc.x_fields.get("X-Curse-Project-ID").map(String::as_str),
        Some("99999")
    );
}

// ---------------------------------------------------------------------------
// Fixture: flavor-specific.toc
// ---------------------------------------------------------------------------

#[test]
fn fixture_flavor_specific_no_generic_interface() {
    let toc = parse_str(FIXTURE_FLAVOR_SPECIFIC);
    assert!(toc.interface.is_empty());
}

#[test]
fn fixture_flavor_specific_all_flavor_keys_present() {
    let toc = parse_str(FIXTURE_FLAVOR_SPECIFIC);
    for flavor in ["Mainline", "Vanilla", "TBC", "Wrath", "Cata", "Mists"] {
        assert!(
            toc.interface_flavor.contains_key(flavor),
            "missing flavor key: {flavor}"
        );
    }
}

// ---------------------------------------------------------------------------
// Fixture: curseforge.toc
// ---------------------------------------------------------------------------

#[test]
fn fixture_curseforge_provider_ids() {
    let toc = parse_str(FIXTURE_CURSEFORGE);
    assert_eq!(
        toc.x_fields.get("X-Curse-Project-ID").map(String::as_str),
        Some("274066")
    );
    assert_eq!(
        toc.x_fields.get("X-Wago-ID").map(String::as_str),
        Some("nQN5aoNB")
    );
}

#[test]
fn fixture_curseforge_optional_deps() {
    let toc = parse_str(FIXTURE_CURSEFORGE);
    assert_eq!(
        toc.optional_deps,
        vec!["Ace3", "LibStub", "LibSharedMedia-3.0"]
    );
}

#[test]
fn fixture_curseforge_saved_variables() {
    let toc = parse_str(FIXTURE_CURSEFORGE);
    assert_eq!(toc.saved_variables, vec!["CurseForgeAddonDB"]);
}

#[test]
fn fixture_curseforge_icon_texture() {
    let toc = parse_str(FIXTURE_CURSEFORGE);
    assert!(toc.icon_texture.is_some());
}

#[test]
fn fixture_curseforge_category() {
    let toc = parse_str(FIXTURE_CURSEFORGE);
    assert_eq!(toc.category.as_deref(), Some("Auction & Economy"));
}

// ---------------------------------------------------------------------------
// Fixture: colorful.toc
// ---------------------------------------------------------------------------

#[test]
fn fixture_colorful_title_stripped() {
    let toc = parse_str(FIXTURE_COLORFUL);
    assert_eq!(toc.title.as_deref(), Some("<DBM> Core"));
}

#[test]
fn fixture_colorful_title_raw_has_codes() {
    let toc = parse_str(FIXTURE_COLORFUL);
    let raw = toc.title_raw.as_deref().unwrap_or("");
    assert!(raw.contains("|c"), "title_raw should retain color codes");
}

#[test]
fn fixture_colorful_locale_titles() {
    let toc = parse_str(FIXTURE_COLORFUL);
    assert!(toc.title_locale.contains_key("dede"));
    assert!(toc.title_locale.contains_key("frfr"));
    assert!(toc.title_locale.contains_key("zhcn"));
}

#[test]
fn fixture_colorful_locale_title_stripped() {
    let toc = parse_str(FIXTURE_COLORFUL);
    let de = toc
        .title_locale
        .get("dede")
        .map(String::as_str)
        .unwrap_or("");
    assert!(
        !de.contains("|c"),
        "localized title should have color codes stripped"
    );
}

#[test]
fn fixture_colorful_requireddeps() {
    let toc = parse_str(FIXTURE_COLORFUL);
    assert_eq!(toc.dependencies, vec!["SomeBaseAddon"]);
}

#[test]
fn fixture_colorful_saved_vars_per_character() {
    let toc = parse_str(FIXTURE_COLORFUL);
    assert_eq!(toc.saved_variables_per_character, vec!["DBMStatsDB"]);
}

// ---------------------------------------------------------------------------
// Fixture: dependencies.toc
// ---------------------------------------------------------------------------

#[test]
fn fixture_deps_load_on_demand() {
    let toc = parse_str(FIXTURE_DEPS);
    assert!(toc.load_on_demand);
}

#[test]
fn fixture_deps_dependencies_list() {
    let toc = parse_str(FIXTURE_DEPS);
    assert_eq!(toc.dependencies, vec!["CoreAddon", "SharedLib"]);
}

#[test]
fn fixture_deps_load_with() {
    let toc = parse_str(FIXTURE_DEPS);
    assert_eq!(toc.load_with, vec!["SomeOtherAddon"]);
}

#[test]
fn fixture_deps_load_managers() {
    let toc = parse_str(FIXTURE_DEPS);
    assert_eq!(toc.load_managers, vec!["AddonLoader"]);
}

#[test]
fn fixture_deps_allow_load_game_type() {
    let toc = parse_str(FIXTURE_DEPS);
    assert_eq!(toc.allow_load_game_type, vec!["mainline", "classic"]);
}

#[test]
fn fixture_deps_group() {
    let toc = parse_str(FIXTURE_DEPS);
    assert_eq!(toc.group.as_deref(), Some("MyAddonSuite"));
}

#[test]
fn fixture_deps_default_state() {
    let toc = parse_str(FIXTURE_DEPS);
    assert_eq!(toc.default_state.as_deref(), Some("enabled"));
}

#[test]
fn fixture_deps_saved_vars_per_char() {
    let toc = parse_str(FIXTURE_DEPS);
    assert_eq!(toc.saved_variables_per_character, vec!["DepCharDB"]);
}
