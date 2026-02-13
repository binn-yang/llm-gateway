use crate::{
    auth::AuthInfo,
    error::AppError,
    models::gemini::{CountTokensRequest, GenerateContentRequest, GenerateContentResponse, Part},
    observability::RequestEvent,
    providers,
    retry::RequestStatus,
    streaming,
};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;

use chrono::{Timelike, Utc, Local};
use std::time::Instant;

/// 复用 chat_completions 的 AppState
pub use crate::handlers::chat_completions::AppState;

/// 用于解析 Axum Path 参数的包装类型
/// 支持路径格式: "gemini-1.5-pro:generateContent" 或 "gemini-1.5-pro"
#[derive(Debug, Deserialize)]
pub struct GeminiFullPath(pub String);

impl GeminiFullPath {
    pub fn parse(path: &str) -> Result<(String, Option<String>), AppError> {
        parse_gemini_path(path)
    }
}

/// 从完整路径解析模型名称和操作类型
/// 路径格式: "models/gemini-1.5-pro:generateContent"
/// 返回: (模型名称, 操作类型)
fn parse_gemini_path(path: &str) -> Result<(String, Option<String>), AppError> {
    // 去掉常见的前缀
    let path = path.strip_prefix("/v1beta/").unwrap_or(path);
    let path = path.strip_prefix("/v1/").unwrap_or(path);
    let path = path.strip_prefix("models/").unwrap_or(path);

    // 检查是否有冒号（表示操作类型）
    if let Some(idx) = path.find(':') {
        let model = path[..idx].to_string();
        let action = path[idx + 1..].to_string();
        Ok((model, Some(action)))
    } else {
        // 没有冒号，整个路径就是模型名称
        Ok((path.to_string(), None))
    }
}

