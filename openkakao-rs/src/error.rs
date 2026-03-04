use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum KakaoError {
    #[error("Token expired or invalid")]
    TokenExpired,
    #[error("API error (status={status}): {message}")]
    ApiError { status: i64, message: String },
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
