use thiserror::Error;

/// Primary error type for openkakao-rs operations.
#[derive(Error, Debug)]
pub enum OpenKakaoError {
    #[error("LOCO command {command} failed (status={status})")]
    LocoStatus {
        command: String,
        status: i64,
        body: Option<bson::Document>,
    },

    #[error("Token expired or invalid (status=-950)")]
    TokenExpired,

    #[error("Network error: {message}")]
    Network { message: String, is_transient: bool },

    #[error("REST API error (status={status}): {message}")]
    RestApi { status: i64, message: String },

    #[error("Safety block: {0}")]
    SafetyBlock(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl OpenKakaoError {
    /// Whether this error is transient and the operation should be retried.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::LocoStatus { status, .. } => matches!(status, -300 | -500),
            Self::TokenExpired => true,
            Self::Network { is_transient, .. } => *is_transient,
            _ => false,
        }
    }

    /// Create a LOCO status error from a command name and response status.
    pub fn loco(command: impl Into<String>, status: i64) -> Self {
        if status == -950 {
            Self::TokenExpired
        } else {
            Self::LocoStatus {
                command: command.into(),
                status,
                body: None,
            }
        }
    }

    /// Create a LOCO status error with the response body attached.
    pub fn loco_with_body(command: impl Into<String>, status: i64, body: bson::Document) -> Self {
        if status == -950 {
            Self::TokenExpired
        } else {
            Self::LocoStatus {
                command: command.into(),
                status,
                body: Some(body),
            }
        }
    }
}

impl From<reqwest::Error> for OpenKakaoError {
    fn from(e: reqwest::Error) -> Self {
        let is_transient = e.is_timeout() || e.is_connect();
        Self::Network {
            message: e.to_string(),
            is_transient,
        }
    }
}

impl From<std::io::Error> for OpenKakaoError {
    fn from(e: std::io::Error) -> Self {
        let is_transient = matches!(
            e.kind(),
            std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::TimedOut
        );
        Self::Network {
            message: e.to_string(),
            is_transient,
        }
    }
}
