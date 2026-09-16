//! CLI integration tests: spawn the real `wau` binary against a fresh copy
//! of one of the fixture environments in `testing/` (see
//! `testing/README.md`), exercising `--config`/`--profile` end to end the
//! way a real invocation would — real argv parsing, real process working
//! directory, real filesystem — rather than calling `libwau`/`wau`'s Rust
//! functions directly the way the unit suites do.
//!
//! Every test only touches its own temp-dir copy of a fixture: never a real
//! WoW installation, never the developer's `~/.config/wau`. None of them
//! need network access (empty add-on dirs, no installed packages), so they
//! stay deterministic without any HTTP mocking.
//!
//! Deliberately excluded from CI (`#[ignore]` on every test here — CI's
//! `cargo test --all-features` doesn't pass `--include-ignored`, so these
//! never run there) since spawning the compiled binary and shelling out to
//! the filesystem is slower and more environment-sensitive than the unit
//! suites CI already runs. Run them locally with:
//!
//! ```sh
//! cargo test -p wau --test cli -- --ignored
//! ```

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use tempfile::TempDir;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("wau/ has a workspace-root parent")
        .to_path_buf()
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dest_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path);
        } else {
            fs::copy(entry.path(), &dest_path).unwrap();
        }
    }
}

/// Copies `testing/<name>` into a fresh temp dir so a test run never writes
/// into the checked-in fixtures (installs, DB files, cache all mutate in
/// place).
fn env_copy(name: &str) -> TempDir {
    let src = workspace_root().join("testing").join(name);
    let tmp = tempfile::tempdir().unwrap();
    copy_dir_recursive(&src, tmp.path());
    tmp
}

/// Runs the compiled `wau` binary with `tmp` as its working directory —
/// `testing/`'s fixtures are wired with paths relative to their own
/// directory (see `testing/README.md`), so this is what makes them resolve
/// correctly.
fn wau(tmp: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wau"))
        .current_dir(tmp.path())
        .args(args)
        .output()
        .expect("failed to run the wau binary")
}

