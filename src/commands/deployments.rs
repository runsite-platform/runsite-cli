use crate::api::{ApiClient, Deployment, DeploymentList};
use crate::cli::OutputFormat;
use crate::output::{format::colorize_status, format::format_time, print_table, render};
use anyhow::Result;
use tabled::Tabled;

const COMMIT_MESSAGE_WIDTH: usize = 50;

#[derive(Tabled)]
struct DeploymentRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "BRANCH")]
    branch: String,
    #[tabled(rename = "COMMIT")]
    commit: String,
    #[tabled(rename = "TRIGGER")]
    trigger: String,
    #[tabled(rename = "CREATED")]
    created: String,
}

impl From<&Deployment> for DeploymentRow {
    fn from(deployment: &Deployment) -> Self {
        let mut status = colorize_status(&deployment.status);
        if deployment.is_live {
            status.push_str(" (live)");
        }

        Self {
            id: short_id(deployment),
            status,
            branch: deployment.branch.clone().unwrap_or_else(|| "—".to_string()),
            commit: describe_commit(deployment),
            trigger: deployment
                .trigger_type
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            created: format_time(&deployment.created_at),
        }
    }
}

fn short_id(deployment: &Deployment) -> String {
    deployment.id.to_string()[..8].to_string()
}

fn describe_commit(deployment: &Deployment) -> String {
    let sha: String = deployment
        .commit_sha
        .as_deref()
        .map(|sha| sha.chars().take(7).collect())
        .unwrap_or_default();
    let first_line = deployment
        .commit_message
        .as_deref()
        .and_then(|message| message.lines().next())
        .unwrap_or_default();
    let message = if first_line.chars().count() > COMMIT_MESSAGE_WIDTH {
        let truncated: String = first_line.chars().take(COMMIT_MESSAGE_WIDTH - 1).collect();
        format!("{}…", truncated)
    } else {
        first_line.to_string()
    };

    match (sha.is_empty(), message.is_empty()) {
        (true, true) => "—".to_string(),
        (false, true) => sha,
        (true, false) => message,
        (false, false) => format!("{} {}", sha, message),
    }
}

pub async fn list(
    client: &ApiClient,
    service: Option<&str>,
    limit: u8,
    format: OutputFormat,
) -> Result<()> {
    let service_id = client.resolve_service_id(service).await?;
    let data: DeploymentList = client
        .get(&format!(
            "/api/v1/web-services/{}/deployments?limit={}",
            service_id, limit
        ))
        .await?;

    render(format, &data, || {
        print_table(data.deployments.iter().map(DeploymentRow::from).collect());
        let total = usize::try_from(data.total).unwrap_or_default();
        if total > data.deployments.len() {
            println!(
                "Showing {} of {} deployments. Use --limit to see more.",
                data.deployments.len(),
                total
            );
        }
    })
}

pub async fn rollback(
    client: &ApiClient,
    service: Option<&str>,
    deployment: &str,
    format: OutputFormat,
) -> Result<()> {
    let service_id = client.resolve_service_id(service).await?;
    let target_id = client.resolve_deployment_id(service_id, deployment).await?;

    let rollback: Deployment = client
        .post_empty(&format!(
            "/api/v1/web-services/{}/deployments/{}/rollback",
            service_id, target_id
        ))
        .await?;

    render(format, &rollback, || {
        println!(
            "Rolling back to deployment {}: new deployment {} is {}.",
            &target_id.to_string()[..8],
            short_id(&rollback),
            colorize_status(&rollback.status)
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn deployment(commit_sha: Option<&str>, commit_message: Option<&str>) -> Deployment {
        Deployment {
            id: Uuid::parse_str("3f2a1b4c-0000-4000-8000-000000000001").unwrap(),
            status: "running".to_string(),
            branch: Some("main".to_string()),
            commit_sha: commit_sha.map(str::to_string),
            commit_message: commit_message.map(str::to_string),
            is_live: false,
            trigger_type: None,
            error_message: None,
            rollback_from_deployment_id: None,
            created_at: None,
        }
    }

    #[test]
    fn a_commit_shows_the_short_sha_and_the_first_message_line() {
        let described = describe_commit(&deployment(
            Some("a1b2c3d4e5f6"),
            Some("fix: login\n\nlong body"),
        ));
        assert_eq!(described, "a1b2c3d fix: login");
    }

    #[test]
    fn a_long_commit_message_is_truncated() {
        let long_message = "x".repeat(80);
        let described = describe_commit(&deployment(None, Some(&long_message)));
        assert_eq!(described.chars().count(), COMMIT_MESSAGE_WIDTH);
        assert!(described.ends_with('…'));
    }

    #[test]
    fn an_image_deployment_without_commit_shows_a_dash() {
        assert_eq!(describe_commit(&deployment(None, None)), "—");
    }
}
