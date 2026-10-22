use super::{ApiClient, Deployment, DeploymentList};
use anyhow::{anyhow, Result};
use uuid::Uuid;

impl ApiClient {
    /// Newest first, without build logs: read one deployment for those.
    pub async fn deployments(&self, service_id: Uuid, limit: u32) -> Result<DeploymentList> {
        self.get_with_query(
            &format!("/api/v1/web-services/{}/deployments", service_id),
            &[("limit", limit.to_string())],
        )
        .await
    }

    pub async fn deployment(&self, service_id: Uuid, deployment_id: Uuid) -> Result<Deployment> {
        self.get(&format!(
            "/api/v1/web-services/{}/deployments/{}",
            service_id, deployment_id
        ))
        .await
    }

    /// A real rebuild (`POST …/deployments`), limited to 5 per minute.
    pub async fn trigger_deployment(&self, service_id: Uuid) -> Result<Deployment> {
        self.post_empty(&format!("/api/v1/web-services/{}/deployments", service_id))
            .await
    }

    /// Returns the new deployment that redeploys `deployment_id`.
    pub async fn roll_back(&self, service_id: Uuid, deployment_id: Uuid) -> Result<Deployment> {
        self.post_empty(&format!(
            "/api/v1/web-services/{}/deployments/{}/rollback",
            service_id, deployment_id
        ))
        .await
    }

    /// Accepts a full deployment ID or the short prefix printed by `deployments list`.
    pub async fn resolve_deployment_id(
        &self,
        service_id: Uuid,
        id_or_prefix: &str,
    ) -> Result<Uuid> {
        if let Ok(id) = Uuid::parse_str(id_or_prefix) {
            return Ok(id);
        }

        let recent: DeploymentList = self.deployments(service_id, 100).await?;
        let ids: Vec<Uuid> = recent.deployments.iter().map(|d| d.id).collect();
        match_id_prefix(&ids, id_or_prefix)
    }
}

fn match_id_prefix(ids: &[Uuid], prefix: &str) -> Result<Uuid> {
    let prefix = prefix.to_lowercase();
    if prefix.len() < 4 {
        return Err(anyhow!(
            "deployment ID '{}' is too short, use at least 4 characters",
            prefix
        ));
    }

    let matched: Vec<_> = ids
        .iter()
        .filter(|id| id.to_string().starts_with(&prefix))
        .collect();
    match matched.len() {
        0 => Err(anyhow!(
            "deployment '{}' not found among the last 100 deployments",
            prefix
        )),
        1 => Ok(*matched[0]),
        _ => Err(anyhow!(
            "deployment ID '{}' is ambiguous, use more characters",
            prefix
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const DEPLOYMENT_ID: &str = "3f2a1b4c-0000-4000-8000-00000000000d";

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
    async fn deployments_include_build_logs() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/web-services/{SERVICE_ID}/deployments"),
            serde_json::json!({"deployments": [deployment_json()], "total": 1}),
        )
        .await;

        let list: DeploymentList = client_for(&server)
            .deployments(uuid(SERVICE_ID), 10)
            .await
            .unwrap();
        assert_eq!(
            list.deployments[0].status,
            crate::api::DeploymentStatus::Building
        );
        assert_eq!(
            list.deployments[0].build_logs.as_deref(),
            Some("Step 1/4\nStep 2/4")
        );
    }

    #[tokio::test]
    async fn the_cli_view_of_deployments_keeps_the_total() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/web-services/{SERVICE_ID}/deployments"),
            serde_json::json!({"deployments": [deployment_json()], "total": 42}),
        )
        .await;

        let list: DeploymentList = client_for(&server)
            .deployments(uuid(SERVICE_ID), 10)
            .await
            .unwrap();
        assert_eq!(list.total, 42);
        assert_eq!(list.deployments[0].trigger_type.as_deref(), Some("manual"));
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

        let deployment = client_for(&server)
            .deployment(uuid(SERVICE_ID), uuid(DEPLOYMENT_ID))
            .await
            .unwrap();
        assert_eq!(deployment.id, uuid(DEPLOYMENT_ID));
        assert_eq!(deployment.commit_message.as_deref(), Some("fix: login"));
    }

    #[tokio::test]
    async fn deploy_posts_a_new_deployment_not_the_start_route() {
        let server = MockServer::start().await;
        expect_request(
            &server,
            "POST",
            &format!("/api/v1/web-services/{SERVICE_ID}/deployments"),
            201,
            deployment_json(),
        )
        .await;
        let deployment = client_for(&server)
            .trigger_deployment(uuid(SERVICE_ID))
            .await
            .unwrap();
        assert_eq!(deployment.id, uuid(DEPLOYMENT_ID));
    }

    #[tokio::test]
    async fn rollback_posts_to_the_target_deployment() {
        let server = MockServer::start().await;
        expect_request(
            &server,
            "POST",
            &format!("/api/v1/web-services/{SERVICE_ID}/deployments/{DEPLOYMENT_ID}/rollback"),
            200,
            deployment_json(),
        )
        .await;
        client_for(&server)
            .roll_back(uuid(SERVICE_ID), uuid(DEPLOYMENT_ID))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_rate_limited_deploy_reports_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!(
                "/api/v1/web-services/{SERVICE_ID}/deployments"
            )))
            .respond_with(ResponseTemplate::new(429).set_body_json(
                serde_json::json!({"detail": "Too many requests. Please try again later."}),
            ))
            .mount(&server)
            .await;
        let error = client_for(&server)
            .trigger_deployment(uuid(SERVICE_ID))
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<crate::error::ApiError>(),
            Some(crate::error::ApiError::RateLimited)
        ));
    }

    fn two_ids_sharing_a_prefix() -> Vec<Uuid> {
        vec![
            uuid("3f2a1b4c-0000-4000-8000-000000000001"),
            uuid("3f2a9999-0000-4000-8000-000000000002"),
        ]
    }

    #[test]
    fn an_ambiguous_prefix_is_rejected() {
        let error = match_id_prefix(&two_ids_sharing_a_prefix(), "3f2a").unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn an_unknown_or_too_short_prefix_is_rejected() {
        assert!(match_id_prefix(&two_ids_sharing_a_prefix(), "ffff").is_err());
        assert!(match_id_prefix(&two_ids_sharing_a_prefix(), "3f").is_err());
    }

    #[tokio::test]
    async fn a_short_prefix_resolves_among_the_last_100_deployments() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v1/web-services/{SERVICE_ID}/deployments"
            )))
            .and(query_param("limit", "100"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"deployments": [deployment_json()], "total": 1}),
                ),
            )
            .mount(&server)
            .await;

        let id = client_for(&server)
            .resolve_deployment_id(uuid(SERVICE_ID), "3F2A1B4C")
            .await
            .unwrap();
        assert_eq!(id, uuid(DEPLOYMENT_ID));
    }
}
