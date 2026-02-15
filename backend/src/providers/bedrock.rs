use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use crate::provider_trait::{LlmProvider, ProviderProtocol, UpstreamRequest};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::BTreeMap;

/// AWS Bedrock provider with SigV4 request signing.
///
/// URL: `https://bedrock-runtime.{region}.amazonaws.com/model/{model_id}/invoke`
/// Auth: AWS SigV4 signing (service = "bedrock")
/// Protocol: Anthropic (Bedrock Claude uses Anthropic Messages format)
pub struct BedrockProvider;

#[async_trait]
impl LlmProvider for BedrockProvider {
    fn provider_type(&self) -> &str {
        "bedrock"
    }

    fn native_protocol(&self) -> ProviderProtocol {
        ProviderProtocol::Anthropic
    }

    async fn send_request(
        &self,
        client: &Client,
        config: &dyn ProviderConfig,
        request: UpstreamRequest,
    ) -> Result<reqwest::Response, AppError> {
        let bedrock_config = config
            .as_any()
            .downcast_ref::<crate::config::BedrockInstanceConfig>()
            .ok_or_else(|| {
                AppError::ConfigError("Expected BedrockInstanceConfig".to_string())
            })?;

        // Map model name to Bedrock model ID
        let model_id = bedrock_config
            .model_id_mapping
            .get(&request.model)
            .cloned()
            .unwrap_or_else(|| request.model.clone());

        let endpoint = if request.stream {
            "invoke-with-response-stream"
        } else {
            "invoke"
        };

        let host = format!(
            "bedrock-runtime.{}.amazonaws.com",
            bedrock_config.region
        );
        let url_str = format!(
            "https://{}/model/{}/{}",
            host,
            url_encode_path(&model_id),
            endpoint
        );

        // Prepare request body (Bedrock expects Anthropic format without model field)
        let mut body = request.body;
        if let Some(obj) = body.as_object_mut() {
            obj.remove("model");
            // Bedrock uses its own version string
            if !obj.contains_key("anthropic_version") {
                obj.insert(
                    "anthropic_version".to_string(),
                    serde_json::Value::String("bedrock-2023-05-31".to_string()),
                );
            }
        }

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| AppError::ConfigError(format!("Failed to serialize body: {}", e)))?;

        // Sign the request with SigV4
        let url = url::Url::parse(&url_str)
            .map_err(|e| AppError::ConfigError(format!("Invalid URL: {}", e)))?;

        let signed_headers = sigv4_sign(
            "POST",
            &url,
            &[("content-type", "application/json")],
            &body_bytes,
            &bedrock_config.access_key_id,
            &bedrock_config.secret_access_key,
            bedrock_config.session_token.as_deref(),
            &bedrock_config.region,
            "bedrock-runtime",
        );

        // Build reqwest request with signed headers
        let mut req = client
            .post(url_str)
            .timeout(std::time::Duration::from_secs(config.timeout_seconds()));

        for (key, value) in &signed_headers {
            req = req.header(key.as_str(), value.as_str());
        }

        req = req
            .header("Content-Type", "application/json")
            .body(body_bytes);

        let response = req.send().await?;
        Ok(response)
    }

    fn health_check_url(&self, config: &dyn ProviderConfig) -> String {
        let bedrock_config = config
            .as_any()
            .downcast_ref::<crate::config::BedrockInstanceConfig>();
        if let Some(cfg) = bedrock_config {
            format!(
                "https://bedrock.{}.amazonaws.com/foundation-models",
                cfg.region
            )
        } else {
            "https://bedrock.us-east-1.amazonaws.com/foundation-models".to_string()
        }
    }
}

// ============================================================
// AWS SigV4 Signing Implementation
// ============================================================

