use crate::api::{
    belongs_to_project, ApiClient, Connection, DatabaseCommand, DatabaseCreate, DatabaseEngine,
    DatabaseRuntimeStatus, ManagedDatabase, ServicePlan,
};
use crate::cli::{DbEngine, PlanServiceType};
use crate::output::format::{colorize_status, or_placeholder, PLACEHOLDER};
use crate::output::{write_table, Report};
use anyhow::{bail, Result};
use dialoguer::Confirm;
use serde::Serialize;
use std::io::{self, Write};
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
            plan: or_placeholder(database.plan_name.as_deref()),
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

impl From<&ServicePlan> for PlanRow {
    fn from(plan: &ServicePlan) -> Self {
        Self {
            slug: plan.slug.clone(),
            name: plan.name.clone(),
            price: format!("€{:.2}", plan.monthly_price_cents as f64 / 100.0),
            cpu: optional_number(plan.cpu_limit),
            memory: optional_number(plan.memory_limit_mb),
            storage: optional_number(plan.storage_limit_gb),
        }
    }
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
        .map(|number| number.to_string())
        .unwrap_or_else(|| PLACEHOLDER.to_string())
}

/// Serialized as a bare array, as `db list --output json` always was.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Databases {
    pub databases: Vec<ManagedDatabase>,
    #[serde(skip)]
    pub filtered_by_project: bool,
}

