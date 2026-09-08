use crate::api::{ApiClient, Project, ProjectList};
use crate::cli::OutputFormat;
use crate::config::{self, Config};
use crate::output::{print_table, render};
use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};
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
    fn from(p: &Project) -> Self {
        Self {
            name: p.name.clone(),
            id: p.id.to_string(),
            description: p.description.clone().unwrap_or_default(),
        }
    }
}

pub async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let data: ProjectList = client.get("/api/v1/projects").await?;
    render(format, &data.projects, || {
        print_table(data.projects.iter().map(ProjectRow::from).collect())
    })
}

pub async fn use_project(
    client: &ApiClient,
    config: Arc<Mutex<Config>>,
    profile_name: &str,
    project_name_or_id: &str,
) -> Result<()> {
    let project_id = if Uuid::parse_str(project_name_or_id).is_ok() {
        project_name_or_id.to_string()
    } else {
        let data: ProjectList = client.get("/api/v1/projects").await?;
        let matched: Vec<_> = data
            .projects
            .iter()
            .filter(|p| p.name == project_name_or_id)
            .collect();

        match matched.len() {
            0 => return Err(anyhow!("project '{}' not found", project_name_or_id)),
            1 => matched[0].id.to_string(),
            _ => return Err(anyhow!("multiple projects named '{}'", project_name_or_id)),
        }
    };

    {
        let mut cfg = config.lock().unwrap();
        let profile = cfg.profiles.entry(profile_name.to_string()).or_default();
        profile.current_project_id = Some(project_id.clone());
        config::save(&cfg)?;
    }

    println!("Switched to project {}", project_name_or_id);
    Ok(())
}
