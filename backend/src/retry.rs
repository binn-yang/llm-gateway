use crate::error::AppError;
use crate::load_balancer::{LoadBalancer, ProviderInstance};
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;

/// Failure type classification for error handling
#[derive(Debug, Clone, PartialEq)]
pub enum FailureType {
    /// Transient error - can retry immediately on other instances
    Transient,
    /// Rate limit error - need delay before retry
    RateLimit { retry_after_secs: u64 },
    /// Instance failure - mark instance unhealthy
    InstanceFailure,
    /// Business error - do not trigger failover
    BusinessError,
}

/// Classify an error to determine how to handle it
///
/// ## Classification Rules:
/// - 401/403 (Auth errors) → InstanceFailure (配置错误)
/// - 429 (Rate Limit) → RateLimit (特殊处理)
/// - 503 (Service Unavailable) → Transient (可能是瞬时过载)
/// - 500/502/504 (Server errors) → InstanceFailure (实例故障)
/// - Connection/Timeout errors → InstanceFailure (网络/实例故障)
/// - Other 4xx errors → BusinessError (客户端错误)
pub fn classify_error(error: &AppError) -> FailureType {
    match error {
        // 401/403 认证错误 - 配置问题，标记实例故障
        AppError::UpstreamError { status, .. } if matches!(status.as_u16(), 401 | 403) => {
            FailureType::InstanceFailure
        }

        // 429 Rate Limit - 特殊处理
        AppError::RateLimitError { retry_after, .. } => FailureType::RateLimit {
            retry_after_secs: retry_after.unwrap_or(2),
        },

        // 503 Service Unavailable - 可能是瞬时过载
        AppError::UpstreamError { status, .. } if status.as_u16() == 503 => {
            FailureType::Transient
        }

        // 502/504 Gateway 错误 - 实例故障
        AppError::UpstreamError { status, .. } if matches!(status.as_u16(), 502 | 504) => {
            FailureType::InstanceFailure
        }

        // 500 Internal Server Error - 实例故障
        AppError::UpstreamError { status, .. } if status.as_u16() == 500 => {
            FailureType::InstanceFailure
        }

        // 连接/超时错误 - 实例故障
        AppError::HttpRequest(e) if e.is_connect() || e.is_timeout() => {
            FailureType::InstanceFailure
        }

        // ServiceOverloaded - 瞬时错误
        AppError::ServiceOverloaded { .. } => FailureType::Transient,

        // 其他 4xx - 业务错误
        AppError::UpstreamError { status, .. } if status.is_client_error() => {
            FailureType::BusinessError
        }

        // Timeout errors - 实例故障
        AppError::InternalError(msg) if msg.contains("timed out") || msg.contains("timeout") => {
            FailureType::InstanceFailure
        }

        // 所有其他错误 - 业务错误
        _ => FailureType::BusinessError,
    }
}

/// Request status for tracking the outcome of a request
#[derive(Debug, Clone, PartialEq)]
pub enum RequestStatus {
    Success,
    InstanceFailure,
    BusinessError,
    Timeout,
}

/// Result of executing a request with session information
///
/// This contains both the actual result (which may be an error) and metadata
/// about which instance was used and what the final status was.
///
/// The `result` field holds `Result<T, AppError>` to preserve error information.
/// The `status` field indicates the overall outcome (success, instance failure, etc.).
pub struct SessionResult<T> {
    pub result: Result<T, AppError>,
    pub instance_name: String,
    pub status: RequestStatus,
}

impl<T> SessionResult<T> {
    /// Create a new successful session result
    pub fn success(result: T, instance_name: String) -> Self {
        Self {
            result: Ok(result),
            instance_name,
            status: RequestStatus::Success,
        }
    }

    /// Create a new instance failure session result
    pub fn instance_failure(error: AppError, instance_name: String) -> Self {
        Self {
            result: Err(error),
            instance_name,
            status: RequestStatus::InstanceFailure,
        }
    }

    /// Create a new business error session result
    pub fn business_error(error: AppError, instance_name: String) -> Self {
        Self {
            result: Err(error),
            instance_name,
            status: RequestStatus::BusinessError,
        }
    }

    /// Create a new timeout session result
    pub fn timeout(error: AppError, instance_name: String) -> Self {
        Self {
            result: Err(error),
            instance_name,
            status: RequestStatus::Timeout,
        }
    }
}

