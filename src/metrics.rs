use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Initialize Prometheus metrics exporter
/// Returns None if the recorder is already installed (e.g., in tests)
pub fn init_metrics() -> PrometheusHandle {
    // Create PrometheusBuilder
    let builder = PrometheusBuilder::new();

    // Install the exporter
    let handle = builder
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    init_metric_descriptions();

    handle
}

/// Initialize metric descriptions (can be called multiple times safely)
fn init_metric_descriptions() {
    // Describe all metrics
    describe_counter!(
        "llm_requests_total",
        "Total number of LLM API requests"
    );
    describe_counter!(
        "llm_tokens_total",
        "Total number of tokens processed"
    );
    describe_histogram!(
        "llm_request_duration_seconds",
        "Request duration in seconds"
    );
    describe_counter!(
        "llm_errors_total",
        "Total number of errors"
    );
    describe_gauge!(
        "llm_gateway_info",
        "Gateway version and build information"
    );

    // Set gateway info metric
    gauge!("llm_gateway_info", "version" => env!("CARGO_PKG_VERSION")).set(1.0);
}

/// Record a request
pub fn record_request(api_key: &str, provider: &str, model: &str, endpoint: &str) {
    counter!(
        "llm_requests_total",
        "api_key" => api_key.to_string(),
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "endpoint" => endpoint.to_string(),
    )
    .increment(1);
}

/// Record tokens
pub fn record_tokens(api_key: &str, provider: &str, model: &str, token_type: &str, count: u64) {
    counter!(
        "llm_tokens_total",
        "api_key" => api_key.to_string(),
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "type" => token_type.to_string(),
    )
    .increment(count);
}

/// Record request duration
pub fn record_duration(api_key: &str, provider: &str, model: &str, duration: Duration) {
    histogram!(
        "llm_request_duration_seconds",
        "api_key" => api_key.to_string(),
        "provider" => provider.to_string(),
        "model" => model.to_string(),
    )
    .record(duration.as_secs_f64());
}

/// Record an error
pub fn record_error(api_key: &str, provider: &str, model: &str, error_type: &str) {
    counter!(
        "llm_errors_total",
        "api_key" => api_key.to_string(),
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "error_type" => error_type.to_string(),
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();


    #[test]
    fn test_record_metrics() {
        init_metric_descriptions();

        // Record some metrics
        record_request("test-key", "openai", "gpt-4", "/v1/chat/completions");
        record_tokens("test-key", "openai", "gpt-4", "input", 100);
        record_tokens("test-key", "openai", "gpt-4", "output", 50);
        record_duration("test-key", "openai", "gpt-4", Duration::from_secs(2));
        record_error("test-key", "openai", "gpt-4", "rate_limit");

        // Just verify the function calls don't panic
        // We can't easily verify the metrics are recorded without access to the handle
    }
}
