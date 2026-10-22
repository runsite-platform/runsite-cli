use super::{ApiClient, Project, ProjectCreate, ProjectDetail, ProjectList};
use anyhow::Result;
use uuid::Uuid;

impl ApiClient {
    pub async fn projects(&self) -> Result<Vec<Project>> {
        let list: ProjectList = self.get("/api/v1/projects").await?;
        Ok(list.projects)
    }

    /// One project with its web services, Postgres and Redis/Valkey instances.
    pub async fn project_detail(&self, project_id: Uuid) -> Result<ProjectDetail> {
        self.get(&format!("/api/v1/projects/{}", project_id)).await
    }

    pub async fn create_project(&self, body: &ProjectCreate) -> Result<Project> {
        self.post("/api/v1/projects", body).await
    }

    pub async fn resolve_project_id(&self, name_or_id: &str) -> Result<Uuid> {
        if let Ok(id) = Uuid::parse_str(name_or_id) {
            return Ok(id);
        }

        let projects: Vec<Project> = self.projects().await?;
        Ok(self.find(projects, Some(name_or_id)).await?.id)
    }

    /// The project a new resource goes into: the one named explicitly, else the
    /// one selected with `runsite project use`, else none.
    pub async fn target_project_id(&self, explicit: Option<&str>) -> Result<Option<Uuid>> {
        match explicit {
            Some(name_or_id) => Ok(Some(self.resolve_project_id(name_or_id).await?)),
            None => Ok(self.current_project_id()),
        }
    }

    /// Best-effort project name lookup; falls back to `None` when the extra
    /// request fails, so error reporting never masks the original problem.
    pub async fn project_name(&self, project_id: Uuid) -> Option<String> {
        let projects: Vec<Project> = self.projects().await.ok()?;
        projects
            .into_iter()
            .find(|p| p.id == project_id)
            .map(|p| p.name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::Project;
    use wiremock::MockServer;

    fn projects_json(names: &[&str]) -> serde_json::Value {
        let projects: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                serde_json::json!({
                    "id": format!("ca83c4a9-1517-4fe3-93aa-00000000000{index}"),
                    "name": name, "description": null
                })
            })
            .collect();
        serde_json::json!({ "projects": projects, "total": names.len() })
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

        let projects: Vec<Project> = client_for(&server).projects().await.unwrap();
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
                    "id": SERVICE_ID, "name": "api", "status": "running",
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

        let detail = client_for(&server)
            .project_detail(uuid(PROJECT_ID))
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
    async fn a_project_is_resolved_by_name() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/projects",
            projects_json(&["blog", "shop"]),
        )
        .await;

        let id = client_for(&server)
            .resolve_project_id("shop")
            .await
            .unwrap();
        assert_eq!(id, uuid("ca83c4a9-1517-4fe3-93aa-000000000001"));
    }

    #[tokio::test]
    async fn an_unknown_or_ambiguous_project_name_is_an_error() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/projects",
            projects_json(&["blog", "blog"]),
        )
        .await;
        let client = client_for(&server);

        let missing = client.resolve_project_id("shop").await.unwrap_err();
        assert_eq!(missing.to_string(), "project 'shop' not found");
        let ambiguous = client.resolve_project_id("blog").await.unwrap_err();
        assert!(ambiguous
            .to_string()
            .starts_with("multiple projects named 'blog'. Use the ID instead:\n"));
    }

    #[tokio::test]
    async fn the_target_project_falls_back_to_the_selected_one() {
        let server = MockServer::start().await;
        let selected = client_in_project(&server, Some(PROJECT_ID));
        assert_eq!(
            selected.target_project_id(None).await.unwrap(),
            Some(uuid(PROJECT_ID))
        );
        assert_eq!(
            client_for(&server).target_project_id(None).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn an_explicit_target_project_wins_over_the_selected_one() {
        let server = MockServer::start().await;
        serve(&server, "/api/v1/projects", projects_json(&["blog"])).await;

        let client = client_in_project(&server, Some(PROJECT_ID));
        assert_eq!(
            client.target_project_id(Some("blog")).await.unwrap(),
            Some(uuid("ca83c4a9-1517-4fe3-93aa-000000000000"))
        );
    }
}
