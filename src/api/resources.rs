use super::{
    ApiClient, ApiKeyIdentity, ContainerLogs, CurrentUser, DatabaseDetail, DeploymentInfo,
    DeploymentInfoList, MetricsHistory, MetricsPoint, ProjectDetail, ProjectSummary,
    ProjectSummaryList, ResourceMetrics, ServiceInfo, ServiceInfoList,
};
use anyhow::Result;
use uuid::Uuid;

impl ApiClient {
    pub async fn current_user(&self) -> Result<CurrentUser> {
        self.get("/api/v1/users/me").await
    }

    /// The key this client authenticates with, including its `scope`.
    pub async fn current_key(&self) -> Result<ApiKeyIdentity> {
        self.get("/api/v1/users/me/api-key").await
    }

    pub async fn project_summaries(&self) -> Result<Vec<ProjectSummary>> {
        let list: ProjectSummaryList = self.get("/api/v1/projects").await?;
        Ok(list.projects)
    }

    /// One project with its web services, Postgres and Redis/Valkey instances.
    pub async fn project_detail(&self, project_id: Uuid) -> Result<ProjectDetail> {
        self.get(&format!("/api/v1/projects/{}", project_id)).await
    }

    pub async fn services(&self) -> Result<Vec<ServiceInfo>> {
        let list: ServiceInfoList = self.get("/api/v1/web-services").await?;
        Ok(list.web_services)
    }

    pub async fn service(&self, service_id: Uuid) -> Result<ServiceInfo> {
        self.get(&format!("/api/v1/web-services/{}", service_id))
            .await
    }

    /// All zeros while the service is not `running`.
    pub async fn service_metrics(&self, service_id: Uuid) -> Result<ResourceMetrics> {
        self.get(&format!("/api/v1/web-services/{}/metrics", service_id))
            .await
    }

    pub async fn service_metrics_history(
        &self,
        service_id: Uuid,
        period_seconds: u32,
    ) -> Result<Vec<MetricsPoint>> {
        let history: MetricsHistory = self
            .get(&format!(
                "/api/v1/web-services/{}/metrics/history?period={}",
                service_id, period_seconds
            ))
            .await?;
        Ok(history.data_points)
    }

    /// Raw log text; `since` is unix seconds.
    pub async fn service_logs(
        &self,
        service_id: Uuid,
        tail: u32,
        since: Option<i64>,
    ) -> Result<String> {
        let mut path = format!("/api/v1/web-services/{}/logs?tail={}", service_id, tail);
        if let Some(since) = since {
            path.push_str(&format!("&since={}", since));
        }
        let logs: ContainerLogs = self.get(&path).await?;
        Ok(logs.logs)
    }

    /// Newest first.
    pub async fn deployments(&self, service_id: Uuid, limit: u32) -> Result<Vec<DeploymentInfo>> {
        let list: DeploymentInfoList = self
            .get(&format!(
                "/api/v1/web-services/{}/deployments?limit={}",
                service_id, limit
            ))
            .await?;
        Ok(list.deployments)
    }

    pub async fn deployment(
        &self,
        service_id: Uuid,
        deployment_id: Uuid,
    ) -> Result<DeploymentInfo> {
        self.get(&format!(
            "/api/v1/web-services/{}/deployments/{}",
            service_id, deployment_id
        ))
        .await
    }

