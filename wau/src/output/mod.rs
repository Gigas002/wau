//! Formatting and printing helpers for all `wau` commands.
//!
//! All user-facing output goes through this module so that formatting decisions
//! are centralised rather than scattered across `app`.

use libwau::{fs::InstalledAddon, lock::LockedAddon, manifest::ManifestAddon};

#[cfg(test)]
mod tests;

const COL_NAME: usize = 42;

// ---------------------------------------------------------------------------
// wau list
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// wau sync
// ---------------------------------------------------------------------------

/// Prints the plan of addons that will be installed.
pub fn print_sync_plan(addons: &[&ManifestAddon]) {
    println!("==> Addons to install ({}):", addons.len());
    for a in addons {
        println!("    {}  ({})", a.name, a.provider);
    }
}

/// Prints a confirmation that an addon was installed (suppressed when quiet).
pub fn print_installed(name: &str, quiet: bool) {
    if !quiet {
        println!("installed  {name}");
    }
}

/// Prints the sync operation summary (suppressed when quiet).
pub fn print_sync_summary(installed: u32, skipped: u32, quiet: bool) {
    if !quiet {
        println!("sync done: {installed} installed, {skipped} skipped");
    }
}

// ---------------------------------------------------------------------------
// wau remove
// ---------------------------------------------------------------------------

/// Prints the plan of addons that will be removed.
pub fn print_remove_plan(names: &[String]) {
    println!("==> Addons to remove ({}):", names.len());
    for name in names {
        println!("    {name}");
    }
}

/// Prints a confirmation that an addon was removed (suppressed when quiet).
pub fn print_removed(name: &str, quiet: bool) {
    if !quiet {
        println!("removed    {name}");
    }
}

// ---------------------------------------------------------------------------
// wau info
// ---------------------------------------------------------------------------

/// Prints the manifest + lock details for a single addon.
pub fn print_addon_info(
    name: &str,
    manifest_entry: Option<&ManifestAddon>,
    lock_entry: Option<&LockedAddon>,
) {
    println!("Name     : {name}");

    if let Some(m) = manifest_entry {
        println!("Provider : {}", m.provider);
        println!(
            "Channel  : {}",
            m.channel
                .as_ref()
                .map(|c| c.as_str())
                .unwrap_or("(default)")
        );
        if let Some(flavors) = &m.flavors {
            let s: Vec<_> = flavors.iter().map(|f| f.as_str()).collect();
            println!("Flavors  : {}", s.join(", "));
        }
        if let Some(id) = m.project_id {
            println!("Project  : {id}");
        }
        if let Some(id) = m.wowi_id {
            println!("WoWI ID  : {id}");
        }
        if let Some(r) = &m.repo {
            println!("Repo     : {r}");
        }
    }

    if let Some(l) = lock_entry {
        println!("Version  : {}", l.resolved_version);
        println!("Installed: {}", l.installed_at.format("%Y-%m-%dT%H:%M:%SZ"));
        if !l.installed_dirs.is_empty() {
            println!("Dirs     : {}", l.installed_dirs.join(", "));
        }
    } else {
        println!("Installed: (not installed)");
    }
}

// ---------------------------------------------------------------------------
// wau search
// ---------------------------------------------------------------------------

/// Prints search results across manifest entries and installed addons.
pub fn print_search_results(
    query: &str,
    manifest_matches: &[&ManifestAddon],
    installed_matches: &[&InstalledAddon],
) {
    let total = manifest_matches.len() + installed_matches.len();
    if total == 0 {
        println!("No results for '{query}'.");
        return;
    }

    if !manifest_matches.is_empty() {
        println!("==> Manifest matches for '{query}':");
        for m in manifest_matches {
            println!("    {}  ({})", m.name, m.provider);
        }
    }

    if !installed_matches.is_empty() {
        println!("==> Installed matches for '{query}':");
        for a in installed_matches {
            let version = a.version().unwrap_or("-");
            println!("    {:<col$}  {version}", a.display_title(), col = COL_NAME);
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive prompt
// ---------------------------------------------------------------------------

/// Asks the user a yes/no question. Returns `true` when the user answers y/yes.
pub fn confirm(prompt: &str) -> bool {
    use std::io::Write;
    print!("{prompt} [y/N] ");
    std::io::stdout().flush().ok();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok();
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

// ---------------------------------------------------------------------------
// Test-only helpers
// ---------------------------------------------------------------------------

/// Formats one addon row as a plain string (used in tests without capturing stdout).
#[cfg(test)]
pub fn format_addon_row(addon: &InstalledAddon) -> String {
    let title = addon.display_title();
    let version = addon.version().unwrap_or("-");
    format!("{:<col$}  {}", title, version, col = COL_NAME)
}
