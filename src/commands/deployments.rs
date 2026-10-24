use crate::api::{ApiClient, Deployment, DeploymentList};
use crate::output::format::{colorize_status, format_time, or_placeholder, short_id, PLACEHOLDER};
use crate::output::{write_table, Report};
use anyhow::Result;
use serde::Serialize;
use std::io::{self, Write};
use tabled::Tabled;
use uuid::Uuid;

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
            id: short_id(&deployment.id),
            status,
            branch: or_placeholder(deployment.branch.as_deref()),
            commit: describe_commit(deployment),
            trigger: or_placeholder(deployment.trigger_type.as_deref()),
            created: format_time(&deployment.created_at),
        }
    }
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
        (true, true) => PLACEHOLDER.to_string(),
        (false, true) => sha,
        (true, false) => message,
        (false, false) => format!("{} {}", sha, message),
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Deployments(pub DeploymentList);

impl Report for Deployments {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        let list = &self.0;
        write_table(
            out,
            list.deployments.iter().map(DeploymentRow::from).collect(),
        )?;
        let total = usize::try_from(list.total).unwrap_or_default();
        if total > list.deployments.len() {
            writeln!(
                out,
                "Showing {} of {} deployments. Use --limit to see more.",
                list.deployments.len(),
                total
            )?;
        }
        Ok(())
    }
}

/// Serialized as the new deployment that redeploys `target_id`.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct RollbackStarted {
    pub deployment: Deployment,
    #[serde(skip)]
    pub target_id: Uuid,
}

impl Report for RollbackStarted {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(
            out,
            "Rolling back to deployment {}: new deployment {} is {}.",
            short_id(&self.target_id),
            short_id(&self.deployment.id),
            colorize_status(&self.deployment.status)
        )
    }
}

pub async fn list(client: &ApiClient, service: Option<&str>, limit: u8) -> Result<Deployments> {
    let service_id = client.resolve_service_id(service).await?;
    Ok(Deployments(
        client.deployments(service_id, u32::from(limit)).await?,
    ))
}

pub async fn rollback(
    client: &ApiClient,
    service: Option<&str>,
    deployment: &str,
) -> Result<RollbackStarted> {
    let service_id = client.resolve_service_id(service).await?;
    let target_id = client.resolve_deployment_id(service_id, deployment).await?;
    Ok(RollbackStarted {
        deployment: client.roll_back(service_id, target_id).await?,
        target_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn deployment(commit_sha: Option<&str>, commit_message: Option<&str>) -> Deployment {
        Deployment {
            id: Uuid::parse_str("3f2a1b4c-0000-4000-8000-000000000001").unwrap(),
            status: crate::api::DeploymentStatus::Running,
            branch: Some("main".to_string()),
            commit_sha: commit_sha.map(str::to_string),
            commit_message: commit_message.map(str::to_string),
            image_ref: None,
            is_live: false,
            trigger_type: None,
            error_message: None,
            rollback_from_deployment_id: None,
            build_logs: None,
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
    fn more_deployments_than_shown_points_to_limit() {
        let report = Deployments(DeploymentList {
            deployments: vec![deployment(None, None)],
            total: 5,
        });
        let text = crate::output::render_text(&report);
        assert!(text.contains("3f2a1b4c"));
        assert!(text.contains("Showing 1 of 5 deployments"));
    }

    #[test]
    fn an_image_deployment_without_commit_shows_a_dash() {
        assert_eq!(describe_commit(&deployment(None, None)), "—");
    }
}
