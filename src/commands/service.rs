use super::parse_key_value;
use crate::api::{
    ApiClient, EnvVarCreate, ProjectScope, Service, ServiceCommand, ServiceRuntimeStatus,
    WebServiceCreate,
};
use crate::cli::ServiceCreateArgs;
use crate::output::format::{colorize_status, format_time, or_placeholder};
use crate::output::{write_table, Report};
use anyhow::{bail, Result};
use serde::Serialize;
use std::io::{self, Write};
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

impl From<&Service> for ServiceRow {
    fn from(service: &Service) -> Self {
        Self {
            name: service.name.clone(),
            status: colorize_status(&service.status),
            url: or_placeholder(service.url.as_deref()),
            updated: format_time(&service.updated_at),
        }
    }
}

/// Serialized as a bare array, as `service list --output json` always was.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Services {
    pub services: Vec<Service>,
    #[serde(skip)]
    pub filtered_by_project: bool,
}

impl Report for Services {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        write_table(out, self.services.iter().map(ServiceRow::from).collect())?;
        if self.services.is_empty() && self.filtered_by_project {
            writeln!(
                out,
                "No services in the current project. Use --all to list every service."
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ServiceCreated(pub Service);

impl Report for ServiceCreated {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        let created = &self.0;
        writeln!(out, "Service {} created.", created.name)?;
        writeln!(out, "ID:     {}", created.id)?;
        writeln!(out, "Status: {}", colorize_status(&created.status))?;
        if let Some(url) = &created.url {
            writeln!(out, "URL:    {}", url)?;
        }
        writeln!(out, "\nDeploy it with: runsite deploy {}", created.id)
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct ServiceStatus(pub Service);

impl Report for ServiceStatus {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        let detail = &self.0;
        writeln!(out, "Name:      {}", detail.name)?;
        writeln!(out, "Status:    {}", colorize_status(&detail.status))?;
        writeln!(out, "URL:       {}", or_placeholder(detail.url.as_deref()))?;
        if let Some(repo) = &detail.github_repo_full_name {
            let branch = or_placeholder(detail.github_branch.as_deref());
            writeln!(out, "Repo:      {} ({})", repo, branch)?;
        }
        if let (Some(min), Some(max)) = (detail.min_instances, detail.max_instances) {
            writeln!(out, "Instances: {}–{}", min, max)?;
        }
        writeln!(out, "Updated:   {}", format_time(&detail.updated_at))?;
        writeln!(out, "ID:        {}", detail.id)
    }
}

#[derive(Debug, Serialize)]
pub struct ServiceActionAccepted {
    pub service_id: Uuid,
    pub action: &'static str,
    pub status: crate::api::ServiceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Report for ServiceActionAccepted {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        let summary = match self.action {
            "start" => "Service starting.",
            "stop" => "Service stopped.",
            _ => "Service restarted.",
        };
        writeln!(out, "{}", summary)
    }
}

pub async fn list(client: &ApiClient, all: bool) -> Result<Services> {
    let project_id = if all {
        None
    } else {
        client.current_project_id()
    };
    Ok(Services {
        services: client.services(ProjectScope::selected(project_id)).await?,
        filtered_by_project: project_id.is_some(),
    })
}

pub async fn create(client: &ApiClient, args: &ServiceCreateArgs) -> Result<ServiceCreated> {
    if let (Some(min), Some(max)) = (args.min_instances, args.max_instances) {
        if min > max {
            bail!(
                "--min-instances ({}) cannot exceed --max-instances ({})",
                min,
                max
            );
        }
    }

    let env_vars = args
        .env_vars
        .iter()
        .map(|raw| {
            let (key, value) = parse_key_value(raw)?;
            Ok(EnvVarCreate {
                key: key.to_string(),
                value: value.to_string(),
                is_secret: true,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let project_id = client.target_project_id(args.project.as_deref()).await?;

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

    Ok(ServiceCreated(client.create_service(&body).await?))
}

pub async fn status(client: &ApiClient, service: Option<&str>) -> Result<ServiceStatus> {
    let id = client.resolve_service_id(service).await?;
    Ok(ServiceStatus(client.service(id).await?))
}

pub async fn run_command(
    client: &ApiClient,
    service: Option<&str>,
    command: ServiceCommand,
) -> Result<ServiceActionAccepted> {
    let id = client.resolve_service_id(service).await?;
    let ServiceRuntimeStatus { status, message } = client.service_command(id, command).await?;
    Ok(ServiceActionAccepted {
        service_id: id,
        action: command.route(),
        status,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::*;
    use crate::output::{render_json, render_text};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn serve_one_service_in(server: &MockServer, project_id: &str) {
        serve(
            server,
            "/api/v1/web-services",
            serde_json::json!({
                "web_services": [{
                    "id": SERVICE_ID, "name": "api", "status": "running",
                    "url": null, "project_id": project_id
                }],
                "total": 1
            }),
        )
        .await;
    }

    #[tokio::test]
    async fn list_json_stays_a_bare_array() {
        let server = MockServer::start().await;
        serve_one_service_in(&server, PROJECT_ID).await;

        let report = list(&client_for(&server), false).await.unwrap();

        assert_eq!(render_json(&report)[0]["name"], "api");
    }

    #[tokio::test]
    async fn an_empty_project_points_to_all() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/web-services"))
            .and(query_param("project_id", PROJECT_ID))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"web_services": [], "total": 0})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let report = list(&client_in_project(&server, Some(PROJECT_ID)), false)
            .await
            .unwrap();

        assert!(report.services.is_empty());
        assert!(render_text(&report).contains("Use --all"));
    }

    #[tokio::test]
    async fn a_restart_reports_the_new_runtime_status() {
        let server = MockServer::start().await;
        expect_request(
            &server,
            "POST",
            &format!("/api/v1/web-services/{SERVICE_ID}/restart"),
            200,
            serde_json::json!({"status": "deploying", "container_id": null, "host_port": null}),
        )
        .await;

        let report = run_command(
            &client_for(&server),
            Some(SERVICE_ID),
            ServiceCommand::Restart,
        )
        .await
        .unwrap();

        assert_eq!(
            render_json(&report),
            serde_json::json!({"service_id": SERVICE_ID, "action": "restart", "status": "deploying"})
        );
        assert_eq!(render_text(&report), "Service restarted.\n");
    }

    #[tokio::test]
    async fn min_instances_above_max_is_rejected_before_any_request() {
        use clap::Parser;
        let server = MockServer::start().await;
        let cli = crate::cli::Cli::try_parse_from([
            "runsite",
            "service",
            "create",
            "api",
            "--min-instances",
            "3",
            "--max-instances",
            "1",
            "--image",
            "nginx:1.27",
        ])
        .unwrap();
        let Some(crate::cli::Commands::Service(crate::cli::ServiceCommands::Create(args))) =
            cli.command
        else {
            panic!("expected service create");
        };

        let error = create(&client_for(&server), &args).await.unwrap_err();

        assert!(error.to_string().contains("--min-instances"));
    }
}