    pub async fn database(&self, database_id: Uuid) -> Result<DatabaseDetail> {
        self.get(&format!("/api/v1/databases/{}", database_id))
            .await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::{Config, Profile};
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    pub(crate) const TEST_KEY: &str = "ak_live_test";
    const PROJECT_ID: &str = "ca83c4a9-1517-4fe3-93aa-bbdab4eaee23";

    pub(crate) fn client_for(server: &MockServer) -> ApiClient {
        let mut config = Config {
            current_profile: "default".to_string(),
            ..Default::default()
        };
        config.profiles.insert(
            "default".to_string(),
            Profile {
                api_url: server.uri(),
                api_key: Some(TEST_KEY.to_string()),
                current_project_id: None,
            },
        );
        ApiClient::new(
            server.uri(),
            Arc::new(Mutex::new(config)),
            "default".to_string(),
        )
    }

    pub(crate) async fn serve(server: &MockServer, route: &str, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path(route))
            .and(header(
                "authorization",
                format!("Bearer {TEST_KEY}").as_str(),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    const SERVICE_ID: &str = "3f2a1b4c-0000-4000-8000-000000000001";
    const DEPLOYMENT_ID: &str = "3f2a1b4c-0000-4000-8000-00000000000d";

    fn service_id() -> Uuid {
        Uuid::parse_str(SERVICE_ID).unwrap()
    }

    fn deployment_json() -> serde_json::Value {
        serde_json::json!({
            "id": DEPLOYMENT_ID, "web_service_id": SERVICE_ID, "commit_sha": "3f2a9c1d",
            "commit_message": "fix: login", "branch": "main", "image_ref": null,
            "status": "building", "is_live": false, "build_logs": "Step 1/4\nStep 2/4",
            "error_message": null, "started_at": null, "finished_at": null,
            "trigger_type": "manual", "created_at": "2026-09-23T10:00:00Z"
        })
    }

    #[tokio::test]
    async fn service_metrics_read_usage_and_instances() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/web-services/{SERVICE_ID}/metrics"),
            serde_json::json!({
                "cpu_usage_millicores": 120, "cpu_limit_millicores": 1000, "cpu_usage_percent": 12.0,
                "memory_usage_bytes": 268435456, "memory_limit_bytes": 536870912,
                "memory_usage_percent": 50.0, "instance_count": 2, "timestamp": "2026-09-23T10:00:00Z"
            }),
        )
        .await;

        let metrics = client_for(&server)
            .service_metrics(service_id())
            .await
            .unwrap();
        assert_eq!(metrics.instance_count, 2);
        assert_eq!(metrics.memory_usage_percent, 50.0);
    }

    #[tokio::test]
    async fn metrics_history_asks_for_the_period() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v1/web-services/{SERVICE_ID}/metrics/history"
            )))
            .and(wiremock::matchers::query_param("period", "3600"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data_points": [{
                    "timestamp": "2026-09-23T10:00:00Z", "cpu_usage_millicores": 120,
                    "cpu_limit_millicores": 1000, "cpu_usage_percent": 12.0,
                    "memory_usage_bytes": 1, "memory_limit_bytes": 2,
                    "memory_usage_percent": 50.0, "instance_count": 1
                }],
                "period_seconds": 3600
            })))
            .mount(&server)
            .await;

        let points = client_for(&server)
            .service_metrics_history(service_id(), 3600)
            .await
            .unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].cpu_usage_percent, 12.0);
    }

    #[tokio::test]
    async fn logs_send_tail_and_since() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/web-services/{SERVICE_ID}/logs")))
            .and(wiremock::matchers::query_param("tail", "1000"))
            .and(wiremock::matchers::query_param("since", "1790157595"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "logs": "2026-09-23T10:00:00.1Z hello\n", "container_id": null,
                "timestamp": "2026-09-23T10:00:01Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let body = client_for(&server)
            .service_logs(service_id(), 1000, Some(1_790_157_595))
            .await
            .unwrap();
        assert_eq!(body, "2026-09-23T10:00:00.1Z hello\n");
    }

    #[tokio::test]
    async fn deployments_include_build_logs() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/web-services/{SERVICE_ID}/deployments"),
            serde_json::json!({"deployments": [deployment_json()], "total": 1}),
        )
        .await;

        let deployments = client_for(&server)
            .deployments(service_id(), 10)
            .await
            .unwrap();
        assert_eq!(deployments[0].status, "building");
        assert_eq!(
            deployments[0].build_logs.as_deref(),
            Some("Step 1/4\nStep 2/4")
        );
    }

    #[tokio::test]
    async fn one_deployment_is_fetched_by_id() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/web-services/{SERVICE_ID}/deployments/{DEPLOYMENT_ID}"),
            deployment_json(),
        )
        .await;

        let deployment_id = Uuid::parse_str(DEPLOYMENT_ID).unwrap();
        let deployment = client_for(&server)
            .deployment(service_id(), deployment_id)
            .await
            .unwrap();
        assert_eq!(deployment.id, deployment_id);
        assert_eq!(deployment.commit_message.as_deref(), Some("fix: login"));
    }

    #[tokio::test]
    async fn database_detail_reads_postgres_usage() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/databases/3f2a1b4c-0000-4000-8000-000000000004",
            serde_json::json!({
                "id": "3f2a1b4c-0000-4000-8000-000000000004", "kind": "postgresql",
                "name": "pg-main", "status": "running", "plan_name": "Starter",
                "memory_limit_mb": 512, "storage_limit_gb": 5, "region": "eu-central",
                "internal_hostname": "pg-main.internal", "external_hostname": null,
                "public_access_enabled": false, "created_at": "2026-01-01T00:00:00Z",
                "cpu_usage_percent": 7.5, "memory_usage_percent": 40.0, "instance_count": 1
            }),
        )
        .await;

        let database_id = Uuid::parse_str("3f2a1b4c-0000-4000-8000-000000000004").unwrap();
        let database = client_for(&server).database(database_id).await.unwrap();
        assert_eq!(database.kind, "postgresql");
        assert_eq!(database.cpu_usage_percent, Some(7.5));
        assert_eq!(database.external_hostname, None);
    }

    #[tokio::test]
    async fn current_user_reads_the_email() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/users/me",
            serde_json::json!({
                "id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "email": "ada@example.com",
                "name": "Ada", "active_workspace_name": null, "created_at": "2026-01-01T00:00:00Z"
            }),
        )
        .await;

        let user = client_for(&server).current_user().await.unwrap();
        assert_eq!(user.email, "ada@example.com");
    }

    #[tokio::test]
    async fn current_key_reads_the_scope() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/users/me/api-key",
            serde_json::json!({
                "id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "name": "cli-laptop",
                "key_prefix": "ak_live_ab", "scope": "read", "expires_at": null, "last_used_at": null
            }),
        )
        .await;

        let key = client_for(&server).current_key().await.unwrap();
        assert_eq!(key.scope, "read");
    }

    #[tokio::test]
    async fn project_summaries_read_the_resource_counts() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/projects",
            serde_json::json!({
                "projects": [{
                    "id": PROJECT_ID, "name": "landing", "description": null,
                    "workspace_id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b",
                    "service_summary": {"web_services": 2, "postgresql": 1, "redis": 0, "object_storage": 3, "email": 1},
                    "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
                }],
                "total": 1
            }),
        )
        .await;

        let projects = client_for(&server).project_summaries().await.unwrap();
        assert_eq!(projects[0].name, "landing");
        assert_eq!(projects[0].service_summary.web_services, 2);
        assert_eq!(projects[0].service_summary.postgresql, 1);
    }

    #[tokio::test]
    async fn project_detail_lists_every_resource_kind() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/projects/{PROJECT_ID}"),
            serde_json::json!({
                "id": PROJECT_ID, "name": "landing", "description": null,
                "workspace_id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b",
                "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
                "web_services_list": [{
                    "id": "3f2a1b4c-0000-4000-8000-000000000001", "name": "api", "status": "running",
                    "url": "https://api-x.runsite.app", "project_id": PROJECT_ID,
                    "project_type": "web_service", "source_type": "git", "image_ref": null,
                    "github_branch": "main", "min_instances": 1, "max_instances": 3
                }],
                "databases_list": [{
                    "id": "3f2a1b4c-0000-4000-8000-000000000002", "name": "pg-main",
                    "status": "running", "postgres_version": "16"
                }],
                "redis_list": [{
                    "id": "3f2a1b4c-0000-4000-8000-000000000003", "name": "cache",
                    "status": "stopped", "redis_version": "7.2"
                }]
            }),
        )
        .await;

        let project_id = Uuid::parse_str(PROJECT_ID).unwrap();
        let detail = client_for(&server)
            .project_detail(project_id)
            .await
            .unwrap();
        assert_eq!(detail.web_services_list[0].max_instances, Some(3));
        assert_eq!(
            detail.databases_list[0].postgres_version.as_deref(),
            Some("16")
        );
        assert_eq!(detail.redis_list[0].redis_version.as_deref(), Some("7.2"));
    }

    #[tokio::test]
    async fn services_tolerate_a_service_without_a_project() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/web-services",
            serde_json::json!({
                "web_services": [{
                    "id": "3f2a1b4c-0000-4000-8000-000000000001", "name": "worker",
                    "status": "stopped", "url": null, "project_id": null,
                    "project_type": "worker", "source_type": "image", "image_ref": "nginx:1.27",
                    "github_branch": "main", "min_instances": 1, "max_instances": 1
                }],
                "total": 1
            }),
        )
        .await;

        let services = client_for(&server).services().await.unwrap();
        assert_eq!(services[0].project_id, None);
        assert_eq!(services[0].image_ref.as_deref(), Some("nginx:1.27"));
    }
}
