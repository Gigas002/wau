//! Core addon domain vocabulary: game flavours, WoW install products,
//! resolution strategies, and addon definitions (`Defn`).
//!
//! Ported from instawow's `wow_installations.py` and `definitions.py`. Where
//! Python kept `Flavour`/`FlavourVersions`/`FlavourTocSuffixes` as three
//! parallel enums (a workaround for `Flavour` already using its enum *value*
//! for the string id), Rust has no such constraint: build-number ranges and
//! `.toc` suffixes are plain methods on one [`Flavour`] enum.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use url::Url;

#[cfg(test)]
mod tests;

// ============================================================================
// Flavour
// ============================================================================

/// WoW client track.
///
/// [`Flavour::CLASSIC`] is a *moving alias* for whichever classic-progression
/// flavour is current (Mists of Pandaria as of this writing) — it is not a
/// distinct client, matching instawow's `Flavour.Classic = MistsClassic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flavour {
    Mainline,
    VanillaClassic,
    TbcClassic,
    WrathClassic,
    TitanClassic,
    CataClassic,
    MistsClassic,
}

impl Flavour {
    /// Alias for the current classic-progression flavour (`instawow`'s `Flavour.Classic`).
    pub const CLASSIC: Flavour = Flavour::MistsClassic;

    pub const ALL: [Flavour; 7] = [
        Flavour::Mainline,
        Flavour::VanillaClassic,
        Flavour::TbcClassic,
        Flavour::WrathClassic,
        Flavour::TitanClassic,
        Flavour::CataClassic,
        Flavour::MistsClassic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mainline => "mainline",
            Self::VanillaClassic => "vanilla_classic",
            Self::TbcClassic => "tbc_classic",
            Self::WrathClassic => "wrath_classic",
            Self::TitanClassic => "titan_classic",
            Self::CataClassic => "cata_classic",
            Self::MistsClassic => "mists_classic",
        }
    }

    /// Parses a flavour id, accepting the `classic` and `retail` back-compat
    /// aliases instawow's `Flavour._missing_` and `Flavour.Classic` support.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mainline" => Some(Self::Mainline),
            "vanilla_classic" => Some(Self::VanillaClassic),
            "tbc_classic" => Some(Self::TbcClassic),
            "wrath_classic" => Some(Self::WrathClassic),
            "titan_classic" => Some(Self::TitanClassic),
            "cata_classic" => Some(Self::CataClassic),
            "mists_classic" => Some(Self::MistsClassic),
            "classic" => Some(Self::CLASSIC),
            "retail" => Some(Self::Mainline),
            _ => None,
        }
    }

    /// `.toc` filename/interface suffixes accepted for this flavour, in priority order.
    pub fn toc_suffixes(self) -> &'static [&'static str] {
        match self {
            Self::Mainline => &["Mainline"],
            Self::VanillaClassic => &["Vanilla", "Classic"],
            Self::TbcClassic => &["TBC", "BCC", "Classic"],
            Self::WrathClassic | Self::TitanClassic => &["Wrath", "WOTLKC", "Classic"],
            Self::CataClassic => &["Cata", "Classic"],
            Self::MistsClassic => &["Mists", "Classic"],
        }
    }

    /// Client build-number ranges (`major*10000 + minor*100 + patch`) that map to
    /// this flavour, per instawow's `FlavourVersions`.
    fn version_ranges(self) -> &'static [VersionRange] {
        match self {
            Self::Mainline => &MAINLINE_RANGES,
            Self::VanillaClassic => std::slice::from_ref(&VANILLA_CLASSIC_RANGE),
            Self::TbcClassic => std::slice::from_ref(&TBC_CLASSIC_RANGE),
            Self::WrathClassic => std::slice::from_ref(&WRATH_CLASSIC_RANGE),
            Self::TitanClassic => std::slice::from_ref(&TITAN_CLASSIC_RANGE),
            Self::CataClassic => std::slice::from_ref(&CATA_CLASSIC_RANGE),
            Self::MistsClassic => std::slice::from_ref(&MISTS_CLASSIC_RANGE),
        }
    }

    /// Maps a client build number to the flavour whose range contains it.
    pub fn from_build_number(build: u32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.version_ranges().iter().any(|r| r.contains(&build)))
    }

    /// Maps a `## Interface` / `X.Y.Z`-style version string to a flavour.
    /// Missing trailing components are treated as `0`; extra components are ignored.
    pub fn from_version_string(s: &str) -> Option<Self> {
        parse_version_number(s).and_then(Self::from_build_number)
    }
}

impl fmt::Display for Flavour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Flavour {
    type Err = FlavourParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| FlavourParseError(s.to_owned()))
    }
}

