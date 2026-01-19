use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::fmt;

/// Application error types
#[derive(Debug)]
pub enum AppError {
    /// Configuration error
    ConfigError(String),
    /// Authentication error
    Unauthorized(String),
    /// Model routing error
    ModelNotFound(String),
    /// Provider not enabled
    ProviderDisabled(String),
    /// Protocol conversion error
    ConversionError(String),
    /// Upstream API error
    UpstreamError { status: StatusCode, message: String },
    /// Internal server error
    InternalError(String),
    /// No healthy provider instances available
    NoHealthyInstances(String),
    /// HTTP request error (preserves reqwest::Error for health detection)
    HttpRequest(reqwest::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Self::ModelNotFound(msg) => write!(f, "Model not found: {}", msg),
            Self::ProviderDisabled(msg) => write!(f, "Provider disabled: {}", msg),
            Self::ConversionError(msg) => write!(f, "Conversion error: {}", msg),
            Self::UpstreamError { status, message } => {
                write!(f, "Upstream error ({}): {}", status, message)
            }
            Self::InternalError(msg) => write!(f, "Internal error: {}", msg),
            Self::NoHealthyInstances(msg) => write!(f, "No healthy instances: {}", msg),
            Self::HttpRequest(err) => write!(f, "HTTP request error: {}", err),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            Self::ConfigError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::ModelNotFound(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::ProviderDisabled(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            Self::ConversionError(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::UpstreamError { status, message } => (*status, message.clone()),
            Self::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            Self::NoHealthyInstances(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            Self::HttpRequest(err) => (StatusCode::BAD_GATEWAY, err.to_string()),
        };

        let body = Json(json!({
            "error": {
                "message": error_message,
                "type": error_type_name(&self),
            }
        }));

        (status, body).into_response()
    }
}

fn error_type_name(error: &AppError) -> &'static str {
    match error {
        AppError::ConfigError(_) => "config_error",
        AppError::Unauthorized(_) => "unauthorized",
        AppError::ModelNotFound(_) => "model_not_found",
        AppError::ProviderDisabled(_) => "provider_disabled",
        AppError::ConversionError(_) => "conversion_error",
        AppError::UpstreamError { .. } => "upstream_error",
        AppError::InternalError(_) => "internal_error",
        AppError::NoHealthyInstances(_) => "no_healthy_instances",
        AppError::HttpRequest(_) => "http_request_error",
    }
}

// Implement conversions from common error types
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::InternalError(err.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        Self::HttpRequest(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::ConversionError(format!("JSON error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = AppError::ModelNotFound("gpt-5".to_string());
        assert_eq!(error.to_string(), "Model not found: gpt-5");
    }

    #[test]
    fn test_error_type_name() {
        assert_eq!(error_type_name(&AppError::Unauthorized("test".to_string())), "unauthorized");
        assert_eq!(error_type_name(&AppError::ModelNotFound("test".to_string())), "model_not_found");
    }

    #[tokio::test]
    async fn test_error_response() {
        let error = AppError::Unauthorized("Invalid API key".to_string());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
