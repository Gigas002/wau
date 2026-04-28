use super::*;
use clap::Parser;

#[test]
fn list_no_args_uses_defaults() {
    let cli = Cli::try_parse_from(["wau", "list"]).unwrap();
    assert!(matches!(cli.command, Command::List(_)));
    let Command::List(args) = cli.command;
    assert!(args.tag.is_none());
}

#[test]
fn list_with_tag() {
    let cli = Cli::try_parse_from(["wau", "list", "--tag", "retail-main"]).unwrap();
    let Command::List(args) = cli.command;
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
