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
/// into the checked-in fixtures (installs, DB files, cache, `profile erase`
/// all mutate in place).
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
fn stats_reports_the_expected_flavour_and_addon_dir_per_environment() {
    let cases = [
        ("retail", "mainline", "_retail_/Interface/AddOns"),
        (
            "classic-era",
            "vanilla_classic",
            "_classic_era_/Interface/AddOns",
        ),
        (
            "classic-mists",
            "mists_classic",
            "_classic_/Interface/AddOns",
        ),
        (
            "custom-server",
            "mainline",
            "MyPrivateServer/Interface/AddOns",
        ),
    ];

    for (env, flavour, addon_dir) in cases {
        let tmp = env_copy(env);
        let output = wau_configured(&tmp, &["stats"]);
        assert!(
            output.status.success(),
            "{env}: wau stats failed: {}",
            stderr(&output)
        );

        let json: serde_json::Value = serde_json::from_str(&stdout(&output))
            .unwrap_or_else(|e| panic!("{env}: stats output wasn't JSON: {e}"));
        assert_eq!(
            json["active_profile_config"]["flavour"], flavour,
            "{env}: unexpected flavour"
        );
        assert_eq!(
            json["active_profile_config"]["addon_dir"], addon_dir,
            "{env}: unexpected addon_dir"
        );
    }
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
fn init_with_an_empty_addon_dir_is_a_no_op() {
    let tmp = env_copy("retail");
    let output = wau_configured(&tmp, &["init", "--auto"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("No add-ons left to reconcile."));
}

#[test]
#[ignore = "spawns the real wau binary; run locally with `cargo test -p wau --test cli -- --ignored`"]
fn profile_erase_then_list_reports_the_profile_as_not_configured() {
    let tmp = env_copy("retail");

    let erase = wau_configured(&tmp, &["profile", "erase"]);
    assert!(erase.status.success(), "{}", stderr(&erase));
    assert!(!tmp.path().join("profile.toml").exists());

    let list = wau_configured(&tmp, &["list"]);
    assert!(!list.status.success());
    assert!(stderr(&list).contains("isn't configured"));
    assert!(stderr(&list).contains("wau init"));
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
    assert!(stderr(&output).contains("wau init"));
}