/// 处理 POST /v1beta/models/* (使用 any 通配符)
/// 接受完整路径字符串，从请求 URI 中提取路径
pub async fn handle_generate_content_any(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Extension(uri): Extension<axum::http::Uri>,
    Json(raw_request): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    let start = Instant::now();

    // 生成唯一的请求 ID
    let request_id = uuid::Uuid::new_v4().to_string();

    // 从 URI 中提取路径（去掉 /v1beta/ 前缀）
    let path = uri.path();
    let path = path.strip_prefix("/v1beta/").unwrap_or(path);
    let path = path.strip_prefix("/v1/").unwrap_or(path);

    // 解析请求
    let request: GenerateContentRequest = match serde_json::from_value(raw_request.clone()) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!(
                error = %e,
                request_sample = ?serde_json::to_string(&raw_request).ok().map(|s| {
                    if s.len() > 500 {
                        format!("{}...", &s[..500])
                    } else {
                        s
                    }
                }),
                "Failed to deserialize GenerateContentRequest"
            );
            return Err(AppError::ConversionError(format!(
                "Failed to deserialize request: {}",
                e
            )));
        }
    };

    // 从完整路径解析模型名称和操作类型
    let (model, _action) = parse_gemini_path(path)?;

    let provider_name = "gemini";

    // 创建请求 span
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        api_key_name = %auth.api_key_name,
        model = %model,
        endpoint = "/v1beta/models/:generateContent",
        provider = %provider_name,
        instance = tracing::field::Empty,
    );

    tracing::info!(
        parent: &span,
        stream = false,
        "Handling Gemini native generateContent request"
    );

    // 记录请求体（如果启用）
    let config = state.config.load();
    if config.observability.body_logging.enabled {
        let body_content = if config.observability.body_logging.simple_mode {
            extract_simple_request_gemini(&request)
        } else {
            let request_body = serde_json::to_string(&raw_request)
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
            "Request body"
        );
    }

    // 获取 Gemini LoadBalancer 和 Provider
    let registry = state.registry.load();
    let registered = registry
        .get("gemini")
        .ok_or_else(|| AppError::ProviderDisabled("Gemini provider not configured".to_string()))?;
    let load_balancer = registered.load_balancer.clone();
    let provider = registered.provider.clone();

    // 使用 execute_with_session 执行请求（粘性会话 + 故障转移）
    let http_client = state.http_client.clone();
    let oauth_manager = state.oauth_manager.clone();
    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let gemini_request = request.clone();
            let oauth_manager = oauth_manager.clone();
            let model_clone = model.clone();
            let provider = provider.clone();
            async move {
                let oauth_token = crate::handlers::common::resolve_oauth_token(
                    instance.config.as_ref(), &oauth_manager,
                ).await?;

                let body = serde_json::to_value(&gemini_request)
                    .map_err(|e| AppError::ConversionError(format!("Failed to serialize request: {}", e)))?;

                crate::handlers::common::send_and_check(
                    provider.as_ref(),
                    &http_client,
                    instance.config.as_ref(),
                    crate::provider_trait::UpstreamRequest {
                        body,
                        model: model_clone,
                        stream: false,
                        oauth_token,
                    },
                ).await
            }
        },
    )
    .await?;

    let instance_name = session_result.instance_name;
    let response = session_result.result?;
    let duration_ms = start.elapsed().as_millis() as i64;

    span.record("instance", instance_name.as_str());

    // 解析响应
    let body: GenerateContentResponse = response.json().await?;

    // 记录响应体（如果启用）
    if config.observability.body_logging.enabled {
        let body_content = if config.observability.body_logging.simple_mode {
            extract_simple_response_gemini(&body)
        } else {
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
            parent: &span,
            event_type = if config.observability.body_logging.simple_mode {
                "simple_response"
            } else {
                "response_body"
            },
            body = %body_content,
            body_size = body_content.len(),
            streaming = false,
            chunks_count = 0,
            "Response body"
        );
    }

    // 提取 token 使用量
    let (input_tokens, output_tokens) = if let Some(ref usage) = body.usage_metadata {
        (
            usage.prompt_token_count as i64,
            usage.candidates_token_count as i64,
        )
    } else {
        (0, 0)
    };

    // 记录请求事件
    if let Some(logger) = &state.request_logger {
        let now_utc = Utc::now();
        let now_local = Local::now();
        let event = RequestEvent {
            request_id: request_id.clone(),
            timestamp: now_utc.timestamp_millis(),
            date: now_local.format("%Y-%m-%d").to_string(),
            hour: now_local.hour() as i32,
            api_key_name: auth.api_key_name.clone(),
            provider: provider_name.to_string(),
            instance: instance_name.clone(),
            model: model.to_string(),
            endpoint: "/v1beta/models/:generateContent".to_string(),
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
            total_tokens: input_tokens + output_tokens,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            duration_ms,
            input_cost: 0.0,
            output_cost: 0.0,
            cache_write_cost: 0.0,
            cache_read_cost: 0.0,
            total_cost: 0.0,
            session_id: None,
        };
        logger.log_request(event).await;
    }

    tracing::info!(
        api_key_name = %auth.api_key_name,
        model = %model,
        instance = %instance_name,
        duration_ms = duration_ms,
        input_tokens,
        output_tokens,
        "Completed Gemini native generateContent request"
    );

    // 构建响应
    let mut response = Json(body).into_response();

    // 添加 X-Request-ID 头
    response.headers_mut().insert(
        "X-Request-ID",
        axum::http::HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("invalid-request-id")),
    );

    Ok(response)
}

