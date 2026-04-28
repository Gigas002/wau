use super::*;
use clap::Parser;
use std::path::Path;

// ── helpers ──────────────────────────────────────────────────────────────────

fn default_cli(command: Command) -> Cli {
    Cli {
        config: None,
        noconfirm: false,
        quiet: false,
        verbose: false,
        command,
    }
}

// ── list ─────────────────────────────────────────────────────────────────────

#[test]
fn list_no_args_uses_defaults() {
    let cli = Cli::try_parse_from(["wau", "list"]).unwrap();
    assert!(matches!(cli.command, Command::List(_)));
    let Command::List(args) = cli.command else {
        panic!()
    };
    assert!(args.tag.is_none());
}

#[test]
fn list_with_tag() {
    let cli = Cli::try_parse_from(["wau", "list", "--tag", "retail-main"]).unwrap();
    let Command::List(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("retail-main"));
}

#[test]
fn list_with_config_override() {
    let cli = Cli::try_parse_from(["wau", "--config", "/tmp/test.toml", "list"]).unwrap();
    assert_eq!(
        cli.config.as_deref(),
        Some(std::path::Path::new("/tmp/test.toml"))
    );
}

// ── sync ─────────────────────────────────────────────────────────────────────

#[test]
fn sync_no_args_uses_defaults() {
    let cli = Cli::try_parse_from(["wau", "sync"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert!(args.tag.is_none());
    assert!(args.manifest.is_none());
    assert!(!args.update);
    assert!(args.flavor.is_none());
    assert!(args.channel.is_none());
}

#[test]
fn sync_with_update_flag() {
    let cli = Cli::try_parse_from(["wau", "sync", "--update"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert!(args.update);
}

#[test]
fn sync_with_manifest_override() {
    let cli = Cli::try_parse_from(["wau", "sync", "--manifest", "/tmp/manifest.toml"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert_eq!(
        args.manifest.as_deref(),
        Some(std::path::Path::new("/tmp/manifest.toml"))
    );
}

#[test]
fn sync_with_tag() {
    let cli = Cli::try_parse_from(["wau", "sync", "--tag", "classic-era"]).unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("classic-era"));
}

#[test]
fn sync_with_flavor_and_channel() {
    let cli = Cli::try_parse_from([
        "wau",
        "sync",
        "--flavor",
        "classic-wrath",
        "--channel",
        "beta",
    ])
    .unwrap();
    let Command::Sync(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.flavor.as_deref(), Some("classic-wrath"));
    assert_eq!(args.channel.as_deref(), Some("beta"));
}

// ── remove ───────────────────────────────────────────────────────────────────

#[test]
fn remove_requires_addon_names() {
    assert!(Cli::try_parse_from(["wau", "remove"]).is_err());
}

#[test]
fn remove_one_addon() {
    let cli = Cli::try_parse_from(["wau", "remove", "WeakAuras"]).unwrap();
    let Command::Remove(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.addons, vec!["WeakAuras"]);
}

#[test]
fn remove_multiple_addons() {
    let cli = Cli::try_parse_from(["wau", "remove", "WeakAuras", "Bagnon", "Details"]).unwrap();
    let Command::Remove(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.addons, vec!["WeakAuras", "Bagnon", "Details"]);
}

#[test]
fn remove_with_tag() {
    let cli = Cli::try_parse_from(["wau", "remove", "--tag", "classic-era", "Questie"]).unwrap();
    let Command::Remove(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("classic-era"));
    assert_eq!(args.addons, vec!["Questie"]);
}

// ── search ───────────────────────────────────────────────────────────────────

#[test]
fn search_requires_query() {
    assert!(Cli::try_parse_from(["wau", "search"]).is_err());
}

#[test]
fn search_parses_query() {
    let cli = Cli::try_parse_from(["wau", "search", "WeakAuras"]).unwrap();
    let Command::Search(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.query, "WeakAuras");
    assert!(args.tag.is_none());
}

#[test]
fn search_with_tag() {
    let cli = Cli::try_parse_from(["wau", "search", "--tag", "retail", "aura"]).unwrap();
    let Command::Search(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.query, "aura");
    assert_eq!(args.tag.as_deref(), Some("retail"));
}

// ── info ─────────────────────────────────────────────────────────────────────

#[test]
fn info_requires_addon() {
    assert!(Cli::try_parse_from(["wau", "info"]).is_err());
}

#[test]
fn info_parses_addon() {
    let cli = Cli::try_parse_from(["wau", "info", "WeakAuras"]).unwrap();
    let Command::Info(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.addon, "WeakAuras");
    assert!(args.tag.is_none());
}

#[test]
fn info_with_tag() {
    let cli = Cli::try_parse_from(["wau", "info", "--tag", "retail", "Details"]).unwrap();
    let Command::Info(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.addon, "Details");
    assert_eq!(args.tag.as_deref(), Some("retail"));
}

// ── global flags ─────────────────────────────────────────────────────────────

#[test]
fn global_noconfirm_flag() {
    let cli = Cli::try_parse_from(["wau", "--noconfirm", "list"]).unwrap();
    assert!(cli.noconfirm);
    assert!(!cli.quiet);
    assert!(!cli.verbose);
}

#[test]
fn global_quiet_short_flag() {
    let cli = Cli::try_parse_from(["wau", "-q", "list"]).unwrap();
    assert!(cli.quiet);
}

#[test]
fn global_verbose_short_flag() {
    let cli = Cli::try_parse_from(["wau", "-v", "list"]).unwrap();
    assert!(cli.verbose);
}

#[test]
fn global_flags_after_subcommand() {
    let cli = Cli::try_parse_from(["wau", "sync", "--noconfirm"]).unwrap();
    assert!(cli.noconfirm);
}

// ── default_cli helper ───────────────────────────────────────────────────────

#[test]
fn default_cli_has_no_flags() {
    let cli = default_cli(Command::List(ListArgs { tag: None }));
    assert!(!cli.noconfirm);
    assert!(!cli.quiet);
    assert!(!cli.verbose);
}

// ── init ─────────────────────────────────────────────────────────────────────

#[test]
fn init_no_args_uses_defaults() {
    let cli = Cli::try_parse_from(["wau", "init"]).unwrap();
    assert!(matches!(cli.command, Command::Init(_)));
    let Command::Init(args) = cli.command else {
        panic!()
    };
    assert!(args.tag.is_none());
    assert!(args.manifest.is_none());
    assert!(!args.force);
}

#[test]
fn init_with_tag() {
    let cli = Cli::try_parse_from(["wau", "init", "--tag", "classic-era"]).unwrap();
    let Command::Init(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("classic-era"));
}

#[test]
fn init_with_manifest() {
    let cli = Cli::try_parse_from(["wau", "init", "--manifest", "/tmp/my.toml"]).unwrap();
    let Command::Init(args) = cli.command else {
        panic!()
    };
    assert_eq!(args.manifest.as_deref(), Some(Path::new("/tmp/my.toml")));
}

#[test]
fn init_with_force() {
    let cli = Cli::try_parse_from(["wau", "init", "--force"]).unwrap();
    let Command::Init(args) = cli.command else {
        panic!()
    };
    assert!(args.force);
}
