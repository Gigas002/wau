//! Terminal styling — thin `owo-colors` wrappers. Every function here is a
//! pure string transform driven by an explicit `color: bool`; nothing here
//! does its own I/O or terminal/environment detection, so callers decide
//! once (via [`color_enabled`]) and the choice is easy to override in tests.
//!
//! [`color_enabled`] checks only `io::stdout().is_terminal()` — never an
//! environment variable (`NO_COLOR` included), consistent with `wau` never
//! reading environment variables for its own behavior.

use std::io::IsTerminal;

use owo_colors::{AnsiColors, OwoColorize};

/// Whether stdout is a real terminal — the sole signal for whether to emit
/// ANSI color codes. Piped/redirected output (including everything captured
/// by tests) always comes back `false`.
pub fn color_enabled() -> bool {
    std::io::stdout().is_terminal()
}

/// A small, stable palette addon sources are hashed into, so each source
/// (`curse`, `github`, ...) gets one consistent color across a whole
/// listing — and across runs, since the hash is over the source id itself.
const SOURCE_PALETTE: [AnsiColors; 5] = [
    AnsiColors::BrightCyan,
    AnsiColors::BrightMagenta,
    AnsiColors::BrightBlue,
    AnsiColors::BrightGreen,
    AnsiColors::BrightYellow,
];

/// FNV-1a — small, dependency-free, and stable across runs/platforms (unlike
/// `RandomState`'s hasher, which is randomized per-process and would make a
/// source's color change from one invocation to the next).
fn hash_str(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn source_color(source: &str) -> AnsiColors {
    SOURCE_PALETTE[(hash_str(source) as usize) % SOURCE_PALETTE.len()]
}

/// An addon source id (`curse`, `github`, ...): bold, hashed to a stable color.
pub fn source(color: bool, s: &str) -> String {
    if color {
        s.color(source_color(s)).bold().to_string()
    } else {
        s.to_owned()
    }
}

/// An addon name/slug/identifier: bold, default color.
pub fn name(color: bool, s: &str) -> String {
    if color {
        s.bold().to_string()
    } else {
        s.to_owned()
    }
}

/// A version string.
pub fn version(color: bool, s: &str) -> String {
    if color {
        s.yellow().to_string()
    } else {
        s.to_owned()
    }
}

/// Secondary/explanatory text (descriptions, detail lines).
pub fn dim(color: bool, s: &str) -> String {
    if color {
        s.dimmed().to_string()
    } else {
        s.to_owned()
    }
}

/// The `[Installed]` tag search results and lists use.
pub fn installed_tag(color: bool) -> String {
    if color {
        "[Installed]".blue().bold().to_string()
    } else {
        "[Installed]".to_owned()
    }
}

/// A search result's leading index number.
pub fn number(color: bool, n: usize) -> String {
    if color {
        n.to_string().bold().to_string()
    } else {
        n.to_string()
    }
}

/// The `::` marker prefixing a paru-style prompt/status line.
pub fn marker(color: bool) -> String {
    if color {
        "::".bright_blue().bold().to_string()
    } else {
        "::".to_owned()
    }
}

/// A successful-outcome symbol/word.
pub fn success(color: bool, s: &str) -> String {
    if color {
        s.bright_green().to_string()
    } else {
        s.to_owned()
    }
}

/// A failed-outcome symbol/word (a recognized `ManagerError`).
pub fn failure(color: bool, s: &str) -> String {
    if color {
        s.bright_red().to_string()
    } else {
        s.to_owned()
    }
}

/// An unexpected/internal-error symbol/word.
pub fn warn(color: bool, s: &str) -> String {
    if color {
        s.bright_yellow().to_string()
    } else {
        s.to_owned()
    }
}
