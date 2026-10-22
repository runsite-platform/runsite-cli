use crate::api::{ApiClient, Credentials, UserResponse};
use crate::config::ActiveProfile;
use crate::output::Report;
use anyhow::Result;
use dialoguer::{Input, Password};
use serde::Serialize;
use std::io::{self, Write};

#[derive(Debug, Serialize)]
pub struct LoggedIn {
    pub profile: String,
    pub email: String,
}

impl Report for LoggedIn {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "Logged in as {}", self.email)
    }
}

#[derive(Debug, Serialize)]
pub struct LoggedOut {
    pub profile: String,
    pub action: &'static str,
}

impl Report for LoggedOut {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(
            out,
            "Logged out. (The key is still active server-side; revoke it in the dashboard.)"
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct CurrentUser(pub UserResponse);

impl Report for CurrentUser {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        let user = &self.0;
        writeln!(out, "Email: {}", user.email)?;
        if let Some(name) = &user.full_name {
            writeln!(out, "Name:  {}", name)?;
        }
        writeln!(out, "ID:    {}", user.id)
    }
}

pub async fn login(
    client: &ApiClient,
    profile: &ActiveProfile,
    token: Option<&str>,
) -> Result<LoggedIn> {
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
    profile.store_api_key(outcome.api_key)?;
    Ok(LoggedIn {
        profile: profile.name(),
        email: outcome.email,
    })
}

pub fn logout(profile: &ActiveProfile) -> Result<LoggedOut> {
    profile.forget_api_key()?;
    Ok(LoggedOut {
        profile: profile.name(),
        action: "logout",
    })
}

pub async fn whoami(client: &ApiClient) -> Result<CurrentUser> {
    Ok(CurrentUser(client.current_user().await?))
}
