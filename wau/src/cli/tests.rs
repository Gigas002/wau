use super::*;

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["wau"];
    full.extend(args);
    Cli::try_parse_from(full).unwrap()
}

#[test]
fn global_flags_parse() {
    let cli = parse(&["-p", "retail", "stats"]);
    assert_eq!(cli.profile, "retail");
}

#[test]
fn default_profile_is_default_marker() {
    let cli = parse(&["stats"]);
    assert_eq!(cli.profile, "default");
    assert_eq!(cli.config, None);
}

#[test]
fn config_flag_parses_as_a_path() {
    let cli = parse(&["--config", "/tmp/fixture/config.toml", "stats"]);
    assert_eq!(
        cli.config,
        Some(std::path::PathBuf::from("/tmp/fixture/config.toml"))
    );
}

#[test]
fn profile_flag_accepts_a_path_value() {
    let cli = parse(&["-p", "/tmp/fixture/profile.toml", "stats"]);
    assert_eq!(cli.profile, "/tmp/fixture/profile.toml");
}

#[test]
fn install_requires_at_least_one_addon() {
    assert!(Cli::try_parse_from(["wau", "install"]).is_err());
    let cli = parse(&["install", "curse:foo", "--replace", "--dry-run"]);
    match cli.command {
        Command::Install(args) => {
            assert_eq!(args.addons, vec!["curse:foo"]);
            assert!(args.replace);
            assert!(args.dry_run);
        }
        _ => panic!("wrong command"),
    }
}

#[test]
fn sync_addons_are_optional() {
    let cli = parse(&["sync"]);
    match cli.command {
        Command::Sync(args) => assert!(args.addons.is_empty()),
        _ => panic!("wrong command"),
    }
}

#[test]
fn remove_requires_at_least_one_addon() {
    assert!(Cli::try_parse_from(["wau", "remove"]).is_err());
}

#[test]
fn replace_takes_exactly_an_old_and_a_new_addon() {
    let cli = parse(&["replace", "curse:foo", "github:someuser/foo"]);
    match cli.command {
        Command::Replace(args) => {
            assert_eq!(args.old, "curse:foo");
            assert_eq!(args.new, "github:someuser/foo");
        }
        _ => panic!("wrong command"),
    }

    assert!(Cli::try_parse_from(["wau", "replace"]).is_err());
    assert!(Cli::try_parse_from(["wau", "replace", "curse:foo"]).is_err());
    assert!(Cli::try_parse_from(["wau", "replace", "curse:foo", "github:foo", "extra"]).is_err());
}

#[test]
fn search_limit_is_bounded_between_1_and_20() {
    assert!(Cli::try_parse_from(["wau", "search", "foo", "--limit", "0"]).is_err());
    assert!(Cli::try_parse_from(["wau", "search", "foo", "--limit", "21"]).is_err());
    let cli = parse(&["search", "foo", "--limit", "5"]);
    match cli.command {
        Command::Search(args) => assert_eq!(args.limit, 5),
        _ => panic!("wrong command"),
    }
}

#[test]
fn search_default_limit_is_ten() {
    let cli = parse(&["search", "foo"]);
    match cli.command {
        Command::Search(args) => assert_eq!(args.limit, 10),
        _ => panic!("wrong command"),
    }
}

#[test]
fn search_source_flag_is_repeatable() {
    let cli = parse(&["search", "foo", "--source", "curse", "--source", "wowi"]);
    match cli.command {
        Command::Search(args) => assert_eq!(args.sources, vec!["curse", "wowi"]),
        _ => panic!("wrong command"),
    }
}

#[test]
fn list_format_defaults_to_simple() {
    let cli = parse(&["list"]);
    match cli.command {
        Command::List(args) => assert_eq!(args.format, ListFormat::Simple),
        _ => panic!("wrong command"),
    }
}

#[test]
fn list_format_accepts_detailed_and_json() {
    let cli = parse(&["list", "-f", "json"]);
    match cli.command {
        Command::List(args) => assert_eq!(args.format, ListFormat::Json),
        _ => panic!("wrong command"),
    }
    assert!(Cli::try_parse_from(["wau", "list", "-f", "bogus"]).is_err());
}

#[test]
fn profile_erase_subcommand_parses() {
    let cli = parse(&["profile", "erase"]);
    assert!(matches!(
        cli.command,
        Command::Profile(ProfileCommand::Erase)
    ));
}

#[test]
fn init_flags_parse() {
    let cli = parse(&["init", "-a", "--list-unreconciled"]);
    match cli.command {
        Command::Init(args) => {
            assert!(args.auto);
            assert!(args.list_unreconciled);
        }
        _ => panic!("wrong command"),
    }
}

#[test]
fn help_subcommand_is_disabled() {
    assert!(Cli::try_parse_from(["wau", "help"]).is_err());
    assert!(Cli::try_parse_from(["wau", "help", "install"]).is_err());
}
