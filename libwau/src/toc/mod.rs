//! World of Warcraft `.toc` file parser.
//!
//! Implements the full TOC metadata format as documented at:
//! - <https://warcraft.wiki.gg/wiki/TOC_format>
//! - <https://www.better-addons.com/toc-format/>
//! - <https://www.addonstudio.org/wiki/WoW:TOC_format>
//!
//! # Format overview
//!
//! | Line prefix | Meaning |
//! |-------------|---------|
//! | `## Key: Value` | Metadata directive |
//! | `#` (single) | Comment, ignored |
//! | (blank) | Ignored |
//! | anything else | File entry (path to a Lua/XML file) |
//!
//! Tag names are **case-insensitive**; both key and value are **whitespace-trimmed**.
//! Lines longer than 1 024 characters are silently truncated (matching the WoW client).
//! Unknown directives are silently ignored (except `X-*` prefixed fields, which are
//! collected verbatim for provider-ID and other custom metadata use).

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

/// Fully parsed contents of one `.toc` file.
///
/// Every field has a natural "absent" default (`None`, empty `Vec`, `false`, …).
/// Use [`parse`] or [`parse_str`] to construct instances.
#[derive(Debug, Clone, Default)]
pub struct TocFile {
    /// Path to this `.toc` file on disk.  Empty when constructed via [`parse_str`].
    pub path: PathBuf,

    // -----------------------------------------------------------------------
    // Interface versions
    // -----------------------------------------------------------------------
    /// `## Interface` — interface version(s) the addon targets (comma-separated in the file).
    pub interface: Vec<u32>,

    /// `## Interface-<Flavor>` — per-flavor interface version overrides, e.g.
    /// `Interface-Mainline: 120001` or `Interface-Vanilla: 11508`.
    /// Keys preserve the original capitalisation from the file.
    pub interface_flavor: HashMap<String, Vec<u32>>,

    // -----------------------------------------------------------------------
    // Display / identity
    // -----------------------------------------------------------------------
    /// `## Title` — display name with WoW UI color codes stripped.
    pub title: Option<String>,

    /// `## Title` — display name exactly as written in the file (color markup intact).
    pub title_raw: Option<String>,

    /// `## Title-<Locale>` — localized titles with color codes stripped.
    /// Keys are the 4-character locale code in lowercase (e.g. `"frfr"`, `"dede"`).
    pub title_locale: HashMap<String, String>,

    /// `## Version` — addon version string.
    pub version: Option<String>,

    /// `## Notes` — description/tooltip with WoW UI color codes stripped.
    pub notes: Option<String>,

    /// `## Notes-<Locale>` — localized notes.
    /// Keys follow the same convention as [`title_locale`](Self::title_locale).
    pub notes_locale: HashMap<String, String>,

    /// `## Author` — author name(s).
    pub author: Option<String>,

    /// `## Category` — collapsible category header in the addon list (patch 11.1.0+).
    pub category: Option<String>,

    /// `## Group` — non-collapsible grouped display in the addon list (patch 11.1.0+).
    pub group: Option<String>,

    // -----------------------------------------------------------------------
    // Icons (patch 10.1.0+)
    // -----------------------------------------------------------------------
    /// `## IconTexture` — relative path to a texture file used as the addon icon.
    pub icon_texture: Option<String>,

    /// `## IconAtlas` — texture atlas name for the addon icon.
    /// Lower priority than [`icon_texture`](Self::icon_texture) when both are set.
    pub icon_atlas: Option<String>,

    // -----------------------------------------------------------------------
    // Dependencies
    // -----------------------------------------------------------------------
    /// `## Dependencies` / `## RequiredDeps` / `## Dep*` — required addons that must
    /// be present and loaded before this addon.
    pub dependencies: Vec<String>,

    /// `## OptionalDeps` — soft dependencies; loaded before this addon when present,
    /// and their presence triggers LoadOnDemand addons to load.
    pub optional_deps: Vec<String>,

