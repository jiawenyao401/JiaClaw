use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Invalid idempotency key: {0}")]
    InvalidIdempotencyKey(String),

    #[error("API error (status {status}): {message}")]
    ApiError { status: u16, message: String },
}

pub type Result<T> = std::result::Result<T, Error>;
