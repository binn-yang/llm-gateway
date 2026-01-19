use crate::{
    auth::AuthInfo,
    converters,
    error::AppError,
    load_balancer::LoadBalancer,
    metrics,
    models::openai::{ChatCompletionRequest, ChatCompletionResponse},
    providers,
    router::{ModelRouter, Provider},
    streaming,
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Application state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<arc_swap::ArcSwap<crate::config::Config>>,
    pub router: Arc<ModelRouter>,
    pub http_client: reqwest::Client,
    pub load_balancers: Arc<arc_swap::ArcSwap<HashMap<Provider, Arc<LoadBalancer>>>>,
}

/// Handle /v1/chat/completions endpoint
/// Currently only supports OpenAI direct passthrough
/// Will be extended to support Anthropic and Gemini via conversion
pub async fn handle_chat_completions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, AppError> {
    let start = Instant::now();
    let model = request.model.clone();
    let is_stream = request.stream.unwrap_or(false);

    // Generate a unique request ID
    let request_id = uuid::Uuid::new_v4().to_string();

    tracing::info!(
        request_id = %request_id,
        api_key_name = %auth.api_key_name,
        model = %model,
        stream = is_stream,
        "Handling chat completion request"
    );

    // Route to provider
    let route_info = state.router.route(&model)?;

    tracing::debug!(
        provider = %route_info.provider,
        requires_conversion = route_info.requires_conversion,
        "Routed model to provider"
    );

    // Record request metric
    metrics::record_request(
        &auth.api_key_name,
        route_info.provider.as_str(),
        &model,
        "/v1/chat/completions",
    );

    // Route based on provider (model name is passed through directly)
    let mut response = match route_info.provider {
        Provider::OpenAI => {
            handle_openai_request(&state, &auth, request, is_stream, &model, start).await
        }
        Provider::Anthropic => {
            handle_anthropic_request(&state, &auth, request, is_stream, &model, start).await
        }
        Provider::Gemini => {
            handle_gemini_request(&state, &auth, request, is_stream, &model, start).await
        }
    }?;

    // Add X-Request-ID header to response
    response.headers_mut().insert(
        "X-Request-ID",
        request_id.parse().unwrap_or_else(|_| "invalid".parse().unwrap()),
    );

    Ok(response)
}

async fn handle_openai_request(
    state: &AppState,
    auth: &AuthInfo,
    request: ChatCompletionRequest,
    is_stream: bool,
    model: &str,
    start: Instant,
) -> Result<Response, AppError> {
    // Get LoadBalancer for OpenAI provider
    let load_balancers_map = state.load_balancers.load();
    let load_balancer = load_balancers_map
        .get(&crate::router::Provider::OpenAI)
        .ok_or_else(|| AppError::ProviderDisabled("OpenAI provider not configured".to_string()))?
        .clone();

    // Execute request with sticky session
    let request_clone = request.clone();
    let http_client = state.http_client.clone();
    let response = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let request_clone = request_clone.clone();
            async move {
                // Extract config from the instance
                let config = match &instance.config {
                    crate::load_balancer::ProviderInstanceConfigEnum::Generic(cfg) => cfg.as_ref(),
                    _ => return Err(AppError::InternalError("Invalid instance config type".to_string())),
                };

                // Call OpenAI API
                providers::openai::chat_completions(&http_client, config, request_clone).await
            }
        },
    )
    .await?;

    if is_stream {
        // Stream response
        tracing::debug!("Streaming response from OpenAI");
        let sse_stream = streaming::create_openai_sse_stream(response);
        Ok(sse_stream.into_response())
    } else {
        // Non-streaming response
        let body: ChatCompletionResponse = response.json().await?;

        // Record metrics
        if let Some(usage) = &body.usage {
            metrics::record_tokens(&auth.api_key_name, "openai", model, "input", usage.prompt_tokens, None);
            metrics::record_tokens(&auth.api_key_name, "openai", model, "output", usage.completion_tokens, None);
        }
        metrics::record_duration(&auth.api_key_name, "openai", model, start.elapsed());

        tracing::info!(
            api_key_name = %auth.api_key_name,
            model = %model,
            duration_ms = start.elapsed().as_millis(),
            prompt_tokens = body.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens = body.usage.as_ref().map(|u| u.completion_tokens),
            "Completed chat completion request"
        );

        Ok(Json(body).into_response())
    }
}

