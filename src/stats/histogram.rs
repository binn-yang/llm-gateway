//! Histogram percentile calculations for Prometheus histograms
//!
//! This module implements percentile calculations (P50, P90, P99) from
//! Prometheus histogram buckets using linear interpolation.

/// Histogram bucket from Prometheus
#[derive(Debug, Clone)]
pub struct HistogramBucket {
    /// Upper bound (le - less than or equal)
    pub upper_bound: f64,
    /// Cumulative count up to this bucket
    pub cumulative_count: u64,
}

/// Percentile calculation result
#[derive(Debug, Clone)]
pub struct Percentiles {
    pub p50: Option<f64>,
    pub p90: Option<f64>,
    pub p99: Option<f64>,
}

/// Calculate a specific percentile from histogram buckets
///
/// # Arguments
/// * `buckets` - Sorted histogram buckets (cumulative)
/// * `percentile` - Percentile to calculate (0.0 to 1.0, e.g., 0.5 for P50)
///
/// # Returns
/// The calculated percentile value, or None if buckets are empty or invalid
///
/// # Algorithm
/// 1. Find total count from +Inf bucket
/// 2. Calculate target rank = percentile * total
/// 3. Find bucket containing target rank
/// 4. Linear interpolation within bucket to estimate value
pub fn calculate_percentile(buckets: &[HistogramBucket], percentile: f64) -> Option<f64> {
    if buckets.is_empty() {
        return None;
    }

    // Get total count from +Inf bucket (last bucket)
    let total = buckets.last()?.cumulative_count;
    if total == 0 {
        return None;
    }

    // Calculate target rank
    let target_rank = (percentile * total as f64).ceil() as u64;

    // Find the bucket containing the target rank
    let mut prev_count = 0;
    let mut prev_bound = 0.0;

    for bucket in buckets {
        if bucket.cumulative_count >= target_rank {
            // Found the bucket containing our target rank

            // Edge case: bucket has no new observations
            if bucket.cumulative_count == prev_count {
                return Some(bucket.upper_bound);
            }

            // Linear interpolation within the bucket
            let bucket_count = bucket.cumulative_count - prev_count;
            let rank_in_bucket = target_rank - prev_count;
            let fraction = rank_in_bucket as f64 / bucket_count as f64;

            // Interpolate between prev_bound and current upper_bound
            let value = prev_bound + fraction * (bucket.upper_bound - prev_bound);
            return Some(value);
        }

        prev_count = bucket.cumulative_count;
        prev_bound = bucket.upper_bound;
    }

    None
}

/// Calculate P50, P90, and P99 percentiles from histogram buckets
///
/// This is a convenience function that calculates all three commonly used
/// percentiles at once.
pub fn calculate_percentiles(buckets: &[HistogramBucket]) -> Percentiles {
    Percentiles {
        p50: calculate_percentile(buckets, 0.50),
        p90: calculate_percentile(buckets, 0.90),
        p99: calculate_percentile(buckets, 0.99),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_percentile_empty_buckets() {
        let buckets: Vec<HistogramBucket> = vec![];
        assert_eq!(calculate_percentile(&buckets, 0.5), None);
    }

    #[test]
    fn test_calculate_percentile_zero_observations() {
        let buckets = vec![
            HistogramBucket {
                upper_bound: 1.0,
                cumulative_count: 0,
            },
            HistogramBucket {
                upper_bound: f64::INFINITY,
                cumulative_count: 0,
            },
        ];
        assert_eq!(calculate_percentile(&buckets, 0.5), None);
    }

    #[test]
    fn test_calculate_percentile_simple_case() {
        // 100 observations evenly distributed:
        // 0-0.1: 5 obs
        // 0.1-0.5: 15 obs (total: 20)
        // 0.5-1.0: 25 obs (total: 45)
        // 1.0-5.0: 55 obs (total: 100)
        let buckets = vec![
            HistogramBucket {
                upper_bound: 0.1,
                cumulative_count: 5,
            },
            HistogramBucket {
                upper_bound: 0.5,
                cumulative_count: 20,
            },
            HistogramBucket {
                upper_bound: 1.0,
                cumulative_count: 45,
            },
            HistogramBucket {
                upper_bound: 5.0,
                cumulative_count: 100,
            },
            HistogramBucket {
                upper_bound: f64::INFINITY,
                cumulative_count: 100,
            },
        ];

        // P50 (median): 50th observation
        // 50 > 45, so it's in the 1.0-5.0 bucket
        // rank_in_bucket = 50 - 45 = 5
        // bucket_count = 100 - 45 = 55
        // fraction = 5/55 = 0.0909
        // value = 1.0 + 0.0909 * (5.0 - 1.0) = 1.0 + 0.364 = 1.364
        let p50 = calculate_percentile(&buckets, 0.50).unwrap();
        assert!((p50 - 1.364).abs() < 0.01);

        // P90: 90th observation
        // 90 > 45, so it's in the 1.0-5.0 bucket
        // rank_in_bucket = 90 - 45 = 45
        // bucket_count = 55
        // fraction = 45/55 = 0.818
        // value = 1.0 + 0.818 * 4.0 = 4.27
        let p90 = calculate_percentile(&buckets, 0.90).unwrap();
        assert!((p90 - 4.27).abs() < 0.01);

        // P99: 99th observation
        // 99 > 45, so it's in the 1.0-5.0 bucket
        // rank_in_bucket = 99 - 45 = 54
        // bucket_count = 55
        // fraction = 54/55 = 0.982
        // value = 1.0 + 0.982 * 4.0 = 4.93
        let p99 = calculate_percentile(&buckets, 0.99).unwrap();
        assert!((p99 - 4.93).abs() < 0.01);
    }

    #[test]
    fn test_calculate_percentiles_batch() {
        let buckets = vec![
            HistogramBucket {
                upper_bound: 0.5,
                cumulative_count: 25,
            },
            HistogramBucket {
                upper_bound: 1.0,
                cumulative_count: 50,
            },
            HistogramBucket {
                upper_bound: 5.0,
                cumulative_count: 95,
            },
            HistogramBucket {
                upper_bound: f64::INFINITY,
                cumulative_count: 100,
            },
        ];

        let percentiles = calculate_percentiles(&buckets);
        assert!(percentiles.p50.is_some());
        assert!(percentiles.p90.is_some());
        assert!(percentiles.p99.is_some());
    }

    #[test]
    fn test_percentile_in_first_bucket() {
        let buckets = vec![
            HistogramBucket {
                upper_bound: 1.0,
                cumulative_count: 90,
            },
            HistogramBucket {
                upper_bound: 5.0,
                cumulative_count: 100,
            },
            HistogramBucket {
                upper_bound: f64::INFINITY,
                cumulative_count: 100,
            },
        ];

        // P50 should be in first bucket (0.0-1.0)
        // rank = 50, bucket has 90 observations
        // fraction = 50/90 = 0.556
        // value = 0.0 + 0.556 * 1.0 = 0.556
        let p50 = calculate_percentile(&buckets, 0.50).unwrap();
        assert!((p50 - 0.556).abs() < 0.01);
    }
}
