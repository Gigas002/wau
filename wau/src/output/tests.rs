use chrono::{TimeZone, Utc};
use libwau::{
    db::{Pkg, PkgDep, PkgFolder, PkgOptions},
    results::{InternalError, ManagerError},
};

use super::*;

fn sample_pkg(source: &str, slug: &str) -> Pkg {
    Pkg {
        source: source.to_owned(),
        id: "123".to_owned(),
        slug: slug.to_owned(),
        name: "Foo".to_owned(),
        description: "A test addon".to_owned(),
        url: "https://example.com/foo".to_owned(),
        download_url: "https://example.com/foo.zip".to_owned(),
        date_published: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        version: "1.0.0".to_owned(),
        changelog_url: "https://example.com/foo/changelog".to_owned(),
        options: PkgOptions {
            any_flavour: false,
            any_release_type: false,
            version_eq: false,
        },
        folders: vec![PkgFolder {
            name: "Foo".to_owned(),
        }],
        deps: vec![PkgDep {
            id: "456".to_owned(),
        }],
    }
}

#[test]
fn symbol_for_maps_ok_manager_and_internal_distinctly() {
    let pkg = sample_pkg("curse", "foo");
    let ok: AnyOutcome<Outcome> = Ok(Outcome::PkgInstalled {
        pkg,
        dry_run: false,
    });
    let manager_err: AnyOutcome<Outcome> = Err(ManagerError::PkgAlreadyInstalled.into());
    let internal_err: AnyOutcome<Outcome> = Err(InternalError::new("boom").into());

    assert_eq!(symbol_for(&ok), "✓");
    assert_eq!(symbol_for(&manager_err), "✗");
    assert_eq!(symbol_for(&internal_err), "!");
}

#[test]
fn format_result_includes_uri_and_indented_detail() {
    let defn = Defn::new("curse", "foo");
    let outcome: AnyOutcome<Outcome> = Err(ManagerError::PkgAlreadyInstalled.into());
    let rendered = format_result(&defn, &outcome);
    assert_eq!(rendered, "✗ curse:foo\n  package already installed");
}

#[test]
fn format_results_sorts_by_uri() {
    let mut results = HashMap::new();
    results.insert(
        Defn::new("curse", "zeta"),
        Ok(Outcome::PkgRemoved {
            pkg: sample_pkg("curse", "zeta"),
        }),
    );
    results.insert(
        Defn::new("curse", "alpha"),
        Ok(Outcome::PkgRemoved {
            pkg: sample_pkg("curse", "alpha"),
        }),
    );

    let rendered = format_results(&results);
    let alpha_pos = rendered.find("alpha").unwrap();
    let zeta_pos = rendered.find("zeta").unwrap();
    assert!(alpha_pos < zeta_pos);
}

#[test]
fn any_errors_detects_at_least_one_failure() {
    let mut results = HashMap::new();
    results.insert(
        Defn::new("curse", "foo"),
        Ok(Outcome::PkgRemoved {
            pkg: sample_pkg("curse", "foo"),
        }),
    );
    assert!(!any_errors(&results));

    results.insert(
        Defn::new("curse", "bar"),
        Err(ManagerError::PkgNotInstalled.into()),
    );
    assert!(any_errors(&results));
}

#[test]
fn format_list_simple_renders_bare_source_slug_uris() {
    let pkgs = [sample_pkg("curse", "foo"), sample_pkg("github", "bar")];
    let refs: Vec<&Pkg> = pkgs.iter().collect();
    assert_eq!(format_list_simple(&refs), "curse:foo\ngithub:bar");
}

#[test]
fn format_list_detailed_includes_key_fields() {
    let pkg = sample_pkg("curse", "foo");
    let rendered = format_list_detailed(&[&pkg]);
    assert!(rendered.contains("name: Foo"));
    assert!(rendered.contains("description: A test addon"));
    assert!(rendered.contains("folders: Foo"));
    assert!(rendered.contains("dependencies: curse:456"));
    assert!(rendered.contains("version: 1.0.0"));
}

#[test]
fn format_list_json_round_trips_through_serde_json() {
    let pkg = sample_pkg("curse", "foo");
    let rendered = format_list_json(&[&pkg]);
    let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed[0]["source"], "curse");
    assert_eq!(parsed[0]["slug"], "foo");
    assert_eq!(parsed[0]["folders"][0], "Foo");
}

#[test]
fn pkg_to_json_includes_options() {
    let pkg = sample_pkg("curse", "foo");
    let json = pkg_to_json(&pkg);
    assert_eq!(json["options"]["any_flavour"], false);
    assert_eq!(json["options"]["version_eq"], false);
}
