//! Why a request to the API failed, in the terms the CLI and the TUI act on.

use serde_json::Value;
use thiserror::Error;

pub const NOT_LOGGED_IN: &str = "Not authenticated. Run 'runsite login'";

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ApiError {
    /// No key is stored, or the API refused it (401).
    #[error("{message}")]
    Unauthorized { message: String },

    #[error("{}", blocked_message(reason))]
    Blocked { reason: String },

    /// The key or the account's role does not allow this (403).
    #[error("{message}")]
    Forbidden { message: String },

    #[error("{message}")]
    NotFound { message: String },

    #[error("Rate limit reached, try again in a minute")]
    RateLimited,

    /// A network failure or a 5xx: worth retrying later.
    #[error("{message}")]
    Offline { message: String },

    /// Any other refusal, such as a conflict or invalid input.
    #[error("{message}")]
    Rejected { status: u16, message: String },
}

fn blocked_message(reason: &str) -> String {
    if reason.is_empty() {
        "Your account is blocked".to_string()
    } else {
        format!("Your account is blocked: {reason}")
    }
}

impl ApiError {
    /// Classifies an error response from its status and JSON body.
    pub fn from_response(status: u16, body: Option<Value>) -> Self {
        let detail = body.map(take_detail).unwrap_or(Value::Null);
        let field = |key: &str| detail.get(key).and_then(Value::as_str);
        let message = describe_detail(&detail).unwrap_or_else(|| default_message(status));
        match (status, field("code")) {
            (401, _) => Self::Unauthorized {
                message: format!("{message}. Run 'runsite login'"),
            },
            (403, Some("user_blocked")) => Self::Blocked {
                reason: field("reason").unwrap_or_default().to_string(),
            },
            (403, _) => Self::Forbidden { message },
            (404, _) => Self::NotFound { message },
            (429, _) => Self::RateLimited,
            (500..=599, _) => Self::Offline { message },
            _ => Self::Rejected { status, message },
        }
    }

    pub fn network(error: &reqwest::Error) -> Self {
        Self::Offline {
            message: format!("Network error: {error}"),
        }
    }

    pub fn not_logged_in() -> Self {
        Self::Unauthorized {
            message: NOT_LOGGED_IN.to_string(),
        }
    }

    /// The `ApiError` behind `error`; anything else becomes a rejection that
    /// names it, so no failure goes unreported.
    pub fn classify(error: &anyhow::Error) -> Self {
        if let Some(api_error) = error.downcast_ref::<ApiError>() {
            return api_error.clone();
        }
        match error.downcast_ref::<reqwest::Error>() {
            Some(inner) if !inner.is_decode() => Self::network(inner),
            _ => Self::Rejected {
                status: 0,
                message: format!("{error:#}"),
            },
        }
    }

    /// Network failures, 5xx and 429 are worth retrying with a growing delay.
    pub fn backs_off(&self) -> bool {
        matches!(self, Self::Offline { .. } | Self::RateLimited)
    }
}

fn default_message(status: u16) -> String {
    reqwest::StatusCode::from_u16(status)
        .map(|code| code.to_string())
        .unwrap_or_else(|_| format!("HTTP {status}"))
}

/// The `detail` field of an error body. A body that is not an object (a proxy
/// answering with a bare string or array) has none.
fn take_detail(mut body: Value) -> Value {
    body.get_mut("detail")
        .map(Value::take)
        .unwrap_or(Value::Null)
}

/// `detail` is a plain string, a `{code, ...}` object, or for 422 a list of
/// `{loc, msg}` entries.
fn describe_detail(detail: &Value) -> Option<String> {
    if let Some(text) = detail.as_str() {
        return Some(text.to_string());
    }
    if detail.is_object() {
        return describe_coded_detail(detail);
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
                Some(field) => format!("{field}: {message}"),
                None => message.to_string(),
            })
        })
        .collect();
    (!problems.is_empty()).then(|| problems.join("; "))
}

