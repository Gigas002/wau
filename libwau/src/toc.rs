use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

// TOC spec:
// https://wowpedia.fandom.com/wiki/TOC_format

// Ajour code ref:
// https://github.com/ajour/ajour/blob/master/crates/core/src/parse.rs

// Example toc file:
// https://github.com/DeadlyBossMods/DeadlyBossMods/blob/master/DBM-Core/DBM-Core_Mainline.toc

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Toc {
    /// Required, `Interface`
    pub interface: GameVersion,

    /// `Title`
    pub title: Option<String>,

    /// `Notes`
    pub notes: Option<String>,

    /// `IconTexture`
    pub icon_texture: Option<String>,

    /// `IconAtlas`
    pub icon_atlas: Option<String>,

    /// `AddonCompartmentFunc`
    pub addon_compartment_func: Option<String>,

    /// `AddonCompartmentFuncOnEnter`
    pub addon_compartment_func_on_enter: Option<String>,

    /// `AddonCompartmentFuncOnLeave`
    pub addon_compartment_func_on_leave: Option<String>,

    /// `LoadOnDemand`
    pub load_on_demand: Option<i32>,

    /// `Dependencies`/`RequiredDeps`
    pub dependencies: Vec<String>,

    /// `OptionalDeps`
    pub optional_dependencies: Vec<String>,

    /// `LoadWith`
    pub load_with: Option<String>,

    /// `LoadManagers`
    pub load_mangers: Option<String>,

    /// `DefaultState`
    pub default_state: Option<State>,

    /// `SavedVariables`
    pub saved_variables: Option<String>,

    /// `SavedVariablesPerCharacter`
    pub saved_variables_per_character: Option<String>,

    /// `Author`
    pub author: Option<String>,

    /// `Version`
    pub version: Option<String>,

    /// Required. Lines without additional symbols in the beginning
    /// of a line
    pub files: Vec<String>,

    // Additional (custom X-) properties
    /// `X-Wago-ID`
    pub wago_id: Option<String>,

    /// `X-WoWI-ID`
    pub wowi_id: Option<String>,

    /// `X-Tukui-ProjectID`
    pub tukui_id: Option<String>,

    /// `X-Curse-Project-ID`
    pub curse_id: Option<usize>,

    /// Lines, starting with one `#`
    pub comments: Vec<String>,

    /// Any other unparsed property as dictionary
    pub unknown_properties: HashMap<String, String>,
}

impl FromStr for Toc {
    type Err = Box<dyn Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut toc = Toc::default();
        toc.dependencies = vec![];
        toc.comments = vec![];
        toc.files = vec![];
        toc.unknown_properties = HashMap::new();

        for line in s.lines() {
            match line.chars().next() {
                Some('#') => match line.chars().nth(1) {
                    Some('#') => toc.parse_toc_property(line),
                    _ => toc.parse_toc_comment(line),
                },
                _ if line.is_empty() => continue,
                _ => toc.files.push(line.to_string()),
            }
        }

        Ok(toc)
    }
}

impl Toc {
    pub fn load(path: &PathBuf) -> Result<Toc, Box<dyn Error>> {
        let mut toc_file = std::fs::File::open(path)?;
        let mut toc_str = String::new();
        toc_file.read_to_string(&mut toc_str)?;

        Toc::from_str(&toc_str)
    }

