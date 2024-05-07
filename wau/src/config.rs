use std::{error::Error, fs, io::Read, path::PathBuf};

use serde::{Deserialize, Serialize};
use libwau::{Flavor, Wow};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    // wow dir root path
    pub path: Option<PathBuf>,

    // tmp addon files path
    pub tmp: Option<PathBuf>,

    // your CurseForge API token
    pub token_curse: Option<String>,
}

impl Config {
    pub fn load(path: &PathBuf) -> Result<Config, Box<dyn Error>> {
        let mut config_file = std::fs::File::open(path)?;
        let mut config_str = String::new();
        config_file.read_to_string(&mut config_str)?;

        toml::from_str(&config_str).map_err(|err| err.into())
    }

    pub fn get_default_path() -> Result<PathBuf, Box<dyn Error>> {
        dirs::config_local_dir()
            .ok_or_else(|| "Couldn't get local config directory path".into())
            .map(|path| path.join("wau").join("config.toml"))
    }

    pub fn get_flavors(&self) -> Vec<Wow> {
        let root = self.path.clone().unwrap();

        let mut flavors = vec![];

        for flavor_dir in fs::read_dir(root).unwrap() {
            let flavor_dir = flavor_dir.unwrap();
            let metadata = flavor_dir.metadata().unwrap();
            if metadata.is_dir() {
                if let Some(dir_name) = flavor_dir.file_name().to_str() {
                    let flavor = match dir_name {
                        "_retail_" => Flavor::Retail,
                        _ => Flavor::Retail,
                    };

                    let wow = Wow::new(flavor_dir.path(), flavor);

                    flavors.push(wow);
                }
            }
        }

        flavors
    }
}
