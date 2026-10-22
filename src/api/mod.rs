mod databases;
mod deployments;
mod env;
mod login;
mod projects;
mod services;
#[cfg(test)]
pub mod test_support;
mod types;
mod users;
pub use databases::{DatabaseCommand, DatabaseEngine};
pub use login::Credentials;
pub use services::ServiceCommand;
pub use types::*;

use crate::config::ActiveProfile;
use crate::error::ApiError;
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A request that hangs longer than this fails instead of freezing the CLI or a TUI poll.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// True when `service` sits in `project_id`, or when no project is selected.
pub fn belongs_to_project(service: &WebService, project_id: Option<Uuid>) -> bool {
    match project_id {
        Some(selected) => service.project_id == Some(selected),
        None => true,
    }
}

/// Error `detail` is a plain string, a `{code, ...}` object, or for 422 a list
/// of `{loc, msg}` entries.
fn describe_error_detail(detail: &Value) -> Option<String> {
    if let Some(text) = detail.as_str() {
        return Some(text.to_string());
    }

    if detail.is_object() {
        return describe_structured_detail(detail);
    }

    let problems: Vec<String> = detail
        .as_array()?
        .iter()
        .filter_map(|problem| {
            let message = problem["msg"].as_str()?;
            let field = problem["loc"]
                .as_array()
                .and_then(|location| location.last())
                .and_then(|last| last.as_str());
            Some(match field {
                Some(field) => format!("{}: {}", field, message),
                None => message.to_string(),
            })
        })
        .collect();

    (!problems.is_empty()).then(|| problems.join("; "))
}

/// The `detail` field of an error body. A body that is not an object (a proxy
/// answering with a bare string or array) has none.
fn take_detail(mut body: Value) -> Value {
    body.get_mut("detail")
        .map(Value::take)
        .unwrap_or(Value::Null)
}

