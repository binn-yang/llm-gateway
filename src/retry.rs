use crate::error::AppError;
use crate::load_balancer::{LoadBalancer, ProviderInstance};
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;

/// Execute a request for a single API key name with automatic failover detection
///
/// This function:
/// 1. Selects a provider instance using sticky session for the given API key name
/// 2. Executes the request function with the selected instance
/// 3. Applies request-level timeout based on instance configuration
/// 4. If the request fails with an instance-level failure, marks the instance as unhealthy
/// 5. Returns the result (success or error)
///
/// Note: This does NOT retry immediately. Instead, the next request from the same API key
/// will automatically select a different healthy instance.
///
/// # Security
/// The `api_key_name` parameter should be a friendly name (e.g., "my-app"), not the actual API key.
/// Actual API keys are never logged.
pub async fn execute_with_session<F, Fut, T>(
    load_balancer: &LoadBalancer,
    api_key_name: &str,
    request_fn: F,
) -> Result<T, AppError>
where
    F: Fn(ProviderInstance) -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    // Select instance for this API key name (sticky session)
    let instance = load_balancer
        .select_instance_for_key(api_key_name)
        .await
        .ok_or_else(|| AppError::NoHealthyInstances("No healthy instances available".to_string()))?;

    // Get timeout from instance configuration
    let timeout_duration = Duration::from_secs(instance.config.timeout_seconds());

    // Execute the request with timeout
    let request_result = timeout(
        timeout_duration,
        request_fn(instance.clone())
    ).await;

    match request_result {
        Ok(Ok(result)) => {
            // Record successful instance request
            crate::metrics::record_instance_request(
                load_balancer.provider_name(),
                &instance.name,
                "success",
            );
            Ok(result)
        }
        Ok(Err(e)) => {
            // Request failed (not timeout)
            // Check if this is an instance-level failure (vs business error)
            if is_instance_failure(&e) {
                // Record failed instance request
                crate::metrics::record_instance_request(
                    load_balancer.provider_name(),
                    &instance.name,
                    "failure",
                );

                load_balancer.mark_instance_failure(&instance.name).await;

                tracing::warn!(
                    api_key_name = %crate::logging::sanitize_log_value(api_key_name),
                    instance = %instance.name,
                    error = %e,
                    "Instance marked unhealthy, session will failover on next request"
                );
            } else {
                // Record business error (not instance failure)
                crate::metrics::record_instance_request(
                    load_balancer.provider_name(),
                    &instance.name,
                    "business_error",
                );
            }
            Err(e)
        }
        Err(_) => {
            // Request timed out - treat as instance failure
            crate::metrics::record_instance_request(
                load_balancer.provider_name(),
                &instance.name,
                "failure",
            );

            load_balancer.mark_instance_failure(&instance.name).await;

            let timeout_error = AppError::InternalError(format!(
                "Request timed out after {} seconds",
                timeout_duration.as_secs()
            ));

            tracing::warn!(
                api_key_name = %crate::logging::sanitize_log_value(api_key_name),
                instance = %instance.name,
                timeout_seconds = timeout_duration.as_secs(),
                "Request timed out, instance marked unhealthy"
            );

            Err(timeout_error)
        }
    }
}

/// Determine if an error represents an instance-level failure
///
/// Instance failures are conditions that indicate the provider instance itself is unhealthy,
/// rather than business logic errors or client errors.
///
/// ## Instance Failures (returns true):
/// - Connection failures (TCP timeout, connection refused, DNS failures)
/// - Request timeouts
/// - HTTP 5xx errors (500, 502, 503, 504)
///
/// ## NOT Instance Failures (returns false):
/// - HTTP 4xx errors (client errors like 401, 403, 429)
/// - Business logic errors (invalid API key, insufficient balance)
/// - Conversion/protocol errors
pub fn is_instance_failure(error: &AppError) -> bool {
    match error {
        // HTTP request errors - check for connection/timeout issues
        AppError::HttpRequest(e) => {
            // Connection failures
            if e.is_connect() {
                return true;
            }
            // Request timeouts
            if e.is_timeout() {
                return true;
            }
            // Check if it's a status code error (5xx)
            if let Some(status) = e.status() {
                return status.is_server_error(); // 5xx
            }
            false
        }

        // Upstream errors - check status code
        AppError::UpstreamError { status, .. } => {
            matches!(
                status.as_u16(),
                500 | 502 | 503 | 504
            )
        }

        // These are NOT instance failures - they're business/client errors
        AppError::Unauthorized(_) => false,
        AppError::ModelNotFound(_) => false,
        AppError::ProviderDisabled(_) => false,
        AppError::ConversionError(_) => false,
        AppError::ConfigError(_) => false,
        // Internal errors are NOT instance failures EXCEPT for timeouts
        AppError::InternalError(msg) => {
            // Check if this is a timeout error from our request-level timeout
            msg.contains("timed out") || msg.contains("timeout")
        }
        AppError::NoHealthyInstances(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn test_is_instance_failure_5xx_errors() {
        // 5xx errors should be considered instance failures
        let error = AppError::UpstreamError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Internal error".to_string(),
        };
        assert!(is_instance_failure(&error));

        let error = AppError::UpstreamError {
            status: StatusCode::BAD_GATEWAY,
            message: "Bad gateway".to_string(),
        };
        assert!(is_instance_failure(&error));

        let error = AppError::UpstreamError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: "Service unavailable".to_string(),
        };
        assert!(is_instance_failure(&error));

        let error = AppError::UpstreamError {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: "Gateway timeout".to_string(),
        };
        assert!(is_instance_failure(&error));
    }

    #[test]
    fn test_is_instance_failure_4xx_errors() {
        // 4xx errors should NOT be considered instance failures
        let error = AppError::UpstreamError {
            status: StatusCode::UNAUTHORIZED,
            message: "Unauthorized".to_string(),
        };
        assert!(!is_instance_failure(&error));

        let error = AppError::UpstreamError {
            status: StatusCode::FORBIDDEN,
            message: "Forbidden".to_string(),
        };
        assert!(!is_instance_failure(&error));

        let error = AppError::UpstreamError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "Rate limited".to_string(),
        };
        assert!(!is_instance_failure(&error));
    }

    #[test]
    fn test_is_instance_failure_business_errors() {
        // Business errors should NOT trigger instance failure
        assert!(!is_instance_failure(&AppError::Unauthorized("Invalid API key".to_string())));
        assert!(!is_instance_failure(&AppError::ModelNotFound("gpt-5".to_string())));
        assert!(!is_instance_failure(&AppError::ConversionError("Invalid format".to_string())));
    }

    #[test]
    fn test_is_instance_failure_timeout() {
        // Timeout errors SHOULD trigger instance failure
        assert!(is_instance_failure(&AppError::InternalError(
            "Request timed out after 300 seconds".to_string()
        )));
        assert!(is_instance_failure(&AppError::InternalError(
            "timeout".to_string()
        )));

        // Other internal errors should NOT trigger instance failure
        assert!(!is_instance_failure(&AppError::InternalError(
            "Some other internal error".to_string()
        )));
    }
}
