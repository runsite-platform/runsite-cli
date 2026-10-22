use super::{
    ApiClient, Connection, ConnectionCreate, DatabaseCreate, DatabaseDetail, DatabaseStatus,
    ManagedDatabase, ManagedDatabaseList, ServicePlan, ServicePlanList,
};
use anyhow::{anyhow, Result};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseEngine {
    Postgres,
    Redis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseCommand {
    Start,
    Stop,
}

impl DatabaseCommand {
    pub fn route(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
        }
    }
}

impl ApiClient {
    /// Postgres and Valkey instances, up to 100.
    pub async fn databases(&self) -> Result<Vec<ManagedDatabase>> {
        let list: ManagedDatabaseList = self.get("/api/v1/databases?limit=100").await?;
        Ok(list.databases)
    }

    pub async fn database(&self, database_id: Uuid) -> Result<DatabaseDetail> {
        self.get(&format!("/api/v1/databases/{}", database_id))
            .await
    }

    pub async fn create_database(
        &self,
        engine: DatabaseEngine,
        body: &DatabaseCreate,
    ) -> Result<ManagedDatabase> {
        let route = match engine {
            DatabaseEngine::Postgres => "/api/v1/databases",
            DatabaseEngine::Redis => "/api/v1/redis",
        };
        self.post(route, body).await
    }

    pub async fn database_command(
        &self,
        database_id: Uuid,
        command: DatabaseCommand,
    ) -> Result<DatabaseStatus> {
        self.post_empty(&format!(
            "/api/v1/databases/{}/{}",
            database_id,
            command.route()
        ))
        .await
    }

    pub async fn delete_database(&self, database_id: Uuid) -> Result<()> {
        self.delete_req(&format!("/api/v1/databases/{}", database_id))
            .await
    }

    /// Exposes `database` to the service as a secret env var.
    pub async fn connect_database(
        &self,
        service_id: Uuid,
        database: &ManagedDatabase,
    ) -> Result<Connection> {
        let target_type = if database.kind == "valkey" {
            "redis"
        } else {
            "database"
        };
        let body = ConnectionCreate {
            target_type: target_type.to_string(),
            target_id: database.id,
        };
        self.post(
            &format!("/api/v1/web-services/{}/connections", service_id),
            &body,
        )
        .await
    }

    /// `service_type` is the API's name: `web_service`, `postgresql` or `redis`.
    pub async fn plans(&self, service_type: &str) -> Result<Vec<ServicePlan>> {
        let list: ServicePlanList = self
            .get(&format!("/api/v1/plans?service_type={}", service_type))
            .await?;
        Ok(list.plans)
    }

    /// A plan ID, or a slug looked up among the plans for `service_type`.
    pub async fn resolve_plan_id(&self, service_type: &str, id_or_slug: &str) -> Result<Uuid> {
        if let Ok(id) = Uuid::parse_str(id_or_slug) {
            return Ok(id);
        }
        let plans = self.plans(service_type).await?;
        plans
            .iter()
            .find(|candidate| candidate.slug == id_or_slug)
            .map(|candidate| candidate.id)
            .ok_or_else(|| {
                let slugs: Vec<_> = plans
                    .iter()
                    .map(|candidate| candidate.slug.as_str())
                    .collect();
                anyhow!(
                    "plan '{}' not found. Available: {}",
                    id_or_slug,
                    slugs.join(", ")
                )
            })
    }

    /// A database ID, or a name looked up in the selected project.
    pub async fn resolve_database(&self, name_or_id: &str) -> Result<ManagedDatabase> {
        let databases = self.databases().await?;
        self.find(databases, Some(name_or_id)).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use wiremock::matchers::{body_partial_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn database_json(id: &str, name: &str, kind: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "kind": kind, "name": name, "status": "running", "plan_name": "Starter"
        })
    }

    async fn serve_databases(server: &MockServer, databases: Vec<serde_json::Value>) {
        serve(
            server,
            "/api/v1/databases",
            serde_json::json!({ "databases": databases, "total": databases.len() }),
        )
        .await;
    }

