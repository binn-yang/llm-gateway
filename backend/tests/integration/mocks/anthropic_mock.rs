use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
use std::time::Duration;

/// 设置 Anthropic Messages API mock 服务器
///
/// # 参数
/// - `latency_ms`: 响应延迟(毫秒)
/// - `error_rate`: 错误率(0.0-1.0),返回 529 错误的概率
pub async fn setup_anthropic_mock(latency_ms: u64, error_rate: f64) -> MockServer {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let is_streaming = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

            if is_streaming {
                // 流式请求将由另一个 mock 处理
                ResponseTemplate::new(404)
            } else {
                // 根据错误率决定是否返回错误
                if error_rate > 0.0 && rand::random::<f64>() < error_rate {
                    ResponseTemplate::new(529)
                        .set_delay(Duration::from_millis(latency_ms))
                        .set_body_json(serde_json::json!({
                            "type": "error",
                            "error": {
                                "type": "overloaded_error",
                                "message": "Overloaded"
                            }
                        }))
                } else {
                    let response = create_messages_response("claude-3-5-sonnet-20241022");
                    ResponseTemplate::new(200)
                        .set_delay(Duration::from_millis(latency_ms))
                        .set_body_json(&response)
                }
            }
        })
        .mount(&mock_server)
        .await;

    mock_server
}

/// 设置支持流式响应的 Anthropic Messages API mock
///
/// # 参数
/// - `latency_ms`: 首字节延迟(TTFB)
/// - `num_chunks`: 生成的 chunk 数量
/// - `chunk_interval_ms`: chunk 之间的间隔
pub async fn setup_anthropic_streaming_mock(
    latency_ms: u64,
    num_chunks: usize,
    chunk_interval_ms: u64,
) -> MockServer {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let is_streaming = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

            if is_streaming {
                let sse_body = create_streaming_response(num_chunks, chunk_interval_ms);
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(latency_ms))
                    .set_body_raw(sse_body, "text/event-stream")
                    .insert_header("cache-control", "no-cache")
                    .insert_header("connection", "keep-alive")
            } else {
                let response = create_messages_response("claude-3-5-sonnet-20241022");
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(latency_ms))
                    .set_body_json(&response)
            }
        })
        .mount(&mock_server)
        .await;

    mock_server
}

/// 创建非流式 Messages 响应
fn create_messages_response(model: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "msg_test123",
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "text",
            "text": "This is a test response from the mock Anthropic API."
        }],
        "model": model,
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 10,
            "output_tokens": 15,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    })
}

/// 创建流式 SSE 响应字符串
///
/// Anthropic 流式响应事件顺序:
/// 1. message_start - 消息开始,包含初始 input_tokens
/// 2. content_block_start - 内容块开始
/// 3. content_block_delta (多次) - 内容增量更新
/// 4. content_block_stop - 内容块结束
/// 5. message_delta - 消息增量,包含 output_tokens
/// 6. message_stop - 消息结束
fn create_streaming_response(num_chunks: usize, _chunk_interval_ms: u64) -> String {
    let mut sse_body = String::new();

    // 1. message_start event
    sse_body.push_str("event: message_start\ndata: ");
    sse_body.push_str(&serde_json::to_string(&serde_json::json!({
        "type": "message_start",
        "message": {
            "id": "msg_stream123",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-20241022",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 0
            }
        }
    })).unwrap());
    sse_body.push_str("\n\n");

    // 2. content_block_start event
    sse_body.push_str("event: content_block_start\ndata: ");
    sse_body.push_str(&serde_json::to_string(&serde_json::json!({
        "type": "content_block_start",
        "index": 0,
        "content_block": {
            "type": "text",
            "text": ""
        }
    })).unwrap());
    sse_body.push_str("\n\n");

    // 3. content_block_delta events (多次)
    for i in 0..num_chunks {
        sse_body.push_str("event: content_block_delta\ndata: ");
        sse_body.push_str(&serde_json::to_string(&serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": format!("Token {} ", i + 1)
            }
        })).unwrap());
        sse_body.push_str("\n\n");
    }

    // 4. content_block_stop event
    sse_body.push_str("event: content_block_stop\ndata: ");
    sse_body.push_str(&serde_json::to_string(&serde_json::json!({
        "type": "content_block_stop",
        "index": 0
    })).unwrap());
    sse_body.push_str("\n\n");

    // 5. message_delta event - 包含完整的 token 使用信息
    sse_body.push_str("event: message_delta\ndata: ");
    sse_body.push_str(&serde_json::to_string(&serde_json::json!({
        "type": "message_delta",
        "delta": {
            "stop_reason": "end_turn",
            "stop_sequence": null
        },
        "usage": {
            "input_tokens": 10,
            "output_tokens": num_chunks as u64,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    })).unwrap());
    sse_body.push_str("\n\n");

    // 6. message_stop event
    sse_body.push_str("event: message_stop\ndata: ");
    sse_body.push_str(&serde_json::to_string(&serde_json::json!({
        "type": "message_stop"
    })).unwrap());
    sse_body.push_str("\n\n");

    sse_body
}

