use crate::{
    auth::AuthInfo,
    converters,
    error::AppError,
    load_balancer::LoadBalancer,
    models::openai::{ChatCompletionRequest, ChatCompletionResponse},
    observability::{RequestEvent, RequestLogger},
    providers,
    retry::RequestStatus,
    router::{ModelRouter, Provider},
    streaming,
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{Timelike, Utc, Local};
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
    pub request_logger: Option<Arc<RequestLogger>>,
    /// OAuth token store for provider authentication
    pub token_store: Option<Arc<crate::oauth::TokenStore>>,
    /// OAuth manager for token refresh and management
    pub oauth_manager: Option<Arc<crate::oauth::OAuthManager>>,
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

    // Create request span with all context, so all subsequent logs include these fields
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        api_key_name = %auth.api_key_name,
        model = %model,
        endpoint = "/v1/chat/completions",
        provider = tracing::field::Empty,
        instance = tracing::field::Empty,
    );
    // Keep span alive for recording fields, but don't enter it to avoid async lifecycle issues

    tracing::info!(
        parent: &span,
        stream = is_stream,
        "Handling chat completion request"
    );

    // Log request body if enabled
    let config = state.config.load();
    if config.observability.body_logging.enabled {
        let body_content = if config.observability.body_logging.simple_mode {
            // Simple mode: extract only user messages (no redaction)
            crate::logging::extract_simple_request_openai(&request)
        } else {
            // Full mode: log complete request with redaction
            let request_body = serde_json::to_string(&request)
                .unwrap_or_else(|_| "{}".to_string());
            let redacted_body = crate::logging::redact_sensitive_data(
                &request_body,
                &config.observability.body_logging.redact_patterns
            );
            let (final_body, _) = crate::logging::truncate_body(
                redacted_body,
                config.observability.body_logging.max_body_size
            );
            final_body
        };

        tracing::info!(
            parent: &span,
            event_type = if config.observability.body_logging.simple_mode {
                "simple_request"
            } else {
                "request_body"
            },
            body = %body_content,
            body_size = body_content.len(),
            truncated = false,
            "Request body"
        );
    }

    // Route to provider
    let routing_start = Instant::now();
    let route_info = state.router.route(&model)?;
    let routing_duration = routing_start.elapsed().as_millis();

    // Record provider in span
    span.record("provider", route_info.provider.to_string().as_str());

    tracing::debug!(
        parent: &span,
        event_type = "trace_span",
        span_name = "route_model",
        span_type = "routing",
        duration_ms = routing_duration,
        status = "ok",
        target_provider = route_info.provider.to_string().as_str(),
        requires_conversion = route_info.requires_conversion,
        "Routing span completed"
    );

    // Route based on provider (model name is passed through directly)
    let mut response = match route_info.provider {
        Provider::OpenAI => {
            handle_openai_request(&state, &auth, request, is_stream, &model, &request_id, start, &span).await
        }
        Provider::Anthropic => {
            handle_anthropic_request(&state, &auth, request, is_stream, &model, &request_id, start, &span).await
        }
        Provider::Gemini => {
            handle_gemini_request(&state, &auth, request, is_stream, &model, &request_id, start, &span).await
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
    request_id: &str,
    start: Instant,
    span: &tracing::Span,
) -> Result<Response, AppError> {
    // Get LoadBalancer for OpenAI provider
    let load_balancers_map = state.load_balancers.load();
    let load_balancer = load_balancers_map
        .get(&crate::router::Provider::OpenAI)
        .ok_or_else(|| AppError::ProviderDisabled("OpenAI provider not configured".to_string()))?
        .clone();

    // Execute request with sticky session (returns SessionResult)
    let request_clone = request.clone();
    let http_client = state.http_client.clone();
    let oauth_manager = state.oauth_manager.clone();
    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let request_clone = request_clone.clone();
            let oauth_manager = oauth_manager.clone();
            async move {
                // Extract config from the instance
                let config = match &instance.config {
                    crate::load_balancer::ProviderInstanceConfigEnum::Generic(cfg) => cfg.as_ref(),
                    _ => return Err(AppError::InternalError("Invalid instance config type".to_string())),
                };

                // Get OAuth token if needed
                let oauth_token = if config.auth_mode == crate::config::AuthMode::OAuth {
                    if let Some(ref oauth_provider_name) = config.oauth_provider {
                        if let Some(ref manager) = oauth_manager {
                            match manager.get_valid_token(oauth_provider_name).await {
                                Ok(token) => {
                                    tracing::debug!(
                                        provider = %oauth_provider_name,
                                        "Retrieved OAuth token for OpenAI request"
                                    );
                                    Some(token.access_token)
                                }
                                Err(e) => {
                                    tracing::error!(
                                        provider = %oauth_provider_name,
                                        error = %e,
                                        "Failed to get OAuth token"
                                    );
                                    return Err(e);
                                }
                            }
                        } else {
                            return Err(AppError::ConfigError(
                                "OAuth mode enabled but OAuth manager not initialized".to_string()
                            ));
                        }
                    } else {
                        return Err(AppError::ConfigError(
                            "OAuth mode enabled but oauth_provider not specified".to_string()
                        ));
                    }
                } else {
                    None
                };

                // Call OpenAI API with OAuth token if available
                providers::openai::chat_completions(
                    &http_client,
                    config,
                    request_clone,
                    oauth_token.as_deref()
                ).await
            }
        },
    )
    .await?;

    let instance_name = session_result.instance_name;
    let response = session_result.result?;
    let duration_ms = start.elapsed().as_millis() as i64;

    // Record instance in span
    span.record("instance", instance_name.as_str());

    if is_stream {
        // Stream response with usage tracking
        tracing::debug!("Streaming response from OpenAI");

        if let Some(logger) = &state.request_logger {
            let now_utc = Utc::now();
            let now_local = Local::now();
            let event = RequestEvent {
                request_id: request_id.to_string(),
                timestamp: now_utc.timestamp_millis(),
                date: now_local.format("%Y-%m-%d").to_string(),
                hour: now_local.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: "openai".to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/chat/completions".to_string(),
                status: match session_result.status {
                    RequestStatus::Success => "success".to_string(),
                    RequestStatus::InstanceFailure => "failure".to_string(),
                    RequestStatus::BusinessError => "business_error".to_string(),
                    RequestStatus::Timeout => "timeout".to_string(),
                },
                error_type: None,
                error_message: None,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                duration_ms,
            };
            logger.log_request(event).await;
        }

        // Create tracker and stream with usage tracking
        let tracker = streaming::StreamingUsageTracker::new(request_id.to_string());
        let sse_stream = streaming::create_openai_sse_stream_with_tracker(response, tracker.clone());

        // Spawn background task to update tokens when stream completes
        if let Some(logger) = state.request_logger.clone() {
            let request_id_owned = request_id.to_string();
            let tracker_clone = tracker.clone();
            let config = state.config.load().clone();
            let span_clone = span.clone();
            tokio::spawn(async move {
                // Wait for tracker to notify completion (no polling/sleeping!)
                if let Some((input, output, cache_creation, cache_read)) = tracker_clone.wait_for_completion().await {
                    logger.update_tokens(
                        &request_id_owned,
                        input as i64,
                        output as i64,
                        (input + output) as i64,
                        cache_creation as i64,
                        cache_read as i64,
                    ).await;

                    // Log response body if enabled
                    if config.observability.body_logging.enabled {
                        let accumulated_response = tracker_clone.get_accumulated_response();

                        let body_content = if config.observability.body_logging.simple_mode {
                            // Simple mode: extract only text deltas (no redaction)
                            crate::logging::extract_simple_response_streaming(&accumulated_response)
                        } else {
                            // Full mode: log complete response with redaction
                            let redacted = crate::logging::redact_sensitive_data(
                                &accumulated_response,
                                &config.observability.body_logging.redact_patterns
                            );
                            let (truncated_body, _) = crate::logging::truncate_body(
                                redacted,
                                config.observability.body_logging.max_body_size
                            );
                            truncated_body
                        };

                        tracing::info!(
                            parent: &span_clone,
                            event_type = if config.observability.body_logging.simple_mode {
                                "simple_response"
                            } else {
                                "response_body"
                            },
                            body = %body_content,
                            body_size = body_content.len(),
                            truncated = false,
                            streaming = true,
                            chunks_count = tracker_clone.chunks_count(),
                            "Response body"
                        );
                    }
                } else {
                    tracing::warn!(
                        request_id = %request_id_owned,
                        "Stream completed without token usage data"
                    );
                }
            });
        }

        Ok(sse_stream.into_response())
    } else {
        // Non-streaming response
        let body: ChatCompletionResponse = response.json().await?;

        // Log response body if enabled
        let config = state.config.load();
        if config.observability.body_logging.enabled {
            let body_content = if config.observability.body_logging.simple_mode {
                // Simple mode: extract only assistant text (no redaction)
                crate::logging::extract_simple_response_openai(&body)
            } else {
                // Full mode: log complete response with redaction
                let response_body = serde_json::to_string(&body)
                    .unwrap_or_else(|_| "{}".to_string());
                let redacted_response = crate::logging::redact_sensitive_data(
                    &response_body,
                    &config.observability.body_logging.redact_patterns
                );
                let (final_response, _) = crate::logging::truncate_body(
                    redacted_response,
                    config.observability.body_logging.max_body_size
                );
                final_response
            };

            tracing::info!(
                parent: span,
                event_type = if config.observability.body_logging.simple_mode {
                    "simple_response"
                } else {
                    "response_body"
                },
                body = %body_content,
                body_size = body_content.len(),
                truncated = false,
                streaming = false,
                chunks_count = 0,
                "Response body"
            );
        }

        // Extract usage
        let (input_tokens, output_tokens, total_tokens) = match &body.usage {
            Some(usage) => (
                usage.prompt_tokens as i64,
                usage.completion_tokens as i64,
                (usage.prompt_tokens + usage.completion_tokens) as i64,
            ),
            None => (0, 0, 0),
        };

        // Log request event
        if let Some(logger) = &state.request_logger {
            let now_utc = Utc::now();
            let now_local = Local::now();
            let event = RequestEvent {
                request_id: request_id.to_string(),
                timestamp: now_utc.timestamp_millis(),
                date: now_local.format("%Y-%m-%d").to_string(),
                hour: now_local.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: "openai".to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/chat/completions".to_string(),
                status: match session_result.status {
                    RequestStatus::Success => "success".to_string(),
                    RequestStatus::InstanceFailure => "failure".to_string(),
                    RequestStatus::BusinessError => "business_error".to_string(),
                    RequestStatus::Timeout => "timeout".to_string(),
                },
                error_type: None,
                error_message: None,
                input_tokens,
                output_tokens,
                total_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                duration_ms,
            };
            logger.log_request(event).await;
        }

        tracing::info!(
            api_key_name = %auth.api_key_name,
            model = %model,
            instance = %instance_name,
            duration_ms = duration_ms,
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
    request_id: &str,
    start: Instant,
    span: &tracing::Span,
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

    // Execute request with sticky session (returns SessionResult)
    let http_client = state.http_client.clone();
    let oauth_manager = state.oauth_manager.clone();
    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let mut anthropic_request = anthropic_request.clone();
            let oauth_manager = oauth_manager.clone();
            async move {
                // Extract config from the instance
                let config = match &instance.config {
                    crate::load_balancer::ProviderInstanceConfigEnum::Anthropic(cfg) => cfg.as_ref(),
                    _ => return Err(AppError::InternalError("Invalid instance config type".to_string())),
                };

                // Apply automatic caching based on configuration
                converters::openai_to_anthropic::apply_auto_caching(&mut anthropic_request, &config.cache);

                // Get OAuth token if needed
                let oauth_token = if config.auth_mode == crate::config::AuthMode::OAuth {
                    if let Some(ref oauth_provider_name) = config.oauth_provider {
                        if let Some(ref manager) = oauth_manager {
                            match manager.get_valid_token(oauth_provider_name).await {
                                Ok(token) => {
                                    tracing::debug!(
                                        provider = %oauth_provider_name,
                                        "Retrieved OAuth token for Anthropic request"
                                    );
                                    Some(token.access_token)
                                }
                                Err(e) => {
                                    tracing::error!(
                                        provider = %oauth_provider_name,
                                        error = %e,
                                        "Failed to get OAuth token"
                                    );
                                    return Err(e);
                                }
                            }
                        } else {
                            return Err(AppError::ConfigError(
                                "OAuth mode enabled but OAuth manager not initialized".to_string()
                            ));
                        }
                    } else {
                        return Err(AppError::ConfigError(
                            "OAuth mode enabled but oauth_provider not specified".to_string()
                        ));
                    }
                } else {
                    None
                };

                // Call Anthropic API with OAuth token if available
                providers::anthropic::create_message(
                    &http_client,
                    config,
                    anthropic_request,
                    oauth_token.as_deref()
                ).await
            }
        },
    )
    .await?;

    let instance_name = session_result.instance_name;
    let response = session_result.result?;
    let duration_ms = start.elapsed().as_millis() as i64;

    // Record instance in span
    span.record("instance", instance_name.as_str());

    if is_stream {
        // Stream response with usage tracking
        tracing::debug!("Streaming response from Anthropic");

        if let Some(logger) = &state.request_logger {
            let now_utc = Utc::now();
            let now_local = Local::now();
            let event = RequestEvent {
                request_id: request_id.to_string(),
                timestamp: now_utc.timestamp_millis(),
                date: now_local.format("%Y-%m-%d").to_string(),
                hour: now_local.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: "anthropic".to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/chat/completions".to_string(),
                status: match session_result.status {
                    RequestStatus::Success => "success".to_string(),
                    RequestStatus::InstanceFailure => "failure".to_string(),
                    RequestStatus::BusinessError => "business_error".to_string(),
                    RequestStatus::Timeout => "timeout".to_string(),
                },
                error_type: None,
                error_message: None,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                duration_ms,
            };
            logger.log_request(event).await;
        }

        // Create tracker and stream with usage tracking
        let tracker = streaming::StreamingUsageTracker::new(request_id.to_string());
        let mut response = streaming::create_anthropic_sse_stream_with_tracker(response, request_id.to_string(), tracker.clone()).into_response();

        // Add warnings header if present
        if let Some(warnings_json) = conversion_warnings.to_header_value() {
            response.headers_mut().insert(
                axum::http::HeaderName::from_static("x-llm-gateway-warnings"),
                axum::http::HeaderValue::from_str(&warnings_json).unwrap_or_else(|_| axum::http::HeaderValue::from_static("[]"))
            );
        }

        // Spawn background task to update tokens when stream completes
        if let Some(logger) = state.request_logger.clone() {
            let request_id_owned = request_id.to_string();
            tokio::spawn(async move {
                // Wait for tracker to notify completion (no polling/sleeping!)
                if let Some((input, output, cache_creation, cache_read)) = tracker.wait_for_completion().await {
                    logger.update_tokens(
                        &request_id_owned,
                        input as i64,
                        output as i64,
                        (input + output) as i64,
                        cache_creation as i64,
                        cache_read as i64,
                    ).await;
                } else {
                    tracing::warn!(
                        request_id = %request_id_owned,
                        provider = "anthropic",
                        "Stream completed without token usage data"
                    );
                }
            });
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

        // Extract usage from OpenAI format (for compatibility)
        let (input_tokens, output_tokens, total_tokens) = match &openai_body.usage {
            Some(usage) => (
                usage.prompt_tokens as i64,
                usage.completion_tokens as i64,
                (usage.prompt_tokens + usage.completion_tokens) as i64,
            ),
            None => (0, 0, 0),
        };

        // Extract cache tokens from original Anthropic response
        let cache_creation_tokens = anthropic_body.usage.cache_creation_input_tokens.unwrap_or(0) as i64;
        let cache_read_tokens = anthropic_body.usage.cache_read_input_tokens.unwrap_or(0) as i64;

        // Log request event
        if let Some(logger) = &state.request_logger {
            let now_utc = Utc::now();
            let now_local = Local::now();
            let event = RequestEvent {
                request_id: request_id.to_string(),
                timestamp: now_utc.timestamp_millis(),
                date: now_local.format("%Y-%m-%d").to_string(),
                hour: now_local.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: "anthropic".to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/chat/completions".to_string(),
                status: match session_result.status {
                    RequestStatus::Success => "success".to_string(),
                    RequestStatus::InstanceFailure => "failure".to_string(),
                    RequestStatus::BusinessError => "business_error".to_string(),
                    RequestStatus::Timeout => "timeout".to_string(),
                },
                error_type: None,
                error_message: None,
                input_tokens,
                output_tokens,
                total_tokens,
                cache_creation_input_tokens: cache_creation_tokens,
                cache_read_input_tokens: cache_read_tokens,
                duration_ms,
            };
            logger.log_request(event).await;
        }

        tracing::info!(
            api_key_name = %auth.api_key_name,
            model = %model,
            instance = %instance_name,
            duration_ms = duration_ms,
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
    request_id: &str,
    start: Instant,
    span: &tracing::Span,
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

    // Execute request with sticky session (returns SessionResult)
    let http_client = state.http_client.clone();
    let oauth_manager = state.oauth_manager.clone();
    let model_str = model.to_string();
    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let model_str = model_str.clone();
            let gemini_request = gemini_request.clone();
            let oauth_manager = oauth_manager.clone();
            async move {
                // Extract config from the instance
                let config = match &instance.config {
                    crate::load_balancer::ProviderInstanceConfigEnum::Generic(cfg) => cfg.as_ref(),
                    _ => return Err(AppError::InternalError("Invalid instance config type".to_string())),
                };

                // Get OAuth token if needed
                let oauth_token = if config.auth_mode == crate::config::AuthMode::OAuth {
                    if let Some(ref oauth_provider_name) = config.oauth_provider {
                        if let Some(ref manager) = oauth_manager {
                            match manager.get_valid_token(oauth_provider_name).await {
                                Ok(token) => {
                                    tracing::debug!(
                                        provider = %oauth_provider_name,
                                        "Retrieved OAuth token for Gemini request"
                                    );
                                    Some(token.access_token)
                                }
                                Err(e) => {
                                    tracing::error!(
                                        provider = %oauth_provider_name,
                                        error = %e,
                                        "Failed to get OAuth token"
                                    );
                                    return Err(e);
                                }
                            }
                        } else {
                            return Err(AppError::ConfigError(
                                "OAuth mode enabled but OAuth manager not initialized".to_string()
                            ));
                        }
                    } else {
                        return Err(AppError::ConfigError(
                            "OAuth mode enabled but oauth_provider not specified".to_string()
                        ));
                    }
                } else {
                    None
                };

                // Call Gemini API with OAuth token if available
                providers::gemini::generate_content(
                    &http_client,
                    config,
                    &model_str,
                    gemini_request,
                    is_stream,
                    oauth_token.as_deref()
                )
                .await
            }
        },
    )
    .await?;

    let instance_name = session_result.instance_name;
    let response = session_result.result?;
    let duration_ms = start.elapsed().as_millis() as i64;

    // Record instance in span
    span.record("instance", instance_name.as_str());

    if is_stream {
        // Stream response with usage tracking
        tracing::debug!("Streaming response from Gemini");

        if let Some(logger) = &state.request_logger {
            let now_utc = Utc::now();
            let now_local = Local::now();
            let event = RequestEvent {
                request_id: request_id.to_string(),
                timestamp: now_utc.timestamp_millis(),
                date: now_local.format("%Y-%m-%d").to_string(),
                hour: now_local.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: "gemini".to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/chat/completions".to_string(),
                status: match session_result.status {
                    RequestStatus::Success => "success".to_string(),
                    RequestStatus::InstanceFailure => "failure".to_string(),
                    RequestStatus::BusinessError => "business_error".to_string(),
                    RequestStatus::Timeout => "timeout".to_string(),
                },
                error_type: None,
                error_message: None,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                duration_ms,
            };
            logger.log_request(event).await;
        }

        // Create tracker and stream with usage tracking
        let tracker = streaming::StreamingUsageTracker::new(request_id.to_string());
        let mut response = streaming::create_gemini_sse_stream_with_tracker(response, request_id.to_string(), tracker.clone()).into_response();

        // Add warnings header if present
        if let Some(warnings_json) = conversion_warnings.to_header_value() {
            response.headers_mut().insert(
                axum::http::HeaderName::from_static("x-llm-gateway-warnings"),
                axum::http::HeaderValue::from_str(&warnings_json).unwrap_or_else(|_| axum::http::HeaderValue::from_static("[]"))
            );
        }

        // Spawn background task to update tokens when stream completes
        if let Some(logger) = state.request_logger.clone() {
            let request_id_owned = request_id.to_string();
            tokio::spawn(async move {
                // Wait for tracker to notify completion (no polling/sleeping!)
                if let Some((input, output, cache_creation, cache_read)) = tracker.wait_for_completion().await {
                    logger.update_tokens(
                        &request_id_owned,
                        input as i64,
                        output as i64,
                        (input + output) as i64,
                        cache_creation as i64,
                        cache_read as i64,
                    ).await;
                } else {
                    tracing::warn!(
                        request_id = %request_id_owned,
                        provider = "gemini",
                        "Stream completed without token usage data"
                    );
                }
            });
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

        // Extract usage
        let (input_tokens, output_tokens, total_tokens) = match &openai_body.usage {
            Some(usage) => (
                usage.prompt_tokens as i64,
                usage.completion_tokens as i64,
                (usage.prompt_tokens + usage.completion_tokens) as i64,
            ),
            None => (0, 0, 0),
        };

        // Log request event
        if let Some(logger) = &state.request_logger {
            let now_utc = Utc::now();
            let now_local = Local::now();
            let event = RequestEvent {
                request_id: request_id.to_string(),
                timestamp: now_utc.timestamp_millis(),
                date: now_local.format("%Y-%m-%d").to_string(),
                hour: now_local.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: "gemini".to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/chat/completions".to_string(),
                status: match session_result.status {
                    RequestStatus::Success => "success".to_string(),
                    RequestStatus::InstanceFailure => "failure".to_string(),
                    RequestStatus::BusinessError => "business_error".to_string(),
                    RequestStatus::Timeout => "timeout".to_string(),
                },
                error_type: None,
                error_message: None,
                input_tokens,
                output_tokens,
                total_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
                duration_ms,
            };
            logger.log_request(event).await;
        }

        tracing::info!(
            api_key_name = %auth.api_key_name,
            model = %model,
            instance = %instance_name,
            duration_ms = duration_ms,
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
        AnthropicInstanceConfig, ApiKeyConfig, Config, DiscoveryConfig, ProviderInstanceConfig,
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
                    api_key: Some("sk-test".to_string()),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "anthropic-test".to_string(),
                    enabled: false,
                    api_key: Some("test".to_string()),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    cache: crate::config::CacheConfig::default(),
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
                gemini: vec![ProviderInstanceConfig {
                    name: "gemini-test".to_string(),
                    enabled: false,
                    api_key: Some("test".to_string()),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
            },
            observability: crate::config::ObservabilityConfig::default(),
            oauth_providers: vec![],
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
            request_logger: None,
            token_store: None,
            oauth_manager: None,
        }
    }

    #[test]
    fn test_app_state_creation() {
        let state = create_test_state();
        assert!(!state.config.load().providers.openai.is_empty());
        assert!(state.config.load().providers.openai[0].enabled);
    }
}
