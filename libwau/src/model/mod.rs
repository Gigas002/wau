use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// WoW client flavor. Covers every expansion client that exists as either an
/// official Blizzard server or a private/community server.
///
/// `Retail` tracks the current mainline client (interface 120000+ as of Midnight).
/// All `Classic*` variants identify a specific expansion-era client regardless of
/// whether the install is an official Blizzard re-release or a private server.
///
/// Interface version ranges (for `.toc` compatibility):
/// - `Retail`        — 120000+ (Mainline / Midnight and beyond)
/// - `Tww`           — 110000–119999 (The War Within)
/// - `Dragonflight`  — 100000–109999 (Dragonflight)
/// - `Shadowlands`   — 90000–99999  (Shadowlands)
/// - `Bfa`           — 80000–89999  (Battle for Azeroth)
/// - `Legion`        — 70000–79999  (Legion)
/// - `Wod`           — 60000–69999  (Warlords of Draenor)
/// - `Mop`           — 50000–59999  (Mists of Pandaria)
/// - `Cata`          — 40000–49999  (Cataclysm)
/// - `Wrath`         — 30000–39999  (Wrath of the Lich King)
/// - `Tbc`           — 20000–29999  (The Burning Crusade)
/// - `Era`           — 10000–19999  (Vanilla / Season of Discovery / Era)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Flavor {
    #[serde(rename = "retail")]
    Retail,
    #[serde(rename = "classic-era")]
    Era,
    #[serde(rename = "classic-tbc")]
    Tbc,
    #[serde(rename = "classic-wrath")]
    Wrath,
    #[serde(rename = "classic-cata")]
    Cata,
    #[serde(rename = "classic-mop")]
    Mop,
    #[serde(rename = "classic-wod")]
    Wod,
    #[serde(rename = "classic-legion")]
    Legion,
    #[serde(rename = "classic-bfa")]
    Bfa,
    #[serde(rename = "classic-shadowlands")]
    Shadowlands,
    #[serde(rename = "classic-dragonflight")]
    Dragonflight,
    #[serde(rename = "classic-tww")]
    Tww,
}

impl Flavor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Retail => "retail",
            Self::Era => "classic-era",
            Self::Tbc => "classic-tbc",
            Self::Wrath => "classic-wrath",
            Self::Cata => "classic-cata",
            Self::Mop => "classic-mop",
            Self::Wod => "classic-wod",
            Self::Legion => "classic-legion",
            Self::Bfa => "classic-bfa",
            Self::Shadowlands => "classic-shadowlands",
            Self::Dragonflight => "classic-dragonflight",
            Self::Tww => "classic-tww",
        }
    }
}

impl std::fmt::Display for Flavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Channel {
    #[serde(rename = "stable")]
    Stable,
    #[serde(rename = "beta")]
    Beta,
    #[serde(rename = "alpha")]
    Alpha,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Known addon providers. New providers are added via PR.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    #[serde(rename = "curseforge")]
    CurseForge,
    #[serde(rename = "wowinterface")]
    WoWInterface,
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "local")]
    Local,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CurseForge => "curseforge",
            Self::WoWInterface => "wowinterface",
            Self::GitHub => "github",
            Self::Local => "local",
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// User-defined label identifying one WoW install tree in `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tag(String);

impl Tag {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Tracing verbosity level for `[logging].level` in `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Flavor {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "retail" => Ok(Self::Retail),
            "classic-era" => Ok(Self::Era),
            "classic-tbc" => Ok(Self::Tbc),
            "classic-wrath" => Ok(Self::Wrath),
            "classic-cata" => Ok(Self::Cata),
            "classic-mop" => Ok(Self::Mop),
            "classic-wod" => Ok(Self::Wod),
            "classic-legion" => Ok(Self::Legion),
            "classic-bfa" => Ok(Self::Bfa),
            "classic-shadowlands" => Ok(Self::Shadowlands),
            "classic-dragonflight" => Ok(Self::Dragonflight),
            "classic-tww" => Ok(Self::Tww),
            _ => Err(format!("unknown flavor '{s}'")),
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "stable" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "alpha" => Ok(Self::Alpha),
            _ => Err(format!("unknown channel '{s}'")),
        }
    }
}
