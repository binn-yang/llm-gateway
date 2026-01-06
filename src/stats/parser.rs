//! Prometheus metrics parsing and aggregation
//!
//! This module parses Prometheus text format metrics and aggregates them
//! by different grouping strategies (API key, provider, model, or all).

use anyhow::Result;
use chrono::{DateTime, Utc};
use prometheus_parse::{Sample, Scrape, Value};
use std::collections::HashMap;

use crate::stats::histogram::{calculate_percentiles, HistogramBucket};

/// Grouping strategy for metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    ApiKey,
    Provider,
    Model,
    All, // Show all dimensions separately
}

/// Raw metric snapshot from Prometheus endpoint
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub requests: Vec<RequestMetric>,
    pub tokens: Vec<TokenMetric>,
    pub durations: Vec<DurationMetric>,
    pub errors: Vec<ErrorMetric>,
}

/// Request counter metric
#[derive(Debug, Clone)]
pub struct RequestMetric {
    pub api_key: String,
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub count: u64,
}

/// Token counter metric
#[derive(Debug, Clone)]
pub struct TokenMetric {
    pub api_key: String,
    pub provider: String,
    pub model: String,
    pub token_type: String, // "input" or "output"
    pub count: u64,
}

/// Duration histogram metric
#[derive(Debug, Clone)]
pub struct DurationMetric {
    pub api_key: String,
    pub provider: String,
    pub model: String,
    pub buckets: Vec<HistogramBucket>,
    pub sum: f64,
    pub count: u64,
}

/// Error counter metric
#[derive(Debug, Clone)]
pub struct ErrorMetric {
    pub api_key: String,
    pub provider: String,
    pub model: String,
    pub error_type: String,
    pub count: u64,
}

/// Aggregated metrics by grouping key
#[derive(Debug, Clone)]
pub struct AggregatedMetrics {
    pub group_key: String,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_errors: u64,
    pub error_rate: f64,
    pub latency_p50: Option<f64>,
    pub latency_p90: Option<f64>,
    pub latency_p99: Option<f64>,
    pub avg_latency: Option<f64>,
}

/// Parse Prometheus text format and aggregate by grouping strategy
pub fn parse_and_aggregate(
    prometheus_text: &str,
    group_by: GroupBy,
) -> Result<Vec<AggregatedMetrics>> {
    // Parse Prometheus text
    let lines: Vec<_> = prometheus_text.lines().map(|s| Ok(s.to_owned())).collect();
    let scrape = Scrape::parse(lines.into_iter())?;

    // Extract metrics
    let snapshot = extract_metrics(&scrape)?;

    // Aggregate by group
    aggregate_by_group(&snapshot, group_by)
}

/// Extract metrics from parsed Prometheus scrape
fn extract_metrics(scrape: &Scrape) -> Result<MetricsSnapshot> {
    let mut requests = Vec::new();
    let mut tokens = Vec::new();
    let mut durations = Vec::new();
    let mut errors = Vec::new();

    for sample in &scrape.samples {
        match sample.metric.as_str() {
            "llm_requests_total" => {
                if let Some(count) = extract_counter_value(&sample.value) {
                    requests.push(RequestMetric {
                        api_key: get_label(&sample, "api_key"),
                        provider: get_label(&sample, "provider"),
                        model: get_label(&sample, "model"),
                        endpoint: get_label(&sample, "endpoint"),
                        count,
                    });
                }
            }
            "llm_tokens_total" => {
                if let Some(count) = extract_counter_value(&sample.value) {
                    tokens.push(TokenMetric {
                        api_key: get_label(&sample, "api_key"),
                        provider: get_label(&sample, "provider"),
                        model: get_label(&sample, "model"),
                        token_type: get_label(&sample, "type"),
                        count,
                    });
                }
            }
            "llm_request_duration_seconds" => {
                if let Value::Histogram(buckets_data) = &sample.value {
                    let buckets: Vec<HistogramBucket> = buckets_data
                        .iter()
                        .map(|b| HistogramBucket {
                            upper_bound: b.less_than,
                            cumulative_count: b.count as u64,
                        })
                        .collect();

                    durations.push(DurationMetric {
                        api_key: get_label(&sample, "api_key"),
                        provider: get_label(&sample, "provider"),
                        model: get_label(&sample, "model"),
                        buckets,
                        sum: buckets_data.last().map(|b| b.count).unwrap_or(0.0),
                        count: buckets_data.last().map(|b| b.count as u64).unwrap_or(0),
                    });
                }
            }
            "llm_errors_total" => {
                if let Some(count) = extract_counter_value(&sample.value) {
                    errors.push(ErrorMetric {
                        api_key: get_label(&sample, "api_key"),
                        provider: get_label(&sample, "provider"),
                        model: get_label(&sample, "model"),
                        error_type: get_label(&sample, "error_type"),
                        count,
                    });
                }
            }
            _ => {} // Ignore other metrics
        }
    }

    Ok(MetricsSnapshot {
        timestamp: Utc::now(),
        requests,
        tokens,
        durations,
        errors,
    })
}