async fn handle_anthropic_request(
    state: &AppState,
    auth: &AuthInfo,
    openai_request: ChatCompletionRequest,
    is_stream: bool,
    model: &str,
    start: Instant,
) -> Result<Response, AppError> {
    // Convert OpenAI request to Anthropic format
    let (mut anthropic_request, conversion_warnings) = converters::openai_to_anthropic::convert_request(&openai_request).await?;

    // Pass through the original model name
    anthropic_request.model = model.to_string();

    tracing::debug!(
        model = %model,
        has_system = anthropic_request.system.is_some(),
        "Converted OpenAI request to Anthropic format"
    );

    // Get LoadBalancer for Anthropic provider
    let load_balancers_map = state.load_balancers.load();
    let load_balancer = load_balancers_map
        .get(&crate::router::Provider::Anthropic)
        .ok_or_else(|| AppError::ProviderDisabled("Anthropic provider not configured".to_string()))?
        .clone();

    // Execute request with sticky session
    let http_client = state.http_client.clone();
    let response = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let mut anthropic_request = anthropic_request.clone();
            async move {
                // Extract config from the instance
                let config = match &instance.config {
                    crate::load_balancer::ProviderInstanceConfigEnum::Anthropic(cfg) => cfg.as_ref(),
                    _ => return Err(AppError::InternalError("Invalid instance config type".to_string())),
                };

                // Apply automatic caching based on configuration
                converters::openai_to_anthropic::apply_auto_caching(&mut anthropic_request, &config.cache);

                // Call Anthropic API
                providers::anthropic::create_message(&http_client, config, anthropic_request).await
            }
        },
    )
    .await?;

    if is_stream {
        // Stream response and convert back to OpenAI format
        tracing::debug!("Streaming response from Anthropic");
        let mut response = streaming::create_anthropic_sse_stream(response).into_response();

        // Add warnings header if present
        if let Some(warnings_json) = conversion_warnings.to_header_value() {
            response.headers_mut().insert(
                axum::http::HeaderName::from_static("x-llm-gateway-warnings"),
                axum::http::HeaderValue::from_str(&warnings_json).unwrap_or_else(|_| axum::http::HeaderValue::from_static("[]"))
            );
        }

        Ok(response)
    } else {
        // Non-streaming response
        let anthropic_body: crate::models::anthropic::MessagesResponse = response.json().await?;

        tracing::debug!(
            anthropic_id = %anthropic_body.id,
            stop_reason = ?anthropic_body.stop_reason,
            "Received Anthropic response"
        );

        // Convert back to OpenAI format
        let openai_body = converters::anthropic_response::convert_response(&anthropic_body)?;

        // Record metrics
        if let Some(usage) = &openai_body.usage {
            metrics::record_tokens(&auth.api_key_name, "anthropic", model, "input", usage.prompt_tokens, None);
            metrics::record_tokens(&auth.api_key_name, "anthropic", model, "output", usage.completion_tokens, None);
        }
        metrics::record_duration(&auth.api_key_name, "anthropic", model, start.elapsed());

        tracing::info!(
            api_key_name = %auth.api_key_name,
            model = %model,
            duration_ms = start.elapsed().as_millis(),
            prompt_tokens = openai_body.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens = openai_body.usage.as_ref().map(|u| u.completion_tokens),
            "Completed chat completion request via Anthropic"
        );

        let mut response = Json(openai_body).into_response();

        // Add warnings header if present
        if let Some(warnings_json) = conversion_warnings.to_header_value() {
            response.headers_mut().insert(
                axum::http::HeaderName::from_static("x-llm-gateway-warnings"),
                axum::http::HeaderValue::from_str(&warnings_json).unwrap_or_else(|_| axum::http::HeaderValue::from_static("[]"))
            );
        }

        Ok(response)
    }
}