impl Report for Databases {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        write_table(out, self.databases.iter().map(DatabaseRow::from).collect())?;
        if self.databases.is_empty() && self.filtered_by_project {
            writeln!(
                out,
                "No databases in the current project. Use --all to list every database."
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Plans(pub Vec<ServicePlan>);

impl Report for Plans {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        write_table(out, self.0.iter().map(PlanRow::from).collect())
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct DatabaseCreated(pub ManagedDatabase);

impl Report for DatabaseCreated {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        let database = &self.0;
        writeln!(
            out,
            "Created {} {} ({}), status {}.",
            database.kind,
            database.name,
            database.id,
            colorize_status(&database.status)
        )?;
        writeln!(
            out,
            "Connect it to a service with: runsite db connect {} --service <service>",
            database.id
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct DatabaseConnected {
    pub connection: Connection,
    #[serde(skip)]
    pub database_name: String,
}

impl Report for DatabaseConnected {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(
            out,
            "Connected {} as secret env var {}. Deploy or restart the service to pick it up.",
            self.database_name, self.connection.env_var_key
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct DatabaseStateChanged {
    pub status: DatabaseRuntimeStatus,
    #[serde(skip)]
    pub database_name: String,
    #[serde(skip)]
    pub action: &'static str,
}

impl Report for DatabaseStateChanged {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(
            out,
            "{} {}: {} accepted, status {}.",
            self.status.kind,
            self.database_name,
            self.action,
            colorize_status(&self.status.status)
        )
    }
}

#[derive(Debug, Serialize)]
pub struct DatabaseDeleted {
    pub database_id: Uuid,
    pub action: &'static str,
    pub kind: String,
    pub name: String,
}

impl Report for DatabaseDeleted {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "Deleted {} {}.", self.kind, self.name)
    }
}

pub async fn list(client: &ApiClient, all: bool) -> Result<Databases> {
    let project_id = if all {
        None
    } else {
        client.current_project_id()
    };
    Ok(Databases {
        databases: client
            .databases()
            .await?
            .into_iter()
            .filter(|database| belongs_to_project(database, project_id))
            .collect(),
        filtered_by_project: project_id.is_some(),
    })
}

pub async fn plans(client: &ApiClient, service_type: PlanServiceType) -> Result<Plans> {
    Ok(Plans(client.plans(plan_api_type(service_type)).await?))
}

pub async fn create(
    client: &ApiClient,
    name: &str,
    plan: &str,
    engine: DbEngine,
    project: Option<&str>,
) -> Result<DatabaseCreated> {
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
    Ok(DatabaseCreated(
        client.create_database(api_engine, &body).await?,
    ))
}

pub async fn connect(
    client: &ApiClient,
    database: &str,
    service: Option<&str>,
) -> Result<DatabaseConnected> {
    let target = client.resolve_database(database).await?;
    let service_id = client.resolve_service_id(service).await?;
    Ok(DatabaseConnected {
        connection: client.connect_database(service_id, &target).await?,
        database_name: target.name,
    })
}

pub async fn change_state(
    client: &ApiClient,
    database: &str,
    command: DatabaseCommand,
) -> Result<DatabaseStateChanged> {
    let target = client.resolve_database(database).await?;
    Ok(DatabaseStateChanged {
        status: client.database_command(target.id, command).await?,
        database_name: target.name,
        action: command.route(),
    })
}

pub async fn delete(
    client: &ApiClient,
    database: &str,
    skip_confirmation: bool,
) -> Result<DatabaseDeleted> {
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
    Ok(DatabaseDeleted {
        database_id: target.id,
        action: "delete",
        kind: target.kind,
        name: target.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::*;
    use crate::output::{render_json, render_text};
    use wiremock::MockServer;

    #[tokio::test]
    async fn delete_reports_what_was_deleted() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/databases",
            serde_json::json!({"databases": [{
                "id": DATABASE_ID, "kind": "postgres", "name": "main", "status": "running",
                "plan_name": null, "storage_limit_gb": null
            }]}),
        )
        .await;
        expect_request(
            &server,
            "DELETE",
            &format!("/api/v1/databases/{DATABASE_ID}"),
            204,
            serde_json::json!({}),
        )
        .await;

        let report = delete(&client_for(&server), "main", true).await.unwrap();

        assert_eq!(
            render_json(&report),
            serde_json::json!({
                "database_id": DATABASE_ID, "action": "delete", "kind": "postgres", "name": "main"
            })
        );
        assert_eq!(render_text(&report), "Deleted postgres main.\n");
    }

    const OTHER_PROJECT_ID: &str = "ca83c4a9-1517-4fe3-93aa-000000000009";

    async fn serve_databases_in_two_projects(server: &MockServer) {
        serve(
            server,
            "/api/v1/databases",
            serde_json::json!({"databases": [
                {"id": DATABASE_ID, "kind": "postgresql", "name": "main", "status": "running",
                 "plan_name": null, "storage_limit_gb": null, "project_id": PROJECT_ID},
                {"id": "3f2a1b4c-0000-4000-8000-000000000005", "kind": "valkey", "name": "cache",
                 "status": "running", "plan_name": null, "storage_limit_gb": null,
                 "project_id": OTHER_PROJECT_ID}
            ]}),
        )
        .await;
    }

    #[tokio::test]
    async fn list_shows_the_selected_project_unless_all() {
        let server = MockServer::start().await;
        serve_databases_in_two_projects(&server).await;
        let client = client_in_project(&server, Some(PROJECT_ID));

        let scoped = list(&client, false).await.unwrap();
        let everything = list(&client, true).await.unwrap();

        assert_eq!(render_json(&scoped).as_array().unwrap().len(), 1);
        assert_eq!(render_json(&everything).as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_database_in_another_project_points_at_that_project() {
        let server = MockServer::start().await;
        serve_databases_in_two_projects(&server).await;
        serve(
            &server,
            "/api/v1/projects",
            serde_json::json!({"projects": [
                {"id": OTHER_PROJECT_ID, "name": "shop", "description": null}
            ], "total": 1}),
        )
        .await;

        let error = change_state(
            &client_in_project(&server, Some(PROJECT_ID)),
            "cache",
            DatabaseCommand::Start,
        )
        .await
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "database 'cache' belongs to project 'shop'. Switch with: runsite project use \"shop\""
        );
    }

    #[test]
    fn a_plan_without_limits_shows_placeholders() {
        let plan: ServicePlan = serde_json::from_value(serde_json::json!({
            "id": DATABASE_ID, "slug": "free", "name": "Free", "monthly_price_cents": 0,
            "cpu_limit": null, "memory_limit_mb": null, "storage_limit_gb": null
        }))
        .unwrap();
        let text = render_text(&Plans(vec![plan]));
        assert!(text.contains("€0.00"));
        assert!(text.contains(PLACEHOLDER));
    }
}
