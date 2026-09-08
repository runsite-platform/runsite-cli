use crate::api::{ApiClient, Deployment};
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

pub async fn trigger(client: &ApiClient, service: Option<&str>, watch: bool) -> Result<()> {
    let id = client.resolve_service_id(service).await?;

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Triggering deployment...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let deployment: Deployment = client
        .post(
            &format!("/api/v1/web-services/{}/deployments", id),
            &serde_json::json!({}),
        )
        .await?;

    spinner.finish_and_clear();
    println!("Deployment {} triggered.", &deployment.id.to_string()[..8]);

    if watch {
        // Live build-log streaming uses a JWT-only WebSocket; not available under API-key auth.
        println!(
            "Live build logs are not available with API-key authentication yet — \
             follow progress in the dashboard."
        );
    }

    Ok(())
}