async fn handle_gemini_request(
    state: &AppState,
    auth: &AuthInfo,
    openai_request: ChatCompletionRequest,
    is_stream: bool,
    model: &str,
    start: Instant,
) -> Result<Response, AppError> {
    // Convert OpenAI request to Gemini format
    let (gemini_request, conversion_warnings) = converters::openai_to_gemini::convert_request(&openai_request).await?;

    tracing::debug!(
        model = %model,
        has_system = gemini_request.system_instruction.is_some(),
        "Converted OpenAI request to Gemini format"
    );

    // Get LoadBalancer for Gemini provider
    let load_balancers_map = state.load_balancers.load();
    let load_balancer = load_balancers_map
        .get(&crate::router::Provider::Gemini)
        .ok_or_else(|| AppError::ProviderDisabled("Gemini provider not configured".to_string()))?
        .clone();

    // Execute request with sticky session
    let http_client = state.http_client.clone();
    let model_str = model.to_string();
    let response = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let model_str = model_str.clone();
            let gemini_request = gemini_request.clone();
            async move {
                // Extract config from the instance
                let config = match &instance.config {
                    crate::load_balancer::ProviderInstanceConfigEnum::Generic(cfg) => cfg.as_ref(),
                    _ => return Err(AppError::InternalError("Invalid instance config type".to_string())),
                };

                // Call Gemini API (pass through original model name)
                providers::gemini::generate_content(
                    &http_client,
                    config,
                    &model_str,
                    gemini_request,
                    is_stream,
                )
                .await
            }
        },
    )
    .await?;

    if is_stream {
        // Stream response and convert back to OpenAI format
        tracing::debug!("Streaming response from Gemini");
        let mut response = streaming::create_gemini_sse_stream(response).into_response();

        // Add warnings header if present
        if let Some(warnings_json) = conversion_warnings.to_header_value() {
            response.headers_mut().insert(
                axum::http::HeaderName::from_static("x-llm-gateway-warnings"),
                axum::http::HeaderValue::from_str(&warnings_json).unwrap_or_else(|_| axum::http::HeaderValue::from_static("[]"))
            );
        }

        Ok(response)
    } else {
        // Non-streaming response
        let gemini_body: crate::models::gemini::GenerateContentResponse = response.json().await?;

        tracing::debug!(
            model_version = ?gemini_body.model_version,
            candidates_count = gemini_body.candidates.len(),
            "Received Gemini response"
        );

        // Convert back to OpenAI format
        let openai_body = converters::gemini_response::convert_response(&gemini_body)?;

        // Record metrics
        if let Some(usage) = &openai_body.usage {
            metrics::record_tokens(&auth.api_key_name, "gemini", model, "input", usage.prompt_tokens, None);
            metrics::record_tokens(&auth.api_key_name, "gemini", model, "output", usage.completion_tokens, None);
        }
        metrics::record_duration(&auth.api_key_name, "gemini", model, start.elapsed());

        tracing::info!(
            api_key_name = %auth.api_key_name,
            model = %model,
            duration_ms = start.elapsed().as_millis(),
            prompt_tokens = openai_body.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens = openai_body.usage.as_ref().map(|u| u.completion_tokens),
            "Completed chat completion request via Gemini"
        );

        let mut response = Json(openai_body).into_response();

        // Add warnings header if present
        if let Some(warnings_json) = conversion_warnings.to_header_value() {
            response.headers_mut().insert(
                axum::http::HeaderName::from_static("x-llm-gateway-warnings"),
                axum::http::HeaderValue::from_str(&warnings_json).unwrap_or_else(|_| axum::http::HeaderValue::from_static("[]"))
            );
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicInstanceConfig, ApiKeyConfig, Config, DiscoveryConfig, MetricsConfig, ProviderInstanceConfig,
        ProvidersConfig, RoutingConfig, ServerConfig,
    };
    use std::collections::HashMap;

    fn create_test_state() -> AppState {
        let mut routing_rules = HashMap::new();
        routing_rules.insert("gpt-".to_string(), "openai".to_string());
        routing_rules.insert("claude-".to_string(), "anthropic".to_string());
        routing_rules.insert("gemini-".to_string(), "gemini".to_string());

        let config = Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                log_level: "info".to_string(),
                log_format: "json".to_string(),
            },
            api_keys: vec![ApiKeyConfig {
                key: "sk-test".to_string(),
                name: "test".to_string(),
                enabled: true,
            }],
            routing: RoutingConfig {
                rules: routing_rules,
                default_provider: Some("openai".to_string()),
                discovery: DiscoveryConfig {
                    enabled: true,
                    cache_ttl_seconds: 3600,
                    refresh_on_startup: true,
                    providers_with_listing: vec!["openai".to_string()],
                },
            },
            providers: ProvidersConfig {
                openai: vec![ProviderInstanceConfig {
                    name: "openai-test".to_string(),
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "anthropic-test".to_string(),
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    cache: crate::config::CacheConfig::default(),
                }],
                gemini: vec![ProviderInstanceConfig {
                    name: "gemini-test".to_string(),
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                }],
            },
            metrics: MetricsConfig {
                enabled: true,
                endpoint: "/metrics".to_string(),
                include_api_key_hash: true,
            },
        };

        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(config)));
        let router = Arc::new(ModelRouter::new(config.clone()));
        let http_client = reqwest::Client::new();
        let empty_lb: std::collections::HashMap<crate::router::Provider, Arc<crate::load_balancer::LoadBalancer>> = std::collections::HashMap::new();
        let load_balancers = Arc::new(arc_swap::ArcSwap::from_pointee(empty_lb));

        AppState {
            config,
            router,
            http_client,
            load_balancers,
        }
    }

    #[test]
    fn test_app_state_creation() {
        let state = create_test_state();
        assert!(!state.config.load().providers.openai.is_empty());
        assert!(state.config.load().providers.openai[0].enabled);
    }
}
