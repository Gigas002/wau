use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

// TOC spec:
// https://wowpedia.fandom.com/wiki/TOC_format

// Ajour code ref:
// https://github.com/ajour/ajour/blob/master/crates/core/src/parse.rs

// Example toc file:
// https://github.com/DeadlyBossMods/DeadlyBossMods/blob/master/DBM-Core/DBM-Core_Mainline.toc

#[derive(Debug, Default)]
pub struct Toc {
    pub interface: String,
    // pub files: Vec<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub dependencies: Option<Vec<String>>,
    pub wago_id: Option<String>,
    pub wowi_id: Option<String>,
    pub tukui_id: Option<String>,
    pub curse_id: Option<usize>,
}

impl Toc {
    pub fn load(path: &PathBuf) -> Result<Toc, Box<dyn Error>> {
        let mut toc_file = std::fs::File::open(path)?;
        let mut toc_str = String::new();
        toc_file.read_to_string(&mut toc_str)?;

        Toc::from_str(&toc_str)
    }

    pub fn from_str(content: &str) -> Result<Toc, Box<dyn Error>> {
        let mut toc = Toc::default();

        for line in content.lines() {
            if let Some((key, value)) = line.split_once(":") {
                let key = key.trim_start_matches("##").trim();
                let value = value.trim();

                match key {
                    "Author" => toc.author = Some(value.to_string()),
                    "Dependencies" | "RequiredDeps" => toc.dependencies = Some(Toc::get_dependencies_vec(value)),
                    "Interface" => toc.interface = Toc::interface_to_game_version(value),
                    "Title" => toc.title = Some(Toc::get_title(value)),
                    "Version" => toc.version = Some(value.to_string()),
                    "X-Curse-Project-ID" => toc.curse_id = Some(value.parse::<usize>()?),
                    "X-WoWI-ID" => toc.wowi_id = Some(value.to_string()),
                    "X-Wago-ID" => toc.wago_id = Some(value.to_string()),
                    "X-Tukui-ProjectID" => toc.tukui_id = Some(value.to_string()),
                    "Notes" => toc.notes = Some(Toc::get_notes(value)),
                    _ => (),
                }
            }
        }

        Ok(toc)
    }

    fn interface_to_game_version(interface: &str) -> String {
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
