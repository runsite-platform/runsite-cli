use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    #[error("Not authenticated. Run 'runsite login'")]
    Unauthenticated,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}
