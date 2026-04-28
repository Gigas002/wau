use crate::cli::{Cli, Command, ListArgs, RemoveArgs, SyncArgs};

fn make_list_cli(tag: Option<&str>) -> Cli {
    Cli {
        config: None,
        command: Command::List(ListArgs {
            tag: tag.map(str::to_owned),
        }),
    }
}

fn make_sync_cli(tag: Option<&str>, update: bool) -> Cli {
    Cli {
        config: None,
        command: Command::Sync(SyncArgs {
            tag: tag.map(str::to_owned),
            manifest: None,
            update,
        }),
    }
}

fn make_remove_cli(tag: Option<&str>, addons: &[&str]) -> Cli {
    Cli {
        config: None,
        command: Command::Remove(RemoveArgs {
            tag: tag.map(str::to_owned),
            addons: addons.iter().map(|s| s.to_string()).collect(),
        }),
    }
}

#[test]
fn list_tag_from_cli() {
    let cli = make_list_cli(Some("classic-official"));
    let Command::List(args) = &cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("classic-official"));
}

#[test]
fn list_tag_absent_in_cli() {
    let cli = make_list_cli(None);
    let Command::List(args) = &cli.command else {
        panic!()
    };
    assert!(args.tag.is_none());
}

#[test]
fn sync_update_flag_stored() {
    let cli = make_sync_cli(None, true);
    let Command::Sync(args) = &cli.command else {
        panic!()
    };
    assert!(args.update);
}

#[test]
fn sync_tag_from_cli() {
    let cli = make_sync_cli(Some("retail-main"), false);
    let Command::Sync(args) = &cli.command else {
        panic!()
    };
    assert_eq!(args.tag.as_deref(), Some("retail-main"));
}

#[test]
fn remove_addons_stored() {
    let cli = make_remove_cli(None, &["WeakAuras", "Bagnon"]);
    let Command::Remove(args) = &cli.command else {
        panic!()
    };
    assert_eq!(args.addons, vec!["WeakAuras", "Bagnon"]);
}
