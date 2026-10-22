use crate::api::ApiClient;
use crate::config::ActiveProfile;
use crate::output::Report;
use anyhow::Result;
use serde::Serialize;
use std::io::{self, Write};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct Context {
    pub profile: String,
    pub api_url: String,
    pub authenticated: bool,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
}

impl Report for Context {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "Profile:  {}", self.profile)?;
        writeln!(out, "API URL:  {}", self.api_url)?;
        let auth = if self.authenticated {
            "authenticated"
        } else {
            "not authenticated"
        };
        writeln!(out, "Auth:     {}", auth)?;
        match (self.project_id, &self.project_name) {
            (Some(id), Some(name)) => writeln!(out, "Project:  {} ({})", name, id),
            (Some(id), None) => writeln!(out, "Project:  {}", id),
            (None, _) => Ok(()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiUrlSet {
    pub api_url: String,
}

impl Report for ApiUrlSet {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "API URL set to {}", self.api_url)
    }
}

pub async fn show(client: &ApiClient, profile: &ActiveProfile) -> Result<Context> {
    let project_id = profile.current_project_id();
    // The config only stores the id; the name needs a lookup, and staying
    // silent about it beats failing `context show` when the API is down.
    let project_name = match project_id {
        Some(id) => client.project_name(id).await,
        None => None,
    };
    Ok(Context {
        profile: profile.name(),
        api_url: profile.api_url(),
        authenticated: profile.is_authenticated(),
        project_id,
        project_name,
    })
}

pub fn set_url(profile: &ActiveProfile, url: &str) -> Result<ApiUrlSet> {
    profile.set_api_url(url)?;
    Ok(ApiUrlSet {
        api_url: profile.api_url(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::*;
    use crate::output::{render_json, render_text};
    use wiremock::MockServer;

    #[tokio::test]
    async fn show_names_the_selected_project() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/projects",
            serde_json::json!({"projects": [{"id": PROJECT_ID, "name": "shop", "description": null}], "total": 1}),
        )
        .await;
        let (client, profile) = session_in_project(&server, Some(PROJECT_ID));

        let context = show(&client, &profile).await.unwrap();

        assert_eq!(context.project_name.as_deref(), Some("shop"));
        assert_eq!(render_json(&context)["authenticated"], true);
        assert!(render_text(&context).contains(&format!("Project:  shop ({PROJECT_ID})")));
    }

    #[tokio::test]
    async fn show_survives_an_unreachable_api() {
        let server = MockServer::start().await;
        let (client, profile) = session_in_project(&server, Some(PROJECT_ID));

        let context = show(&client, &profile).await.unwrap();

        assert_eq!(context.project_name, None);
        assert!(render_text(&context).contains(&format!("Project:  {PROJECT_ID}\n")));
    }
}
