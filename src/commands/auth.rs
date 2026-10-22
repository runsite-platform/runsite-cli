use crate::api::{ApiClient, Credentials, UserResponse};
use crate::cli::OutputFormat;
use crate::config::ActiveProfile;
use crate::output::render;
use anyhow::Result;
use dialoguer::{Input, Password};

pub async fn login(client: &ApiClient, profile: &ActiveProfile, token: Option<&str>) -> Result<()> {
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
    profile.store_api_key(outcome.api_key)
}

pub fn logout(profile: &ActiveProfile) -> Result<()> {
    profile.forget_api_key()?;
    println!("Logged out. (The key is still active server-side; revoke it in the dashboard.)");
    Ok(())
}

pub async fn whoami(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let user: UserResponse = client.current_user().await?;
    render(format, &user, || {
        println!("Email: {}", user.email);
        if let Some(name) = &user.full_name {
            println!("Name:  {}", name);
        }
        println!("ID:    {}", user.id);
    })
}
