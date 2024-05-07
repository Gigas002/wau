use std::{fs, path::PathBuf};

use providers::Provider;

pub mod providers;

// TODO: providers can be hidden under features

pub struct Wow {
    pub path: PathBuf,
    pub flavor: Flavor,
    pub addons: Option<Vec<Addon>>,
}

impl Wow {
    pub fn new(path: PathBuf, flavor: Flavor) -> Self {
        let addons = Wow::get_installed_addons(&path);

        Wow {
            path,
            flavor,
            addons,
        }
    }

    pub fn get_installed_addons(flavor_path: &PathBuf) -> Option<Vec<Addon>> {
        let addons_directory = "Interface/AddOns";
        let addons_directory = flavor_path.join(addons_directory);

        Some(Addon::get_installed_addons(&addons_directory))
    }
}

// TODO: check if this is curse-specific id or spec
pub enum Flavor {
    Retail = 517,
    Classic = 67408,
    ClassicBC = 73246,
    ClassicWOTLK = 73713,
    ClassicCata = 77522,
}

pub struct Addon {
    pub name: String,
    pub id: usize,
    pub dirs: Option<Vec<PathBuf>>,
    pub url: Option<String>,
    // pub version: Version
    pub provider: Provider,
}

impl Addon {
    pub fn new(name: &str, id: usize, provider: Provider) -> Self {
        Addon {
            name: name.to_string(),
            id,
            dirs: None,
            url: None,
            provider,
        }
    }

    pub fn get_installed_addons(path: &PathBuf) -> Vec<Addon> {
        let mut addons = vec![];

        for addon_dir in fs::read_dir(path).unwrap() {
            let addon_dir = addon_dir.unwrap();
            let p = addon_dir.path();

            // TODO: check if addon is already exists in list
            let addon = Addon::determine_addon(&p);
            addons.push(addon)
        }

        addons
    }

    pub fn determine_addon(_path: &PathBuf) -> Addon {
        panic!("aaa")
    }

    pub fn update() {
        // check if version is newer
        // download new
        // remove old
        // unpack new
    }

    pub fn proc_get_addons() {
        // conifg = load config
        // addons_path = load addons dir
        // installed_addons = get list of installed addons
        // updatable_addons = get list of updatable addons
        // print(all_addons) = print list of all packages and updates if available
        // ask confirmation for update
        // update(updatable_addons)
    }

    pub fn proc_update() {
        // proc_get_addons
        // get list of updatable addons
        // update
    }

    pub fn proc_search() {
        // parse user input string
        // send search request
        // parse list of addons from response
        // print
    }
}
