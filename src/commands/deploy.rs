use crate::api::{ApiClient, Deployment};
use crate::output::{format::short_id, Report};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::io::{self, Write};

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct DeploymentTriggered(pub Deployment);

impl Report for DeploymentTriggered {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "Deployment {} triggered.", short_id(&self.0.id))
    }
}

pub async fn trigger(
    client: &ApiClient,
    service: Option<&str>,
    watch: bool,
) -> Result<DeploymentTriggered> {
    let id = client.resolve_service_id(service).await?;

    // indicatif draws on stderr, so the spinner never mixes into JSON output.
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Triggering deployment...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let deployment = client.trigger_deployment(id).await;
    spinner.finish_and_clear();
    let deployment = deployment?;

    if watch {
        // Live build-log streaming uses a JWT-only WebSocket; not available under API-key auth.
        eprintln!(
            "Live build logs are not available with API-key authentication yet — \
             follow progress in the dashboard."
        );
    }

    Ok(DeploymentTriggered(deployment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::*;
    use crate::output::{render_json, render_text};
    use wiremock::MockServer;

    #[tokio::test]
    async fn the_report_is_the_new_deployment() {
        let server = MockServer::start().await;
        expect_request(
            &server,
            "POST",
            &format!("/api/v1/web-services/{SERVICE_ID}/deployments"),
            201,
            serde_json::json!({
                "id": "3f2a1b4c-0000-4000-8000-000000000009", "status": "pending",
                "branch": "main", "commit_sha": null, "commit_message": null,
                "trigger_type": "manual", "error_message": null,
                "rollback_from_deployment_id": null, "created_at": null
            }),
        )
        .await;

        let report = trigger(&client_for(&server), Some(SERVICE_ID), false)
            .await
            .unwrap();

        assert_eq!(render_json(&report)["status"], "pending");
        assert_eq!(render_text(&report), "Deployment 3f2a1b4c triggered.\n");
    }
}
