use crate::error::AppError;
use crate::load_balancer::{LoadBalancer, ProviderInstance};
use std::future::Future;

/// Execute a request for a single API key with automatic failover detection
///
/// This function:
/// 1. Selects a provider instance using sticky session for the given API key
/// 2. Executes the request function with the selected instance
/// 3. If the request fails with an instance-level failure, marks the instance as unhealthy
/// 4. Returns the result (success or error)
///
/// Note: This does NOT retry immediately. Instead, the next request from the same API key
/// will automatically select a different healthy instance.
pub async fn execute_with_session<F, Fut, T>(
    load_balancer: &LoadBalancer,
    api_key: &str,
    request_fn: F,
) -> Result<T, AppError>
where
    F: Fn(ProviderInstance) -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    // Select instance for this API key (sticky session)
    let instance = load_balancer
        .select_instance_for_key(api_key)
        .await
        .ok_or_else(|| AppError::NoHealthyInstances("No healthy instances available".to_string()))?;

    // Execute the request
    match request_fn(instance.clone()).await {
        Ok(result) => {
            // Record successful instance request
            crate::metrics::record_instance_request(
                load_balancer.provider_name(),
                &instance.name,
                "success",
            );
            Ok(result)
        }
        Err(e) => {
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
                    api_key = api_key,
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

        // HTTP client errors (generic string) - treat as potential instance failure
        AppError::HttpClientError(_) => true,

        // These are NOT instance failures - they're business/client errors
        AppError::Unauthorized(_) => false,
        AppError::ModelNotFound(_) => false,
        AppError::ProviderDisabled(_) => false,
        AppError::ConversionError(_) => false,
        AppError::ConfigError(_) => false,
        AppError::InternalError(_) => false,
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
    fn test_is_instance_failure_http_client_error() {
        // Generic HTTP client errors are treated as potential instance failures
        assert!(is_instance_failure(&AppError::HttpClientError("Connection failed".to_string())));
    }
}
