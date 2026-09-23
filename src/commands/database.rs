use crate::api::{
    ApiClient, Connection, ConnectionCreate, DatabaseCreate, DatabaseStatus, ManagedDatabase,
    ManagedDatabaseList, ServicePlanList,
};
use crate::cli::{DbEngine, OutputFormat, PlanServiceType};
use crate::output::{format::colorize_status, print_table, render};
use anyhow::{anyhow, bail, Result};
use dialoguer::Confirm;
use tabled::Tabled;
use uuid::Uuid;

#[derive(Tabled)]
struct DatabaseRow {
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "ENGINE")]
    engine: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "PLAN")]
    plan: String,
    #[tabled(rename = "ID")]
    id: String,
}

impl From<&ManagedDatabase> for DatabaseRow {
    fn from(database: &ManagedDatabase) -> Self {
        Self {
            name: database.name.clone(),
            engine: database.kind.clone(),
            status: colorize_status(&database.status),
            plan: database
                .plan_name
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            id: database.id.to_string(),
        }
    }
}

#[derive(Tabled)]
struct PlanRow {
    #[tabled(rename = "SLUG")]
    slug: String,
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "PER MONTH")]
    price: String,
    #[tabled(rename = "CPU")]
    cpu: String,
    #[tabled(rename = "MEMORY MB")]
    memory: String,
    #[tabled(rename = "STORAGE GB")]
    storage: String,
}

fn plan_api_type(service_type: PlanServiceType) -> &'static str {
    match service_type {
        PlanServiceType::Web => "web_service",
        PlanServiceType::Postgres => "postgresql",
        PlanServiceType::Redis => "redis",
    }
}

fn optional_number<T: ToString>(value: Option<T>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "—".to_string())
}

async fn fetch_databases(client: &ApiClient) -> Result<Vec<ManagedDatabase>> {
    let data: ManagedDatabaseList = client.get("/api/v1/databases?limit=100").await?;
    Ok(data.databases)
}

async fn resolve_plan_id(
    client: &ApiClient,
    service_type: PlanServiceType,
    plan: &str,
) -> Result<Uuid> {
    if let Ok(id) = Uuid::parse_str(plan) {
        return Ok(id);
    }
    let data: ServicePlanList = client
        .get(&format!(
            "/api/v1/plans?service_type={}",
            plan_api_type(service_type)
        ))
        .await?;
    data.plans
        .iter()
        .find(|candidate| candidate.slug == plan)
        .map(|candidate| candidate.id)
        .ok_or_else(|| {
            let slugs: Vec<_> = data
                .plans
                .iter()
                .map(|candidate| candidate.slug.as_str())
                .collect();
            anyhow!("plan '{}' not found. Available: {}", plan, slugs.join(", "))
        })
}

async fn resolve_database(client: &ApiClient, name_or_id: &str) -> Result<ManagedDatabase> {
    let by_id = Uuid::parse_str(name_or_id).ok();
    let mut matched: Vec<ManagedDatabase> = fetch_databases(client)
        .await?
        .into_iter()
        .filter(|database| Some(database.id) == by_id || database.name == name_or_id)
        .collect();
    match matched.len() {
        0 => Err(anyhow!("database '{}' not found", name_or_id)),
        1 => Ok(matched.remove(0)),
        _ => Err(anyhow!(
            "multiple databases named '{}'. Use the ID instead",
            name_or_id
        )),
    }
}

pub async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let databases = fetch_databases(client).await?;
    render(format, &databases, || {
        print_table(databases.iter().map(DatabaseRow::from).collect())
    })
}

pub async fn plans(
    client: &ApiClient,
    service_type: PlanServiceType,
    format: OutputFormat,
) -> Result<()> {
    let data: ServicePlanList = client
        .get(&format!(
            "/api/v1/plans?service_type={}",
            plan_api_type(service_type)
        ))
        .await?;
    render(format, &data.plans, || {
        print_table(
            data.plans
                .iter()
                .map(|plan| PlanRow {
                    slug: plan.slug.clone(),
                    name: plan.name.clone(),
                    price: format!("€{:.2}", plan.monthly_price_cents as f64 / 100.0),
                    cpu: optional_number(plan.cpu_limit),
                    memory: optional_number(plan.memory_limit_mb),
                    storage: optional_number(plan.storage_limit_gb),
                })
                .collect(),
        )
    })
}

pub async fn create(
    client: &ApiClient,
    name: &str,
    plan: &str,
    engine: DbEngine,
    project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let (plan_type, path) = match engine {
        DbEngine::Postgres => (PlanServiceType::Postgres, "/api/v1/databases"),
        DbEngine::Redis => (PlanServiceType::Redis, "/api/v1/redis"),
    };
    let project_id = match project {
        Some(project) => Some(client.resolve_project_id(project).await?),
        None => client
            .current_project_id()
            .and_then(|id| Uuid::parse_str(&id).ok()),
    };
    let body = DatabaseCreate {
        name: name.to_string(),
        plan_id: resolve_plan_id(client, plan_type, plan).await?,
        project_id,
    };
    let database: ManagedDatabase = client.post(path, &body).await?;
    render(format, &database, || {
        println!(
            "Created {} {} ({}), status {}.",
            database.kind,
            database.name,
            database.id,
            colorize_status(&database.status)
        );
        println!(
            "Connect it to a service with: runsite db connect {} --service <service>",
            database.id
        );
    })
}

pub async fn connect(
    client: &ApiClient,
    database: &str,
    service: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let target = resolve_database(client, database).await?;
    let service_id = client.resolve_service_id(service).await?;
    let target_type = if target.kind == "valkey" {
        "redis"
    } else {
        "database"
    };
    let body = ConnectionCreate {
        target_type: target_type.to_string(),
        target_id: target.id,
    };
    let connection: Connection = client
        .post(
            &format!("/api/v1/web-services/{}/connections", service_id),
            &body,
        )
        .await?;
    render(format, &connection, || {
        println!(
            "Connected {} as secret env var {}. Deploy or restart the service to pick it up.",
            target.name, connection.env_var_key
        );
    })
}

pub async fn change_state(
    client: &ApiClient,
    database: &str,
    action: &str,
    format: OutputFormat,
) -> Result<()> {
    let target = resolve_database(client, database).await?;
    let result: DatabaseStatus = client
        .post_empty(&format!("/api/v1/databases/{}/{}", target.id, action))
        .await?;
    render(format, &result, || {
        println!(
            "{} {}: {} accepted, status {}.",
            result.kind,
            target.name,
            action,
            colorize_status(&result.status)
        );
    })
}

pub async fn delete(client: &ApiClient, database: &str, skip_confirmation: bool) -> Result<()> {
    let target = resolve_database(client, database).await?;
    if !skip_confirmation {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "Delete {} {} and all its data? This cannot be undone",
                target.kind, target.name
            ))
            .default(false)
            .interact()?;
        if !confirmed {
            bail!("aborted");
        }
    }
    client
        .delete_req(&format!("/api/v1/databases/{}", target.id))
        .await?;
    println!("Deleted {} {}.", target.kind, target.name);
    Ok(())
}
