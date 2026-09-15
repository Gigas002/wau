//! Interactive prompts, built on `inquire`: confirm/text/password/
//! select-one/select-multiple. No "open in browser" key binding on select
//! widgets — `inquire` has no custom-keybinding hook for it, and it's a
//! convenience, not core behaviour.

use inquire::{Confirm, MultiSelect, Password, Select, Text, error::InquireResult};

#[cfg(test)]
mod tests;

pub fn confirm(message: &str, default: bool) -> InquireResult<bool> {
    Confirm::new(message).with_default(default).prompt()
}

pub fn text(message: &str) -> InquireResult<String> {
    Text::new(message).prompt()
}

pub fn password(message: &str) -> InquireResult<String> {
    Password::new(message)
        .without_confirmation()
        .with_display_mode(inquire::PasswordDisplayMode::Masked)
        .prompt()
}

/// A labelled option carrying an arbitrary value, for [`select_one`]/[`select_multiple`].
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

pub fn select_multiple<T>(message: &str, mut choices: Vec<Choice<T>>) -> InquireResult<Vec<T>> {
    let labels: Vec<String> = choices.iter().map(|c| c.label.clone()).collect();
    let selected_labels = MultiSelect::new(message, labels).prompt()?;
    Ok(selected_labels
        .into_iter()
        .filter_map(|label| {
            let index = choices.iter().position(|c| c.label == label)?;
            Some(choices.remove(index).value)
        })
        .collect())
}
