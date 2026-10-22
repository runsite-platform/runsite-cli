use super::{ApiClient, ApiKeyIdentity};
use anyhow::Result;
use serde::de::DeserializeOwned;

impl ApiClient {
    /// `T` is the caller's view of the user: `UserResponse` or `CurrentUser`.
    pub async fn current_user<T: DeserializeOwned>(&self) -> Result<T> {
        self.get("/api/v1/users/me").await
    }

    /// The key this client authenticates with, including its `scope`.
    pub async fn current_key(&self) -> Result<ApiKeyIdentity> {
        self.get("/api/v1/users/me/api-key").await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::super::{CurrentUser, UserResponse};
    use wiremock::MockServer;

    fn user_json() -> serde_json::Value {
        serde_json::json!({
            "id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "email": "ada@example.com",
            "name": "Ada", "active_workspace_name": null, "created_at": "2026-01-01T00:00:00Z"
        })
    }

    #[tokio::test]
    async fn current_user_reads_the_email() {
        let server = MockServer::start().await;
        serve(&server, "/api/v1/users/me", user_json()).await;

        let user: CurrentUser = client_for(&server).current_user().await.unwrap();
        assert_eq!(user.email, "ada@example.com");
    }

    #[tokio::test]
    async fn the_cli_view_of_the_user_keeps_the_name() {
        let server = MockServer::start().await;
        serve(&server, "/api/v1/users/me", user_json()).await;

        let user: UserResponse = client_for(&server).current_user().await.unwrap();
        assert_eq!(user.full_name.as_deref(), Some("Ada"));
    }

    #[tokio::test]
    async fn a_key_stored_after_login_is_used_by_an_existing_client() {
        let server = MockServer::start().await;
        serve(&server, "/api/v1/users/me", user_json()).await;

        let profile = crate::config::ActiveProfile::in_memory(
            "default",
            crate::config::Profile {
                api_url: server.uri(),
                ..Default::default()
            },
            None,
        );
        let client = super::ApiClient::new(profile.clone());
        assert!(client.current_user::<CurrentUser>().await.is_err());

        profile.store_api_key(TEST_KEY.to_string()).unwrap();
        let user: CurrentUser = client.current_user().await.unwrap();
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
}
