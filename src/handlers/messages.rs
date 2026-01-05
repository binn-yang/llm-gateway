use crate::{
    auth::AuthInfo,
    error::AppError,
    metrics,
    models::anthropic::{MessagesRequest, MessagesResponse},
    providers,
    router::Provider,
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
/// 严格模式：只允许 provider=anthropic 的模型
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

    // 1. 验证模型并获取路由信息
    let route_info = state.router.route(&model)?;

    // 2. 严格验证：只允许 Anthropic provider
    if route_info.provider != Provider::Anthropic {
        return Err(AppError::ProviderDisabled(format!(
            "Model '{}' is not an Anthropic model. The /v1/messages endpoint only supports Anthropic models. Provider: {}",
            model,
            route_info.provider
        )));
    }

    tracing::debug!(
        provider = %route_info.provider,
        api_model = %route_info.api_model,
        "Validated Anthropic model"
    );

    // 3. 记录请求指标
    metrics::record_request(
        &auth.api_key_name,
        "anthropic",
        &model,
        "/v1/messages",
    );

    // 4. 使用路由的 api_model 替换请求中的模型名
    let mut anthropic_request = request;
    anthropic_request.model = route_info.api_model;

    // 5. 加载配置
    let config = state.config.load();

    // 6. 调用 Anthropic API（复用现有的 provider 函数）
    let response = providers::anthropic::create_message(
        &state.http_client,
        &config.providers.anthropic,
        anthropic_request,
    )
    .await?;

    // 7. 根据 stream 参数处理响应
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
            "anthropic",
            &model,
            "input",
            body.usage.input_tokens,
        );
        metrics::record_tokens(
            &auth.api_key_name,
            "anthropic",
            &model,
            "output",
            body.usage.output_tokens,
        );
        metrics::record_duration(&auth.api_key_name, "anthropic", &model, start.elapsed());

        tracing::info!(
            api_key = %auth.api_key_name,
            model = %model,
            duration_ms = start.elapsed().as_millis(),
            input_tokens = body.usage.input_tokens,
            output_tokens = body.usage.output_tokens,
            "Completed native Anthropic messages request"
        );

        Ok(Json(body).into_response())
    }
}
