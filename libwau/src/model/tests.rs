use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct W<T> {
    v: T,
}

fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(value: T) {
    let wrapped = W { v: value };
    let s = toml::to_string(&wrapped).unwrap();
    let parsed: W<T> = toml::from_str(&s).unwrap();
    assert_eq!(wrapped, parsed);
}

#[test]
fn flavor_serde_all_variants() {
    let cases = [
        (Flavor::Retail, "retail"),
        (Flavor::Era, "classic-era"),
        (Flavor::Tbc, "classic-tbc"),
        (Flavor::Wrath, "classic-wrath"),
        (Flavor::Cata, "classic-cata"),
        (Flavor::Mop, "classic-mop"),
        (Flavor::Wod, "classic-wod"),
        (Flavor::Legion, "classic-legion"),
        (Flavor::Bfa, "classic-bfa"),
        (Flavor::Shadowlands, "classic-shadowlands"),
        (Flavor::Dragonflight, "classic-dragonflight"),
        (Flavor::Tww, "classic-tww"),
    ];
    for (variant, slug) in cases {
        // as_str matches the slug
        assert_eq!(variant.as_str(), slug, "as_str mismatch for {slug}");
        // serde round-trip
        round_trip(variant.clone());
        // deserialization from canonical slug
        let toml_str = format!(r#"v = "{slug}""#);
        assert_eq!(
            W { v: variant },
            toml::from_str(&toml_str).unwrap(),
            "deserialization failed for '{slug}'"
        );
    }
}

#[test]
fn channel_serde_round_trip() {
    round_trip(Channel::Stable);
    round_trip(Channel::Beta);
    round_trip(Channel::Alpha);
}

#[test]
fn channel_serde_names() {
    assert_eq!(
        W { v: Channel::Stable },
        toml::from_str(r#"v = "stable""#).unwrap()
    );
    assert_eq!(
        W { v: Channel::Beta },
        toml::from_str(r#"v = "beta""#).unwrap()
    );
    assert_eq!(
        W { v: Channel::Alpha },
        toml::from_str(r#"v = "alpha""#).unwrap()
    );
}

#[test]
fn provider_serde_round_trip() {
    round_trip(Provider::CurseForge);
    round_trip(Provider::WoWInterface);
    round_trip(Provider::GitHub);
    round_trip(Provider::Local);
}

#[test]
fn provider_serde_names() {
    assert_eq!(
        W {
            v: Provider::CurseForge
        },
        toml::from_str(r#"v = "curseforge""#).unwrap()
    );
    assert_eq!(
        W {
            v: Provider::WoWInterface
        },
        toml::from_str(r#"v = "wowinterface""#).unwrap()
    );
    assert_eq!(
        W {
            v: Provider::GitHub
        },
        toml::from_str(r#"v = "github""#).unwrap()
    );
    assert_eq!(
        W { v: Provider::Local },
        toml::from_str(r#"v = "local""#).unwrap()
    );
}

#[test]
fn tag_serde_round_trip() {
    round_trip(Tag::new("retail-main"));
    round_trip(Tag::new("classic-turtle"));
}

#[test]
fn log_level_serde_all_variants() {
    for (variant, expected_str) in [
        (LogLevel::Trace, "trace"),
        (LogLevel::Debug, "debug"),
        (LogLevel::Info, "info"),
        (LogLevel::Warn, "warn"),
        (LogLevel::Error, "error"),
    ] {
        let w = W { v: variant.clone() };
        let s = toml::to_string(&w).unwrap();
        assert!(
            s.contains(expected_str),
            "expected '{expected_str}' in '{s}'"
        );
        let parsed: W<LogLevel> = toml::from_str(&s).unwrap();
        assert_eq!(w, parsed);
    }
}

#[test]
fn log_level_default_is_info() {
    assert_eq!(LogLevel::default(), LogLevel::Info);
}

#[test]
fn log_level_as_str_all_variants() {
    assert_eq!(LogLevel::Trace.as_str(), "trace");
    assert_eq!(LogLevel::Debug.as_str(), "debug");
    assert_eq!(LogLevel::Info.as_str(), "info");
    assert_eq!(LogLevel::Warn.as_str(), "warn");
    assert_eq!(LogLevel::Error.as_str(), "error");
}

#[test]
fn flavor_from_str_all_variants() {
    let cases = [
        ("retail", Flavor::Retail),
        ("classic-era", Flavor::Era),
        ("classic-tbc", Flavor::Tbc),
        ("classic-wrath", Flavor::Wrath),
        ("classic-cata", Flavor::Cata),
        ("classic-mop", Flavor::Mop),
        ("classic-wod", Flavor::Wod),
        ("classic-legion", Flavor::Legion),
        ("classic-bfa", Flavor::Bfa),
        ("classic-shadowlands", Flavor::Shadowlands),
        ("classic-dragonflight", Flavor::Dragonflight),
        ("classic-tww", Flavor::Tww),
    ];
    for (s, expected) in cases {
        assert_eq!(
            s.parse::<Flavor>().unwrap(),
            expected,
            "parse failed for '{s}'"
        );
    }
}

#[test]
fn flavor_from_str_roundtrip() {
    let variants = [
        Flavor::Retail,
        Flavor::Era,
        Flavor::Tbc,
        Flavor::Wrath,
        Flavor::Cata,
        Flavor::Mop,
        Flavor::Wod,
        Flavor::Legion,
        Flavor::Bfa,
        Flavor::Shadowlands,
        Flavor::Dragonflight,
        Flavor::Tww,
    ];
    for v in variants {
        assert_eq!(v.as_str().parse::<Flavor>().unwrap(), v);
    }
}

#[test]
fn flavor_from_str_unknown_errors() {
    assert!("bogus".parse::<Flavor>().is_err());
    assert!("Retail".parse::<Flavor>().is_err()); // case-sensitive
}

#[test]
fn channel_from_str_all_variants() {
    assert_eq!("stable".parse::<Channel>().unwrap(), Channel::Stable);
    assert_eq!("beta".parse::<Channel>().unwrap(), Channel::Beta);
    assert_eq!("alpha".parse::<Channel>().unwrap(), Channel::Alpha);
}

#[test]
fn channel_from_str_unknown_errors() {
    assert!("nightly".parse::<Channel>().is_err());
    assert!("Stable".parse::<Channel>().is_err());
}

#[test]
fn display_impls() {
    assert_eq!(Flavor::Retail.to_string(), "retail");
    assert_eq!(Flavor::Era.to_string(), "classic-era");
    assert_eq!(Flavor::Tbc.to_string(), "classic-tbc");
    assert_eq!(Flavor::Wrath.to_string(), "classic-wrath");
    assert_eq!(Flavor::Cata.to_string(), "classic-cata");
    assert_eq!(Flavor::Mop.to_string(), "classic-mop");
    assert_eq!(Flavor::Wod.to_string(), "classic-wod");
    assert_eq!(Flavor::Legion.to_string(), "classic-legion");
    assert_eq!(Flavor::Bfa.to_string(), "classic-bfa");
    assert_eq!(Flavor::Shadowlands.to_string(), "classic-shadowlands");
    assert_eq!(Flavor::Dragonflight.to_string(), "classic-dragonflight");
    assert_eq!(Flavor::Tww.to_string(), "classic-tww");
    assert_eq!(Channel::Beta.to_string(), "beta");
    assert_eq!(Provider::GitHub.to_string(), "github");
    assert_eq!(Provider::WoWInterface.to_string(), "wowinterface");
    assert_eq!(Tag::new("turtle-wow").to_string(), "turtle-wow");
    assert_eq!(LogLevel::Warn.to_string(), "warn");
}
