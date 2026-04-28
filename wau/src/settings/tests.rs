use crate::cli::{Cli, Command, ListArgs};

fn make_cli(tag: Option<&str>) -> Cli {
    Cli {
        config: None,
        command: Command::List(ListArgs {
            tag: tag.map(str::to_owned),
        }),
    }
}

#[test]
fn tag_from_cli_overrides_config_default() {
    let cli = make_cli(Some("classic-official"));
    let Command::List(args) = &cli.command;
    assert_eq!(args.tag.as_deref(), Some("classic-official"));
}

#[test]
fn tag_absent_in_cli() {
    let cli = make_cli(None);
    let Command::List(args) = &cli.command;
    assert!(args.tag.is_none());
}
