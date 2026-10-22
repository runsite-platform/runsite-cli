use crate::api::ApiClient;
use crate::config::ActiveProfile;
use anyhow::Result;

pub async fn show(client: &ApiClient, profile: &ActiveProfile) -> Result<()> {
    println!("Profile:  {}", profile.name());
    println!("API URL:  {}", profile.api_url());
    println!(
        "Auth:     {}",
        if profile.is_authenticated() {
            "authenticated"
        } else {
            "not authenticated"
        }
    );
    if let Some(project_id) = profile.current_project_id() {
        // The config only stores the id; the name needs a lookup, and staying
        // silent about it beats failing `context show` when the API is down.
        match client.project_name(project_id).await {
            Some(name) => println!("Project:  {} ({})", name, project_id),
            None => println!("Project:  {}", project_id),
        }
    }
    Ok(())
}

pub fn set_url(profile: &ActiveProfile, url: &str) -> Result<()> {
    profile.set_api_url(url)?;
    println!("API URL set to {}", profile.api_url());
    Ok(())
}
