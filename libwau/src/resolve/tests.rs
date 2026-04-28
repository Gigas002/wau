use chrono::Utc;

use crate::{
    lock::{Lock, LockedAddon},
    manifest::{Manifest, ManifestAddon},
    model::{Channel, Flavor, Provider, Tag},
    resolve::plan,
};

fn empty_manifest() -> Manifest {
    Manifest {
        schema: 1,
        addon: vec![],
    }
}

fn manifest_with(addons: Vec<ManifestAddon>) -> Manifest {
    Manifest {
        schema: 1,
        addon: addons,
    }
}

fn empty_lock() -> Lock {
    Lock::new(Tag::new("test"))
}

fn addon(name: &str, flavors: Option<Vec<Flavor>>) -> ManifestAddon {
    ManifestAddon {
        name: name.into(),
        provider: Provider::Local,
        channel: None,
        flavors,
        pin: None,
        project_id: None,
        wowi_id: None,
        repo: None,
        asset_regex: None,
        git_ref: None,
        url: None,
    }
}

fn locked(name: &str, flavor: Flavor) -> LockedAddon {
    LockedAddon {
        name: name.into(),
        provider: Provider::Local,
        flavor,
        channel: Channel::Stable,
        project_id: None,
        resolved_version: "1.0".into(),
        resolved_id: "1".into(),
        download_url: "file:///tmp/addon.zip".into(),
        sha256: None,
        installed_dirs: vec![],
        installed_at: Utc::now(),
    }
}

#[test]
fn plan_empty_manifest_returns_empty() {
    let m = empty_manifest();
    let result = plan(&m, &empty_lock(), &Flavor::Retail, false);
    assert!(result.to_install.is_empty());
    assert_eq!(result.skipped, 0);
}

#[test]
fn plan_includes_addon_when_no_flavors_field() {
    let m = manifest_with(vec![addon("WeakAuras", None)]);
    let result = plan(&m, &empty_lock(), &Flavor::Retail, false);
    assert_eq!(result.to_install.len(), 1);
    assert_eq!(result.to_install[0].name, "WeakAuras");
}

#[test]
fn plan_includes_addon_when_flavor_matches() {
    let m = manifest_with(vec![addon("WeakAuras", Some(vec![Flavor::Retail]))]);
    let result = plan(&m, &empty_lock(), &Flavor::Retail, false);
    assert_eq!(result.to_install.len(), 1);
}

#[test]
fn plan_excludes_addon_when_flavor_does_not_match() {
    let m = manifest_with(vec![addon("ClassicAddon", Some(vec![Flavor::Era]))]);
    let result = plan(&m, &empty_lock(), &Flavor::Retail, false);
    assert!(result.to_install.is_empty());
    assert_eq!(result.skipped, 0);
}

#[test]
fn plan_skips_already_locked_addon_without_update() {
    let m = manifest_with(vec![addon("WeakAuras", None)]);
    let mut lock = empty_lock();
    lock.addon.push(locked("WeakAuras", Flavor::Retail));

    let result = plan(&m, &lock, &Flavor::Retail, false);
    assert!(result.to_install.is_empty());
    assert_eq!(result.skipped, 1);
}

#[test]
fn plan_includes_already_locked_addon_when_update() {
    let m = manifest_with(vec![addon("WeakAuras", None)]);
    let mut lock = empty_lock();
    lock.addon.push(locked("WeakAuras", Flavor::Retail));

    let result = plan(&m, &lock, &Flavor::Retail, true);
    assert_eq!(result.to_install.len(), 1);
    assert_eq!(result.skipped, 0);
}

#[test]
fn plan_lock_match_is_flavor_scoped() {
    // Addon is locked for Era but we're syncing Retail — should not be skipped.
    let m = manifest_with(vec![addon("WeakAuras", None)]);
    let mut lock = empty_lock();
    lock.addon.push(locked("WeakAuras", Flavor::Era));

    let result = plan(&m, &lock, &Flavor::Retail, false);
    assert_eq!(result.to_install.len(), 1);
    assert_eq!(result.skipped, 0);
}

#[test]
fn plan_mixed_scenario_counts_correctly() {
    let m = manifest_with(vec![
        addon("A", None),                                    // new → install
        addon("B", None),                                    // already locked → skip
        addon("C", Some(vec![Flavor::Era])),                 // wrong flavor → ignored
        addon("D", Some(vec![Flavor::Retail, Flavor::Era])), // multi-flavor → install
    ]);
    let mut lock = empty_lock();
    lock.addon.push(locked("B", Flavor::Retail));

    let result = plan(&m, &lock, &Flavor::Retail, false);
    assert_eq!(result.to_install.len(), 2);
    assert_eq!(result.to_install[0].name, "A");
    assert_eq!(result.to_install[1].name, "D");
    assert_eq!(result.skipped, 1);
}

#[test]
fn plan_preserves_manifest_order() {
    let names = ["Z", "A", "M", "B"];
    let addons = names.iter().map(|n| addon(n, None)).collect();
    let m = manifest_with(addons);

    let result = plan(&m, &empty_lock(), &Flavor::Retail, false);
    let got: Vec<&str> = result.to_install.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(got, names);
}
