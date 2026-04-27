use std::path::{Path, PathBuf};

use libwau::{
    lock, manifest,
    model::{Channel, Flavor, Provider},
};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples")
}

// ── manifest.toml ────────────────────────────────────────────────────────────

#[test]
fn example_manifest_parses() {
    manifest::load(&examples_dir().join("manifest.toml")).unwrap();
}

#[test]
fn example_manifest_schema_version() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    assert_eq!(m.schema, manifest::SUPPORTED_SCHEMA);
}

#[test]
fn example_manifest_addon_count() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    // Examples file has 12 [[addon]] entries (as of initial spec).
    assert!(
        m.addon.len() >= 8,
        "expected at least 8 addons in examples/manifest.toml, got {}",
        m.addon.len()
    );
}

#[test]
fn example_manifest_curseforge_addons() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    let cf: Vec<_> = m
        .addon
        .iter()
        .filter(|a| a.provider == Provider::CurseForge)
        .collect();
    assert!(!cf.is_empty(), "expected at least one CurseForge addon");
    for addon in &cf {
        assert!(
            addon.project_id.is_some(),
            "CurseForge addon '{}' missing project_id",
            addon.name
        );
    }
}

#[test]
fn example_manifest_github_addons() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    let gh: Vec<_> = m
        .addon
        .iter()
        .filter(|a| a.provider == Provider::GitHub)
        .collect();
    assert!(!gh.is_empty(), "expected at least one GitHub addon");
    for addon in &gh {
        assert!(
            addon.repo.is_some(),
            "GitHub addon '{}' missing repo",
            addon.name
        );
    }
}

#[test]
fn example_manifest_flavors_are_valid_variants() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    // Parsing itself validates the variants; this test confirms every flavor
    // field round-trips through the closed enum without error.
    for addon in &m.addon {
        if let Some(flavors) = &addon.flavors {
            for f in flavors {
                // as_str round-trip: the string we get back is the canonical name
                let _ = f.as_str();
            }
        }
    }
}

#[test]
fn example_manifest_channels_are_valid_variants() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    for addon in &m.addon {
        if let Some(ch) = &addon.channel {
            let _ = ch.as_str();
        }
    }
}

#[test]
fn example_manifest_known_addon_deadlybossmods() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    let dbm = m
        .addon
        .iter()
        .find(|a| a.name == "DeadlyBossMods")
        .expect("DeadlyBossMods not found in examples/manifest.toml");
    assert_eq!(dbm.provider, Provider::CurseForge);
    assert_eq!(dbm.channel.as_ref().unwrap(), &Channel::Stable);
    assert_eq!(dbm.flavors.as_deref().unwrap(), &[Flavor::Retail]);
}

#[test]
fn example_manifest_known_addon_github() {
    let m = manifest::load(&examples_dir().join("manifest.toml")).unwrap();
    let addon = m
        .addon
        .iter()
        .find(|a| a.provider == Provider::GitHub && a.asset_regex.is_some())
        .expect("no GitHub addon with asset_regex in examples/manifest.toml");
    assert!(addon.repo.is_some());
}

// ── lock.toml ────────────────────────────────────────────────────────────────

#[test]
fn example_lock_parses() {
    lock::load(&examples_dir().join("lock.toml")).unwrap();
}

#[test]
fn example_lock_schema_version() {
    let l = lock::load(&examples_dir().join("lock.toml")).unwrap();
    assert_eq!(l.schema, lock::SUPPORTED_SCHEMA);
}

#[test]
fn example_lock_install_tag() {
    let l = lock::load(&examples_dir().join("lock.toml")).unwrap();
    assert!(!l.install_tag.as_str().is_empty());
}

#[test]
fn example_lock_addon_fields() {
    let l = lock::load(&examples_dir().join("lock.toml")).unwrap();
    assert!(
        !l.addon.is_empty(),
        "expected at least one addon in examples/lock.toml"
    );
    for addon in &l.addon {
        assert!(!addon.name.is_empty());
        assert!(!addon.resolved_version.is_empty());
        assert!(!addon.resolved_id.is_empty());
        assert!(!addon.download_url.is_empty());
        assert!(!addon.installed_dirs.is_empty());
    }
}

#[test]
fn example_lock_known_addon_bagnon() {
    let l = lock::load(&examples_dir().join("lock.toml")).unwrap();
    let bagnon = l
        .addon
        .iter()
        .find(|a| a.name == "Bagnon")
        .expect("Bagnon not found in examples/lock.toml");
    assert_eq!(bagnon.provider, Provider::CurseForge);
    assert_eq!(bagnon.flavor, Flavor::Retail);
    assert_eq!(bagnon.channel, Channel::Stable);
    assert_eq!(bagnon.installed_dirs, vec!["Bagnon", "Bagnon_Config"]);
}