/// Extract counter value from Prometheus value
fn extract_counter_value(value: &Value) -> Option<u64> {
    match value {
        Value::Counter(v) | Value::Gauge(v) | Value::Untyped(v) => Some(*v as u64),
        _ => None,
    }
}

/// Get label value from sample, returning empty string if not found
fn get_label(sample: &Sample, label_name: &str) -> String {
    sample
        .labels
        .get(label_name)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// Aggregate metrics by grouping strategy
fn aggregate_by_group(
    snapshot: &MetricsSnapshot,
    group_by: GroupBy,
) -> Result<Vec<AggregatedMetrics>> {
    // Group metrics by key
    let mut groups: HashMap<String, Vec<MetricRefs>> = HashMap::new();

    // Group requests
    for req in &snapshot.requests {
        let key = get_group_key(&req.api_key, &req.provider, &req.model, group_by);
        groups
            .entry(key)
            .or_default()
            .push(MetricRefs::Request(req));
    }

    // Group tokens
    for tok in &snapshot.tokens {
        let key = get_group_key(&tok.api_key, &tok.provider, &tok.model, group_by);
        groups
            .entry(key)
            .or_default()
            .push(MetricRefs::Token(tok));
    }

    // Group durations
    for dur in &snapshot.durations {
        let key = get_group_key(&dur.api_key, &dur.provider, &dur.model, group_by);
        groups
            .entry(key)
            .or_default()
            .push(MetricRefs::Duration(dur));
    }

    // Group errors
    for err in &snapshot.errors {
        let key = get_group_key(&err.api_key, &err.provider, &err.model, group_by);
        groups
            .entry(key)
            .or_default()
            .push(MetricRefs::Error(err));
    }

    // Aggregate each group
    let mut results: Vec<AggregatedMetrics> = groups
        .iter()
        .map(|(key, metrics)| aggregate_group(key.clone(), metrics))
        .collect();

    // Sort by group key for consistent display
    results.sort_by(|a, b| a.group_key.cmp(&b.group_key));

    Ok(results)
}

/// Get grouping key based on strategy
fn get_group_key(api_key: &str, provider: &str, model: &str, group_by: GroupBy) -> String {
    match group_by {
        GroupBy::ApiKey => api_key.to_string(),
        GroupBy::Provider => provider.to_string(),
        GroupBy::Model => model.to_string(),
        GroupBy::All => format!("{}:{}:{}", api_key, provider, model),
    }
}

/// Reference to different metric types for grouping
enum MetricRefs<'a> {
    Request(&'a RequestMetric),
    Token(&'a TokenMetric),
    Duration(&'a DurationMetric),
    Error(&'a ErrorMetric),
}

/// Aggregate a single group of metrics
fn aggregate_group(group_key: String, metrics: &[MetricRefs]) -> AggregatedMetrics {
    let mut total_requests = 0u64;
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    let mut total_errors = 0u64;
    let mut all_buckets: Vec<&HistogramBucket> = Vec::new();
    let mut total_latency_sum = 0.0;
    let mut total_latency_count = 0u64;

    for metric in metrics {
        match metric {
            MetricRefs::Request(req) => {
                total_requests += req.count;
            }
            MetricRefs::Token(tok) => {
                if tok.token_type == "input" {
                    total_input_tokens += tok.count;
                } else if tok.token_type == "output" {
                    total_output_tokens += tok.count;
                }
            }
            MetricRefs::Duration(dur) => {
                all_buckets.extend(&dur.buckets);
                total_latency_sum += dur.sum;
                total_latency_count += dur.count;
            }
            MetricRefs::Error(err) => {
                total_errors += err.count;
            }
        }
    }

    // Calculate error rate
    let error_rate = if total_requests > 0 {
        (total_errors as f64 / total_requests as f64) * 100.0
    } else {
        0.0
    };

    // Calculate average latency
    let avg_latency = if total_latency_count > 0 {
        Some(total_latency_sum / total_latency_count as f64)
    } else {
        None
    };

    // Merge histogram buckets if we have multiple sources
    let merged_buckets = merge_histogram_buckets(&all_buckets);

    // Calculate percentiles
    let percentiles = if !merged_buckets.is_empty() {
        calculate_percentiles(&merged_buckets)
    } else {
        crate::stats::histogram::Percentiles {
            p50: None,
            p90: None,
            p99: None,
        }
    };

    AggregatedMetrics {
        group_key,
        total_requests,
        total_input_tokens,
        total_output_tokens,
        total_errors,
        error_rate,
        latency_p50: percentiles.p50,
        latency_p90: percentiles.p90,
        latency_p99: percentiles.p99,
        avg_latency,
    }
}

/// Merge multiple histogram buckets by summing cumulative counts
fn merge_histogram_buckets(buckets: &[&HistogramBucket]) -> Vec<HistogramBucket> {
    if buckets.is_empty() {
        return Vec::new();
    }

    let mut bucket_map: HashMap<String, u64> = HashMap::new();

    for bucket in buckets {
        let key = if bucket.upper_bound.is_infinite() {
            "+Inf".to_string()
        } else {
            format!("{:.6}", bucket.upper_bound)
        };
        *bucket_map.entry(key).or_insert(0) += bucket.cumulative_count;
    }

    let mut merged: Vec<HistogramBucket> = bucket_map
        .into_iter()
        .map(|(key, count)| {
            let upper_bound = if key == "+Inf" {
                f64::INFINITY
            } else {
                key.parse::<f64>().unwrap_or(0.0)
            };
            HistogramBucket {
                upper_bound,
                cumulative_count: count,
            }
        })
        .collect();

    // Sort by upper bound
    merged.sort_by(|a, b| a.upper_bound.partial_cmp(&b.upper_bound).unwrap());

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_by_provider() {
        let key1 = get_group_key("api1", "openai", "gpt-4", GroupBy::Provider);
        let key2 = get_group_key("api2", "openai", "gpt-3.5", GroupBy::Provider);
        let key3 = get_group_key("api1", "anthropic", "claude", GroupBy::Provider);

        assert_eq!(key1, "openai");
        assert_eq!(key2, "openai");
        assert_eq!(key3, "anthropic");
    }

    #[test]
    fn test_group_by_model() {
        let key1 = get_group_key("api1", "openai", "gpt-4", GroupBy::Model);
        let key2 = get_group_key("api2", "openai", "gpt-4", GroupBy::Model);
        let key3 = get_group_key("api1", "openai", "gpt-3.5", GroupBy::Model);

        assert_eq!(key1, "gpt-4");
        assert_eq!(key2, "gpt-4");
        assert_eq!(key3, "gpt-3.5");
    }

    #[test]
    fn test_group_by_api_key() {
        let key1 = get_group_key("api1", "openai", "gpt-4", GroupBy::ApiKey);
        let key2 = get_group_key("api1", "anthropic", "claude", GroupBy::ApiKey);
        let key3 = get_group_key("api2", "openai", "gpt-4", GroupBy::ApiKey);

        assert_eq!(key1, "api1");
        assert_eq!(key2, "api1");
        assert_eq!(key3, "api2");
    }

    #[test]
    fn test_merge_histogram_buckets() {
        let bucket1 = HistogramBucket {
            upper_bound: 1.0,
            cumulative_count: 10,
        };
        let bucket2 = HistogramBucket {
            upper_bound: 1.0,
            cumulative_count: 20,
        };
        let bucket3 = HistogramBucket {
            upper_bound: 5.0,
            cumulative_count: 15,
        };

        let buckets = vec![&bucket1, &bucket2, &bucket3];
        let merged = merge_histogram_buckets(&buckets);

        // Should have 2 unique buckets (1.0 and 5.0)
        assert_eq!(merged.len(), 2);

        // 1.0 bucket should have count 30 (10 + 20)
        let bucket_1 = merged.iter().find(|b| b.upper_bound == 1.0).unwrap();
        assert_eq!(bucket_1.cumulative_count, 30);

        // 5.0 bucket should have count 15
        let bucket_5 = merged.iter().find(|b| b.upper_bound == 5.0).unwrap();
        assert_eq!(bucket_5.cumulative_count, 15);
    }
}
