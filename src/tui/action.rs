use crate::api::{
    ApiKeyIdentity, Credentials, CurrentUser, ProjectDetail, ProjectSummary, ServiceInfo,
};
use crate::error::ApiError;
use chrono::{DateTime, Utc};
use ratatui::crossterm::event::KeyEvent;
use uuid::Uuid;

/// Everything the TUI fetches. Also the key the poller schedules by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Request {
    CurrentUser,
    KeyScope,
    Projects,
    ProjectDetail(Uuid),
    UnassignedServices,
}

#[derive(Debug)]
pub enum Payload {
    CurrentUser(CurrentUser),
    KeyScope(ApiKeyIdentity),
    Projects(Vec<ProjectSummary>),
    ProjectDetail(ProjectDetail),
    UnassignedServices(Vec<ServiceInfo>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    Unauthorized {
        message: String,
    },
    Blocked {
        reason: String,
    },
    Forbidden {
        message: String,
    },
    NotFound {
        message: String,
    },
    RateLimited,
    /// Network failure or 5xx: the data on screen goes stale.
    Offline {
        message: String,
    },
    Rejected {
        status: u16,
        message: String,
    },
}

impl FetchError {
    pub fn from_error(error: &anyhow::Error) -> Self {
        if let Some(api_error) = error.downcast_ref::<ApiError>() {
            return match api_error {
                ApiError::Unauthenticated => FetchError::Unauthorized {
                    message: api_error.to_string(),
                },
                ApiError::Network(inner) => FetchError::Offline {
                    message: inner.to_string(),
                },
                ApiError::Http {
                    status,
                    message,
                    code,
                    detail,
                } => classify_http(*status, message, code.as_deref(), detail.as_ref()),
            };
        }
        match error.downcast_ref::<reqwest::Error>() {
            Some(inner) if !inner.is_decode() => FetchError::Offline {
                message: inner.to_string(),
            },
            _ => FetchError::Rejected {
                status: 0,
                message: format!("{:#}", error),
            },
        }
    }

    pub fn message(&self) -> String {
        match self {
            FetchError::Unauthorized { message }
            | FetchError::Forbidden { message }
            | FetchError::NotFound { message }
            | FetchError::Offline { message }
            | FetchError::Rejected { message, .. } => message.clone(),
            FetchError::Blocked { reason } if reason.is_empty() => {
                "Your account is blocked".to_string()
            }
            FetchError::Blocked { reason } => format!("Your account is blocked: {reason}"),
            FetchError::RateLimited => "Rate limit reached, try again in a minute".to_string(),
        }
    }
}

fn classify_http(
    status: u16,
    message: &str,
    code: Option<&str>,
    detail: Option<&serde_json::Value>,
) -> FetchError {
    let message = message.to_string();
    match (status, code) {
        (401, _) => FetchError::Unauthorized { message },
        (403, Some("user_blocked")) => FetchError::Blocked {
            reason: detail
                .and_then(|detail| detail["reason"].as_str())
                .unwrap_or_default()
                .to_string(),
        },
        (403, _) => FetchError::Forbidden { message },
        (404, _) => FetchError::NotFound { message },
        (429, _) => FetchError::RateLimited,
        (500..=599, _) => FetchError::Offline { message },
        _ => FetchError::Rejected { status, message },
    }
}

pub enum Action {
    Key(KeyEvent),
    Resize {
        width: u16,
        height: u16,
    },
    Tick {
        now: DateTime<Utc>,
    },
    Loaded {
        generation: u64,
        request: Request,
        result: Result<Payload, FetchError>,
    },
    /// Carries the logged-in email.
    LoggedIn {
        generation: u64,
        result: Result<String, FetchError>,
    },
    ProfilesListed(Vec<ProfileEntry>),
    /// SIGTERM, SIGHUP or the console window closing.
    Terminate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEntry {
    pub name: String,
    pub api_url: String,
    pub has_key: bool,
    pub current_project_id: Option<Uuid>,
}

pub enum Effect {
    Fetch {
        generation: u64,
        request: Request,
    },
    LogIn {
        generation: u64,
        credentials: Credentials,
    },
    ListProfiles,
    SwitchProfile {
        profile: String,
        api_url: String,
    },
    Exit,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: u16, code: Option<&str>, detail: Option<serde_json::Value>) -> anyhow::Error {
        ApiError::Http {
            status,
            message: "detail text".to_string(),
            code: code.map(str::to_string),
            detail,
        }
        .into()
    }

    #[test]
    fn http_statuses_map_to_their_handling_class() {
        assert!(matches!(
            FetchError::from_error(&http(401, None, None)),
            FetchError::Unauthorized { .. }
        ));
        assert_eq!(
            FetchError::from_error(&http(429, None, None)),
            FetchError::RateLimited
        );
        assert!(matches!(
            FetchError::from_error(&http(503, None, None)),
            FetchError::Offline { .. }
        ));
        assert!(matches!(
            FetchError::from_error(&http(404, None, None)),
            FetchError::NotFound { .. }
        ));
        assert_eq!(
            FetchError::from_error(&http(400, None, None)),
            FetchError::Rejected {
                status: 400,
                message: "detail text".to_string()
            }
        );
    }

    #[test]
    fn a_blocked_user_carries_the_reason() {
        let error = http(
            403,
            Some("user_blocked"),
            Some(serde_json::json!({"code": "user_blocked", "reason": "abuse report"})),
        );
        assert_eq!(
            FetchError::from_error(&error),
            FetchError::Blocked {
                reason: "abuse report".to_string()
            }
        );
    }

    #[test]
    fn other_403s_keep_the_server_message() {
        let error = http(403, Some("insufficient_scope"), None);
        assert_eq!(
            FetchError::from_error(&error),
            FetchError::Forbidden {
                message: "detail text".to_string()
            }
        );
    }

    #[test]
    fn a_missing_key_is_unauthorized() {
        let error: anyhow::Error = ApiError::Unauthenticated.into();
        assert!(matches!(
            FetchError::from_error(&error),
            FetchError::Unauthorized { .. }
        ));
    }
}
