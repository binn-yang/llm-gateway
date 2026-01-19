use crate::{
    auth::AuthInfo,
    error::AppError,
    models::anthropic::{MessageContent, MessagesRequest, MessagesResponse},
    observability::RequestEvent,
    providers,
    retry::RequestStatus,
    streaming,
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{Timelike, Utc};
use std::time::Instant;

/// 复用 chat_completions 的 AppState
pub use crate::handlers::chat_completions::AppState;

/// 处理 POST /v1/messages 端点（Anthropic 原生 API）
///
/// 透传模式：路由到任何 provider（主要用于 Anthropic 兼容的 API）
pub async fn handle_messages(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Json(raw_request): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    let start = Instant::now();

    // 尝试反序列化，记录错误详情
    let request: MessagesRequest = match serde_json::from_value(raw_request.clone()) {
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
                "Failed to deserialize MessagesRequest"
            );
            return Err(AppError::ConversionError(format!(
                "Failed to deserialize request: {}",
                e
            )));
        }
    };

    let model = request.model.clone();
    let is_stream = request.stream.unwrap_or(false);

    tracing::info!(
        api_key_name = %auth.api_key_name,
        model = %model,
        stream = is_stream,
        "Handling native Anthropic messages request"
    );

    // 1. 路由到 provider
    let route_info = state.router.route(&model)?;

    tracing::debug!(
        provider = %route_info.provider,
        "Routed model to provider"
    );

    // 2. 清理 assistant 消息中的 thinking 字段
    // Anthropic API 的不对称设计：响应中的 thinking 格式 ≠ 请求中的 thinking 格式
    // 当 Claude Code 将之前的响应作为历史发送时，需要清理不符合请求格式的 thinking
    let mut anthropic_request = request;
    for message in &mut anthropic_request.messages {
        if message.role == "assistant" {
            if let MessageContent::Blocks(ref mut blocks) = &mut message.content {
                for block in blocks.iter_mut() {
                    // 检查 thinking 字段是否存在且格式不正确
                    if let Some(thinking) = &block.thinking {
                        // 如果 thinking 是对象但缺少 signature 字段，删除它
                        if let Some(obj) = thinking.as_object() {
                            if !obj.contains_key("signature") {
                                tracing::debug!(
                                    thinking_content = ?obj.get("thinking"),
                                    "Removing thinking field without signature from assistant message"
                                );
                                block.thinking = None;
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Get LoadBalancer for Anthropic provider
    let load_balancers_map = state.load_balancers.load();
    let load_balancer = load_balancers_map
        .get(&crate::router::Provider::Anthropic)
        .ok_or_else(|| AppError::ProviderDisabled("Anthropic provider not configured".to_string()))?
        .clone();

    // 3. Execute request with sticky session (returns SessionResult)
    let http_client = state.http_client.clone();
    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let anthropic_request = anthropic_request.clone();
            async move {
                // Extract config from the instance
                let config = match &instance.config {
                    crate::load_balancer::ProviderInstanceConfigEnum::Anthropic(cfg) => cfg.as_ref(),
                    _ => return Err(AppError::InternalError("Invalid instance config type".to_string())),
                };

                // Call Anthropic API
                providers::anthropic::create_message(&http_client, config, anthropic_request).await
            }
        },
    )
    .await?;

    let instance_name = session_result.instance_name;
    let provider_name = route_info.provider.as_str();
    let response = session_result.result?;
    let duration_ms = start.elapsed().as_millis() as i64;

    // 4. 根据 stream 参数处理响应
    if is_stream {
        // 流式响应 - 直接转发原生 Anthropic SSE
        tracing::debug!("Streaming native Anthropic SSE response");

        // Log request event (streaming)
        if let Some(logger) = &state.request_logger {
            let now = Utc::now();
            let event = RequestEvent {
                request_id: uuid::Uuid::new_v4().to_string(),
                timestamp: now.timestamp_millis(),
                date: now.format("%Y-%m-%d").to_string(),
                hour: now.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: provider_name.to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/messages".to_string(),
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
                duration_ms,
            };
            logger.log_request(event).await;
        }

        let sse_stream = streaming::create_native_anthropic_sse_stream(response);
        Ok(sse_stream.into_response())
    } else {
        // 非流式响应 - 返回原生 Anthropic JSON
        let body: MessagesResponse = response.json().await?;

        // Log request event
        if let Some(logger) = &state.request_logger {
            let now = Utc::now();
            let event = RequestEvent {
                request_id: uuid::Uuid::new_v4().to_string(),
                timestamp: now.timestamp_millis(),
                date: now.format("%Y-%m-%d").to_string(),
                hour: now.hour() as i32,
                api_key_name: auth.api_key_name.clone(),
                provider: provider_name.to_string(),
                instance: instance_name.clone(),
                model: model.to_string(),
                endpoint: "/v1/messages".to_string(),
                status: match session_result.status {
                    RequestStatus::Success => "success".to_string(),
                    RequestStatus::InstanceFailure => "failure".to_string(),
                    RequestStatus::BusinessError => "business_error".to_string(),
                    RequestStatus::Timeout => "timeout".to_string(),
                },
                error_type: None,
                error_message: None,
                input_tokens: body.usage.input_tokens as i64,
                output_tokens: body.usage.output_tokens as i64,
                total_tokens: (body.usage.input_tokens + body.usage.output_tokens) as i64,
                duration_ms,
            };
            logger.log_request(event).await;
        }

        tracing::info!(
            api_key_name = %auth.api_key_name,
            model = %model,
            instance = %instance_name,
            duration_ms = duration_ms,
            input_tokens = body.usage.input_tokens,
            output_tokens = body.usage.output_tokens,
            stop_reason = ?body.stop_reason,
            content_blocks = body.content.len(),
            "Completed native Anthropic messages request"
        );

        Ok(Json(body).into_response())
    }
}