/// 处理 POST /v1beta/models/* (流式，使用 any 通配符)
pub async fn handle_stream_generate_content_any(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Path(full_path): Path<GeminiFullPath>,
    Json(raw_request): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    let start = Instant::now();

    // 生成唯一的请求 ID
    let request_id = uuid::Uuid::new_v4().to_string();

    // 解析请求
    let request: GenerateContentRequest = match serde_json::from_value(raw_request.clone()) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!(
                error = %e,
                request_sample = ?serde_json::to_string(&raw_request).ok().map(|s| {
                    if s.len() > 500 {
                        format!("{}...", &s[..500])
                    } else {
                        s
                    }
                }),
                "Failed to deserialize GenerateContentRequest"
            );
            return Err(AppError::ConversionError(format!(
                "Failed to deserialize request: {}",
                e
            )));
        }
    };

    // 从完整路径解析模型名称和操作类型
    let (model, _action) = GeminiFullPath::parse(&full_path.0)?;

    let provider_name = "gemini";

    // 创建请求 span
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        api_key_name = %auth.api_key_name,
        model = %model,
        endpoint = "/v1beta/models/:streamGenerateContent",
        provider = %provider_name,
        instance = tracing::field::Empty,
    );

    tracing::info!(
        parent: &span,
        stream = true,
        "Handling Gemini native streamGenerateContent request"
    );

    // 记录请求体（如果启用）
    let config = state.config.load();
    if config.observability.body_logging.enabled {
        let body_content = if config.observability.body_logging.simple_mode {
            extract_simple_request_gemini(&request)
        } else {
            let request_body = serde_json::to_string(&raw_request)
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
            "Request body"
        );
    }

    // 获取 Gemini LoadBalancer 和 Provider
    let registry = state.registry.load();
    let registered = registry
        .get("gemini")
        .ok_or_else(|| AppError::ProviderDisabled("Gemini provider not configured".to_string()))?;
    let load_balancer = registered.load_balancer.clone();
    let provider = registered.provider.clone();

    // 使用 execute_with_session 执行请求
    let http_client = state.http_client.clone();
    let oauth_manager = state.oauth_manager.clone();
    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let gemini_request = request.clone();
            let oauth_manager = oauth_manager.clone();
            let model_clone = model.clone();
            let provider = provider.clone();
            async move {
                let oauth_token = crate::handlers::common::resolve_oauth_token(
                    instance.config.as_ref(), &oauth_manager,
                ).await?;

                let body = serde_json::to_value(&gemini_request)
                    .map_err(|e| AppError::ConversionError(format!("Failed to serialize request: {}", e)))?;

                crate::handlers::common::send_and_check(
                    provider.as_ref(),
                    &http_client,
                    instance.config.as_ref(),
                    crate::provider_trait::UpstreamRequest {
                        body,
                        model: model_clone,
                        stream: true,
                        oauth_token,
                    },
                ).await
            }
        },
    )
    .await?;

    let instance_name = session_result.instance_name;
    let response = session_result.result?;
    let duration_ms = start.elapsed().as_millis() as i64;

    span.record("instance", instance_name.as_str());

    // 记录初始请求（token 计数为 0，稍后更新）
    if let Some(logger) = &state.request_logger {
        let now_utc = Utc::now();
        let now_local = Local::now();
        let event = RequestEvent {
            request_id: request_id.clone(),
            timestamp: now_utc.timestamp_millis(),
            date: now_local.format("%Y-%m-%d").to_string(),
            hour: now_local.hour() as i32,
            api_key_name: auth.api_key_name.clone(),
            provider: provider_name.to_string(),
            instance: instance_name.clone(),
            model: model.to_string(),
            endpoint: "/v1beta/models/:streamGenerateContent".to_string(),
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
            input_cost: 0.0,
            output_cost: 0.0,
            cache_write_cost: 0.0,
            cache_read_cost: 0.0,
            total_cost: 0.0,
            session_id: None,
        };
        logger.log_request(event).await;
    }

    tracing::debug!("Creating native Gemini SSE stream with usage tracking");

    // 创建流式响应 tracker
    let tracker = streaming::StreamingUsageTracker::new(request_id.clone());
    let sse_stream = streaming::create_native_gemini_sse_stream_with_tracker(response, tracker.clone());

    // 启动后台任务在流完成后更新 token
    if let Some(logger) = state.request_logger.clone() {
        let request_id_owned = request_id.clone();
        let model_owned = model.clone();
        let tracker_clone = tracker.clone();
        let config = state.config.load().clone();
        let span_clone = span.clone();
        tokio::spawn(async move {
            if let Some((input, output, _, _)) = tracker_clone.wait_for_completion().await {
                logger.update_tokens(
                    &request_id_owned,
                    &model_owned,
                    input as i64,
                    output as i64,
                    (input + output) as i64,
                    0, // cache_creation
                    0, // cache_read
                ).await;

                // 记录响应体
                if config.observability.body_logging.enabled {
                    let accumulated_response = tracker_clone.get_accumulated_response();

                    let body_content = if config.observability.body_logging.simple_mode {
                        extract_simple_response_streaming_gemini(&accumulated_response)
                    } else {
                        let redacted = crate::logging::redact_sensitive_data(
                            &accumulated_response,
                            &config.observability.body_logging.redact_patterns
                        );
                        let (truncated, _) = crate::logging::truncate_body(
                            redacted,
                            config.observability.body_logging.max_body_size
                        );
                        truncated
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
                        streaming = true,
                        chunks_count = tracker_clone.chunks_count(),
                        "Response body"
                    );
                }
            } else {
                tracing::warn!(
                    request_id = %request_id_owned,
                    provider = "gemini",
                    endpoint = "/v1beta/models/:streamGenerateContent",
                    "Stream completed without token usage data"
                );
            }
        });
    }

    let mut response = sse_stream.into_response();
    response.headers_mut().insert(
        "X-Request-ID",
        axum::http::HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("invalid-request-id")),
    );
    Ok(response)
}

