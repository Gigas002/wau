use std::{collections::HashMap, error::Error, io::Read, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    addons: HashMap<String, Addon>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Addon {
    version: String,
    provider: String,
    flavor: String,
}

impl Manifest {
    pub fn get(path: &PathBuf) -> Result<Manifest, Box<dyn Error>> {
        let mut manifest_file = std::fs::File::open(path)?;
        let mut manifest_str = String::new();
        manifest_file.read_to_string(&mut manifest_str)?;

        let parsed: Manifest = toml::from_str(&manifest_str)?;

        Ok(parsed)
    }
}
