//! Formatting and printing helpers for all `wau` commands.
//!
//! All output goes through this module so that formatting decisions are
//! centralised rather than scattered across `app`.

use libwau::fs::InstalledAddon;

#[cfg(test)]
mod tests;

const COL_NAME: usize = 42;

/// Prints the installed-addon list to stdout.
pub fn print_addon_list(addons: &[InstalledAddon]) {
    if addons.is_empty() {
        println!("No addons found.");
        return;
    }

    println!("{:<col$}  Version", "Name", col = COL_NAME);
    println!("{}", "-".repeat(COL_NAME + 12));

    for addon in addons {
        let title = addon.display_title();
        let version = addon.version().unwrap_or("-");
        println!("{:<col$}  {}", title, version, col = COL_NAME);
    }
}

/// Formats one addon row as a plain string (used in tests without capturing stdout).
#[cfg(test)]
pub fn format_addon_row(addon: &InstalledAddon) -> String {
    let title = addon.display_title();
    let version = addon.version().unwrap_or("-");
    format!("{:<col$}  {}", title, version, col = COL_NAME)
}
