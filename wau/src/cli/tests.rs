use super::*;

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["wau"];
    full.extend(args);
    Cli::try_parse_from(full).unwrap()
}

#[test]
fn global_flags_parse() {
    let cli = parse(&["-vv", "--no-cache", "-p", "retail", "debug"]);
    assert_eq!(cli.verbose, 2);
    assert!(cli.no_cache);
    assert_eq!(cli.profile, "retail");
}

#[test]
fn default_profile_is_default_marker() {
    let cli = parse(&["debug"]);
    assert_eq!(cli.profile, "__default__");
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
fn update_addons_are_optional() {
    let cli = parse(&["update"]);
    match cli.command {
        Command::Update(args) => assert!(args.addons.is_empty()),
        _ => panic!("wrong command"),
    }
}

#[test]
fn remove_requires_at_least_one_addon() {
    assert!(Cli::try_parse_from(["wau", "remove"]).is_err());
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
fn cache_clear_subcommand_parses() {
    let cli = parse(&["cache", "clear"]);
    assert!(matches!(cli.command, Command::Cache(CacheCommand::Clear)));
}

#[test]
fn profile_configure_and_erase_subcommands_parse() {
    let cli = parse(&["profile", "erase"]);
    assert!(matches!(
        cli.command,
        Command::Profile(ProfileCommand::Erase)
    ));

    let cli = parse(&["profile", "configure", "addon_dir=/tmp/addons"]);
    match cli.command {
        Command::Profile(ProfileCommand::Configure(args)) => {
            assert_eq!(args.options, vec!["addon_dir=/tmp/addons"]);
        }
        _ => panic!("wrong command"),
    }
}

#[test]
fn top_level_configure_alias_parses() {
    let cli = parse(&["configure"]);
    assert!(matches!(cli.command, Command::Configure(_)));
}

#[test]
fn rollback_requires_addon_and_accepts_undo() {
    assert!(Cli::try_parse_from(["wau", "rollback"]).is_err());
    let cli = parse(&["rollback", "curse:foo", "--undo"]);
    match cli.command {
        Command::Rollback(args) => {
            assert_eq!(args.addon, "curse:foo");
            assert!(args.undo);
        }
        _ => panic!("wrong command"),
    }
}

#[test]
fn reconcile_flags_parse() {
    let cli = parse(&["reconcile", "-a", "--list-unreconciled"]);
    match cli.command {
        Command::Reconcile(args) => {
            assert!(args.auto);
            assert!(args.list_unreconciled);
        }
        _ => panic!("wrong command"),
    }
}