impl Serialize for Flavour {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Flavour {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown flavour '{s}'")))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown flavour '{0}'")]
pub struct FlavourParseError(pub String);

type VersionRange = std::ops::Range<u32>;

const fn norm(major: u32, minor: u32, patch: u32) -> u32 {
    major * 1_00_00 + minor * 1_00 + patch
}

const MAINLINE_RANGES: [VersionRange; 6] = [
    norm(1, 0, 0)..norm(1, 13, 0),
    norm(2, 0, 0)..norm(2, 5, 0),
    norm(3, 0, 0)..norm(3, 4, 0),
    norm(4, 0, 0)..norm(4, 4, 0),
    norm(5, 0, 0)..norm(5, 5, 0),
    norm(6, 0, 0)..norm(13, 0, 0),
];
const VANILLA_CLASSIC_RANGE: VersionRange = norm(1, 13, 0)..norm(2, 0, 0);
const TBC_CLASSIC_RANGE: VersionRange = norm(2, 5, 0)..norm(3, 0, 0);
const WRATH_CLASSIC_RANGE: VersionRange = norm(3, 4, 0)..norm(3, 8, 0);
const TITAN_CLASSIC_RANGE: VersionRange = norm(3, 8, 0)..norm(4, 0, 0);
const CATA_CLASSIC_RANGE: VersionRange = norm(4, 4, 0)..norm(5, 0, 0);
const MISTS_CLASSIC_RANGE: VersionRange = norm(5, 5, 0)..norm(6, 0, 0);

/// Parses `"X[.Y[.Z]]"` into a normalised build number, padding missing
/// components with `0` and ignoring anything beyond the third component.
fn parse_version_number(s: &str) -> Option<u32> {
    let mut parts = s.split('.').map(|p| p.parse::<u32>());
    let major = match parts.next() {
        Some(Ok(v)) => v,
        Some(Err(_)) => return None,
        None => 0,
    };
    let minor = match parts.next() {
        Some(Ok(v)) => v,
        Some(Err(_)) => return None,
        None => 0,
    };
    let patch = match parts.next() {
        Some(Ok(v)) => v,
        Some(Err(_)) => return None,
        None => 0,
    };
    Some(norm(major, minor, patch))
}

// ============================================================================
// Product — WoW client install-directory detection
// ============================================================================

/// A known Blizzard product: its code, the [`Flavour`] it represents, and the
/// install subfolder name it uses (e.g. `_retail_`, `_classic_era_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductInfo {
    pub code: &'static str,
    pub flavour: Flavour,
    pub subfolder: &'static str,
}

/// Every known Blizzard product, ported verbatim from instawow's `PRODUCTS` table.
pub const PRODUCTS: &[ProductInfo] = &[
    ProductInfo {
        code: "wow",
        flavour: Flavour::Mainline,
        subfolder: "_retail_",
    },
    ProductInfo {
        code: "wow_anniversary",
        flavour: Flavour::TbcClassic,
        subfolder: "_anniversary_",
    },
    ProductInfo {
        code: "wow_beta",
        flavour: Flavour::Mainline,
        subfolder: "_beta_",
    },
    ProductInfo {
        code: "wow_classic",
        flavour: Flavour::MistsClassic,
        subfolder: "_classic_",
    },
    ProductInfo {
        code: "wow_classic_beta",
        flavour: Flavour::MistsClassic,
        subfolder: "_classic_beta_",
    },
    ProductInfo {
        code: "wow_classic_era",
        flavour: Flavour::VanillaClassic,
        subfolder: "_classic_era_",
    },
    ProductInfo {
        code: "wow_classic_era_ptr",
        flavour: Flavour::TbcClassic,
        subfolder: "_classic_era_ptr_",
    },
    ProductInfo {
        code: "wow_classic_ptr",
        flavour: Flavour::MistsClassic,
        subfolder: "_classic_ptr_",
    },
    ProductInfo {
        code: "wowdev",
        flavour: Flavour::Mainline,
        subfolder: "_alpha_",
    },
    ProductInfo {
        code: "wowdev2",
        flavour: Flavour::VanillaClassic,
        subfolder: "_classic_alpha_",
    },
    ProductInfo {
        code: "wowe1",
        flavour: Flavour::Mainline,
        subfolder: "_event1_",
    },
    ProductInfo {
        code: "wowlivetest",
        flavour: Flavour::Mainline,
        subfolder: "_dark_realm_",
    },
    ProductInfo {
        code: "wowlivetest2",
        flavour: Flavour::Mainline,
        subfolder: "_dark_realm_2_",
    },
    ProductInfo {
        code: "wowt",
        flavour: Flavour::Mainline,
        subfolder: "_ptr_",
    },
    ProductInfo {
        code: "wowv",
        flavour: Flavour::MistsClassic,
        subfolder: "_vendor_",
    },
    ProductInfo {
        code: "wowv10",
        flavour: Flavour::Mainline,
        subfolder: "_vendor10_",
    },
    ProductInfo {
        code: "wowv2",
        flavour: Flavour::Mainline,
        subfolder: "_vendor2_",
    },
    ProductInfo {
        code: "wowv3",
        flavour: Flavour::Mainline,
        subfolder: "_vendor3_",
    },
    ProductInfo {
        code: "wowv4",
        flavour: Flavour::TitanClassic,
        subfolder: "_vendor4_",
    },
    ProductInfo {
        code: "wowv5",
        flavour: Flavour::TbcClassic,
        subfolder: "_vendor5_",
    },
    ProductInfo {
        code: "wowv6",
        flavour: Flavour::VanillaClassic,
        subfolder: "_vendor6_",
    },
    ProductInfo {
        code: "wowv7",
        flavour: Flavour::MistsClassic,
        subfolder: "_vendor7_",
    },
    ProductInfo {
        code: "wowv8",
        flavour: Flavour::Mainline,
        subfolder: "_vendor8_",
    },
    ProductInfo {
        code: "wowv9",
        flavour: Flavour::TitanClassic,
        subfolder: "_vendor9_",
    },
    ProductInfo {
        code: "wowxptr",
        flavour: Flavour::Mainline,
        subfolder: "_xptr_",
    },
    ProductInfo {
        code: "wowz",
        flavour: Flavour::VanillaClassic,
        subfolder: "_submission_",
    },
];