    /// `## LoadWith` — load this addon whenever any of the listed addons loads.
    pub load_with: Vec<String>,

    /// `## LoadManagers` — if any listed manager is loaded, treat this addon as
    /// LoadOnDemand; otherwise load normally.
    pub load_managers: Vec<String>,

    // -----------------------------------------------------------------------
    // Load behaviour
    // -----------------------------------------------------------------------
    /// `## LoadOnDemand` — when `true`, the addon is not loaded automatically at startup.
    pub load_on_demand: bool,

    /// `## DefaultState` — default enable state on first install.
    pub default_state: Option<String>,

    /// `## LoadFirst` — when `true`, load before all non-secure addons (secure addons only).
    pub load_first: bool,

    /// `## AllowLoadGameType` — restrict loading to specific game client types
    /// (e.g. `"mainline"`, `"classic"`).
    pub allow_load_game_type: Vec<String>,

    // -----------------------------------------------------------------------
    // Saved variables
    // -----------------------------------------------------------------------
    /// `## SavedVariables` — account-wide Lua global variable names persisted by WoW.
    pub saved_variables: Vec<String>,

    /// `## SavedVariablesPerCharacter` — character-specific Lua global variable names.
    pub saved_variables_per_character: Vec<String>,

    // -----------------------------------------------------------------------
    // Custom fields
    // -----------------------------------------------------------------------
    /// `## X-*` — custom metadata tags.  Keys include the `X-` prefix exactly as
    /// written in the file (original case preserved).
    pub x_fields: HashMap<String, String>,

    // -----------------------------------------------------------------------
    // File entries
    // -----------------------------------------------------------------------
    /// Non-comment, non-blank lines that are not metadata directives.
    /// These are the Lua/XML file paths listed in the `.toc`.
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Reads a `.toc` file from `path` and parses it.
///
/// Returns `None` if the file cannot be read (logged at `DEBUG` level).
/// Individual malformed directives are silently skipped.
pub fn parse(path: &Path) -> Option<TocFile> {
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(path = %path.display(), error = %e, "cannot read .toc file");
            return None;
        }
    };
    let mut toc = parse_str(&content);
    toc.path = path.to_path_buf();
    Some(toc)
}

/// Parses `.toc` content from a string slice.
///
/// Best-effort and infallible: unknown directives are ignored, malformed lines are
/// skipped, and the returned [`TocFile::path`] is an empty [`PathBuf`].
pub fn parse_str(content: &str) -> TocFile {
    let mut toc = TocFile::default();

    for raw_line in content.lines() {
        // WoW client silently ignores characters beyond position 1024.
        let line = char_truncate(raw_line, 1024);

        if let Some(rest) = line
            .strip_prefix("## ")
            .or_else(|| line.strip_prefix("##\t"))
        {
            // Metadata directive: "## Key: Value"
            if let Some((key, value)) = rest.split_once(':') {
                apply_directive(&mut toc, key.trim(), value.trim());
            }
        } else if line.starts_with('#') || line.trim().is_empty() {
            // Comment or blank line — skip.
        } else {
            // File entry.
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                toc.files.push(trimmed.to_owned());
            }
        }
    }

    toc
}

// ---------------------------------------------------------------------------
// Directive dispatch
// ---------------------------------------------------------------------------