fn describe_coded_detail(detail: &Value) -> Option<String> {
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
        "registration_review_required" => "Your account is awaiting review".to_string(),
        other => other.to_string(),
    };
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(status: u16, body: Value) -> String {
        ApiError::from_response(status, Some(body)).to_string()
    }

    #[test]
    fn a_validation_error_names_each_field() {
        let body = json!({"detail": [
            {"loc": ["body", "port"], "msg": "Input should be less than or equal to 65535"},
            {"loc": ["body", "env_vars", 0, "key"], "msg": "String should match pattern"}
        ]});
        assert_eq!(
            message(422, body),
            "port: Input should be less than or equal to 65535; key: String should match pattern"
        );
    }

    #[test]
    fn a_plain_detail_is_the_message() {
        let error = ApiError::from_response(404, Some(json!({"detail": "Web service not found"})));
        assert_eq!(
            error,
            ApiError::NotFound {
                message: "Web service not found".to_string()
            }
        );
    }

    #[test]
    fn a_body_that_is_not_an_object_falls_back_to_the_status() {
        assert_eq!(message(502, json!("Bad Gateway")), "502 Bad Gateway");
        assert_eq!(message(400, json!(["x"])), "400 Bad Request");
        assert_eq!(
            ApiError::from_response(503, None),
            ApiError::Offline {
                message: "503 Service Unavailable".to_string()
            }
        );
    }

    #[test]
    fn an_insufficient_scope_names_both_scopes() {
        let body = json!({"detail": {"code": "insufficient_scope", "required": "write", "actual": "read"}});
        assert_eq!(
            ApiError::from_response(403, Some(body)),
            ApiError::Forbidden {
                message: "This action needs a `write` key (yours: `read`)".to_string()
            }
        );
    }

    #[test]
    fn an_admin_scope_explains_how_to_get_an_admin_key() {
        let body = json!({"detail": {"code": "insufficient_scope", "required": "admin", "actual": "write"}});
        assert_eq!(
            message(403, body),
            "This action needs a `admin` key (yours: `write`). Create an admin API key in the dashboard and run `runsite login --token <key>`"
        );
    }

    #[test]
    fn a_blocked_account_carries_the_reason() {
        let body = json!({"detail": {"code": "user_blocked", "reason": "unpaid invoice"}});
        let error = ApiError::from_response(403, Some(body));
        assert_eq!(
            error,
            ApiError::Blocked {
                reason: "unpaid invoice".to_string()
            }
        );
        assert_eq!(error.to_string(), "Your account is blocked: unpaid invoice");
        let without_reason = json!({"detail": {"code": "user_blocked", "reason": ""}});
        assert_eq!(message(403, without_reason), "Your account is blocked");
    }

    #[test]
    fn a_registration_review_is_explained() {
        let body = json!({"detail": {"code": "registration_review_required", "status": "pending"}});
        assert_eq!(message(403, body), "Your account is awaiting review");
    }

    #[test]
    fn an_unknown_code_falls_back_to_the_code_itself() {
        assert_eq!(
            message(400, json!({"detail": {"code": "quota_exceeded"}})),
            "quota_exceeded"
        );
        assert_eq!(message(400, json!({"detail": {"x": 1}})), "400 Bad Request");
    }

    #[test]
    fn statuses_map_to_what_a_client_does_next() {
        assert_eq!(
            message(401, json!({"detail": "Invalid API key"})),
            "Invalid API key. Run 'runsite login'"
        );
        assert_eq!(ApiError::from_response(429, None), ApiError::RateLimited);
        assert!(ApiError::from_response(429, None).backs_off());
        assert!(ApiError::from_response(500, None).backs_off());
        assert!(!ApiError::from_response(409, None).backs_off());
        assert_eq!(
            ApiError::from_response(409, Some(json!({"detail": "Already exists"}))),
            ApiError::Rejected {
                status: 409,
                message: "Already exists".to_string()
            }
        );
    }

    #[test]
    fn a_failure_outside_the_api_is_still_reported() {
        let error = anyhow::anyhow!("failed to parse response");
        assert_eq!(
            ApiError::classify(&error),
            ApiError::Rejected {
                status: 0,
                message: "failed to parse response".to_string()
            }
        );
    }
}
