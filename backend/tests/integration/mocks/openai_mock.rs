use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
use std::time::Duration;

/// 设置 OpenAI API mock 服务器
///
/// # 参数
/// - `latency_ms`: 响应延迟(毫秒)
/// - `error_rate`: 错误率(0.0-1.0),返回 503 错误的概率
pub async fn setup_openai_mock(latency_ms: u64, error_rate: f64) -> MockServer {
    let mock_server = MockServer::start().await;

    // 非流式响应 mock
    let non_streaming_response = create_chat_completion_response("gpt-4", false);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |req: &wiremock::Request| {
            // 检查是否是流式请求
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let is_streaming = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

            if is_streaming {
                // 流式请求将由另一个 mock 处理
                ResponseTemplate::new(404)
            } else {
                // 根据错误率决定是否返回错误
                if error_rate > 0.0 && rand::random::<f64>() < error_rate {
                    ResponseTemplate::new(503)
                        .set_delay(Duration::from_millis(latency_ms))
                        .set_body_json(serde_json::json!({
                            "error": {
                                "message": "Service temporarily unavailable",
                                "type": "server_error",
                                "code": 503
                            }
                        }))
                } else {
                    ResponseTemplate::new(200)
                        .set_delay(Duration::from_millis(latency_ms))
                        .set_body_json(&non_streaming_response)
                }
            }
        })
        .mount(&mock_server)
        .await;

    mock_server
}

/// 设置支持流式响应的 OpenAI API mock
///
/// # 参数
/// - `latency_ms`: 首字节延迟(TTFB)
/// - `num_chunks`: 生成的 chunk 数量
/// - `chunk_interval_ms`: chunk 之间的间隔
pub async fn setup_openai_streaming_mock(
    latency_ms: u64,
    num_chunks: usize,
    chunk_interval_ms: u64,
) -> MockServer {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |req: &wiremock::Request| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let is_streaming = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

            if is_streaming {
                let sse_body = create_streaming_response(num_chunks, chunk_interval_ms);
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(latency_ms))
                    .append_header("Content-Type", "text/event-stream")
                    .append_header("Cache-Control", "no-cache")
                    .append_header("Connection", "keep-alive")
                    .set_body_raw(sse_body.as_bytes(), "text/event-stream")
            } else {
                let response = create_chat_completion_response("gpt-4", false);
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(latency_ms))
                    .set_body_json(&response)
            }
        })
        .mount(&mock_server)
        .await;

    mock_server
}

/// 创建非流式 ChatCompletion 响应
fn create_chat_completion_response(model: &str, include_usage: bool) -> serde_json::Value {
    let mut response = serde_json::json!({
        "id": "chatcmpl-test123",
        "object": "chat.completion",
        "created": 1234567890,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "This is a test response from the mock OpenAI API."
            },
            "finish_reason": "stop"
        }]
    });

    if include_usage {
        response["usage"] = serde_json::json!({
            "prompt_tokens": 10,
            "completion_tokens": 15,
            "total_tokens": 25
        });
    }

    response
}

/// 创建流式 SSE 响应字符串
fn create_streaming_response(num_chunks: usize, _chunk_interval_ms: u64) -> String {
    let mut sse_body = String::new();

    // 首个 chunk
    sse_body.push_str("data: ");
    sse_body.push_str(&serde_json::to_string(&serde_json::json!({
        "id": "chatcmpl-stream123",
        "object": "chat.completion.chunk",
        "created": 1234567890,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "content": ""
            },
            "finish_reason": null
        }]
    })).unwrap());
    sse_body.push_str("\n\n");

    // 内容 chunks
    for i in 0..num_chunks {
        sse_body.push_str("data: ");
        sse_body.push_str(&serde_json::to_string(&serde_json::json!({
            "id": "chatcmpl-stream123",
            "object": "chat.completion.chunk",
            "created": 1234567890,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {
                    "content": format!("Token {} ", i + 1)
                },
                "finish_reason": null
            }]
        })).unwrap());
        sse_body.push_str("\n\n");
    }

    // 最后一个 chunk 包含 usage
    sse_body.push_str("data: ");
    sse_body.push_str(&serde_json::to_string(&serde_json::json!({
        "id": "chatcmpl-stream123",
        "object": "chat.completion.chunk",
        "created": 1234567890,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": num_chunks as u64,
            "total_tokens": 10 + num_chunks as u64
        }
    })).unwrap());
    sse_body.push_str("\n\n");

    // [DONE] 标记
    sse_body.push_str("data: [DONE]\n\n");

    sse_body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_openai_mock_non_streaming() {
        let mock_server = setup_openai_mock(0, 0.0).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/chat/completions", mock_server.uri()))
            .json(&serde_json::json!({
                "model": "gpt-4",
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["object"], "chat.completion");
        assert!(body["choices"][0]["message"]["content"].is_string());
    }

    #[tokio::test]
    async fn test_openai_mock_streaming() {
        let mock_server = setup_openai_streaming_mock(0, 5, 0).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/chat/completions", mock_server.uri()))
            .json(&serde_json::json!({
                "model": "gpt-4",
                "messages": [{"role": "user", "content": "Hello"}],
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
        assert!(body.contains("data: "));
        assert!(body.contains("[DONE]"));
    }

    #[tokio::test]
    async fn test_openai_mock_error_rate() {
        let mock_server = setup_openai_mock(0, 1.0).await;  // 100% error rate

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/chat/completions", mock_server.uri()))
            .json(&serde_json::json!({
                "model": "gpt-4",
                "messages": [{"role": "user", "content": "Hello"}]
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 503);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body["error"]["message"].is_string());
    }
}