/// URL-encode a path segment for Bedrock model IDs (e.g. colons in model IDs).
fn url_encode_path(s: &str) -> String {
    // Encode everything except unreserved characters (RFC 3986)
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => {
                // Percent-encode each UTF-8 byte for correct multi-byte handling
                let mut buf = [0u8; 4];
                let bytes = c.encode_utf8(&mut buf).as_bytes();
                bytes.iter().map(|b| format!("%{:02X}", b)).collect::<String>()
            }
        })
        .collect()
}

/// Compute HMAC-SHA256.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Hex-encode bytes.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// SHA-256 hash and hex-encode.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex_encode(&Sha256::digest(data))
}

/// Sign an HTTP request with AWS SigV4.
///
/// Returns a list of headers to add to the request.
#[allow(clippy::too_many_arguments)]
fn sigv4_sign(
    method: &str,
    url: &url::Url,
    extra_headers: &[(&str, &str)],
    body: &[u8],
    access_key_id: &str,
    secret_access_key: &str,
    session_token: Option<&str>,
    region: &str,
    service: &str,
) -> Vec<(String, String)> {
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();

    let host = url.host_str().unwrap_or("");
    let payload_hash = sha256_hex(body);

    // Build canonical headers (sorted by lowercase key)
    let mut headers_map: BTreeMap<&str, String> = BTreeMap::new();
    headers_map.insert("host", host.to_string());
    headers_map.insert("x-amz-date", amz_date.clone());
    headers_map.insert("x-amz-content-sha256", payload_hash.clone());
    if let Some(token) = session_token {
        headers_map.insert("x-amz-security-token", token.to_string());
    }
    for (k, v) in extra_headers {
        headers_map.insert(k, v.to_string());
    }

    let canonical_headers: String = headers_map
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect();
    let signed_headers: String = headers_map
        .keys()
        .copied()
        .collect::<Vec<_>>()
        .join(";");

    // Canonical URI (already URL-encoded in the URL)
    let canonical_uri = url.path();
    let canonical_querystring = url.query().unwrap_or("");

    // Step 1: Canonical request
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, canonical_uri, canonical_querystring,
        canonical_headers, signed_headers, payload_hash
    );

    // Step 2: String to sign
    let algorithm = "AWS4-HMAC-SHA256";
    let credential_scope = format!("{}/{}/{}/aws4_request", date_stamp, region, service);
    let string_to_sign = format!(
        "{}\n{}\n{}\n{}",
        algorithm, amz_date, credential_scope,
        sha256_hex(canonical_request.as_bytes())
    );

    // Step 3: Signing key
    let k_date = hmac_sha256(
        format!("AWS4{}", secret_access_key).as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");

    // Step 4: Signature
    let signature = hex_encode(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    // Step 5: Authorization header
    let authorization = format!(
        "{} Credential={}/{}, SignedHeaders={}, Signature={}",
        algorithm, access_key_id, credential_scope, signed_headers, signature
    );

    // Collect result headers
    let mut result = vec![
        ("Authorization".to_string(), authorization),
        ("x-amz-date".to_string(), amz_date),
        ("x-amz-content-sha256".to_string(), payload_hash),
    ];
    if let Some(token) = session_token {
        result.push(("x-amz-security-token".to_string(), token.to_string()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encode_path_ascii_special_chars() {
        // Colons in Bedrock model IDs must be percent-encoded
        assert_eq!(url_encode_path("anthropic.claude-3:0"), "anthropic.claude-3%3A0");
        // Spaces
        assert_eq!(url_encode_path("a b"), "a%20b");
        // Unreserved chars pass through
        assert_eq!(url_encode_path("abc-123_v2.0~x"), "abc-123_v2.0~x");
    }

    #[test]
    fn test_url_encode_path_multibyte_utf8() {
        // Multi-byte UTF-8: each byte must be individually percent-encoded
        // '中' is U+4E2D → UTF-8 bytes: E4 B8 AD
        assert_eq!(url_encode_path("中"), "%E4%B8%AD");
        // Emoji '😀' is U+1F600 → UTF-8 bytes: F0 9F 98 80
        assert_eq!(url_encode_path("😀"), "%F0%9F%98%80");
    }
}
