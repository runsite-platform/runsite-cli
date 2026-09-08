mod types;
pub use types::*;

use crate::config::Config;
use crate::error::ApiError;
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

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
            let message = response
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| v["detail"].as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| status.to_string());

            Err(ApiError::Http {
                status: status.as_u16(),
                message,
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
            let status = resp.status().as_u16();
            Err(ApiError::Http {
                status,
                message: resp.status().to_string(),
            }
            .into())
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

        let project_id = self
            .config
            .lock()
            .unwrap()
            .profiles
            .get(&self.profile_name)
            .and_then(|p| p.current_project_id.clone());

        let candidates: Vec<_> = list
            .web_services
            .iter()
            .filter(|s| {
                if let Some(pid) = &project_id {
                    s.project_id.map(|id| id.to_string()).as_deref() == Some(pid.as_str())
                } else {
                    true
                }
            })
            .collect();

        if let Some(name) = name_or_id {
            let matched: Vec<_> = candidates.iter().filter(|s| s.name == name).collect();
            match matched.len() {
                0 => Err(anyhow!("service '{}' not found", name)),
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
