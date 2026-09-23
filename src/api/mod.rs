mod login;
mod types;
pub use login::Credentials;
pub use types::*;

use crate::config::Config;
use crate::error::ApiError;
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// True when `service` sits in `project_id`, or when no project is selected.
pub fn belongs_to_project(service: &WebService, project_id: Option<&str>) -> bool {
    match project_id {
        Some(pid) => service.project_id.map(|id| id.to_string()).as_deref() == Some(pid),
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

fn describe_structured_detail(detail: &Value) -> Option<String> {
    let field = |key: &str| detail.get(key).and_then(Value::as_str);
    let message = match field("code")? {
        "insufficient_scope" => format!(
            "This action needs a `{}` key (yours: `{}`)",
            field("required").unwrap_or("?"),
            field("actual").unwrap_or("?")
        ),
        "user_blocked" => match field("reason").filter(|reason| !reason.is_empty()) {
            Some(reason) => format!("Your account is blocked: {}", reason),
            None => "Your account is blocked".to_string(),
        },
        "registration_review_required" => "Your account is awaiting review".to_string(),
        other => other.to_string(),
    };
    Some(message)
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

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    pub base_url: String,
    config: Arc<Mutex<Config>>,
    profile_name: String,
}

impl ApiClient {
    pub fn new(base_url: String, config: Arc<Mutex<Config>>, profile_name: String) -> Self {
        // The login flow authenticates with httpOnly cookies: /api/auth/login sets
        // `access_token`, which the follow-up /api/api-keys call must carry.
        let http = Client::builder()
            .cookie_store(true)
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            base_url,
            config,
            profile_name,
        }
    }

    /// Project selected with `runsite project use`, if any.
    pub fn current_project_id(&self) -> Option<String> {
        self.config
            .lock()
            .unwrap()
            .profiles
            .get(&self.profile_name)
            .and_then(|p| p.current_project_id.clone())
    }

    fn auth_token(&self) -> Option<String> {
        if let Ok(token) = std::env::var("RUNSITE_API_TOKEN") {
            return Some(token);
        }
        let cfg = self.config.lock().unwrap();
        cfg.profiles.get(&self.profile_name)?.api_key.clone()
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
                .map(|mut body| body["detail"].take())
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

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let token = self.auth_token().ok_or(ApiError::Unauthenticated)?;
        let resp = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&token)
            .send()
            .await?;
        self.handle_response(resp).await
    }

    pub async fn post<T: DeserializeOwned>(
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

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.post(path, &serde_json::json!({})).await
    }

    pub async fn put<T: DeserializeOwned>(
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

    pub async fn delete_req(&self, path: &str) -> Result<()> {
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

    // Resolve service name to UUID
    pub async fn resolve_service_id(&self, name_or_id: Option<&str>) -> Result<Uuid> {
        if let Some(val) = name_or_id {
            if let Ok(id) = Uuid::parse_str(val) {
                return Ok(id);
            }
        }

        let list: WebServiceList = self.get("/api/v1/web-services").await?;

        let project_id = self.current_project_id();

        let candidates: Vec<_> = list
            .web_services
            .iter()
            .filter(|s| belongs_to_project(s, project_id.as_deref()))
            .collect();

        if let Some(name) = name_or_id {
            let matched: Vec<_> = candidates.iter().filter(|s| s.name == name).collect();
            match matched.len() {
                0 => Err(self.service_not_found_error(name, &list).await),
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

    pub async fn resolve_project_id(&self, name_or_id: &str) -> Result<Uuid> {
        if let Ok(id) = Uuid::parse_str(name_or_id) {
            return Ok(id);
        }

        let data: ProjectList = self.get("/api/v1/projects").await?;
        let matched: Vec<_> = data
            .projects
            .iter()
            .filter(|p| p.name == name_or_id)
            .collect();

        match matched.len() {
            0 => Err(anyhow!("project '{}' not found", name_or_id)),
            1 => Ok(matched[0].id),
            _ => Err(anyhow!("multiple projects named '{}'", name_or_id)),
        }
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

        let data: DeploymentList = self
            .get(&format!(
                "/api/v1/web-services/{}/deployments?limit=100",
                service_id
            ))
            .await?;
        let ids: Vec<Uuid> = data.deployments.iter().map(|d| d.id).collect();
        match_id_prefix(&ids, id_or_prefix)
    }

    /// Build the error for a name that resolved to nothing in the current project.
    ///
    /// `service list` and this resolver used to disagree: the list showed every
    /// service on the account while the resolver only accepted ones in the
    /// selected project, so a name printed by the list could still be rejected
    /// here. The list now filters too, and this points at the owning project
    /// whenever the name exists somewhere else.
    async fn service_not_found_error(&self, name: &str, list: &WebServiceList) -> anyhow::Error {
        let elsewhere: Vec<_> = list
            .web_services
            .iter()
            .filter(|s| s.name == name)
            .collect();

        let Some(service) = elsewhere.first() else {
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

    /// Best-effort project name lookup; falls back to `None` when the extra
    /// request fails, so error reporting never masks the original problem.
    pub async fn project_name(&self, project_id: Uuid) -> Option<String> {
        let list: ProjectList = self.get("/api/v1/projects").await.ok()?;
        list.projects
            .into_iter()
            .find(|p| p.id == project_id)
            .map(|p| p.name)
    }

    // WebSocket URL helper: replaces http(s) scheme with ws(s)
    // Dormant under API-key auth: WebSocket routes are JWT-only. Kept for future rewiring.
    #[allow(dead_code)]
    pub fn ws_url(&self, path: &str) -> String {
        let base = self
            .base_url
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        format!("{}{}", base, path)
    }

    #[allow(dead_code)]
    pub fn ws_token(&self) -> Option<String> {
        self.auth_token()
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

    const PROJECT: &str = "ca83c4a9-1517-4fe3-93aa-bbdab4eaee23";
    const OTHER_PROJECT: &str = "8d5b5e8e-2348-4a29-8c05-bcfa1e599ef2";

    #[test]
    fn a_service_in_the_selected_project_belongs_to_it() {
        assert!(belongs_to_project(
            &service("api", Some(PROJECT)),
            Some(PROJECT)
        ));
    }

    #[test]
    fn a_service_from_another_project_is_filtered_out() {
        assert!(!belongs_to_project(
            &service("api", Some(OTHER_PROJECT)),
            Some(PROJECT)
        ));
    }

    #[test]
    fn every_service_is_listed_when_no_project_is_selected() {
        assert!(belongs_to_project(&service("api", Some(PROJECT)), None));
        assert!(belongs_to_project(&service("api", None), None));
    }

    const DEPLOYMENT: &str = "3f2a1b4c-0000-4000-8000-000000000001";
    const SIBLING_DEPLOYMENT: &str = "3f2a9999-0000-4000-8000-000000000002";

    fn deployment_ids() -> Vec<Uuid> {
        vec![
            Uuid::parse_str(DEPLOYMENT).unwrap(),
            Uuid::parse_str(SIBLING_DEPLOYMENT).unwrap(),
        ]
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
    fn an_insufficient_scope_detail_names_both_scopes() {
        let detail = serde_json::json!({"code": "insufficient_scope", "required": "write", "actual": "read"});
        assert_eq!(
            describe_error_detail(&detail).unwrap(),
            "This action needs a `write` key (yours: `read`)"
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
    fn a_unique_prefix_resolves_to_its_deployment() {
        let resolved = match_id_prefix(&deployment_ids(), "3F2A1B4C").unwrap();
        assert_eq!(resolved.to_string(), DEPLOYMENT);
    }

    #[test]
    fn an_ambiguous_prefix_is_rejected() {
        let error = match_id_prefix(&deployment_ids(), "3f2a").unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn an_unknown_or_too_short_prefix_is_rejected() {
        assert!(match_id_prefix(&deployment_ids(), "ffff").is_err());
        assert!(match_id_prefix(&deployment_ids(), "3f").is_err());
    }

    #[test]
    fn a_service_without_a_project_is_filtered_out_under_a_selection() {
        assert!(!belongs_to_project(&service("api", None), Some(PROJECT)));
    }
}
