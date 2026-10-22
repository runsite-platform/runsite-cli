use super::ApiClient;
use anyhow::Result;

/// What the user typed on a login form or passed on the command line.
pub enum Credentials {
    Password { email: String, password: String },
    ApiKey(String),
}

pub struct LoginOutcome {
    pub email: String,
    pub api_key: String,
}

impl ApiClient {
    /// Turn credentials into a verified API key. Nothing is written to the config.
    ///
    /// A password login creates a new `write` key named after this machine; the
    /// session cookie from `/api/auth/login` authorizes that request.
    pub async fn log_in(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        match credentials {
            Credentials::ApiKey(key) => {
                let user = self.verify_api_key(key).await?;
                Ok(LoginOutcome {
                    email: user.email,
                    api_key: key.clone(),
                })
            }
            Credentials::Password { email, password } => {
                let user = self.post_login(email, password).await?;
                let api_key = self.mint_api_key(&machine_key_name()).await?;
                Ok(LoginOutcome {
                    email: user.email,
                    api_key,
                })
            }
        }
    }
}

fn machine_key_name() -> String {
    let host = hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .unwrap_or_else(|| "cli".to_string());
    format!("cli-{host}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::error::ApiError;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const USER: &str = r#"{"id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "email": "ada@example.com", "name": "Ada"}"#;

    fn anonymous_client(server: &MockServer) -> ApiClient {
        ApiClient::new(
            server.uri(),
            Arc::new(Mutex::new(Config::default())),
            "default".to_string(),
        )
    }

    #[tokio::test]
    async fn a_pasted_key_is_verified_and_returned() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/users/me"))
            .and(header("authorization", "Bearer ak_live_pasted"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(USER, "application/json"))
            .expect(1)
            .mount(&server)
            .await;

        let outcome = anonymous_client(&server)
            .log_in(&Credentials::ApiKey("ak_live_pasted".to_string()))
            .await
            .unwrap();

        assert_eq!(outcome.email, "ada@example.com");
        assert_eq!(outcome.api_key, "ak_live_pasted");
    }

    #[tokio::test]
    async fn a_password_login_mints_a_write_key_with_the_session_cookie() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_partial_json(
                serde_json::json!({"email": "ada@example.com", "password": "hunter2"}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "access_token=session123; Path=/")
                    .set_body_raw(USER, "application/json"),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/api-keys"))
            .and(header("cookie", "access_token=session123"))
            .and(body_partial_json(serde_json::json!({"scope": "write"})))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!({"plaintext_key": "ak_live_minted"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let outcome = anonymous_client(&server)
            .log_in(&Credentials::Password {
                email: "ada@example.com".to_string(),
                password: "hunter2".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(outcome.email, "ada@example.com");
        assert_eq!(outcome.api_key, "ak_live_minted");
    }

    #[tokio::test]
    async fn an_account_under_review_is_rejected_when_minting_the_key() {
        // `/api/auth/login` only checks the password; the review and blocked
        // checks run on the next, cookie-authenticated request.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "access_token=session123; Path=/")
                    .set_body_raw(USER, "application/json"),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/api-keys"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "detail": {"code": "registration_review_required", "status": "pending"}
            })))
            .mount(&server)
            .await;

        let error = anonymous_client(&server)
            .log_in(&Credentials::Password {
                email: "ada@example.com".to_string(),
                password: "hunter2".to_string(),
            })
            .await
            .err()
            .unwrap();

        match error.downcast_ref::<ApiError>() {
            Some(ApiError::Http {
                status: 403,
                code: Some(code),
                message,
                ..
            }) => {
                assert_eq!(code, "registration_review_required");
                assert_eq!(message, "Your account is awaiting review");
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }
}