/// 设置带缓存指标的 Anthropic mock
///
/// 模拟 prompt caching 场景,返回 cache_creation 和 cache_read tokens
pub async fn setup_anthropic_mock_with_cache(
    latency_ms: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> MockServer {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(move |_req: &wiremock::Request| {
            let response = serde_json::json!({
                "id": "msg_cached123",
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "text",
                    "text": "Response with cache metrics."
                }],
                "model": "claude-3-5-sonnet-20241022",
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "cache_creation_input_tokens": cache_creation_tokens,
                    "cache_read_input_tokens": cache_read_tokens
                }
            });
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(latency_ms))
                .set_body_json(&response)
        })
        .mount(&mock_server)
        .await;

    mock_server
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anthropic_mock_non_streaming() {
        let mock_server = setup_anthropic_mock(0, 0.0).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/messages", mock_server.uri()))
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-3-5-sonnet-20241022",
                "messages": [{"role": "user", "content": "Hello"}],
                "max_tokens": 1024
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["type"], "message");
        assert_eq!(body["role"], "assistant");
        assert!(body["content"][0]["text"].is_string());
    }

    #[tokio::test]
    async fn test_anthropic_mock_streaming() {
        let mock_server = setup_anthropic_streaming_mock(0, 5, 0).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/messages", mock_server.uri()))
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-3-5-sonnet-20241022",
                "messages": [{"role": "user", "content": "Hello"}],
                "max_tokens": 1024,
                "stream": true
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/event-stream"
        );

        let body = response.text().await.unwrap();
        assert!(body.contains("event: message_start"));
        assert!(body.contains("event: content_block_delta"));
        assert!(body.contains("event: message_delta"));
        assert!(body.contains("event: message_stop"));
    }

    #[tokio::test]
    async fn test_anthropic_mock_error_rate() {
        let mock_server = setup_anthropic_mock(0, 1.0).await;  // 100% error rate

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/messages", mock_server.uri()))
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-3-5-sonnet-20241022",
                "messages": [{"role": "user", "content": "Hello"}],
                "max_tokens": 1024
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 529);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["type"], "error");
    }

    #[tokio::test]
    async fn test_anthropic_mock_with_cache() {
        let mock_server = setup_anthropic_mock_with_cache(0, 100, 50).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/messages", mock_server.uri()))
            .header("anthropic-version", "2023-06-01")
            .json(&serde_json::json!({
                "model": "claude-3-5-sonnet-20241022",
                "messages": [{"role": "user", "content": "Hello"}],
                "max_tokens": 1024
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["usage"]["cache_creation_input_tokens"], 100);
        assert_eq!(body["usage"]["cache_read_input_tokens"], 50);
    }
}