/// Execute a request for a single API key name with automatic failover detection
///
/// This function:
/// 1. Selects a provider instance using sticky session for the given API key name
/// 2. Executes the request function with the selected instance
/// 3. Applies request-level timeout based on instance configuration
/// 4. Classifies errors and handles them appropriately:
///    - Rate limit (429): Delays and retries with different instance
///    - Transient (503): Immediately retries with different instance
///    - Instance failure: Marks instance unhealthy via circuit breaker and retries
///    - Business error: Returns error immediately without retry
/// 5. Returns SessionResult with instance information and status
///
/// Note: This uses a loop for automatic retry. Failed instances are marked unhealthy
/// and the next iteration automatically selects a different healthy instance.
///
/// # Security
/// The `api_key_name` parameter should be a friendly name (e.g., "my-app"), not the actual API key.
/// Actual API keys are never logged.
pub async fn execute_with_session<F, Fut, T>(
    load_balancer: &LoadBalancer,
    api_key_name: &str,
    request_fn: F,
) -> Result<SessionResult<T>, AppError>
where
    F: Fn(ProviderInstance) -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    const MAX_RETRIES: usize = 3; // Maximum retry attempts to prevent infinite loop
    let mut retry_count = 0;

    loop {
        if retry_count >= MAX_RETRIES {
            return Err(AppError::NoHealthyInstances(
                format!("Failed after {} retries, no healthy instances available", MAX_RETRIES)
            ));
        }

        // Select instance for this API key name (sticky session)
        let instance = match load_balancer.select_instance_for_key(api_key_name).await {
            Some(inst) => inst,
            None => {
                return Err(AppError::NoHealthyInstances(
                    "No healthy instances available".to_string()
                ));
            }
        };

        let instance_name = instance.name.to_string();

        // Get timeout from instance configuration
        let timeout_duration = Duration::from_secs(instance.config.timeout_seconds());

        // Execute the request with timeout
        let request_result = timeout(
            timeout_duration,
            request_fn(instance.clone())
        ).await;

        match request_result {
            Ok(Ok(result)) => {
                // Success - record success for circuit breaker
                load_balancer.record_success(&instance_name).await;
                return Ok(SessionResult::success(result, instance_name));
            }
            Ok(Err(error)) => {
                // Request failed (not timeout) - classify error
                let failure_type = classify_error(&error);

                match failure_type {
                    FailureType::RateLimit { retry_after_secs } => {
                        // 429 Rate Limit - mark instance unhealthy and delay retry
                        load_balancer.record_failure(&instance_name, failure_type.clone()).await;

                        // Cap retry delay to prevent malicious upstream from blocking indefinitely
                        let capped_delay = retry_after_secs.min(60);

                        tracing::warn!(
                            api_key_name = %crate::logging::sanitize_log_value(api_key_name),
                            instance = %instance_name,
                            retry_after_secs = capped_delay,
                            original_retry_after = retry_after_secs,
                            retry_count,
                            "⏱️ Rate limit hit, delaying {}s before retry (max 60s)",
                            capped_delay
                        );

                        // Delay before retrying with different instance
                        tokio::time::sleep(Duration::from_secs(capped_delay)).await;
                        retry_count += 1;
                        continue;
                    }

                    FailureType::Transient => {
                        // Transient error (503) - immediately retry with different instance
                        // Don't mark instance unhealthy for transient errors
                        tracing::warn!(
                            api_key_name = %crate::logging::sanitize_log_value(api_key_name),
                            instance = %instance_name,
                            retry_count,
                            "⚡ Transient error, retrying immediately with different instance"
                        );

                        retry_count += 1;
                        continue;
                    }

                    FailureType::InstanceFailure => {
                        // Instance failure - record failure (circuit breaker) and retry
                        load_balancer.record_failure(&instance_name, failure_type.clone()).await;

                        tracing::warn!(
                            api_key_name = %crate::logging::sanitize_log_value(api_key_name),
                            instance = %instance_name,
                            error = %error,
                            retry_count,
                            "🔴 Instance failure, marking unhealthy and retrying with different instance"
                        );

                        retry_count += 1;
                        continue;
                    }

                    FailureType::BusinessError => {
                        // Business error - return immediately without retry
                        tracing::debug!(
                            api_key_name = %crate::logging::sanitize_log_value(api_key_name),
                            instance = %instance_name,
                            error = %error,
                            "Business error, not retrying"
                        );

                        return Ok(SessionResult::business_error(error, instance_name));
                    }
                }
            }
            Err(_) => {
                // Request timed out - treat as instance failure
                load_balancer.record_failure(&instance_name, FailureType::InstanceFailure).await;

                tracing::warn!(
                    api_key_name = %crate::logging::sanitize_log_value(api_key_name),
                    instance = %instance_name,
                    timeout_seconds = timeout_duration.as_secs(),
                    retry_count,
                    "⏱️ Request timeout, marking instance unhealthy and retrying"
                );

                retry_count += 1;
                continue;
            }
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
        // OAuth errors are NOT instance failures - they're auth/config errors
        AppError::OAuthError { .. } => false,
        // Rate limit errors - use classify_error to determine
        AppError::RateLimitError { .. } => false, // Will be handled by classify_error
        // Service overloaded - transient error
        AppError::ServiceOverloaded { .. } => false, // Will be handled by classify_error
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

    #[test]
    fn test_session_result_success() {
        let session_result = SessionResult::success("test".to_string(), "test-instance".to_string());
        assert_eq!(session_result.instance_name, "test-instance");
        assert!(matches!(session_result.status, RequestStatus::Success));
        assert!(session_result.result.is_ok());
        assert_eq!(session_result.result.unwrap(), "test");
    }

    #[test]
    fn test_session_result_instance_failure() {
        let error = AppError::InternalError("error".to_string());
        let session_result: SessionResult<String> = SessionResult::instance_failure(error, "test-instance".to_string());
        assert_eq!(session_result.instance_name, "test-instance");
        assert!(matches!(session_result.status, RequestStatus::InstanceFailure));
        assert!(session_result.result.is_err());
    }

    #[test]
    fn test_classify_error_auth_errors() {
        // 401/403 should be instance failures (config errors)
        let error = AppError::UpstreamError {
            status: StatusCode::UNAUTHORIZED,
            message: "Unauthorized".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::InstanceFailure);

        let error = AppError::UpstreamError {
            status: StatusCode::FORBIDDEN,
            message: "Forbidden".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::InstanceFailure);
    }

    #[test]
    fn test_classify_error_rate_limit() {
        // 429 with retry_after
        let error = AppError::RateLimitError {
            provider: "test".to_string(),
            instance: "test-instance".to_string(),
            retry_after: Some(10),
            message: "Rate limited".to_string(),
        };
        assert_eq!(
            classify_error(&error),
            FailureType::RateLimit { retry_after_secs: 10 }
        );

        // 429 without retry_after (default 2s)
        let error = AppError::RateLimitError {
            provider: "test".to_string(),
            instance: "test-instance".to_string(),
            retry_after: None,
            message: "Rate limited".to_string(),
        };
        assert_eq!(
            classify_error(&error),
            FailureType::RateLimit { retry_after_secs: 2 }
        );
    }

    #[test]
    fn test_classify_error_transient() {
        // 503 should be transient
        let error = AppError::UpstreamError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: "Service unavailable".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::Transient);

        // ServiceOverloaded should be transient
        let error = AppError::ServiceOverloaded {
            provider: "test".to_string(),
            instance: "test-instance".to_string(),
            message: "Overloaded".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::Transient);
    }

    #[test]
    fn test_classify_error_instance_failure() {
        // 500/502/504 should be instance failures
        let error = AppError::UpstreamError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Internal error".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::InstanceFailure);

        let error = AppError::UpstreamError {
            status: StatusCode::BAD_GATEWAY,
            message: "Bad gateway".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::InstanceFailure);

        let error = AppError::UpstreamError {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: "Gateway timeout".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::InstanceFailure);
    }

    #[test]
    fn test_classify_error_business_error() {
        // Other 4xx should be business errors
        let error = AppError::UpstreamError {
            status: StatusCode::BAD_REQUEST,
            message: "Bad request".to_string(),
        };
        assert_eq!(classify_error(&error), FailureType::BusinessError);

        // Model not found
        let error = AppError::ModelNotFound("gpt-5".to_string());
        assert_eq!(classify_error(&error), FailureType::BusinessError);

        // Conversion error
        let error = AppError::ConversionError("Invalid format".to_string());
        assert_eq!(classify_error(&error), FailureType::BusinessError);
    }
}
