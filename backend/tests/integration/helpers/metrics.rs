use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 请求结果
#[derive(Debug, Clone)]
pub enum RequestResult {
    Success,
    ClientError(u16),  // 4xx
    ServerError(u16),  // 5xx
    NetworkError,
    Timeout,
}

impl RequestResult {
    pub fn is_success(&self) -> bool {
        matches!(self, RequestResult::Success)
    }

    pub fn is_error(&self) -> bool {
        !self.is_success()
    }
}

/// 延迟记录
#[derive(Debug, Clone)]
pub struct LatencyRecord {
    pub duration: Duration,
    pub result: RequestResult,
    pub timestamp: Instant,
}

/// 压力测试指标收集器
#[derive(Debug, Clone)]
pub struct StressTestMetrics {
    records: Arc<Mutex<Vec<LatencyRecord>>>,
    start_time: Instant,
}

impl StressTestMetrics {
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    /// 记录一个请求
    pub fn record_request(&self, duration: Duration, result: RequestResult) {
        let record = LatencyRecord {
            duration,
            result,
            timestamp: Instant::now(),
        };
        self.records.lock().unwrap().push(record);
    }

    /// 生成指标报告
    pub fn report(&self) -> MetricsReport {
        let records = self.records.lock().unwrap();
        let total_duration = self.start_time.elapsed();

        // 过滤成功请求
        let mut success_durations: Vec<Duration> = records
            .iter()
            .filter(|r| r.result.is_success())
            .map(|r| r.duration)
            .collect();

        success_durations.sort();

        // 计算百分位数
        let (p50, p95, p99) = if success_durations.is_empty() {
            (Duration::ZERO, Duration::ZERO, Duration::ZERO)
        } else {
            let p50 = percentile(&success_durations, 0.50);
            let p95 = percentile(&success_durations, 0.95);
            let p99 = percentile(&success_durations, 0.99);
            (p50, p95, p99)
        };

        // 统计错误
        let total_requests = records.len();
        let successful_requests = records.iter().filter(|r| r.result.is_success()).count();
        let failed_requests = total_requests - successful_requests;

        let client_errors = records.iter().filter(|r| matches!(r.result, RequestResult::ClientError(_))).count();
        let server_errors = records.iter().filter(|r| matches!(r.result, RequestResult::ServerError(_))).count();
        let network_errors = records.iter().filter(|r| matches!(r.result, RequestResult::NetworkError)).count();
        let timeouts = records.iter().filter(|r| matches!(r.result, RequestResult::Timeout)).count();

        // 计算 QPS
        let qps = if total_duration.as_secs_f64() > 0.0 {
            total_requests as f64 / total_duration.as_secs_f64()
        } else {
            0.0
        };

        MetricsReport {
            total_requests,
            successful_requests,
            failed_requests,
            client_errors,
            server_errors,
            network_errors,
            timeouts,
            success_rate: if total_requests > 0 {
                (successful_requests as f64 / total_requests as f64) * 100.0
            } else {
                0.0
            },
            p50_latency: p50,
            p95_latency: p95,
            p99_latency: p99,
            min_latency: success_durations.first().copied().unwrap_or(Duration::ZERO),
            max_latency: success_durations.last().copied().unwrap_or(Duration::ZERO),
            avg_latency: if !success_durations.is_empty() {
                Duration::from_secs_f64(
                    success_durations.iter().map(|d| d.as_secs_f64()).sum::<f64>()
                        / success_durations.len() as f64,
                )
            } else {
                Duration::ZERO
            },
            qps,
            total_duration,
        }
    }

    /// 重置指标
    pub fn reset(&self) {
        self.records.lock().unwrap().clear();
    }

    /// 获取当前请求数
    pub fn request_count(&self) -> usize {
        self.records.lock().unwrap().len()
    }
}

impl Default for StressTestMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 指标报告
#[derive(Debug, Clone)]
pub struct MetricsReport {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub client_errors: usize,
    pub server_errors: usize,
    pub network_errors: usize,
    pub timeouts: usize,
    pub success_rate: f64,  // 百分比
    pub p50_latency: Duration,
    pub p95_latency: Duration,
    pub p99_latency: Duration,
    pub min_latency: Duration,
    pub max_latency: Duration,
    pub avg_latency: Duration,
    pub qps: f64,
    pub total_duration: Duration,
}

impl MetricsReport {
    /// 打印报告
    pub fn print(&self) {
        println!("\n========== Stress Test Metrics Report ==========");
        println!("Total Requests:      {}", self.total_requests);
        println!("Successful:          {} ({:.2}%)", self.successful_requests, self.success_rate);
        println!("Failed:              {}", self.failed_requests);
        if self.client_errors > 0 {
            println!("  - Client Errors:   {}", self.client_errors);
        }
        if self.server_errors > 0 {
            println!("  - Server Errors:   {}", self.server_errors);
        }
        if self.network_errors > 0 {
            println!("  - Network Errors:  {}", self.network_errors);
        }
        if self.timeouts > 0 {
            println!("  - Timeouts:        {}", self.timeouts);
        }
        println!("\nLatency (successful requests only):");
        println!("  Min:               {:?}", self.min_latency);
        println!("  Avg:               {:?}", self.avg_latency);
        println!("  P50:               {:?}", self.p50_latency);
        println!("  P95:               {:?}", self.p95_latency);
        println!("  P99:               {:?}", self.p99_latency);
        println!("  Max:               {:?}", self.max_latency);
        println!("\nThroughput:");
        println!("  QPS:               {:.2}", self.qps);
        println!("  Total Duration:    {:?}", self.total_duration);
        println!("=================================================\n");
    }

