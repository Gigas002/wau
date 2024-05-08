use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

// TOC spec:
// https://wowpedia.fandom.com/wiki/TOC_format

// Ajour code ref:
// https://github.com/ajour/ajour/blob/master/crates/core/src/parse.rs

// Example toc file:
// https://github.com/DeadlyBossMods/DeadlyBossMods/blob/master/DBM-Core/DBM-Core_Mainline.toc

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Toc {

    // TODO: consider separate struct for conversible value?
    /// Required, `Interface`
    pub interface: String,

    /// `Title`, usually same as folder name
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

    // TODO: consider enum
    /// `DefaultState`
    pub default_state: Option<String>,

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

impl Toc {
    // TODO: try to parse flavor from filename
    pub fn load(path: &PathBuf) -> Result<Toc, Box<dyn Error>> {
        let mut toc_file = std::fs::File::open(path)?;
        let mut toc_str = String::new();
        toc_file.read_to_string(&mut toc_str)?;

        Toc::from_str(&toc_str)
    }

    pub fn from_str(content: &str) -> Result<Toc, Box<dyn Error>> {
        let mut toc = Toc::default();
        toc.dependencies = vec![];
        toc.comments = vec![];
        toc.files = vec![];
        toc.unknown_properties = HashMap::new();

        for line in content.lines() {
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

    fn parse_toc_property(&mut self, content: &str) {
        let delim = ":";
        let start = "##";

        if let Some((key, value)) = content.split_once(delim) {
            let key = key.trim_start_matches(start).trim();
            let value = value.trim();

            match key {
                "Interface" => self.interface = Toc::interface_to_game_version(value),
                "Title" => self.title = Some(Toc::get_title(value)),
                "Notes" => self.notes = Some(Toc::get_notes(value)),
                "IconTexture" => self.icon_texture = Some(value.to_string()),
                "IconAtlas" => self.icon_atlas = Some(value.to_string()),

                "AddonCompartmentFunc" => self.addon_compartment_func = Some(value.to_string()),
                "AddonCompartmentFuncOnEnter" => self.addon_compartment_func_on_enter = Some(value.to_string()),
                "AddonCompartmentFuncOnLeave" => self.addon_compartment_func_on_leave = Some(value.to_string()),

                "LoadOnDemand" => self.load_on_demand = Some(value.parse::<i32>().unwrap()),
                "Dependencies" | "RequiredDeps" => self.dependencies = Toc::get_dependencies_vec(value),
                "OptionalDependencies" | "OptionalDeps" => self.optional_dependencies = Toc::get_dependencies_vec(value),
                "LoadWith" => self.load_with = Some(value.to_string()),
                "LoadManagers" => self.load_mangers = Some(value.to_string()),
                "DefaultState" => self.default_state = Some(value.to_string()),

                "SavedVariables" => self.saved_variables = Some(value.to_string()),
                "SavedVariablesPerCharacter" => self.saved_variables_per_character = Some(value.to_string()),

                "Author" => self.author = Some(value.to_string()),
                "Version" => self.version = Some(value.to_string()),

                "X-Wago-ID" => self.wago_id = Some(value.to_string()),
                "X-WoWI-ID" => self.wowi_id = Some(value.to_string()),
                "X-Tukui-ProjectID" => self.tukui_id = Some(value.to_string()),
                "X-Curse-Project-ID" => self.curse_id = Some(value.parse::<usize>().unwrap()),

                _ => { 
                    self.unknown_properties.insert(key.to_string(), value.to_string());
                },
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

    fn interface_to_game_version(interface: &str) -> String {
        // xyyzz for 5 digit and xxyyzz for 6 digit
        let major_len = if interface.len() == 5 { 1 } else { 2 };
        let (major, minor, patch) = (
            &interface[..major_len],
            &interface[major_len..major_len + 2],
            &interface[major_len + 2..],
        );

        match (major.parse::<u8>(), minor.parse::<u8>(), patch.parse::<u8>()) {
            (Ok(major), Ok(minor), Ok(patch)) => format!("{}.{}.{}", major, minor, patch),
            _ => interface.to_owned(),
        }
    }

    fn get_dependencies_vec(dependencies_str: &str) -> Vec<String> {
        if dependencies_str.is_empty() {
            return vec![];
        }

        dependencies_str.split([','].as_ref())
             .map(|s| s.trim().to_string())
             .collect()
    }

    // TODO: regex required I guess...

    fn get_title(title: &str) -> String {
        title.replace("|c", "").replace("|r", "")
    }

    fn get_notes(notes: &str) -> String {
        notes.replace("|c", "").replace("|r", "")
    }
}
