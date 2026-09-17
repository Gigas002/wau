//! Interactive prompts: `confirm`/`select_one` (`inquire`-based widgets) plus
//! a plain [`read_line`] for `search`'s paru-style free-text selection
//! prompt (`1 2 3`, `1-3`, ...) — that one isn't a discrete-choice widget, so
//! there's nothing for `inquire` to render. `init`'s per-group source picker
//! is the other `select_one` user; profile/global config setup stays fully
//! non-interactive — hand-write the TOML. No "open in browser" key binding
//! on `select_one` — `inquire` has no custom-keybinding hook for it, and
//! it's a convenience, not core behaviour.

use std::io::Write as _;

use inquire::{Confirm, Select, error::InquireResult};

#[cfg(test)]
mod tests;

pub fn confirm(message: &str, default: bool) -> InquireResult<bool> {
    Confirm::new(message).with_default(default).prompt()
}

/// A labelled option carrying an arbitrary value, for [`select_one`].
pub struct Choice<T> {
    pub label: String,
    pub value: T,
}

impl<T> Choice<T> {
    pub fn new(label: impl Into<String>, value: T) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

pub fn select_one<T>(message: &str, mut choices: Vec<Choice<T>>) -> InquireResult<T> {
    let labels: Vec<String> = choices.iter().map(|c| c.label.clone()).collect();
    let selected = Select::new(message, labels).prompt()?;
    let index = choices
        .iter()
        .position(|c| c.label == selected)
        .expect("selected label came from the same choice list");
    Ok(choices.remove(index).value)
}

/// Prints `prompt` with no trailing newline, then reads and trims one line
/// from stdin. Used for freeform prompts no `inquire` widget fits (`search`'s
/// paru-style `1 2 3` / `1-3` selection line).
pub fn read_line(prompt: &str) -> std::io::Result<String> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_owned())
}
