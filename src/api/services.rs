use super::{
    belongs_to_project, ApiClient, ContainerLogs, MetricsHistory, MetricsPoint, ResourceMetrics,
    ServiceRuntimeStatus, WebService, WebServiceCreate, WebServiceDetail, WebServiceList,
};
use anyhow::{anyhow, Result};
use serde::de::DeserializeOwned;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart,
}

impl ServiceCommand {
    pub fn route(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

impl ApiClient {
    /// Every service on the account. `T` is the caller's view of a service:
    /// `WebService` or `ServiceInfo`.
    pub async fn services<T: DeserializeOwned>(&self) -> Result<Vec<T>> {
        let list: WebServiceList<T> = self.get("/api/v1/web-services").await?;
        Ok(list.web_services)
    }

    /// `T` is the caller's view of a service: `WebServiceDetail` or `ServiceInfo`.
    pub async fn service<T: DeserializeOwned>(&self, service_id: Uuid) -> Result<T> {
        self.get(&format!("/api/v1/web-services/{}", service_id))
            .await
    }

    pub async fn create_service(&self, body: &WebServiceCreate) -> Result<WebServiceDetail> {
        self.post("/api/v1/web-services", body).await
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

    /// `POST …/start` only starts a stopped service; a rebuild is
    /// `trigger_deployment`.
    pub async fn service_command(
        &self,
        service_id: Uuid,
        command: ServiceCommand,
    ) -> Result<ServiceRuntimeStatus> {
        self.post_empty(&format!(
            "/api/v1/web-services/{}/{}",
            service_id,
            command.route()
        ))
        .await
    }

    /// A service ID, or a name looked up in the selected project. With no
    /// argument, the project's only service.
    pub async fn resolve_service_id(&self, name_or_id: Option<&str>) -> Result<Uuid> {
        if let Some(val) = name_or_id {
            if let Ok(id) = Uuid::parse_str(val) {
                return Ok(id);
            }
        }

        let services: Vec<WebService> = self.services().await?;
        let project_id = self.current_project_id();
        let candidates: Vec<_> = services
            .iter()
            .filter(|s| belongs_to_project(s, project_id))
            .collect();

        if let Some(name) = name_or_id {
            let matched: Vec<_> = candidates.iter().filter(|s| s.name == name).collect();
            match matched.len() {
                0 => Err(self.service_not_found_error(name, &services).await),
                1 => Ok(matched[0].id),
                _ => {
                    let ids: Vec<_> = matched
                        .iter()
                        .map(|s| format!("  {} ({})", s.name, s.id))
                        .collect();
                    Err(anyhow!(
                        "multiple services named '{}'. Use ID instead:\n{}",
                        name,
                        ids.join("\n")
                    ))
                }
            }
        } else {
            match candidates.len() {
                0 => Err(anyhow!("no services found")),
                1 => Ok(candidates[0].id),
                _ => {
                    let list: Vec<_> = candidates
                        .iter()
                        .map(|s| format!("  {} ({})", s.name, s.id))
                        .collect();
                    Err(anyhow!(
                        "multiple services found. Specify a service name:\n{}",
                        list.join("\n")
                    ))
                }
            }
        }
    }

    /// Build the error for a name that resolved to nothing in the current project.
    ///
    /// `service list` and this resolver used to disagree: the list showed every
    /// service on the account while the resolver only accepted ones in the
    /// selected project, so a name printed by the list could still be rejected
    /// here. The list now filters too, and this points at the owning project
    /// whenever the name exists somewhere else.
    async fn service_not_found_error(&self, name: &str, services: &[WebService]) -> anyhow::Error {
        let Some(service) = services.iter().find(|s| s.name == name) else {
            return anyhow!("service '{}' not found", name);
        };

        let owner = match service.project_id {
            Some(project_id) => self.project_name(project_id).await,
            None => None,
        };

        match owner {
            Some(project_name) => anyhow!(
                "service '{}' belongs to project '{}'. Switch with: runsite project use \"{}\"",
                name,
                project_name,
                project_name
            ),
            None => anyhow!(
                "service '{}' is not in the current project. See all services with: runsite service list --all",
                name
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::ServiceInfo;
    use super::*;
    use wiremock::MockServer;

    const OTHER_PROJECT_ID: &str = "ca83c4a9-1517-4fe3-93aa-000000000009";

    fn service_json(id: &str, name: &str, project_id: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "id": id, "name": name, "status": "running", "url": null,
            "project_id": project_id, "updated_at": null
        })
    }

    async fn serve_services(server: &MockServer, services: Vec<serde_json::Value>) {
        serve(
            server,
            "/api/v1/web-services",
            serde_json::json!({ "web_services": services }),
        )
        .await;
    }

    #[tokio::test]
    async fn services_tolerate_a_service_without_a_project() {
        let server = MockServer::start().await;
        serve(
            &server,
            "/api/v1/web-services",
            serde_json::json!({
                "web_services": [{
                    "id": SERVICE_ID, "name": "worker",
                    "status": "stopped", "url": null, "project_id": null,
                    "project_type": "worker", "source_type": "image", "image_ref": "nginx:1.27",
                    "github_branch": "main", "min_instances": 1, "max_instances": 1
                }],
                "total": 1
            }),
        )
        .await;

        let services: Vec<ServiceInfo> = client_for(&server).services().await.unwrap();
        assert_eq!(services[0].project_id, None);
        assert_eq!(services[0].image_ref.as_deref(), Some("nginx:1.27"));
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
            .service_metrics(uuid(SERVICE_ID))
            .await
            .unwrap();
        assert_eq!(metrics.instance_count, 2);
        assert_eq!(metrics.memory_usage_percent, 50.0);
    }

    #[tokio::test]
    async fn metrics_history_asks_for_the_period() {
        let server = MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!(
                "/api/v1/web-services/{SERVICE_ID}/metrics/history"
            )))
            .and(wiremock::matchers::query_param("period", "3600"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data_points": [{
                        "timestamp": "2026-09-23T10:00:00Z", "cpu_usage_millicores": 120,
                        "cpu_limit_millicores": 1000, "cpu_usage_percent": 12.0,
                        "memory_usage_bytes": 1, "memory_limit_bytes": 2,
                        "memory_usage_percent": 50.0, "instance_count": 1
                    }],
                    "period_seconds": 3600
                })),
            )
            .mount(&server)
            .await;

        let points = client_for(&server)
            .service_metrics_history(uuid(SERVICE_ID), 3600)
            .await
            .unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].cpu_usage_percent, 12.0);
    }

    #[tokio::test]
    async fn logs_send_tail_and_since() {
        let server = MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!(
                "/api/v1/web-services/{SERVICE_ID}/logs"
            )))
            .and(wiremock::matchers::query_param("tail", "1000"))
            .and(wiremock::matchers::query_param("since", "1790157595"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "logs": "2026-09-23T10:00:00.1Z hello\n", "container_id": null,
                    "timestamp": "2026-09-23T10:00:01Z"
                })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let body = client_for(&server)
            .service_logs(uuid(SERVICE_ID), 1000, Some(1_790_157_595))
            .await
            .unwrap();
        assert_eq!(body, "2026-09-23T10:00:00.1Z hello\n");
    }

    #[tokio::test]
    async fn service_commands_post_to_their_route() {
        let server = MockServer::start().await;
        for route in ["restart", "start", "stop"] {
            expect_request(
                &server,
                "POST",
                &format!("/api/v1/web-services/{SERVICE_ID}/{route}"),
                200,
                serde_json::json!({"status": "deploying", "container_id": null, "host_port": null}),
            )
            .await;
        }
        let client = client_for(&server);
        for command in [
            ServiceCommand::Restart,
            ServiceCommand::Start,
            ServiceCommand::Stop,
        ] {
            client
                .service_command(uuid(SERVICE_ID), command)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn a_service_id_resolves_without_a_request() {
        let server = MockServer::start().await;
        let id = client_for(&server)
            .resolve_service_id(Some(SERVICE_ID))
            .await
            .unwrap();
        assert_eq!(id, uuid(SERVICE_ID));
    }

    #[tokio::test]
    async fn a_name_resolves_inside_the_selected_project_only() {
        let server = MockServer::start().await;
        serve_services(
            &server,
            vec![
                service_json(SERVICE_ID, "api", Some(PROJECT_ID)),
                service_json(
                    "3f2a1b4c-0000-4000-8000-000000000002",
                    "api",
                    Some(OTHER_PROJECT_ID),
                ),
            ],
        )
        .await;

        let id = client_in_project(&server, Some(PROJECT_ID))
            .resolve_service_id(Some("api"))
            .await
            .unwrap();
        assert_eq!(id, uuid(SERVICE_ID));
    }

    #[tokio::test]
    async fn no_argument_picks_the_only_service_in_the_project() {
        let server = MockServer::start().await;
        serve_services(
            &server,
            vec![
                service_json(SERVICE_ID, "api", Some(PROJECT_ID)),
                service_json(
                    "3f2a1b4c-0000-4000-8000-000000000002",
                    "worker",
                    Some(OTHER_PROJECT_ID),
                ),
            ],
        )
        .await;

        let id = client_in_project(&server, Some(PROJECT_ID))
            .resolve_service_id(None)
            .await
            .unwrap();
        assert_eq!(id, uuid(SERVICE_ID));
    }

    #[tokio::test]
    async fn no_argument_with_several_services_asks_for_a_name() {
        let server = MockServer::start().await;
        serve_services(
            &server,
            vec![
                service_json(SERVICE_ID, "api", None),
                service_json("3f2a1b4c-0000-4000-8000-000000000002", "worker", None),
            ],
        )
        .await;

        let error = client_for(&server)
            .resolve_service_id(None)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .starts_with("multiple services found. Specify a service name:"));
    }

    #[tokio::test]
    async fn a_name_in_another_project_points_at_that_project() {
        let server = MockServer::start().await;
        serve_services(
            &server,
            vec![service_json(SERVICE_ID, "api", Some(OTHER_PROJECT_ID))],
        )
        .await;
        serve(
            &server,
            "/api/v1/projects",
            serde_json::json!({"projects": [
                {"id": OTHER_PROJECT_ID, "name": "shop", "description": null}
            ]}),
        )
        .await;

        let error = client_in_project(&server, Some(PROJECT_ID))
            .resolve_service_id(Some("api"))
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "service 'api' belongs to project 'shop'. Switch with: runsite project use \"shop\""
        );
    }

    #[tokio::test]
    async fn an_unknown_name_is_not_found() {
        let server = MockServer::start().await;
        serve_services(&server, vec![service_json(SERVICE_ID, "api", None)]).await;

        let error = client_for(&server)
            .resolve_service_id(Some("web"))
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "service 'web' not found");
    }
}
