use axum::{extract::State, http::StatusCode, response::IntoResponse};
use metrics_exporter_prometheus::PrometheusHandle;
use std::sync::Arc;

/// Handle /metrics endpoint
pub async fn metrics(State(handle): State<Arc<PrometheusHandle>>) -> impl IntoResponse {
    let metrics = handle.render();
    (StatusCode::OK, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_handler() {
        // Create a handle for testing without initializing global recorder
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new()
            .build_recorder();
        let handle = recorder.handle();
        let state = Arc::new(handle);

        let response = metrics(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