    fn parse_toc_property(&mut self, content: &str) {
        let delim = ":";
        let start = "##";

        if let Some((key, value)) = content.split_once(delim) {
            let key = key.trim_start_matches(start).trim();
            let value = value.trim();

            match key {
                "Interface" => self.interface = GameVersion::from_interface(value),
                "Title" => self.title = Some(Toc::parse_colored_line(value)),
                "Notes" => self.notes = Some(Toc::parse_colored_line(value)),
                "IconTexture" => self.icon_texture = Some(value.to_string()),
                "IconAtlas" => self.icon_atlas = Some(value.to_string()),

                "AddonCompartmentFunc" => self.addon_compartment_func = Some(value.to_string()),
                "AddonCompartmentFuncOnEnter" => {
                    self.addon_compartment_func_on_enter = Some(value.to_string())
                }
                "AddonCompartmentFuncOnLeave" => {
                    self.addon_compartment_func_on_leave = Some(value.to_string())
                }

                "LoadOnDemand" => {
                    self.load_on_demand = Some(value.parse::<i32>().unwrap_or_default())
                }
                "Dependencies" | "RequiredDeps" => {
                    self.dependencies = Toc::get_dependencies_vec(value)
                }
                "OptionalDependencies" | "OptionalDeps" => {
                    self.optional_dependencies = Toc::get_dependencies_vec(value)
                }
                "LoadWith" => self.load_with = Some(value.to_string()),
                "LoadManagers" => self.load_mangers = Some(value.to_string()),
                "DefaultState" => self.default_state = Some(value.parse().unwrap_or_default()),

                "SavedVariables" => self.saved_variables = Some(value.to_string()),
                "SavedVariablesPerCharacter" => {
                    self.saved_variables_per_character = Some(value.to_string())
                }

                "Author" => self.author = Some(value.to_string()),
                "Version" => self.version = Some(value.to_string()),

                "X-Wago-ID" => self.wago_id = Some(value.to_string()),
                "X-WoWI-ID" => self.wowi_id = Some(value.to_string()),
                "X-Tukui-ProjectID" => self.tukui_id = Some(value.to_string()),
                "X-Curse-Project-ID" => self.curse_id = Some(value.parse::<usize>().unwrap()),

                _ => {
                    self.unknown_properties
                        .insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    fn parse_toc_comment(&mut self, content: &str) {
        let start = "#";

        if let Some((_, value)) = content.split_once(start) {
            let value = value.trim();

            self.comments.push(value.to_string())
        }
    }

    fn get_dependencies_vec(dependencies_str: &str) -> Vec<String> {
        if dependencies_str.is_empty() {
            return vec![];
        }

        dependencies_str
            .split([','].as_ref())
            .map(|s| s.trim().to_string())
            .collect()
    }

    fn parse_colored_line(line: &str) -> String {
        let mut result = String::new();
        let mut temp_line = line.to_string();

        while let Some(start) = temp_line.find("|c") {
            result.push_str(&temp_line[..start]);

            if let Some(end) = temp_line[start..].find("|r") {
                let escape_seq = &temp_line[start..start + end + 2];
                result.push_str(&Toc::parse_escape_sequence(escape_seq));

                temp_line = temp_line[start + end + 2..].to_string();
            }
            // no closing tag, add the rest of the string
            else {
                result.push_str(&temp_line[start..]);

                break;
            }
        }

        // add any remaining text after the last escape sequence
        result.push_str(&temp_line);

        result
    }

    fn parse_escape_sequence(seq: &str) -> String {
        // check if using global color notaion with name, |cnCOLOR:text|r
        let is_glob = seq.chars().nth(2).unwrap().eq(&'n');
        let text_end = seq.len() - 2;

        if is_glob {
            seq[..text_end].split(":").nth(1).unwrap().to_string()
        }
        // |cAARRGGBBtext|r
        else {
            seq[10..text_end].to_string()
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct GameVersion {
    value: String,
}

impl GameVersion {
    pub fn from_interface(interface: &str) -> GameVersion {
        // xyyzz for 5 digit and xxyyzz for 6 digit
        let major_len = if interface.len() == 5 { 1 } else { 2 };
        let (major, minor, patch) = (
            &interface[..major_len],
            &interface[major_len..major_len + 2],
            &interface[major_len + 2..],
        );

        let value = match (
            major.parse::<u8>(),
            minor.parse::<u8>(),
            patch.parse::<u8>(),
        ) {
            (Ok(major), Ok(minor), Ok(patch)) => format!("{}.{}.{}", major, minor, patch),
            _ => interface.to_owned(),
        };

        GameVersion { value }
    }

    pub fn to_interface(&self) -> String {
        let mut segments = self.value.split('.').peekable();
        let mut interface = String::new();

        if let Some(major) = segments.next() {
            interface.push_str(&major);
        }

        for segment in segments {
            interface.push_str(&format!("{:02}", segment.parse::<usize>().unwrap()));
        }

        interface
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub enum State {
    Enabled,
    #[default]
    Disabled,
}

impl FromStr for State {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "enabled" => Ok(State::Enabled),
            _ => Ok(State::Disabled),
        }
    }
}
