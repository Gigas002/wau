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
