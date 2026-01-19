use crate::error::AppError;
use base64::{engine::general_purpose, Engine as _};

/// Parse a data URL (e.g., "data:image/jpeg;base64,<data>")
/// Returns (mime_type, base64_data)
pub fn parse_data_url(data_url: &str) -> Result<(String, String), AppError> {
    if !data_url.starts_with("data:") {
        return Err(AppError::ConversionError(
            "Invalid data URL: must start with 'data:'".to_string(),
        ));
    }

    let url_body = &data_url[5..]; // Remove "data:" prefix

    let parts: Vec<&str> = url_body.splitn(2, ',').collect();
    if parts.len() != 2 {
        return Err(AppError::ConversionError(
            "Invalid data URL format: missing comma separator".to_string(),
        ));
    }

    let header = parts[0];
    let data = parts[1];

    // Parse header: "image/jpeg;base64" or just "image/jpeg"
    let header_parts: Vec<&str> = header.split(';').collect();
    let mime_type = header_parts[0].to_string();

    // Check if base64 encoded
    let is_base64 = header_parts.iter().any(|&part| part == "base64");

    if !is_base64 {
        return Err(AppError::ConversionError(
            "Only base64-encoded data URLs are supported".to_string(),
        ));
    }

    // Validate MIME type (case-insensitive)
    if !mime_type.to_lowercase().starts_with("image/") {
        return Err(AppError::ConversionError(format!(
            "Invalid MIME type for image: {}",
            mime_type
        )));
    }

    // Validate base64 encoding and decode to check size
    let decoded = general_purpose::STANDARD
        .decode(data)
        .map_err(|e| AppError::ConversionError(format!("Invalid base64 data: {}", e)))?;

    // Validate image size
    validate_image_size(&decoded, &mime_type)?;

    Ok((mime_type, data.to_string()))
}

/// Fetch image from HTTP(S) URL and convert to base64
/// Returns (mime_type, base64_data)
pub async fn fetch_image_as_base64(url: &str) -> Result<(String, String), AppError> {
    // Validate URL scheme to prevent SSRF
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::ConversionError(
            "Only HTTP(S) URLs are supported for image fetching".to_string(),
        ));
    }

    tracing::debug!("Fetching image from URL: {}", url);

    // Use a client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(AppError::HttpRequest)?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(AppError::HttpRequest)?;

    if !response.status().is_success() {
        return Err(AppError::ConversionError(format!(
            "Image fetch failed with status: {}",
            response.status()
        )));
    }

    // Get MIME type from Content-Type header
    let mime_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg") // Default to JPEG
        .to_string();

    // Validate that it's an image (case-insensitive)
    if !mime_type.to_lowercase().starts_with("image/") {
        return Err(AppError::ConversionError(format!(
            "URL does not point to an image (content-type: {})",
            mime_type
        )));
    }

    // Check Content-Length header before downloading to avoid downloading huge files
    const MAX_SIZE_BYTES: usize = 20 * 1024 * 1024; // 20MB
    if let Some(content_length) = response.headers().get("content-length") {
        if let Ok(length_str) = content_length.to_str() {
            if let Ok(length) = length_str.parse::<usize>() {
                if length > MAX_SIZE_BYTES {
                    return Err(AppError::ConversionError(format!(
                        "Image too large: {} bytes (max: {} bytes)",
                        length, MAX_SIZE_BYTES
                    )));
                }
            }
        }
    }

    // Get the image bytes
    let bytes = response
        .bytes()
        .await
        .map_err(AppError::HttpRequest)?;

    // Validate size again (double-check actual size)
    validate_image_size(&bytes, &mime_type)?;

    // Encode to base64
    let base64_data = general_purpose::STANDARD.encode(&bytes);

    tracing::debug!(
        "Successfully fetched and encoded image: {} bytes, MIME: {}",
        bytes.len(),
        mime_type
    );

    Ok((mime_type, base64_data))
}

/// Validate image format and size per provider limits
pub fn validate_image_size(data: &[u8], mime_type: &str) -> Result<(), AppError> {
    const MAX_SIZE_BYTES: usize = 20 * 1024 * 1024; // 20MB (Gemini limit)

    if data.len() > MAX_SIZE_BYTES {
        return Err(AppError::ConversionError(format!(
            "Image too large: {} bytes (max: {} bytes)",
            data.len(),
            MAX_SIZE_BYTES
        )));
    }

    // Validate supported formats (case-insensitive)
    let supported_formats = ["image/jpeg", "image/png", "image/gif", "image/webp"];
    let mime_base = mime_type
        .split(';')
        .next()
        .unwrap_or(mime_type)
        .to_lowercase();

    if !supported_formats.contains(&mime_base.as_str()) {
        return Err(AppError::ConversionError(format!(
            "Unsupported image format: {} (supported: jpeg, png, gif, webp)",
            mime_base
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_data_url_valid() {
        let data_url = "data:image/jpeg;base64,/9j/4AAQSkZJRg==";
        let (mime_type, data) = parse_data_url(data_url).unwrap();
        assert_eq!(mime_type, "image/jpeg");
        assert_eq!(data, "/9j/4AAQSkZJRg==");
    }

    #[test]
    fn test_parse_data_url_png() {
        let data_url = "data:image/png;base64,iVBORw0KGgo=";
        let (mime_type, data) = parse_data_url(data_url).unwrap();
        assert_eq!(mime_type, "image/png");
        assert_eq!(data, "iVBORw0KGgo=");
    }

    #[test]
    fn test_parse_data_url_invalid_prefix() {
        let data_url = "http://example.com/image.jpg";
        let result = parse_data_url(data_url);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_data_url_missing_comma() {
        let data_url = "data:image/jpeg;base64";
        let result = parse_data_url(data_url);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_data_url_not_base64() {
        let data_url = "data:image/jpeg,notbase64data";
        let result = parse_data_url(data_url);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_image_size_valid() {
        let data = vec![0u8; 1024 * 1024]; // 1MB
        let result = validate_image_size(&data, "image/jpeg");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_image_size_too_large() {
        let data = vec![0u8; 25 * 1024 * 1024]; // 25MB
        let result = validate_image_size(&data, "image/jpeg");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_image_unsupported_format() {
        let data = vec![0u8; 1024];
        let result = validate_image_size(&data, "image/bmp");
        assert!(result.is_err());
    }
}
