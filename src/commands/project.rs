use crate::api::{ApiClient, Project, ProjectCreate, ProjectList};
use crate::cli::OutputFormat;
use crate::config::{self, Config};
use crate::output::{print_table, render};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use tabled::Tabled;

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

pub async fn create(
    client: &ApiClient,
    name: &str,
    description: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let body = ProjectCreate {
        name: name.to_string(),
        description,
    };
    let project: Project = client.post("/api/v1/projects", &body).await?;
    render(format, &project, || {
        println!("Created project {} ({})", project.name, project.id);
        println!("Select it with: runsite project use {}", project.id);
    })
}

pub async fn use_project(
    client: &ApiClient,
    config: Arc<Mutex<Config>>,
    profile_name: &str,
    project_name_or_id: &str,
) -> Result<()> {
    let project_id = client.resolve_project_id(project_name_or_id).await?;

    {
        let mut cfg = config.lock().unwrap();
        let profile = cfg.profiles.entry(profile_name.to_string()).or_default();
        profile.current_project_id = Some(project_id.to_string());
        config::save_profile(&cfg, profile_name)?;
    }

    println!("Switched to project {}", project_name_or_id);
    Ok(())
}

pub fn unset_project(config: Arc<Mutex<Config>>, profile_name: &str) -> Result<()> {
    {
        let mut cfg = config.lock().unwrap();
        let profile = cfg.profiles.entry(profile_name.to_string()).or_default();
        if profile.current_project_id.take().is_none() {
            println!("No project selected.");
            return Ok(());
        }
        config::save_profile(&cfg, profile_name)?;
    }

    println!("Project context cleared.");
    Ok(())
}
