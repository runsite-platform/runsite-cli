use crate::api::{ApiClient, Credentials, UserResponse};
use crate::cli::OutputFormat;
use crate::config::{self, Config};
use crate::output::render;
use anyhow::Result;
use dialoguer::{Input, Password};
use std::sync::{Arc, Mutex};

pub async fn login(
    config: Arc<Mutex<Config>>,
    profile_name: &str,
    base_url: &str,
    token: Option<&str>,
) -> Result<()> {
    let client = ApiClient::new(
        base_url.to_string(),
        config.clone(),
        profile_name.to_string(),
    );

    let credentials = match token {
        // An API key created in the dashboard: the only path that works for
        // accounts without a password (social login).
        Some(key) => Credentials::ApiKey(key.to_string()),
        None => {
            let email: String = Input::new().with_prompt("Email").interact_text()?;
            let password: String = Password::new().with_prompt("Password").interact()?;
            Credentials::Password { email, password }
        }
    };

    let outcome = client.log_in(&credentials).await?;
    println!("Logged in as {}", outcome.email);
    store_key(&config, profile_name, outcome.api_key)
}

fn store_key(config: &Arc<Mutex<Config>>, profile_name: &str, api_key: String) -> Result<()> {
    let mut cfg = config.lock().unwrap();
    let profile = cfg.profiles.entry(profile_name.to_string()).or_default();
    profile.api_key = Some(api_key);
    config::save(&cfg)
}

pub fn logout(config: Arc<Mutex<Config>>, profile_name: &str) -> Result<()> {
    {
        let mut cfg = config.lock().unwrap();
        if let Some(profile) = cfg.profiles.get_mut(profile_name) {
            profile.api_key = None;
        }
        config::save(&cfg)?;
    }
    println!("Logged out. (The key is still active server-side; revoke it in the dashboard.)");
    Ok(())
}

pub async fn whoami(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let user: UserResponse = client.get("/api/v1/users/me").await?;
    render(format, &user, || {
        println!("Email: {}", user.email);
        if let Some(name) = &user.full_name {
            println!("Name:  {}", name);
        }
        println!("ID:    {}", user.id);
    })
}
