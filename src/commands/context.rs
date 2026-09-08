use crate::config::{self, Config};
use anyhow::Result;
use std::sync::{Arc, Mutex};

pub fn show(config: Arc<Mutex<Config>>, profile_name: &str) -> Result<()> {
    let cfg = config.lock().unwrap();
    let profile = cfg.profiles.get(profile_name).cloned().unwrap_or_default();

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
    if let Some(pid) = &profile.current_project_id {
        println!("Project:  {}", pid);
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
