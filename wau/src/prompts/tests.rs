//! `confirm`/`select_one`/`read_line` all drive real terminal I/O and aren't
//! practically unit-testable (no TTY in CI); only the plain data carrier
//! below has independent logic.

use super::*;

#[test]
fn choice_new_stores_label_and_value() {
    let choice = Choice::new("Curse: Foo", 42);
    assert_eq!(choice.label, "Curse: Foo");
    assert_eq!(choice.value, 42);
}

#[test]
fn esc_skips_but_ctrl_c_and_no_terminal_are_errors() {
    // `init` used `.ok()`, so Ctrl-C moved on to the next group and a non-interactive run
    // skipped every group silently.
    use inquire::InquireError;

    assert_eq!(esc_as_none(Ok(3)).unwrap(), Some(3));
    assert_eq!(
        esc_as_none::<i32>(Err(InquireError::OperationCanceled)).unwrap(),
        None
    );
    assert!(matches!(
        esc_as_none::<i32>(Err(InquireError::OperationInterrupted)),
        Err(InquireError::OperationInterrupted)
    ));
    assert!(matches!(
        esc_as_none::<i32>(Err(InquireError::NotTTY)),
        Err(InquireError::NotTTY)
    ));
}
