use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Backend API error: {0}")]
    BackendApiError(String),
    #[error("Auth error: {0}")]
    AuthError(String),
    #[error("Config error: {0}")]
    ConfigError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, ProxyError>;
