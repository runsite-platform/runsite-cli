use super::{
    ApiClient, ApiKeyIdentity, CurrentUser, ProjectDetail, ProjectSummary, ProjectSummaryList,
    ServiceInfo, ServiceInfoList,
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
