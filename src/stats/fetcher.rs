//! Metrics fetcher for retrieving Prometheus metrics from HTTP endpoint
//!
//! This module provides a simple HTTP client wrapper for fetching metrics
//! from the gateway's /metrics endpoint.

use anyhow::Result;
use reqwest::Client;

/// HTTP client wrapper for fetching metrics
pub struct MetricsFetcher {
    client: Client,
    url: String,
}

impl MetricsFetcher {
    /// Create a new metrics fetcher
    ///
    /// # Arguments
    /// * `url` - Full URL to the metrics endpoint (e.g., "http://localhost:8080/metrics")
    pub fn new(url: String) -> Self {
        Self {
            client: Client::new(),
            url,
        }
    }

    /// Fetch metrics from the endpoint
    ///
    /// # Returns
    /// Raw Prometheus text format as a String
    ///
    /// # Errors
    /// Returns an error if:
    /// - Network request fails
    /// - Response status is not successful (2xx)
    /// - Response body cannot be read as text
    pub async fn fetch(&self) -> Result<String> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch metrics: {}", e))?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to fetch metrics: HTTP {}",
                response.status()
            );
        }

        let text = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read metrics response: {}", e))?;

        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetcher_creation() {
        let url = "http://localhost:8080/metrics".to_string();
        let fetcher = MetricsFetcher::new(url.clone());
        assert_eq!(fetcher.url, url);
    }
}
