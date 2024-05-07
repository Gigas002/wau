use std::error::Error;

use crate::Addon;

use self::curse::Curse;

pub mod curse;

pub enum Provider {
    Curse(String),
}

impl Provider {
    pub async fn get(&self, id: usize) -> Result<Addon, Box<dyn Error>> {
        match self {
            Provider::Curse(token) => Provider::get_addon_curse(id, token).await,
        }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Addon>, Box<dyn Error>> {
        match self {
            Provider::Curse(token) => Provider::search_addon_curse(query, token).await,
        }
    }

    async fn get_addon_curse(id: usize, token: &str) -> Result<Addon, Box<dyn Error>> {
        let curse_client = Curse::new(token);

        let addon = curse_client.get_mod(id).await?;

        let addon = addon.to_addon();

        Ok(addon)
    }

    async fn search_addon_curse(query: &str, token: &str) -> Result<Vec<Addon>, Box<dyn Error>> {
        let curse_client = Curse::new(token);

        let curse_addons = curse_client.search_mods(query).await?;

        let mut addons = vec![];

        for curse_addon in curse_addons {
            addons.push(curse_addon.to_addon());
        }

        Ok(addons)
    }
}