fn apply_directive(toc: &mut TocFile, key: &str, value: &str) {
    let key_lc = key.to_ascii_lowercase();

    // --- Interface-<Flavor> (must check before locale-suffix detection) ---
    if let Some(flavor_part) = key_lc.strip_prefix("interface-")
        && !flavor_part.is_empty()
    {
        // Preserve the original capitalisation of the flavor name.
        let flavor = key[10..].to_owned();
        toc.interface_flavor.insert(flavor, parse_csv_u32(value));
        return;
    }

    // --- X-* custom fields ---
    if key_lc.starts_with("x-") {
        toc.x_fields.insert(key.to_owned(), value.to_owned());
        return;
    }

    // --- Locale-suffixed fields: Title-frFR, Notes-deDE, … ---
    if let Some((base, locale)) = split_locale_suffix(&key_lc) {
        match base {
            "title" => {
                toc.title_locale
                    .insert(locale.to_owned(), strip_color_codes(value));
            }
            "notes" => {
                toc.notes_locale
                    .insert(locale.to_owned(), strip_color_codes(value));
            }
            _ => {}
        }
        return;
    }

    // --- Standard fields ---
    match key_lc.as_str() {
        "interface" => {
            toc.interface = parse_csv_u32(value);
        }
        "title" => {
            toc.title_raw = Some(value.to_owned());
            toc.title = Some(strip_color_codes(value));
        }
        "version" => {
            toc.version = Some(value.to_owned());
        }
        "notes" => {
            toc.notes = Some(strip_color_codes(value));
        }
        "author" => {
            toc.author = Some(value.to_owned());
        }
        "category" => {
            toc.category = Some(value.to_owned());
        }
        "group" => {
            toc.group = Some(value.to_owned());
        }
        "icontexture" => {
            toc.icon_texture = Some(value.to_owned());
        }
        "iconatlas" => {
            toc.icon_atlas = Some(value.to_owned());
        }
        // Required-dependency aliases: Dependencies, RequiredDeps, Dep*
        "dependencies" | "requireddeps" => {
            toc.dependencies = parse_csv(value);
        }
        k if k.starts_with("dep") => {
            toc.dependencies = parse_csv(value);
        }
        "optionaldeps" => {
            toc.optional_deps = parse_csv(value);
        }
        "loadwith" => {
            toc.load_with = parse_csv(value);
        }
        "loadmanagers" => {
            toc.load_managers = parse_csv(value);
        }
        "loadondemand" => {
            toc.load_on_demand = parse_bool(value);
        }
        "defaultstate" => {
            toc.default_state = Some(value.to_owned());
        }
        "loadfirst" => {
            toc.load_first = parse_bool(value);
        }
        "allowloadgametype" => {
            toc.allow_load_game_type = parse_csv(value);
        }
        "savedvariables" => {
            toc.saved_variables = parse_csv(value);
        }
        "savedvariablespercharacter" => {
            toc.saved_variables_per_character = parse_csv(value);
        }
        _ => {
            // Unknown directive — silently ignore per spec.
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Splits `key_lc` (already lowercased) at a locale suffix.
///
/// A locale suffix is a trailing `-XXXX` where XXXX is exactly 4 ASCII
/// alphanumeric characters (e.g. `"frfr"`, `"dede"`, `"zhcn"`).
/// Returns `(base, locale)` on success.
fn split_locale_suffix(key_lc: &str) -> Option<(&str, &str)> {
    let dash = key_lc.rfind('-')?;
    let suffix = &key_lc[dash + 1..];
    if suffix.len() == 4 && suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Some((&key_lc[..dash], suffix))
    } else {
        None
    }
}

/// Parses a comma-separated list, trimming each element and dropping empties.
fn parse_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Parses a comma-separated list of `u32` values; non-numeric entries are silently skipped.
fn parse_csv_u32(value: &str) -> Vec<u32> {
    value
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect()
}

/// Interprets a boolean directive value.  `"1"`, `"true"`, and `"yes"` are `true`.
fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

/// Strips WoW UI color codes (`|cAARRGGBB` … `|r`) from `s` and trims the result.
///
/// Color codes use the format `|c` followed by 8 hex digits (AARRGGBB), and `|r` to reset.
pub fn strip_color_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '|' {
            match chars.peek() {
                Some('c') => {
                    chars.next(); // consume 'c'
                    for _ in 0..8 {
                        chars.next(); // skip the 8-char AARRGGBB hex value
                    }
                }
                Some('r') => {
                    chars.next(); // consume 'r'
                }
                _ => result.push(c),
            }
        } else {
            result.push(c);
        }
    }

    result.trim().to_owned()
}

/// Truncates `s` to at most `max_chars` Unicode scalar values.
fn char_truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}
