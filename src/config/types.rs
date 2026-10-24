use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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

impl Profile {
    /// The project selected with `runsite project use`; an unreadable id counts as none.
    pub fn selected_project(&self) -> Option<Uuid> {
        self.current_project_id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
    }

    pub fn select_project(&mut self, project_id: Option<Uuid>) {
        self.current_project_id = project_id.map(|id| id.to_string());
    }
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
