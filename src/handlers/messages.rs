use crate::{
    auth::AuthInfo,
    error::AppError,
    metrics,
    models::anthropic::{MessagesRequest, MessagesResponse},
    providers,
    streaming,
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::time::Instant;

/// 复用 chat_completions 的 AppState
pub use crate::handlers::chat_completions::AppState;

/// 处理 POST /v1/messages 端点（Anthropic 原生 API）
///
/// 透传模式：路由到任何 provider（主要用于 Anthropic 兼容的 API）
pub async fn handle_messages(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Json(request): Json<MessagesRequest>,
) -> Result<Response, AppError> {
    let start = Instant::now();
    let model = request.model.clone();
    let is_stream = request.stream.unwrap_or(false);

    tracing::info!(
        api_key = %auth.api_key_name,
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

    // 2. 记录请求指标
    metrics::record_request(
        &auth.api_key_name,
        route_info.provider.as_str(),
        &model,
        "/v1/messages",
    );

    // 3. 透传原始模型名
    let anthropic_request = request;

    // 4. Get LoadBalancer for Anthropic provider
    let load_balancer = state
        .load_balancers
        .get(&crate::router::Provider::Anthropic)
        .ok_or_else(|| AppError::ProviderDisabled("Anthropic provider not configured".to_string()))?;

    // 5. Execute request with sticky session
    let http_client = state.http_client.clone();
    let response = crate::retry::execute_with_session(
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

    let provider_name = route_info.provider.as_str();

    // 6. 根据 stream 参数处理响应
    if is_stream {
        // 流式响应 - 直接转发原生 Anthropic SSE
        tracing::debug!("Streaming native Anthropic SSE response");
        let sse_stream = streaming::create_native_anthropic_sse_stream(response);
        Ok(sse_stream.into_response())
    } else {
        // 非流式响应 - 返回原生 Anthropic JSON
        let body: MessagesResponse = response.json().await?;

        // 记录指标
        metrics::record_tokens(
            &auth.api_key_name,
            provider_name,
            &model,
            "input",
            body.usage.input_tokens,
        );
        metrics::record_tokens(
            &auth.api_key_name,
            provider_name,
            &model,
            "output",
            body.usage.output_tokens,
        );
        metrics::record_duration(&auth.api_key_name, provider_name, &model, start.elapsed());

        tracing::info!(
            api_key = %auth.api_key_name,
            model = %model,
            duration_ms = start.elapsed().as_millis(),
            input_tokens = body.usage.input_tokens,
            output_tokens = body.usage.output_tokens,
            stop_reason = ?body.stop_reason,
            content_blocks = body.content.len(),
            "Completed native Anthropic messages request"
        );

        Ok(Json(body).into_response())
    }
}
