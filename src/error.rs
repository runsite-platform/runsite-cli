use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    /// `code` and `detail` are set when the API answered with a structured
    /// `detail` object such as `{"code": "insufficient_scope", ...}`.
    #[error("HTTP {status}: {message}")]
    Http {
        status: u16,
        message: String,
        code: Option<String>,
        detail: Option<Value>,
    },

    #[error("Not authenticated. Run 'runsite login'")]
    Unauthenticated,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}
