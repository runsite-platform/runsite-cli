use crate::api::{ApiClient, ContainerLogs};
use crate::cli::OutputFormat;
use crate::output::render;
use anyhow::Result;

pub async fn tail(
    client: &ApiClient,
    service: Option<&str>,
    lines: u32,
    format: OutputFormat,
) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let data: ContainerLogs = client
        .get(&format!("/api/v1/web-services/{}/logs?tail={}", id, lines))
        .await?;

    render(format, &data, || {
        if data.logs.trim().is_empty() {
            println!("No logs available.");
        } else {
            print!("{}", data.logs);
            if !data.logs.ends_with('\n') {
                println!();
            }
        }
    })
}
