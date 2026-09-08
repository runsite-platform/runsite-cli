use crate::api::{ApiClient, BulkEnvVarItem, BulkEnvVars, EnvVar, EnvVarList};
use crate::cli::OutputFormat;
use crate::output::{print_table, render};
use anyhow::{anyhow, Result};
use serde_json::Value;
use tabled::Tabled;
use uuid::Uuid;

#[derive(Tabled)]
struct EnvRow {
    #[tabled(rename = "KEY")]
    key: String,
    #[tabled(rename = "VALUE")]
    value: String,
}

impl From<&EnvVar> for EnvRow {
    fn from(v: &EnvVar) -> Self {
        Self {
            key: v.key.clone(),
            value: v.value.clone().unwrap_or_else(|| "***".to_string()),
        }
    }
}

async fn fetch(client: &ApiClient, service_id: Uuid) -> Result<Vec<EnvVar>> {
    let data: EnvVarList = client
        .get(&format!("/api/v1/web-services/{}/env", service_id))
        .await?;
    Ok(data.environment_variables)
}

pub async fn list(client: &ApiClient, service: Option<&str>, format: OutputFormat) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let vars = fetch(client, id).await?;
    render(format, &vars, || {
        print_table(vars.iter().map(EnvRow::from).collect())
    })
}

pub async fn set(client: &ApiClient, service: Option<&str>, vars: &[String]) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let existing = fetch(client, id).await?;

    let items: Vec<BulkEnvVarItem> = vars
        .iter()
        .map(|raw| {
            let (key, value) = raw
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid format '{}', expected KEY=VALUE", raw))?;
            let current = existing.iter().find(|v| v.key == key);
            Ok(BulkEnvVarItem {
                id: current.map(|v| v.id),
                key: key.to_string(),
                value: value.to_string(),
                is_secret: current.map(|v| v.is_secret).unwrap_or(true),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let body = BulkEnvVars { variables: items };
    let _: Value = client
        .put(&format!("/api/v1/web-services/{}/env/bulk", id), &body)
        .await?;

    println!("{} variable(s) set.", vars.len());
    Ok(())
}

pub async fn delete(client: &ApiClient, service: Option<&str>, keys: &[String]) -> Result<()> {
    let id = client.resolve_service_id(service).await?;
    let existing = fetch(client, id).await?;

    for key in keys {
        let var = existing
            .iter()
            .find(|v| &v.key == key)
            .ok_or_else(|| anyhow!("variable '{}' not found", key))?;

        client
            .delete_req(&format!("/api/v1/web-services/{}/env/{}", id, var.id))
            .await?;

        println!("Deleted: {}", key);
    }

    Ok(())
}
