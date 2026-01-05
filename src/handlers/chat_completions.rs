use crate::{
    auth::AuthInfo,
    converters,
    error::AppError,
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
use std::sync::Arc;
use std::time::Instant;

/// Application state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<arc_swap::ArcSwap<crate::config::Config>>,
    pub router: Arc<ModelRouter>,
    pub http_client: reqwest::Client,
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

    tracing::info!(
        api_key = %auth.api_key_name,
        model = %model,
        stream = is_stream,
        "Handling chat completion request"
    );

    // Route to provider
    let route_info = state.router.route(&model)?;

    tracing::debug!(
        provider = %route_info.provider,
        api_model = %route_info.api_model,
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

    // Route based on provider
    match route_info.provider {
        Provider::OpenAI => {
            handle_openai_request(&state, &auth, request, is_stream, &model, start).await
        }
        Provider::Anthropic => {
            handle_anthropic_request(&state, &auth, request, is_stream, &model, &route_info.api_model, start).await
        }
        Provider::Gemini => {
            handle_gemini_request(&state, &auth, request, is_stream, &model, &route_info.api_model, start).await
        }
    }
}

async fn handle_openai_request(
    state: &AppState,
    auth: &AuthInfo,
    request: ChatCompletionRequest,
    is_stream: bool,
    model: &str,
    start: Instant,
) -> Result<Response, AppError> {
    // Load current configuration
    let config = state.config.load();

    // Call OpenAI API
    let response = providers::openai::chat_completions(
        &state.http_client,
        &config.providers.openai,
        request,
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
            metrics::record_tokens(&auth.api_key_name, "openai", model, "input", usage.prompt_tokens);
            metrics::record_tokens(&auth.api_key_name, "openai", model, "output", usage.completion_tokens);
        }
        metrics::record_duration(&auth.api_key_name, "openai", model, start.elapsed());

        tracing::info!(
            api_key = %auth.api_key_name,
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
    api_model: &str,
    start: Instant,
) -> Result<Response, AppError> {
    // Convert OpenAI request to Anthropic format
    let mut anthropic_request = converters::openai_to_anthropic::convert_request(&openai_request)?;

    // Use the correct API model name
    anthropic_request.model = api_model.to_string();

    tracing::debug!(
        original_model = %model,
        api_model = %api_model,
        has_system = anthropic_request.system.is_some(),
        "Converted OpenAI request to Anthropic format"
    );

    // Load current configuration
    let config = state.config.load();

    // Call Anthropic API
    let response = providers::anthropic::create_message(
        &state.http_client,
        &config.providers.anthropic,
        anthropic_request,
    )
    .await?;

    if is_stream {
        // Stream response and convert back to OpenAI format
        tracing::debug!("Streaming response from Anthropic");
        let sse_stream = streaming::create_anthropic_sse_stream(response);
        Ok(sse_stream.into_response())
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
            metrics::record_tokens(&auth.api_key_name, "anthropic", model, "input", usage.prompt_tokens);
            metrics::record_tokens(&auth.api_key_name, "anthropic", model, "output", usage.completion_tokens);
        }
        metrics::record_duration(&auth.api_key_name, "anthropic", model, start.elapsed());

        tracing::info!(
            api_key = %auth.api_key_name,
            model = %model,
            duration_ms = start.elapsed().as_millis(),
            prompt_tokens = openai_body.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens = openai_body.usage.as_ref().map(|u| u.completion_tokens),
            "Completed chat completion request via Anthropic"
        );

        Ok(Json(openai_body).into_response())
    }
}

async fn handle_gemini_request(
    state: &AppState,
    auth: &AuthInfo,
    openai_request: ChatCompletionRequest,
    is_stream: bool,
    model: &str,
    api_model: &str,
    start: Instant,
) -> Result<Response, AppError> {
    // Convert OpenAI request to Gemini format
    let gemini_request = converters::openai_to_gemini::convert_request(&openai_request)?;

    tracing::debug!(
        original_model = %model,
        api_model = %api_model,
        has_system = gemini_request.system_instruction.is_some(),
        "Converted OpenAI request to Gemini format"
    );

    // Load current configuration
    let config = state.config.load();

    // Call Gemini API
    let response = providers::gemini::generate_content(
        &state.http_client,
        &config.providers.gemini,
        api_model,
        gemini_request,
        is_stream,
    )
    .await?;

    if is_stream {
        // Stream response and convert back to OpenAI format
        tracing::debug!("Streaming response from Gemini");
        // TODO: Implement Gemini SSE stream conversion (Phase 6 if needed)
        Err(AppError::InternalError(
            "Gemini streaming not yet implemented".to_string(),
        ))
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
            metrics::record_tokens(&auth.api_key_name, "gemini", model, "input", usage.prompt_tokens);
            metrics::record_tokens(&auth.api_key_name, "gemini", model, "output", usage.completion_tokens);
        }
        metrics::record_duration(&auth.api_key_name, "gemini", model, start.elapsed());

        tracing::info!(
            api_key = %auth.api_key_name,
            model = %model,
            duration_ms = start.elapsed().as_millis(),
            prompt_tokens = openai_body.usage.as_ref().map(|u| u.prompt_tokens),
            completion_tokens = openai_body.usage.as_ref().map(|u| u.completion_tokens),
            "Completed chat completion request via Gemini"
        );

        Ok(Json(openai_body).into_response())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, ApiKeyConfig, Config, MetricsConfig, ModelConfig, ProviderConfig,
        ProvidersConfig, ServerConfig,
    };
    use std::collections::HashMap;

    fn create_test_state() -> AppState {
        let mut models = HashMap::new();
        models.insert(
            "gpt-4".to_string(),
            ModelConfig {
                provider: "openai".to_string(),
                api_model: "gpt-4".to_string(),
            },
        );

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
            models,
            providers: ProvidersConfig {
                openai: ProviderConfig {
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                },
                anthropic: AnthropicConfig {
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                },
                gemini: ProviderConfig {
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                },
            },
            metrics: MetricsConfig {
                enabled: true,
                endpoint: "/metrics".to_string(),
                include_api_key_hash: true,
            },
        };

        let config = Arc::new(config);
        let router = Arc::new(ModelRouter::new(config.clone()));
        let http_client = reqwest::Client::new();

        AppState {
            config,
            router,
            http_client,
        }
    }

    #[test]
    fn test_app_state_creation() {
        let state = create_test_state();
        assert!(state.config.providers.openai.enabled);
    }
}
