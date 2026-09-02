use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentication is required; run `ga4 auth login --client-secret <path>`")]
    Unauthenticated,
    #[error("the signed-in account cannot access this Google Analytics resource")]
    PermissionDenied,
    #[error("the Google Analytics API is not enabled for the OAuth project")]
    ApiDisabled,
    #[error("the report uses incompatible dimensions or metrics: {0}")]
    IncompatibleQuery(String),
    #[error("Google Analytics quota is exhausted{retry}", retry = retry_hint(*.0))]
    QuotaLimited(Option<Duration>),
    #[error("network unavailable: {0}")]
    Offline(String),
    #[error("Google Analytics service failed: {0}")]
    Server(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("system keyring unavailable: {0}")]
    Keyring(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("{0}")]
    Other(String),
}

fn retry_hint(retry: Option<Duration>) -> String {
    retry
        .map(|duration| format!("; retry in {} seconds", duration.as_secs()))
        .unwrap_or_default()
}

impl AppError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::InvalidInput(_) | Self::Config(_) => 2,
            Self::Unauthenticated => 3,
            Self::PermissionDenied | Self::ApiDisabled => 4,
            Self::QuotaLimited(_) => 5,
            Self::Offline(_) | Self::Server(_) => 6,
            _ => 1,
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
