use crate::error::AppError;
use crate::pricing::models::ModelPrice;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::{debug, info};

/// Pricing data file structure from claude-relay-service
/// This is a flat HashMap of model names to their pricing data
pub type PricingDataFile = std::collections::HashMap<String, ModelPriceData>;

/// Individual model pricing data
#[derive(Debug, Deserialize)]
pub struct ModelPriceData {
    #[serde(default)]
    pub input_cost_per_token: Option<f64>,
    #[serde(default)]
    pub output_cost_per_token: Option<f64>,
    #[serde(default)]
    pub cache_creation_input_token_cost: Option<f64>,
    #[serde(default)]
    pub cache_read_input_token_cost: Option<f64>,
    #[serde(default)]
    pub litellm_provider: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
}

/// Download pricing data from remote URL
pub async fn download_pricing_from_url(url: &str) -> Result<String, AppError> {
    info!("Downloading pricing data from: {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::ConfigError(format!("Failed to build HTTP client: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(AppError::HttpRequest)?;

    if !response.status().is_success() {
        return Err(AppError::ConfigError(format!(
            "Failed to download pricing: HTTP {}",
            response.status()
        )));
    }

    let content = response
        .text()
        .await
        .map_err(|e| AppError::ConfigError(format!("Failed to read response: {}", e)))?;

    debug!("Downloaded {} bytes of pricing data", content.len());
    Ok(content)
}

/// Parse pricing JSON and convert to ModelPrice list
pub async fn parse_pricing_json(json: &str) -> Result<Vec<ModelPrice>, AppError> {
    let data: PricingDataFile = serde_json::from_str(json)
        .map_err(|e| AppError::ConfigError(format!("Failed to parse pricing JSON: {}", e)))?;

    let effective_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let currency = "USD".to_string();

    let mut prices = Vec::new();

    for (model_name, price_data) in data {
        // Skip sample_spec and non-chat models
        if model_name == "sample_spec" {
            continue;
        }

        // Only include chat models
        if let Some(mode) = &price_data.mode {
            if mode != "chat" {
                continue;
            }
        } else {
            continue;
        }

        // Skip models without pricing data
        let input_cost = match price_data.input_cost_per_token {
            Some(cost) if cost > 0.0 => cost,
            _ => continue,
        };

        let output_cost = match price_data.output_cost_per_token {
            Some(cost) if cost > 0.0 => cost,
            _ => continue,
        };

        // Infer provider from litellm_provider or model name
        let provider = if let Some(ref provider) = price_data.litellm_provider {
            provider.clone()
        } else {
            infer_provider(&model_name)
        };

        // Convert from per-token to per-1M-tokens pricing
        prices.push(ModelPrice {
            model_name,
            provider,
            input_price: input_cost * 1_000_000.0,
            output_price: output_cost * 1_000_000.0,
            cache_write_price: price_data.cache_creation_input_token_cost.map(|c| c * 1_000_000.0),
            cache_read_price: price_data.cache_read_input_token_cost.map(|c| c * 1_000_000.0),
            currency: currency.clone(),
            effective_date: effective_date.clone(),
            notes: None,
        });
    }

    info!("Parsed {} model prices", prices.len());
    Ok(prices)
}

/// Infer provider from model name
fn infer_provider(model_name: &str) -> String {
    if model_name.starts_with("claude-") {
        "anthropic".to_string()
    } else if model_name.starts_with("gpt-") || model_name.starts_with("o1-") {
        "openai".to_string()
    } else if model_name.starts_with("gemini-") {
        "gemini".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Calculate SHA256 hash of content
pub fn calculate_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Save backup of pricing file
pub async fn save_backup(content: &str, backup_dir: &str) -> Result<String, AppError> {
    // Create backup directory if it doesn't exist
    tokio::fs::create_dir_all(backup_dir)
        .await
        .map_err(|e| AppError::ConfigError(format!("Failed to create backup dir: {}", e)))?;

    // Generate filename with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("model_prices_{}.json", timestamp);
    let filepath = Path::new(backup_dir).join(&filename);

    // Write file
    tokio::fs::write(&filepath, content)
        .await
        .map_err(|e| AppError::ConfigError(format!("Failed to write backup: {}", e)))?;

    info!("Saved pricing backup to: {}", filepath.display());
    Ok(filepath.to_string_lossy().to_string())
}
