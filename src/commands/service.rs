use crate::api::{ApiClient, WebService, WebServiceDetail, WebServiceList};
use crate::cli::OutputFormat;
use crate::output::{format::colorize_status, format::format_time, print_table, render};
use anyhow::Result;
use serde_json::Value;
use tabled::Tabled;

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

pub async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let data: WebServiceList = client.get("/api/v1/web-services").await?;
    render(format, &data.web_services, || {
        print_table(data.web_services.iter().map(ServiceRow::from).collect())
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
