use super::{ApiClient, BulkEnvVars, EnvVar, EnvVarBulkResult, EnvVarList};
use anyhow::Result;
use uuid::Uuid;

impl ApiClient {
    pub async fn env_vars(&self, service_id: Uuid) -> Result<Vec<EnvVar>> {
        let list: EnvVarList = self
            .get(&format!("/api/v1/web-services/{}/env", service_id))
            .await?;
        Ok(list.environment_variables)
    }

    /// Creates or updates every variable in `body` in one request.
    pub async fn set_env_vars(
        &self,
        service_id: Uuid,
        body: &BulkEnvVars,
    ) -> Result<EnvVarBulkResult> {
        self.put(
            &format!("/api/v1/web-services/{}/env/bulk", service_id),
            body,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::BulkEnvVarItem;
    use super::*;
    use wiremock::MockServer;

    #[tokio::test]
    async fn env_vars_are_read_from_their_key() {
        let server = MockServer::start().await;
        serve(
            &server,
            &format!("/api/v1/web-services/{SERVICE_ID}/env"),
            serde_json::json!({"environment_variables": [{
                "id": "3f2a1b4c-0000-4000-8000-0000000000e1", "key": "PORT",
                "value": "8080", "is_secret": false
            }]}),
        )
        .await;

        let vars = client_for(&server)
            .env_vars(uuid(SERVICE_ID))
            .await
            .unwrap();
        assert_eq!(vars[0].key, "PORT");
    }

    #[tokio::test]
    async fn setting_env_vars_puts_the_bulk_body() {
        let server = MockServer::start().await;
        expect_request(
            &server,
            "PUT",
            &format!("/api/v1/web-services/{SERVICE_ID}/env/bulk"),
            200,
            serde_json::json!({
                "environment_variables": [
                    {"id": "3f2a1b4c-0000-4000-8000-0000000000e1", "key": "PORT", "value": "8080", "is_secret": false}
                ],
                "total": 1, "created": 1, "updated": 0, "deleted": 0
            }),
        )
        .await;

        let body = BulkEnvVars {
            variables: vec![BulkEnvVarItem {
                id: None,
                key: "PORT".to_string(),
                value: Some("8080".to_string()),
                is_secret: false,
                deleted: false,
            }],
        };
        let result = client_for(&server)
            .set_env_vars(uuid(SERVICE_ID), &body)
            .await
            .unwrap();
        assert_eq!(result.environment_variables[0].key, "PORT");
        assert_eq!(result.created, 1);
    }
}
