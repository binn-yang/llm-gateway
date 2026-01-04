use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::json;

/// Health check endpoint
/// Returns 200 OK if the service is running
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({
        "status": "healthy",
        "service": "llm-gateway",
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

/// Readiness check endpoint
/// Returns 200 OK if the service is ready to accept traffic
pub async fn readiness_check() -> impl IntoResponse {
    // For now, just return OK
    // In the future, could check provider connectivity, config validity, etc.
    (StatusCode::OK, Json(json!({
        "status": "ready",
        "service": "llm-gateway",
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_check_returns_ok() {
        let response = health_check().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_readiness_check_returns_ok() {
        let response = readiness_check().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