fn product_by_subfolder(subfolder: &str) -> Option<&'static ProductInfo> {
    PRODUCTS.iter().find(|p| p.subfolder == subfolder)
}

/// If `addon_dir`'s last two path components are `Interface/AddOns`
/// (case-insensitive), returns the installation directory two levels up.
pub fn extract_installation_dir_from_addon_dir(addon_dir: &Path) -> Option<PathBuf> {
    let interface = addon_dir.parent()?;
    let install_dir = interface.parent()?;

    let interface_name = interface.file_name()?.to_str()?;
    let addons_name = addon_dir.file_name()?.to_str()?;
    if interface_name.eq_ignore_ascii_case("interface")
        && addons_name.eq_ignore_ascii_case("addons")
    {
        Some(install_dir.to_path_buf())
    } else {
        None
    }
}

/// `<installation_dir>/Interface/AddOns`.
pub fn get_addon_dir_from_installation_dir(installation_dir: &Path) -> PathBuf {
    installation_dir.join("Interface").join("AddOns")
}

/// Looks up the [`ProductInfo`] for `addon_dir` from its installation directory's
/// subfolder name. Returns `None` for custom-named installs Blizzard doesn't recognise.
pub fn infer_product_from_addon_dir(addon_dir: &Path) -> Option<&'static ProductInfo> {
    let install_dir = extract_installation_dir_from_addon_dir(addon_dir)?;
    let name = install_dir.file_name()?.to_str()?;
    product_by_subfolder(name)
}

// ============================================================================
// Strategy / Strategies
// ============================================================================

/// A non-default resolution behaviour a source may opt into supporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Strategy {
    /// Resolve ignoring the addon's declared flavour compatibility.
    AnyFlavour,
    /// Include alpha/beta releases, not only stable.
    AnyReleaseType,
    /// Pin to an exact version string.
    VersionEq,
}

impl Strategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnyFlavour => "any_flavour",
            Self::AnyReleaseType => "any_release_type",
            Self::VersionEq => "version_eq",
        }
    }
}

impl fmt::Display for Strategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The three [`Strategy`] values a `Defn` may request, each with its own
/// storage shape (flag vs. pinned-version string) — mirrors instawow's
/// `Strategies` mapping, specialised to its fixed 3-key set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Strategies {
    pub any_flavour: bool,
    pub any_release_type: bool,
    pub version_eq: Option<String>,
}

impl Strategies {
    pub fn is_empty(&self) -> bool {
        !self.any_flavour && !self.any_release_type && self.version_eq.is_none()
    }

    /// URI fragment tokens for the set strategies, e.g. `["any_flavour", "version_eq=1.2.3"]`.
    fn uri_tokens(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.any_flavour {
            out.push(Strategy::AnyFlavour.as_str().to_owned());
        }
        if self.any_release_type {
            out.push(Strategy::AnyReleaseType.as_str().to_owned());
        }
        if let Some(v) = &self.version_eq {
            out.push(format!("{}={v}", Strategy::VersionEq));
        }
        out
    }
}

