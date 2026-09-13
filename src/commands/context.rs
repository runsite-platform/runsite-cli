use crate::api::ApiClient;
use crate::config::{self, Config};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub async fn show(
    client: &ApiClient,
    config: Arc<Mutex<Config>>,
    profile_name: &str,
) -> Result<()> {
    // Released right away: the project lookup below locks the same config.
    let profile = config
        .lock()
        .unwrap()
        .profiles
        .get(profile_name)
        .cloned()
        .unwrap_or_default();

    let authenticated = profile.api_key.is_some() || std::env::var("RUNSITE_API_TOKEN").is_ok();

    println!("Profile:  {}", profile_name);
    println!("API URL:  {}", profile.api_url);
    println!(
        "Auth:     {}",
        if authenticated {
            "authenticated"
        } else {
            "not authenticated"
        }
    );
    if let Some(project_id) = &profile.current_project_id {
        // The config only stores the id; the name needs a lookup, and staying
        // silent about it beats failing `context show` when the API is down.
        let name = match Uuid::parse_str(project_id) {
            Ok(parsed) => client.project_name(parsed).await,
            Err(_) => None,
        };
        match name {
            Some(name) => println!("Project:  {} ({})", name, project_id),
            None => println!("Project:  {}", project_id),
        }
    }
    Ok(())
}

pub fn set_url(config: Arc<Mutex<Config>>, profile_name: &str, url: &str) -> Result<()> {
    {
        let mut cfg = config.lock().unwrap();
        let profile = cfg.profiles.entry(profile_name.to_string()).or_default();
        profile.api_url = url.to_string();
        config::save(&cfg)?;
    }
    println!("API URL set to {}", url);
    Ok(())
}
