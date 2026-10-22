use crate::api::{ApiClient, Project, ProjectCreate};
use crate::config::ActiveProfile;
use crate::output::{write_table, Report};
use anyhow::Result;
use serde::Serialize;
use std::io::{self, Write};
use tabled::Tabled;
use uuid::Uuid;

#[derive(Tabled)]
struct ProjectRow {
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "DESCRIPTION")]
    description: String,
}

impl From<&Project> for ProjectRow {
    fn from(project: &Project) -> Self {
        Self {
            name: project.name.clone(),
            id: project.id.to_string(),
            description: project.description.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Projects(pub Vec<Project>);

impl Report for Projects {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        write_table(out, self.0.iter().map(ProjectRow::from).collect())
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ProjectCreated(pub Project);

impl Report for ProjectCreated {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "Created project {} ({})", self.0.name, self.0.id)?;
        writeln!(out, "Select it with: runsite project use {}", self.0.id)
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectSelected {
    pub project_id: Uuid,
    pub action: &'static str,
    #[serde(skip)]
    pub reference: String,
}

impl Report for ProjectSelected {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "Switched to project {}", self.reference)
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectUnset {
    pub action: &'static str,
    /// False when no project was selected in the first place.
    pub cleared: bool,
}

impl Report for ProjectUnset {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        if self.cleared {
            writeln!(out, "Project context cleared.")
        } else {
            writeln!(out, "No project selected.")
        }
    }
}

pub async fn list(client: &ApiClient) -> Result<Projects> {
    Ok(Projects(client.projects().await?))
}

pub async fn create(
    client: &ApiClient,
    name: &str,
    description: Option<String>,
) -> Result<ProjectCreated> {
    let body = ProjectCreate {
        name: name.to_string(),
        description,
    };
    Ok(ProjectCreated(client.create_project(&body).await?))
}

pub async fn use_project(
    client: &ApiClient,
    profile: &ActiveProfile,
    project_name_or_id: &str,
) -> Result<ProjectSelected> {
    let project_id = client.resolve_project_id(project_name_or_id).await?;
    profile.select_project(project_id)?;
    Ok(ProjectSelected {
        project_id,
        action: "use",
        reference: project_name_or_id.to_string(),
    })
}

pub fn unset_project(profile: &ActiveProfile) -> Result<ProjectUnset> {
    Ok(ProjectUnset {
        action: "unset",
        cleared: profile.clear_project()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::*;
    use crate::output::{render_json, render_text};
    use wiremock::MockServer;

    #[tokio::test]
    async fn use_selects_the_project_by_name() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/projects",
            serde_json::json!({"projects": [{"id": PROJECT_ID, "name": "shop", "description": null}], "total": 1}),
        )
        .await;
        let (client, profile) = session_in_project(&server, None);

        let report = use_project(&client, &profile, "shop").await.unwrap();

        assert_eq!(profile.current_project_id(), Some(uuid(PROJECT_ID)));
        assert_eq!(
            render_json(&report),
            serde_json::json!({"project_id": PROJECT_ID, "action": "use"})
        );
        assert_eq!(render_text(&report), "Switched to project shop\n");
    }

    #[tokio::test]
    async fn unset_says_whether_anything_was_selected() {
        let server = MockServer::start().await;
        let (_, profile) = session_in_project(&server, Some(PROJECT_ID));

        assert!(unset_project(&profile).unwrap().cleared);
        let second = unset_project(&profile).unwrap();
        assert!(!second.cleared);
        assert_eq!(render_text(&second), "No project selected.\n");
    }
}
