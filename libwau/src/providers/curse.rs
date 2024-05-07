use std::error::Error;

use chrono::{DateTime, Utc};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, USER_AGENT},
    Client,
};
use serde::{Deserialize, Serialize};

use crate::{Addon, Flavor};

const API_URL_BASE: &str = "https://api.curseforge.com/v1";

// API docs: https://docs.curseforge.com/

pub struct Curse {
    pub client: Client,
}

impl Curse {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        // headers.insert("x-api-key", HeaderValue::from_static(token));
        headers.insert(USER_AGENT, HeaderValue::from_static("wau/v0.1.0"));

        let builder = Client::builder().default_headers(headers);

        Curse {
            client: builder.build().unwrap(),
        }
    }

    pub async fn search_mods(&self, query: &str) -> Result<Vec<CurseAddon>, Box<dyn Error>> {
        let url = format!("{API_URL_BASE}/mods/search");
        // TODO: this is test
        let flavor_id = Flavor::Retail as usize;
        let response = self
            .client
            .get(url)
            .form(&[
                ("gameId", "1"),
                ("gameVersionTypeId", &format!("{}", flavor_id)),
                ("searchFilter", query),
                ("pageSize", "10"),
            ])
            .send()
            .await?;

        let response_json = response.text().await?;

        // let mut file = File::open("response.json")?;
        // let mut response_json = String::new();
        // let _ = file.read_to_string(&mut response_json);

        let response_obj: CurseSearchResponse = serde_json::from_str(&response_json)?;
        let addons = response_obj.data.unwrap();

        Ok(addons)
    }

    pub async fn get_mod(&self, id: usize) -> Result<CurseAddon, Box<dyn Error>> {
        let url = format!("{API_URL_BASE}/mods/{id}");
        let response = self.client.get(url).send().await?;

        let response_json = response.text().await?;

        // let mut file = File::open("response.json")?;
        // let mut response_json = String::new();
        // let _ = file.read_to_string(&mut response_json);

        let response_obj: CurseGetReponse = serde_json::from_str(&response_json)?;
        let addon = response_obj.data.unwrap();

        Ok(addon)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseSearchResponse {
    pub data: Option<Vec<CurseAddon>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseGetReponse {
    pub data: Option<CurseAddon>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseAddon {
    pub id: Option<usize>,
    pub name: Option<String>,
    pub links: Option<CurseAddonUrls>,
    pub summary: Option<String>,
    pub status: Option<usize>,
    pub download_count: Option<usize>,
    pub is_featured: Option<bool>,
    pub categories: Option<Vec<CurseAddonCategory>>,
    pub authors: Option<Vec<CurseAddonAuthor>>,
    pub latest_files: Option<Vec<CurseAddonFile>>,
    pub date_created: Option<DateTime<Utc>>,
    pub date_modified: Option<DateTime<Utc>>,
    pub date_released: Option<DateTime<Utc>>,
    pub thumbs_up_count: Option<usize>,
    pub rating: Option<usize>,
}

impl CurseAddon {
    pub fn to_addon(&self) -> Addon {
        // version is self.latest_files.display_name
        // url is self.latest_files.download_url
        // Addon {
        //     id: self.id.unwrap().to_string(),
        //     title: self.name.unwrap(),
        //     provider: Provider::Curse,
        //     url: self.links.unwrap().website_url,
        //     dirs: None,
        // }

        panic!("Not yet implemented")
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseAddonUrls {
    pub website_url: Option<String>,
    pub wiki_url: Option<String>,
    pub issues_url: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseAddonAuthor {
    pub id: Option<usize>,
    pub name: Option<String>,
    pub url: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseAddonCategory {
    pub id: Option<usize>,
    pub name: Option<String>,
    pub url: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseAddonFile {
    pub id: Option<usize>,
    pub display_name: Option<String>,
    pub file_name: Option<String>,
    pub release_type: Option<usize>,
    pub file_date: Option<String>,
    pub download_url: Option<String>,
    pub game_versions: Option<Vec<String>>,
    pub modules: Option<Vec<CurseAddonModule>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseAddonModule {
    pub name: Option<String>,
    pub fingerprint: Option<usize>,
}
