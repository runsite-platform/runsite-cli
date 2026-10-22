use crate::api::{ApiClient, DatabaseCommand, DatabaseCreate, DatabaseEngine, ManagedDatabase};
use crate::cli::{DbEngine, OutputFormat, PlanServiceType};
use crate::output::{format::colorize_status, print_table, render};
use anyhow::{bail, Result};
use dialoguer::Confirm;
use tabled::Tabled;

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

pub async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let databases = client.databases().await?;
    render(format, &databases, || {
        print_table(databases.iter().map(DatabaseRow::from).collect())
    })
}

pub async fn plans(
    client: &ApiClient,
    service_type: PlanServiceType,
    format: OutputFormat,
) -> Result<()> {
    let plans = client.plans(plan_api_type(service_type)).await?;
    render(format, &plans, || {
        print_table(
            plans
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
    let (plan_type, api_engine) = match engine {
        DbEngine::Postgres => (PlanServiceType::Postgres, DatabaseEngine::Postgres),
        DbEngine::Redis => (PlanServiceType::Redis, DatabaseEngine::Redis),
    };
    let body = DatabaseCreate {
        name: name.to_string(),
        plan_id: client
            .resolve_plan_id(plan_api_type(plan_type), plan)
            .await?,
        project_id: client.target_project_id(project).await?,
    };
    let database: ManagedDatabase = client.create_database(api_engine, &body).await?;
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
    let target = client.resolve_database(database).await?;
    let service_id = client.resolve_service_id(service).await?;
    let connection = client.connect_database(service_id, &target).await?;
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
    command: DatabaseCommand,
    format: OutputFormat,
) -> Result<()> {
    let target = client.resolve_database(database).await?;
    let result = client.database_command(target.id, command).await?;
    render(format, &result, || {
        println!(
            "{} {}: {} accepted, status {}.",
            result.kind,
            target.name,
            command.route(),
            colorize_status(&result.status)
        );
    })
}

pub async fn delete(client: &ApiClient, database: &str, skip_confirmation: bool) -> Result<()> {
    let target = client.resolve_database(database).await?;
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
    client.delete_database(target.id).await?;
    println!("Deleted {} {}.", target.kind, target.name);
    Ok(())
}