/// 处理 POST /v1beta/models/:model:countTokens
pub async fn handle_count_tokens(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Path(full_path): Path<GeminiFullPath>,
    Json(request): Json<CountTokensRequest>,
) -> Result<Response, AppError> {
    let start = Instant::now();
    let request_id = uuid::Uuid::new_v4().to_string();

    // 从完整路径解析模型名称和操作类型
    let (model, _action) = GeminiFullPath::parse(&full_path.0)?;

    tracing::info!(
        request_id = %request_id,
        api_key_name = %auth.api_key_name,
        model = %model,
        "Handling Gemini native countTokens request"
    );

    // 获取 Gemini LoadBalancer
    let registry = state.registry.load();
    let registered = registry
        .get("gemini")
        .ok_or_else(|| AppError::ProviderDisabled("Gemini provider not configured".to_string()))?;
    let load_balancer = registered.load_balancer.clone();

    // 执行请求
    let http_client = state.http_client.clone();
    let oauth_manager = state.oauth_manager.clone();
    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let request_clone = request.clone();
            let oauth_manager = oauth_manager.clone();
            let model_clone = model.clone();
            async move {
                let config = instance.config
                    .as_any()
                    .downcast_ref::<crate::config::ProviderInstanceConfig>()
                    .ok_or_else(|| AppError::InternalError("Invalid instance config type".to_string()))?;

                let oauth_token = if config.auth_mode == crate::config::AuthMode::OAuth {
                    if let Some(ref oauth_provider_name) = config.oauth_provider {
                        if let Some(ref manager) = oauth_manager {
                            match manager.get_valid_token(oauth_provider_name).await {
                                Ok(token) => Some(token.access_token),
                                Err(e) => return Err(e),
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

                providers::gemini::count_tokens(
                    &http_client,
                    config,
                    &model_clone,
                    &request_clone,
                    oauth_token.as_deref(),
                ).await
            }
        },
    )
    .await?;

    let instance_name = session_result.instance_name;

    tracing::info!(
        api_key_name = %auth.api_key_name,
        model = %model,
        instance = %instance_name,
        duration_ms = start.elapsed().as_millis(),
        "Completed Gemini native countTokens request"
    );

    let mut resp = Json(session_result.result?).into_response();
    resp.headers_mut().insert(
        "X-Request-ID",
        axum::http::HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("invalid-request-id")),
    );
    Ok(resp)
}

/// 处理 GET /v1beta/models (列出所有模型)
pub async fn handle_list_models(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
) -> Result<Response, AppError> {
    let request_id = uuid::Uuid::new_v4().to_string();

    tracing::info!(
        request_id = %request_id,
        api_key_name = %auth.api_key_name,
        "Handling Gemini native listModels request"
    );

    // 获取 Gemini LoadBalancer
    let registry = state.registry.load();
    let registered = registry
        .get("gemini")
        .ok_or_else(|| AppError::ProviderDisabled("Gemini provider not configured".to_string()))?;
    let load_balancer = registered.load_balancer.clone();

    // 获取任意一个实例配置（用于 base_url 和认证）
    let config = {
        let instances = load_balancer.get_all_instances_health().await;
        if instances.is_empty() {
            return Err(AppError::NoHealthyInstances("No healthy Gemini instances".to_string()));
        }
        let instance_name = &instances[0].instance;
        let instance = load_balancer.get_instance_by_name(instance_name)
            .ok_or_else(|| AppError::InternalError(format!("Instance {} not found", instance_name)))?;
        instance.config
            .as_any()
            .downcast_ref::<crate::config::ProviderInstanceConfig>()
            .ok_or_else(|| AppError::InternalError("Invalid instance config type".to_string()))?
            .clone()
    };

    // 获取 OAuth token（如需要）
    let oauth_token = if config.auth_mode == crate::config::AuthMode::OAuth {
        if let Some(ref oauth_provider_name) = config.oauth_provider {
            if let Some(ref manager) = state.oauth_manager {
                match manager.get_valid_token(oauth_provider_name).await {
                    Ok(token) => Some(token.access_token),
                    Err(e) => return Err(e),
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

    // 调用 list_models
    let http_client = state.http_client.clone();
    let response = providers::gemini::list_models(
        &http_client,
        &config,
        oauth_token.as_deref(),
    ).await?;

    tracing::info!(
        api_key_name = %auth.api_key_name,
        instance = %config.name,
        "Completed Gemini native listModels request"
    );

    let mut resp = Json(response).into_response();
    resp.headers_mut().insert(
        "X-Request-ID",
        axum::http::HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("invalid-request-id")),
    );
    Ok(resp)
}

/// 处理 GET /v1beta/models/:model (获取单个模型详情)
pub async fn handle_get_model(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Path(full_path): Path<GeminiFullPath>,
) -> Result<Response, AppError> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // 从完整路径解析模型名称
    let (model, _action) = GeminiFullPath::parse(&full_path.0)?;

    tracing::info!(
        request_id = %request_id,
        api_key_name = %auth.api_key_name,
        model = %model,
        "Handling Gemini native getModel request"
    );

    // 获取 Gemini LoadBalancer
    let registry = state.registry.load();
    let registered = registry
        .get("gemini")
        .ok_or_else(|| AppError::ProviderDisabled("Gemini provider not configured".to_string()))?;
    let load_balancer = registered.load_balancer.clone();

    // 获取实例配置
    let config = {
        let instances = load_balancer.get_all_instances_health().await;
        if instances.is_empty() {
            return Err(AppError::NoHealthyInstances("No healthy Gemini instances".to_string()));
        }
        let instance_name = &instances[0].instance;
        let instance = load_balancer.get_instance_by_name(instance_name)
            .ok_or_else(|| AppError::InternalError(format!("Instance {} not found", instance_name)))?;
        instance.config
            .as_any()
            .downcast_ref::<crate::config::ProviderInstanceConfig>()
            .ok_or_else(|| AppError::InternalError("Invalid instance config type".to_string()))?
            .clone()
    };

    // 获取 OAuth token（如需要）
    let oauth_token = if config.auth_mode == crate::config::AuthMode::OAuth {
        if let Some(ref oauth_provider_name) = config.oauth_provider {
            if let Some(ref manager) = state.oauth_manager {
                match manager.get_valid_token(oauth_provider_name).await {
                    Ok(token) => Some(token.access_token),
                    Err(e) => return Err(e),
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

    // 调用 get_model（透传上游 API 响应）
    let http_client = state.http_client.clone();
    let response = providers::gemini::get_model(
        &http_client,
        &config,
        &model,
        oauth_token.as_deref(),
    ).await?;

    tracing::info!(
        api_key_name = %auth.api_key_name,
        instance = %config.name,
        model = %model,
        "Completed Gemini native getModel request"
    );

    let mut resp = Json(response).into_response();
    resp.headers_mut().insert(
        "X-Request-ID",
        axum::http::HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("invalid-request-id")),
    );
    Ok(resp)
}

// ============================================================================
// Simple Mode Logging Helpers
// ============================================================================

/// 提取 Gemini 请求的简单日志（仅用户消息内容）
fn extract_simple_request_gemini(request: &GenerateContentRequest) -> String {
    let mut parts = Vec::new();

    for content in &request.contents {
        if content.role == "user" {
            for part in &content.parts {
                if let Part::Text { text } = part {
                    parts.push(text.clone());
                }
            }
        }
    }

    // 限制内容长度
    let joined = parts.join(" ");
    if joined.len() > 500 {
        format!("{}...", &joined[..500])
    } else {
        joined
    }
}

/// 提取 Gemini 非流式响应的简单日志
fn extract_simple_response_gemini(response: &GenerateContentResponse) -> String {
    let mut texts = Vec::new();

    for candidate in &response.candidates {
        for part in &candidate.content.parts {
            if let Part::Text { text } = part {
                texts.push(text.clone());
            }
        }
    }

    let joined = texts.join(" ");
    if joined.len() > 1000 {
        format!("{}...", &joined[..1000])
    } else {
        joined
    }
}

/// 提取 Gemini 流式响应的简单日志
fn extract_simple_response_streaming_gemini(accumulated: &str) -> String {
    // 从累积的 SSE 响应中提取文本内容
    // Gemini SSE 格式: data: {"candidates": [{"content": {"parts": [{"text": "..."}]}]}
    let mut texts = Vec::new();

    for line in accumulated.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(candidates) = json.get("candidates").and_then(|v| v.as_array()) {
                    for candidate in candidates {
                        if let Some(content) = candidate.get("content") {
                            if let Some(parts) = content.get("parts").and_then(|v| v.as_array()) {
                                for part in parts {
                                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                        texts.push(text.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let joined = texts.join(" ");
    if joined.len() > 1000 {
        format!("{}...", &joined[..1000])
    } else {
        joined
    }
}