    /// 验证性能指标是否满足目标
    pub fn assert_performance(
        &self,
        min_success_rate: f64,
        max_p99_latency: Duration,
        min_qps: f64,
    ) {
        assert!(
            self.success_rate >= min_success_rate,
            "Success rate {:.2}% is below target {:.2}%",
            self.success_rate,
            min_success_rate
        );

        assert!(
            self.p99_latency <= max_p99_latency,
            "P99 latency {:?} exceeds target {:?}",
            self.p99_latency,
            max_p99_latency
        );

        if min_qps > 0.0 {
            assert!(
                self.qps >= min_qps,
                "QPS {:.2} is below target {:.2}",
                self.qps,
                min_qps
            );
        }
    }
}

/// 计算百分位数
fn percentile(sorted_durations: &[Duration], percentile: f64) -> Duration {
    if sorted_durations.is_empty() {
        return Duration::ZERO;
    }

    let index = ((sorted_durations.len() as f64 - 1.0) * percentile) as usize;
    sorted_durations[index]
}

/// 实例分布统计器
///
/// 用于验证负载均衡分布
#[derive(Debug, Clone)]
pub struct InstanceDistribution {
    counts: Arc<Mutex<std::collections::HashMap<String, usize>>>,
}

impl InstanceDistribution {
    pub fn new() -> Self {
        Self {
            counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// 记录实例被选中
    pub fn record(&self, instance_name: &str) {
        let mut counts = self.counts.lock().unwrap();
        *counts.entry(instance_name.to_string()).or_insert(0) += 1;
    }

    /// 获取分布报告
    pub fn report(&self) -> std::collections::HashMap<String, usize> {
        self.counts.lock().unwrap().clone()
    }

    /// 打印分布
    pub fn print(&self) {
        let counts = self.counts.lock().unwrap();
        let total: usize = counts.values().sum();

        println!("\n========== Instance Distribution ==========");
        for (instance, count) in counts.iter() {
            let percentage = (*count as f64 / total as f64) * 100.0;
            println!("{}: {} ({:.2}%)", instance, count, percentage);
        }
        println!("Total: {}", total);
        println!("===========================================\n");
    }

    /// 验证分布是否符合预期(使用卡方检验)
    pub fn assert_distribution(&self, expected: &[(String, f64)], tolerance: f64) {
        let counts = self.counts.lock().unwrap();
        let total: usize = counts.values().sum();

        for (instance, expected_ratio) in expected {
            let actual_count = counts.get(instance).copied().unwrap_or(0);
            let actual_ratio = actual_count as f64 / total as f64;
            let diff = (actual_ratio - expected_ratio).abs();

            assert!(
                diff <= tolerance,
                "Instance {} distribution {:.2}% is outside tolerance {:.2}% (expected {:.2}%)",
                instance,
                actual_ratio * 100.0,
                tolerance * 100.0,
                expected_ratio * 100.0
            );
        }
    }
}

impl Default for InstanceDistribution {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_basic() {
        let metrics = StressTestMetrics::new();

        metrics.record_request(Duration::from_millis(10), RequestResult::Success);
        metrics.record_request(Duration::from_millis(20), RequestResult::Success);
        metrics.record_request(Duration::from_millis(30), RequestResult::ServerError(503));

        let report = metrics.report();
        assert_eq!(report.total_requests, 3);
        assert_eq!(report.successful_requests, 2);
        assert_eq!(report.failed_requests, 1);
        assert_eq!(report.server_errors, 1);
    }

    #[test]
    fn test_percentile_calculation() {
        let metrics = StressTestMetrics::new();

        for i in 1..=100 {
            metrics.record_request(
                Duration::from_millis(i),
                RequestResult::Success,
            );
        }

        let report = metrics.report();
        assert!(report.p50_latency >= Duration::from_millis(49));
        assert!(report.p50_latency <= Duration::from_millis(51));
        assert!(report.p95_latency >= Duration::from_millis(94));
        assert!(report.p99_latency >= Duration::from_millis(98));
    }

    #[test]
    fn test_instance_distribution() {
        let dist = InstanceDistribution::new();

        dist.record("instance-0");
        dist.record("instance-1");
        dist.record("instance-1");

        let report = dist.report();
        assert_eq!(report.get("instance-0"), Some(&1));
        assert_eq!(report.get("instance-1"), Some(&2));
    }
}
