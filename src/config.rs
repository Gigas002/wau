use std::{error::Error, fs, io::Read, path::PathBuf};

use serde::{Deserialize, Serialize};
use wau::{Flavor, Wow};

const DEFAULT_CONFIG_SUBDIR: &str = "wau";
const DEFAULT_CONFIG_FILENAME: &str = "config.toml";

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
            .map(|path| {
                path.join(DEFAULT_CONFIG_SUBDIR)
                    .join(DEFAULT_CONFIG_FILENAME)
            })
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

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        error::Error,
        fs::{create_dir, remove_dir_all, File},
        io::Write,
        path::PathBuf,
    };

    use rand::distributions::{Alphanumeric, DistString};

    use super::Config;

    struct TmpPath {
        path: PathBuf,
    }

    impl TmpPath {
        fn new(path: PathBuf) -> Result<Self, Box<dyn Error>> {
            create_dir(&path)?;
            Ok(TmpPath { path })
        }

        fn create_config(&self, name: &str, data: &str) -> Result<PathBuf, Box<dyn Error>> {
            let config_file_path = self.path.join(name);
            let mut config_file = File::create(&config_file_path)?;
            config_file.write_all(data.as_bytes())?;
            Ok(config_file_path)
        }
    }

    impl Drop for TmpPath {
        fn drop(&mut self) {
            if let Err(e) = remove_dir_all(&self.path) {
                log::warn!(
                    "cannot delete temp dir {}, error: {}",
                    self.path.to_str().unwrap_or_default(),
                    e
                )
            }
        }
    }

    #[test]
    fn load_config() {
        let temp_path =
            TmpPath::new(temp_dir().join(Alphanumeric.sample_string(&mut rand::thread_rng(), 16)))
                .expect("cannot create temp dir");

        let test_cases = vec![
            ("ololo.toml", "ololo", true, None),
            (
                "empty.toml",
                "# ololo",
                false,
                Some(Config {
                    path: None,
                    tmp: None,
                    token_curse: None,
                }),
            ),
            (
                "valid.toml",
                "path = \"olololo\"",
                false,
                Some(Config {
                    path: Some(PathBuf::from("olololo")),
                    tmp: None,
                    token_curse: None,
                }),
            ),
            (
                "valid_full.toml",
                "path = \"olololo\"\ntmp = \"kek\"\ntoken_curse = \"secret\"",
                false,
                Some(Config {
                    path: Some(PathBuf::from("olololo")),
                    tmp: Some(PathBuf::from("kek")),
                    token_curse: Some("secret".to_string()),
                }),
            ),
        ];

        for (config_file_name, config_data, is_error_expected, result) in test_cases {
            let config_file_path = temp_path
                .create_config(&config_file_name, &config_data)
                .expect("cannot create config file");
            let parsed_config = Config::load(&config_file_path);
            if is_error_expected {
                assert!(
                    parsed_config.is_err(),
                    "Expected parsing error for file: {}",
                    config_file_name
                );
            } else {
                assert!(
                    !parsed_config.is_err(),
                    "Expected successful parsing for file: {}",
                    config_file_name
                );
                assert_eq!(parsed_config.ok(), result)
            }
        }
    }
}
