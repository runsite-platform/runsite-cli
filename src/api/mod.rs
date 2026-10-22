mod databases;
mod deployments;
mod env;
mod login;
mod lookup;
mod projects;
mod services;
mod status;
#[cfg(test)]
pub mod test_support;
mod types;
mod users;
pub use databases::{DatabaseCommand, DatabaseEngine};
pub use login::Credentials;
pub use lookup::ProjectScope;
pub use services::ServiceCommand;
pub use status::{Condition, DatabaseStatus, DeploymentStatus, ServiceStatus, Status};
pub use types::*;

use crate::config::ActiveProfile;
use crate::error::ApiError;
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A request that hangs longer than this fails instead of freezing the CLI or a TUI poll.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    base_url: String,
    profile: ActiveProfile,
}

impl ApiClient {
    /// Talks to the profile's API URL with its current token, so a key stored
    /// on `profile` later is used by this client too.
    pub fn new(profile: ActiveProfile) -> Self {
        // The login flow authenticates with httpOnly cookies: /api/auth/login sets
        // `access_token`, which the follow-up /api/api-keys call must carry.
        let http = Client::builder()
            .cookie_store(true)
            .user_agent(crate::USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            base_url: profile.api_url().trim_end_matches('/').to_string(),
            profile,
        }
    }

    /// Project selected with `runsite project use`, if any.
    pub fn current_project_id(&self) -> Option<Uuid> {
        self.profile.current_project_id()
    }

    fn auth_token(&self) -> Option<String> {
        self.profile.token()
    }

    pub async fn mint_api_key(&self, name: &str) -> Result<String> {
        let value: Value = self
            .send(
                self.http
                    .post(format!("{}/api/api-keys", self.base_url))
                    .json(&serde_json::json!({ "name": name, "scope": "write" })),
            )
            .await?;
        value["plaintext_key"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("server did not return plaintext_key"))
    }

    async fn handle_response<T: DeserializeOwned>(&self, response: Response) -> Result<T> {
        let status = response.status();
        if status.is_success() {
            response
                .json::<T>()
                .await
                .context("failed to parse response")
        } else {
            let body = response.json::<Value>().await.ok();
            Err(ApiError::from_response(status.as_u16(), body).into())
        }
    }

    /// A request to `path` on the API, carrying the profile's token.
    fn authorized(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        let token = self.auth_token().ok_or_else(ApiError::not_logged_in)?;
        Ok(self
            .http
            .request(method, format!("{}{}", self.base_url, path))
            .bearer_auth(token))
    }

    async fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = request
            .send()
            .await
            .map_err(|error| ApiError::network(&error))?;
        self.handle_response(response).await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(self.authorized(Method::GET, path)?).await
    }

    async fn get_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        self.send(self.authorized(Method::GET, path)?.query(query))
            .await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        self.send(self.authorized(Method::POST, path)?.json(body))
            .await
    }

    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.post(path, &serde_json::json!({})).await
    }

    async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        self.send(self.authorized(Method::PUT, path)?.json(body))
            .await
    }

    /// A DELETE answers 204 with no body, so success is not parsed.
    async fn delete_req(&self, path: &str) -> Result<()> {
        let response = self
            .authorized(Method::DELETE, path)?
            .send()
            .await
            .map_err(|error| ApiError::network(&error))?;
        if response.status().is_success() {
            return Ok(());
        }
        self.handle_response::<Value>(response).await.map(drop)
    }

    /// Confirm an API key is valid and return its owner.
    pub async fn verify_api_key(&self, key: &str) -> Result<User> {
        self.send(
            self.http
                .get(format!("{}/api/v1/users/me", self.base_url))
                .bearer_auth(key),
        )
        .await
    }

    /// Log in with email + password. The session lives in the client's cookie jar;
    /// the response body carries the user, not a token.
    pub async fn post_login(&self, email: &str, password: &str) -> Result<User> {
        self.send(
            self.http
                .post(format!("{}/api/auth/login", self.base_url))
                .json(&serde_json::json!({ "email": email, "password": password })),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::lookup::belongs_to_project;
    use super::*;

    fn service(name: &str, project_id: Option<&str>) -> Service {
        Service {
            id: Uuid::parse_str("8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b").unwrap(),
            name: name.to_string(),
            status: ServiceStatus::Running,
            url: None,
            subdomain: None,
            project_id: project_id.map(|id| Uuid::parse_str(id).unwrap()),
            project_type: None,
            source_type: None,
            image_ref: None,
            github_branch: None,
            github_repo_full_name: None,
            min_instances: None,
            max_instances: None,
            updated_at: None,
        }
    }

    #[test]
    fn a_trailing_slash_in_the_api_url_is_dropped() {
        let profile = crate::config::Profile {
            api_url: "https://api.runsite.app/".to_string(),
            ..Default::default()
        };
        let client = ApiClient::new(ActiveProfile::in_memory("default", profile, None));
        assert_eq!(client.base_url, "https://api.runsite.app");
    }

    const PROJECT: &str = "ca83c4a9-1517-4fe3-93aa-bbdab4eaee23";
    const OTHER_PROJECT: &str = "8d5b5e8e-2348-4a29-8c05-bcfa1e599ef2";

    fn selected() -> Option<Uuid> {
        Some(Uuid::parse_str(PROJECT).unwrap())
    }

    #[test]
    fn a_service_in_the_selected_project_belongs_to_it() {
        assert!(belongs_to_project(
            &service("api", Some(PROJECT)),
            selected()
        ));
    }

    #[test]
    fn a_service_from_another_project_is_filtered_out() {
        assert!(!belongs_to_project(
            &service("api", Some(OTHER_PROJECT)),
            selected()
        ));
    }

    #[test]
    fn every_service_is_listed_when_no_project_is_selected() {
        assert!(belongs_to_project(&service("api", Some(PROJECT)), None));
        assert!(belongs_to_project(&service("api", None), None));
    }

    #[test]
    fn a_service_without_a_project_is_filtered_out_under_a_selection() {
        assert!(!belongs_to_project(&service("api", None), selected()));
    }
}