    #[tokio::test]
    async fn database_detail_reads_postgres_usage() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/databases/{DATABASE_ID}"),
            serde_json::json!({
                "id": DATABASE_ID, "kind": "postgresql",
                "name": "pg-main", "status": "running", "plan_name": "Starter",
                "memory_limit_mb": 512, "storage_limit_gb": 5, "region": "eu-central",
                "internal_hostname": "pg-main.internal", "external_hostname": null,
                "public_access_enabled": false, "created_at": "2026-01-01T00:00:00Z",
                "cpu_usage_percent": 7.5, "memory_usage_percent": 40.0, "instance_count": 1
            }),
        )
        .await;

        let database = client_for(&server)
            .database(uuid(DATABASE_ID))
            .await
            .unwrap();
        assert_eq!(database.kind, "postgresql");
        assert_eq!(database.cpu_usage_percent, Some(7.5));
        assert_eq!(database.external_hostname, None);
    }

    #[tokio::test]
    async fn database_commands_post_to_their_route() {
        let server = MockServer::start().await;
        expect_request(
            &server,
            "POST",
            &format!("/api/v1/databases/{DATABASE_ID}/stop"),
            200,
            serde_json::json!({"id": DATABASE_ID, "kind": "postgresql", "status": "stopping"}),
        )
        .await;
        let status = client_for(&server)
            .database_command(uuid(DATABASE_ID), DatabaseCommand::Stop)
            .await
            .unwrap();
        assert_eq!(status.status, "stopping");
    }

    #[tokio::test]
    async fn each_engine_is_created_on_its_own_route() {
        let server = MockServer::start().await;
        for route in ["/api/v1/databases", "/api/v1/redis"] {
            expect_request(
                &server,
                "POST",
                route,
                201,
                database_json(DATABASE_ID, "store", "postgresql"),
            )
            .await;
        }
        let client = client_for(&server);
        let body = DatabaseCreate {
            name: "store".to_string(),
            plan_id: uuid(DATABASE_ID),
            project_id: None,
        };
        for engine in [DatabaseEngine::Postgres, DatabaseEngine::Redis] {
            client.create_database(engine, &body).await.unwrap();
        }
    }

    #[tokio::test]
    async fn a_valkey_instance_connects_as_redis() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/api/v1/web-services/{SERVICE_ID}/connections"
            )))
            .and(body_partial_json(
                serde_json::json!({"target_type": "redis", "target_id": DATABASE_ID}),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": SERVICE_ID, "target_type": "redis", "env_var_key": "REDIS_URL"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let cache: ManagedDatabase =
            serde_json::from_value(database_json(DATABASE_ID, "cache", "valkey")).unwrap();
        let connection = client_for(&server)
            .connect_database(uuid(SERVICE_ID), &cache)
            .await
            .unwrap();
        assert_eq!(connection.env_var_key, "REDIS_URL");
    }

    #[tokio::test]
    async fn a_database_is_resolved_by_name_or_id() {
        let server = MockServer::start().await;
        serve_databases(
            &server,
            vec![
                database_json(DATABASE_ID, "pg-main", "postgresql"),
                database_json("3f2a1b4c-0000-4000-8000-000000000005", "cache", "valkey"),
            ],
        )
        .await;
        let client = client_for(&server);

        assert_eq!(
            client.resolve_database("cache").await.unwrap().kind,
            "valkey"
        );
        assert_eq!(
            client.resolve_database(DATABASE_ID).await.unwrap().name,
            "pg-main"
        );
    }

    #[tokio::test]
    async fn a_shared_database_name_asks_for_the_id() {
        let server = MockServer::start().await;
        serve_databases(
            &server,
            vec![
                database_json(DATABASE_ID, "pg", "postgresql"),
                database_json("3f2a1b4c-0000-4000-8000-000000000005", "pg", "postgresql"),
            ],
        )
        .await;
        let client = client_for(&server);

        let ambiguous = client.resolve_database("pg").await.unwrap_err();
        assert!(ambiguous
            .to_string()
            .starts_with("multiple databases named 'pg'. Use the ID instead:\n"));
        let missing = client.resolve_database("other").await.unwrap_err();
        assert_eq!(missing.to_string(), "database 'other' not found");
    }

    #[tokio::test]
    async fn a_plan_slug_resolves_within_its_service_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/plans"))
            .and(query_param("service_type", "postgresql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "plans": [
                    {"id": DATABASE_ID, "slug": "pg-starter", "name": "Starter",
                     "monthly_price_cents": 500, "cpu_limit": null,
                     "memory_limit_mb": 512, "storage_limit_gb": 5}
                ]
            })))
            .mount(&server)
            .await;
        let client = client_for(&server);

        assert_eq!(
            client
                .resolve_plan_id("postgresql", "pg-starter")
                .await
                .unwrap(),
            uuid(DATABASE_ID)
        );
        let missing = client
            .resolve_plan_id("postgresql", "pg-huge")
            .await
            .unwrap_err();
        assert_eq!(
            missing.to_string(),
            "plan 'pg-huge' not found. Available: pg-starter"
        );
    }
}
