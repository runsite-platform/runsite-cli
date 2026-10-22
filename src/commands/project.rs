use crate::api::{ApiClient, Project, ProjectCreate};
use crate::cli::OutputFormat;
use crate::config::ActiveProfile;
use crate::output::{print_table, render};
use anyhow::Result;
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
    let projects: Vec<Project> = client.projects().await?;
    render(format, &projects, || {
        print_table(projects.iter().map(ProjectRow::from).collect())
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
    let project = client.create_project(&body).await?;
    render(format, &project, || {
        println!("Created project {} ({})", project.name, project.id);
        println!("Select it with: runsite project use {}", project.id);
    })
}

pub async fn use_project(
    client: &ApiClient,
    profile: &ActiveProfile,
    project_name_or_id: &str,
) -> Result<()> {
    let project_id = client.resolve_project_id(project_name_or_id).await?;
    profile.select_project(project_id)?;
    println!("Switched to project {}", project_name_or_id);
    Ok(())
}

pub fn unset_project(profile: &ActiveProfile) -> Result<()> {
    if profile.clear_project()? {
        println!("Project context cleared.");
    } else {
        println!("No project selected.");
    }
    Ok(())
}