/// Rendering hint for a source's changelog content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangelogFormat {
    Html,
    Markdown,
    Raw,
}

/// Static description of a source, returned by `Resolver::metadata()`.
#[derive(Debug, Clone, Copy)]
pub struct SourceMetadata {
    /// The source id used as `Defn::source` and the DB `pkg.source` value (e.g. `"curse"`).
    pub id: &'static str,
    pub name: &'static str,
    pub strategies: &'static [Strategy],
    pub changelog_format: ChangelogFormat,
    /// `.toc` metadata key used to cross-reference this source's id from a local
    /// addon folder (e.g. `"X-WoWI-ID"`), if the source has one.
    pub addon_toc_key: Option<&'static str>,
}

/// Which headers a `Resolver::request_headers()` call is being made for — some
/// sources need a different `Accept`/auth header set for downloading an
/// artifact than for ordinary API calls (e.g. GitHub's octet-stream `Accept`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadersIntent {
    Fetch,
    Download,
}

// ============================================================================
// Defn — an addon definition (what to install/update)
// ============================================================================

/// What to install: a source, a human-entered alias (slug, search term, or URL
/// path), an optional resolved id, and any requested [`Strategy`] overrides.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Defn {
    pub source: String,
    pub alias: String,
    /// Source-native id, once known — set after resolution or when reconstructed
    /// from an installed package, so re-resolution is stable even if the alias changes.
    pub id: Option<String>,
    pub strategies: Strategies,
}

impl Defn {
    pub fn new(source: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            alias: alias.into(),
            id: None,
            strategies: Strategies::default(),
        }
    }

    /// Parses a `source:alias#strategy1,strategy2=value` URI.
    ///
    /// If the scheme isn't in `known_sources` and `retain_unknown_source` is
    /// `false`, the whole URI is treated as a bare alias with an empty source
    /// (a resolver's `get_alias_from_url` is expected to re-parse it later).
    pub fn from_uri(
        uri: &str,
        known_sources: &[&str],
        retain_unknown_source: bool,
    ) -> Result<Self, DefnParseError> {
        let url = Url::parse(uri).map_err(|_| DefnParseError::Invalid(uri.to_owned()))?;
        let scheme = url.scheme();

        let (source, alias) = if !retain_unknown_source && !known_sources.contains(&scheme) {
            (String::new(), uri.to_owned())
        } else {
            (scheme.to_owned(), url.path().to_owned())
        };

        let strategies = match url.fragment() {
            None => Strategies::default(),
            Some(frag) if frag.is_empty() || frag == "=" => Strategies::default(),
            Some(frag) => parse_strategies(frag)?,
        };

        Ok(Defn {
            source,
            alias,
            id: None,
            strategies,
        })
    }

    /// Renders back to URI form. With `alias_is_id`, uses `id` (falling back to
    /// `alias`) as the path segment; with `include_strategies`, appends the
    /// `#`-fragment for any set strategies.
    pub fn as_uri(&self, alias_is_id: bool, include_strategies: bool) -> String {
        let ident = if alias_is_id {
            self.id.as_deref().unwrap_or(&self.alias)
        } else {
            self.alias.as_str()
        };
        let mut uri = format!("{}:{ident}", self.source);

        if include_strategies {
            let tokens = self.strategies.uri_tokens();
            if !tokens.is_empty() {
                uri.push('#');
                uri.push_str(&tokens.join(","));
            }
        }

        uri
    }

    /// Returns a copy with `Strategy::VersionEq` pinned to `version`.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.strategies.version_eq = Some(version.into());
        self
    }
}

fn parse_strategies(frag: &str) -> Result<Strategies, DefnParseError> {
    let mut strategies = Strategies::default();
    let mut unknown = Vec::new();

    for part in frag.split(',') {
        if part.is_empty() {
            continue;
        }
        let (key, value) = match part.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (part, None),
        };
        match key {
            "any_flavour" => strategies.any_flavour = true,
            "any_release_type" => strategies.any_release_type = true,
            "version_eq" => strategies.version_eq = value.map(str::to_owned),
            other => unknown.push(other.to_owned()),
        }
    }

    if unknown.is_empty() {
        Ok(strategies)
    } else {
        Err(DefnParseError::UnknownStrategies(unknown))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DefnParseError {
    #[error("invalid addon URI '{0}'")]
    Invalid(String),
    #[error("unknown strategies: {}", .0.join(", "))]
    UnknownStrategies(Vec<String>),
}