fn describe_structured_detail(detail: &Value) -> Option<String> {
    let field = |key: &str| detail.get(key).and_then(Value::as_str);
    let message = match field("code")? {
        "insufficient_scope" => {
            let required = field("required").unwrap_or("?");
            let mut message = format!(
                "This action needs a `{}` key (yours: `{}`)",
                required,
                field("actual").unwrap_or("?")
            );
            if required == "admin" {
                message.push_str(
                    ". Create an admin API key in the dashboard and run `runsite login --token <key>`",
                );
            }
            message
        }
        "user_blocked" => match field("reason").filter(|reason| !reason.is_empty()) {
            Some(reason) => format!("Your account is blocked: {}", reason),
            None => "Your account is blocked".to_string(),
        },
        "registration_review_required" => "Your account is awaiting review".to_string(),
        other => other.to_string(),
    };
    Some(message)
}

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
        let resp = self
            .http
            .post(format!("{}/api/api-keys", self.base_url))
            .json(&serde_json::json!({ "name": name, "scope": "write" }))
            .send()
            .await?;
        let value: Value = self.handle_response(resp).await?;
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
            let detail = response
                .json::<Value>()
                .await
                .ok()
                .map(take_detail)
                .unwrap_or(Value::Null);
            let message = describe_error_detail(&detail).unwrap_or_else(|| status.to_string());
            let code = detail["code"].as_str().map(str::to_string);

            Err(ApiError::Http {
                status: status.as_u16(),
                message,
                code,
                detail: detail.is_object().then_some(detail),
            }
            .into())
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let token = self.auth_token().ok_or(ApiError::Unauthenticated)?;
        let resp = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&token)
            .send()
            .await?;
        self.handle_response(resp).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let token = self.auth_token().ok_or(ApiError::Unauthenticated)?;
        let resp = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&token)
            .json(body)
            .send()
            .await?;
        self.handle_response(resp).await
    }

    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.post(path, &serde_json::json!({})).await
    }

    async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let token = self.auth_token().ok_or(ApiError::Unauthenticated)?;
        let resp = self
            .http
            .put(format!("{}{}", self.base_url, path))
            .bearer_auth(&token)
            .json(body)
            .send()
            .await?;
        self.handle_response(resp).await
    }

    async fn delete_req(&self, path: &str) -> Result<()> {
        let token = self.auth_token().ok_or(ApiError::Unauthenticated)?;
        let resp = self
            .http
            .delete(format!("{}{}", self.base_url, path))
            .bearer_auth(&token)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            self.handle_response::<Value>(resp).await.map(|_| ())
        }
    }

    /// Confirm an API key is valid and return its owner.
    pub async fn verify_api_key(&self, key: &str) -> Result<UserResponse> {
        let resp = self
            .http
            .get(format!("{}/api/v1/users/me", self.base_url))
            .bearer_auth(key)
            .send()
            .await?;
        self.handle_response(resp).await
    }

    /// Log in with email + password. The session lives in the client's cookie jar;
    /// the response body carries the user, not a token.
    pub async fn post_login(&self, email: &str, password: &str) -> Result<UserResponse> {
        let resp = self
            .http
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await?;

        self.handle_response(resp).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(name: &str, project_id: Option<&str>) -> WebService {
        WebService {
            id: Uuid::parse_str("8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b").unwrap(),
            name: name.to_string(),
            status: Some("running".to_string()),
            url: None,
            updated_at: None,
            project_id: project_id.map(|id| Uuid::parse_str(id).unwrap()),
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
    fn a_validation_error_names_each_field() {
        let detail = serde_json::json!([
            {"loc": ["body", "port"], "msg": "Input should be less than or equal to 65535"},
            {"loc": ["body", "env_vars", 0, "key"], "msg": "String should match pattern"}
        ]);
        assert_eq!(
            describe_error_detail(&detail).unwrap(),
            "port: Input should be less than or equal to 65535; key: String should match pattern"
        );
    }

    #[test]
    fn a_plain_error_detail_is_kept_as_is() {
        let detail = serde_json::json!("Web service not found");
        assert_eq!(
            describe_error_detail(&detail).as_deref(),
            Some("Web service not found")
        );
    }

    #[test]
    fn an_error_body_that_is_not_an_object_has_no_detail() {
        assert_eq!(take_detail(serde_json::json!("Bad Gateway")), Value::Null);
        assert_eq!(take_detail(serde_json::json!(["x"])), Value::Null);
        assert_eq!(
            take_detail(serde_json::json!({"detail": "Not found"})),
            serde_json::json!("Not found")
        );
    }

    #[test]
    fn an_insufficient_scope_detail_names_both_scopes() {
        let detail = serde_json::json!({"code": "insufficient_scope", "required": "write", "actual": "read"});
        assert_eq!(
            describe_error_detail(&detail).unwrap(),
            "This action needs a `write` key (yours: `read`)"
        );
    }

    #[test]
    fn an_admin_scope_detail_explains_how_to_get_an_admin_key() {
        let detail = serde_json::json!({"code": "insufficient_scope", "required": "admin", "actual": "write"});
        assert_eq!(
            describe_error_detail(&detail).unwrap(),
            "This action needs a `admin` key (yours: `write`). Create an admin API key in the dashboard and run `runsite login --token <key>`"
        );
    }

    #[test]
    fn a_blocked_user_detail_carries_the_reason() {
        let detail = serde_json::json!({"code": "user_blocked", "reason": "unpaid invoice"});
        assert_eq!(
            describe_error_detail(&detail).unwrap(),
            "Your account is blocked: unpaid invoice"
        );
        let without_reason = serde_json::json!({"code": "user_blocked", "reason": ""});
        assert_eq!(
            describe_error_detail(&without_reason).unwrap(),
            "Your account is blocked"
        );
    }

    #[test]
    fn a_registration_review_detail_is_explained() {
        let detail =
            serde_json::json!({"code": "registration_review_required", "status": "pending"});
        assert_eq!(
            describe_error_detail(&detail).unwrap(),
            "Your account is awaiting review"
        );
    }

    #[test]
    fn an_unknown_structured_detail_falls_back_to_its_code() {
        let detail = serde_json::json!({"code": "quota_exceeded"});
        assert_eq!(describe_error_detail(&detail).unwrap(), "quota_exceeded");
        assert_eq!(describe_error_detail(&serde_json::json!({"x": 1})), None);
    }

    #[test]
    fn a_service_without_a_project_is_filtered_out_under_a_selection() {
        assert!(!belongs_to_project(&service("api", None), selected()));
    }
}
