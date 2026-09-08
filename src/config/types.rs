use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub current_profile: String,
    pub profiles: HashMap<String, Profile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Profile {
    pub api_url: String,
    pub api_key: Option<String>,
    pub current_project_id: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            api_url: "https://api.runsite.app".to_string(),
            api_key: None,
            current_project_id: None,
        }
    }
}