/// [`wau`], with `--config config.toml --profile profile.toml` prepended —
/// every configured `testing/` environment names its files exactly that.
fn wau_configured(tmp: &TempDir, args: &[&str]) -> Output {
    let mut full = vec!["--config", "config.toml", "--profile", "profile.toml"];
    full.extend_from_slice(args);
    wau(tmp, &full)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn list_on_a_fresh_profile_prints_nothing_and_succeeds() {
    let tmp = env_copy("retail");
    let output = wau_configured(&tmp, &["list"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).trim().is_empty());
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn sync_with_no_installed_addons_succeeds_without_network() {
    let tmp = env_copy("retail");
    let output = wau_configured(&tmp, &["sync"]);

    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn remove_of_a_never_installed_addon_reports_not_installed() {
    let tmp = env_copy("retail");
    let output = wau_configured(&tmp, &["remove", "curse:nonexistent-addon"]);

    assert!(!output.status.success());
    assert!(stdout(&output).contains("curse:nonexistent-addon"));
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn replace_of_a_never_installed_addon_reports_errors_for_both_sides() {
    let tmp = env_copy("retail");
    // CurseForge has no API key in this fixture's config.toml, so it's
    // disabled — resolving `new` fails fast on that check, before any
    // network call, keeping this test deterministic without HTTP mocking.
    let output = wau_configured(&tmp, &["replace", "curse:old-addon", "curse:new-addon"]);

    assert!(!output.status.success());
    let text = stdout(&output);
    assert!(text.contains("curse:old-addon"));
    assert!(text.contains("not installed"));
    assert!(text.contains("curse:new-addon"));
    assert!(text.contains("disabled"));
}

/// `init` ignores `-p`/`--profile` and instead scans `<config-dir>/profiles/*`
/// for name-based profiles, so this promotes a fixture's flat `profile.toml`
/// (normally read via `--profile profile.toml`'s direct-path override) into
/// that layout. The addon dir path inside it is relative to the process's
/// working directory, not to the TOML file, so moving the file doesn't break it.
fn promote_to_named_profile(tmp: &TempDir, name: &str) {
    let dir = tmp.path().join("profiles").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::rename(tmp.path().join("profile.toml"), dir.join("profile.toml")).unwrap();
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn init_with_an_empty_addon_dir_is_a_no_op() {
    let tmp = env_copy("retail");
    promote_to_named_profile(&tmp, "retail");

    let output = wau(&tmp, &["--config", "config.toml", "init"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("No add-ons left to reconcile."));
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn init_list_unreconciled_reports_untracked_folders_without_network() {
    let tmp = env_copy("retail");
    promote_to_named_profile(&tmp, "retail");
    let addon_dir = tmp
        .path()
        .join("_retail_")
        .join("Interface")
        .join("AddOns")
        .join("SomeHandInstalledAddon");
    fs::create_dir_all(&addon_dir).unwrap();
    fs::write(
        addon_dir.join("SomeHandInstalledAddon.toc"),
        "## Version: 1",
    )
    .unwrap();

    let output = wau(
        &tmp,
        &["--config", "config.toml", "init", "--list-unreconciled"],
    );

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("unreconciled:"));
    assert!(stdout(&output).contains("SomeHandInstalledAddon"));
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn init_processes_every_configured_profile_in_one_run() {
    let tmp = env_copy("retail");
    promote_to_named_profile(&tmp, "retail");
    copy_dir_recursive(
        &workspace_root()
            .join("testing")
            .join("classic-era")
            .join("_classic_era_"),
        &tmp.path().join("_classic_era_"),
    );
    fs::create_dir_all(tmp.path().join("profiles").join("classic-era")).unwrap();
    fs::copy(
        workspace_root()
            .join("testing")
            .join("classic-era")
            .join("profile.toml"),
        tmp.path()
            .join("profiles")
            .join("classic-era")
            .join("profile.toml"),
    )
    .unwrap();

    let output = wau(&tmp, &["--config", "config.toml", "init"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("== retail =="));
    assert!(text.contains("== classic-era =="));
    assert_eq!(text.matches("No add-ons left to reconcile.").count(), 2);
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn init_with_explicit_profile_flag_only_reconciles_that_one() {
    let tmp = env_copy("retail");
    promote_to_named_profile(&tmp, "retail");
    copy_dir_recursive(
        &workspace_root()
            .join("testing")
            .join("classic-era")
            .join("_classic_era_"),
        &tmp.path().join("_classic_era_"),
    );
    fs::create_dir_all(tmp.path().join("profiles").join("classic-era")).unwrap();
    fs::copy(
        workspace_root()
            .join("testing")
            .join("classic-era")
            .join("profile.toml"),
        tmp.path()
            .join("profiles")
            .join("classic-era")
            .join("profile.toml"),
    )
    .unwrap();

    let output = wau(&tmp, &["--config", "config.toml", "-p", "retail", "init"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    // Single explicit target: no `== name ==` headers, and only the one
    // profile's "nothing to do" line — classic-era is never touched.
    assert!(!text.contains("=="));
    assert_eq!(text.matches("No add-ons left to reconcile.").count(), 1);
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn no_profile_flag_auto_picks_the_sole_configured_profile() {
    // Regression test: `-p` used to default to a literal "default", so any
    // command without an explicit `-p` failed outright unless a profile was
    // literally named that. There's no implicit default name anymore — with
    // exactly one profile configured, omitting `-p` should just use it.
    let tmp = env_copy("retail");
    promote_to_named_profile(&tmp, "retail");

    let output = wau(&tmp, &["--config", "config.toml", "list"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).trim().is_empty());
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn no_profile_flag_with_zero_profiles_configured_errors_clearly() {
    let tmp = env_copy("retail");
    // No `promote_to_named_profile` call: `profiles/` never gets created, so
    // there is nothing to auto-pick — deliberately not testing against the
    // flat `--profile profile.toml`-override fixture layout, which `-p`-less
    // resolution never looks at.

    let output = wau(&tmp, &["--config", "config.toml", "list"]);

    assert!(!output.status.success());
    assert!(stderr(&output).contains("no profiles configured"));
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn no_profile_flag_with_multiple_profiles_configured_errors_listing_them() {
    let tmp = env_copy("retail");
    promote_to_named_profile(&tmp, "retail");
    copy_dir_recursive(
        &workspace_root()
            .join("testing")
            .join("classic-era")
            .join("_classic_era_"),
        &tmp.path().join("_classic_era_"),
    );
    fs::create_dir_all(tmp.path().join("profiles").join("classic-era")).unwrap();
    fs::copy(
        workspace_root()
            .join("testing")
            .join("classic-era")
            .join("profile.toml"),
        tmp.path()
            .join("profiles")
            .join("classic-era")
            .join("profile.toml"),
    )
    .unwrap();

    let output = wau(&tmp, &["--config", "config.toml", "list"]);

    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("multiple profiles configured"));
    assert!(err.contains("retail"));
    assert!(err.contains("classic-era"));
    assert!(err.contains("-p/--profile"));
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn unconfigured_environment_errors_with_a_helpful_message() {
    let tmp = env_copy("unconfigured");

    let output = wau(
        &tmp,
        &[
            "--config",
            "config.toml",
            "--profile",
            "profile.toml",
            "list",
        ],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("isn't configured"));
    assert!(stderr(&output).contains("examples/config.toml"));
}
