use crate::api::{
    belongs_to_project, ApiClient, EnvVarCreate, WebService, WebServiceCreate, WebServiceDetail,
    WebServiceList,
};
use crate::cli::{OutputFormat, ServiceCreateArgs};
use crate::output::{format::colorize_status, format::format_time, print_table, render};
use anyhow::{anyhow, Result};
use serde_json::Value;
use tabled::Tabled;
use uuid::Uuid;

#[derive(Tabled)]
struct ServiceRow {
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "URL")]
    url: String,
    #[tabled(rename = "UPDATED")]
    updated: String,
}

impl From<&WebService> for ServiceRow {
    fn from(s: &WebService) -> Self {
        Self {
            name: s.name.clone(),
            status: colorize_status(s.status.as_deref().unwrap_or("unknown")),
            url: s.url.clone().unwrap_or_else(|| "—".to_string()),
            updated: format_time(&s.updated_at),
        }
    }
}

pub async fn list(client: &ApiClient, all: bool, format: OutputFormat) -> Result<()> {
    let data: WebServiceList = client.get("/api/v1/web-services").await?;
    let project_id = if all {
        None
    } else {
        client.current_project_id()
    };
    let services: Vec<&WebService> = data
        .web_services
        .iter()
        .filter(|s| belongs_to_project(s, project_id.as_deref()))
        .collect();

    render(format, &services, || {
        print_table(services.iter().map(|s| ServiceRow::from(*s)).collect());
        if services.is_empty() && project_id.is_some() {
            println!("No services in the current project. Use --all to list every service.");
        }
    })
}

pub async fn create(
    client: &ApiClient,
    args: &ServiceCreateArgs,
    format: OutputFormat,
) -> Result<()> {
    if let (Some(min), Some(max)) = (args.min_instances, args.max_instances) {
        if min > max {
            return Err(anyhow!(
                "--min-instances ({}) cannot exceed --max-instances ({})",
                min,
                max
            ));
        }
    }

    let env_vars = args
        .env_vars
        .iter()
        .map(|raw| {
            let (key, value) = raw
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid --env '{}', expected KEY=VALUE", raw))?;
            Ok(EnvVarCreate {
                key: key.to_string(),
                value: value.to_string(),
                is_secret: true,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let project_id = match &args.project {
        Some(project) => Some(client.resolve_project_id(project).await?),
        None => client
            .current_project_id()
            .and_then(|id| Uuid::parse_str(&id).ok()),
    };

    let body = WebServiceCreate {
        name: args.name.clone(),
        project_id,
        project_type: args.service_type.map(|t| t.as_api_value().to_string()),
        source_type: Some(if args.image.is_some() { "image" } else { "git" }.to_string()),
        git_repository_url: args.repo.clone(),
        github_branch: args.branch.clone(),
        image_ref: args.image.clone(),
        port: args.port,
        build_command: args.build_command.clone(),
        start_command: args.start_command.clone(),
        dockerfile_path: args.dockerfile.clone(),
        root_directory: args.root_dir.clone(),
        min_instances: args.min_instances,
        max_instances: args.max_instances,
        auto_deploy: args.auto_deploy,
        env_vars,
    };

    let created: WebServiceDetail = client.post("/api/v1/web-services", &body).await?;

    render(format, &created, || {
        println!("Service {} created.", created.name);
        println!("ID:     {}", created.id);
        println!("Status: {}", colorize_status(&created.status));
        if let Some(url) = &created.url {
            println!("URL:    {}", url);
        }
        println!("\nDeploy it with: runsite deploy {}", created.id);
    })
}

pub async fn status(client: &ApiClient, service: Option<&str>, format: OutputFormat) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let detail: WebServiceDetail = client.get(&format!("/api/v1/web-services/{}", id)).await?;

    render(format, &detail, || {
        println!("Name:      {}", detail.name);
        println!("Status:    {}", colorize_status(&detail.status));
        println!("URL:       {}", detail.url.as_deref().unwrap_or("—"));
        if let Some(repo) = &detail.github_repo_full_name {
            let branch = detail.github_branch.as_deref().unwrap_or("—");
            println!("Repo:      {} ({})", repo, branch);
        }
        if let (Some(min), Some(max)) = (detail.min_instances, detail.max_instances) {
            println!("Instances: {}–{}", min, max);
        }
        println!("Updated:   {}", format_time(&detail.updated_at));
        println!("ID:        {}", detail.id);
    })
}

pub async fn start(client: &ApiClient, service: Option<&str>) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let _: Value = client
        .post_empty(&format!("/api/v1/web-services/{}/deploy", id))
        .await?;
    println!("Service starting.");
    Ok(())
}

pub async fn stop(client: &ApiClient, service: Option<&str>) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let _: Value = client
        .post_empty(&format!("/api/v1/web-services/{}/stop", id))
        .await?;
    println!("Service stopped.");
    Ok(())
}

pub async fn restart(client: &ApiClient, service: Option<&str>) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let _: Value = client
        .post_empty(&format!("/api/v1/web-services/{}/restart", id))
        .await?;
    println!("Service restarted.");
    Ok(())
}
