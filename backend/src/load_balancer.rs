use crate::provider_config::ProviderConfig;
use dashmap::DashMap;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ============================================================
// Helper Functions
// ============================================================

/// Select an instance using weighted random selection
/// Higher weight = higher probability of being selected
fn select_by_weight(instances: &[&&ProviderInstance]) -> Option<ProviderInstance> {
    if instances.is_empty() {
        return None;
    }

    // Calculate total weight
    let total_weight: u32 = instances.iter()
        .map(|inst| inst.config.weight())
        .sum();

    if total_weight == 0 {
        // Fallback to random selection if all weights are 0
        let mut rng = rand::thread_rng();
        return instances.choose(&mut rng).map(|&&inst| inst.clone());
    }

    // Generate random weight value
    let mut rng = rand::thread_rng();
    let mut random_weight = rng.gen_range(0..total_weight);

    // Select instance based on weight
    for &&inst in instances {
        let weight = inst.config.weight();
        if random_weight < weight {
            return Some(inst.clone());
        }
        random_weight -= weight;
    }

    // Fallback (should not reach here if weights are valid)
    instances.first().map(|&&inst| inst.clone())
}

// ============================================================
// Data Structures
// ============================================================

pub struct LoadBalancer {
    // Session mapping (DashMap for low lock contention)
    sessions: Arc<DashMap<String, SessionInfo>>,

    // Global health state (RwLock, read-heavy workload)
    health_state: Arc<RwLock<HealthState>>,

    // Instance list (immutable, Arc shared)
    instances: Arc<Vec<ProviderInstance>>,

    // Provider name for metrics
    provider_name: String,

    // Optional HTTP client for active health checks
    http_client: Option<reqwest::Client>,
}

/// Session information (simplified, no consecutive_failures)
#[derive(Clone)]
pub struct SessionInfo {
    instance_name: String,           // Currently bound instance
    last_request_time: Instant,      // Last request time (for 1-hour timeout detection)
}

/// Global instance health state
struct HealthState {
    instances: HashMap<String, InstanceHealth>,
}

/// Circuit breaker state for instance health management
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitState {
    /// Circuit is closed - instance is working normally
    Closed,
    /// Circuit is open - instance is failing, do not accept requests
    Open,
    /// Circuit is half-open - testing if instance has recovered
    HalfOpen,
}

struct InstanceHealth {
    is_healthy: bool,                        // Whether instance is healthy
    last_failure_time: Option<Instant>,     // Last failure time (for recovery)
    circuit_state: CircuitState,            // Circuit breaker state
    consecutive_failures: u32,              // Consecutive failure count
    consecutive_successes: u32,             // Consecutive success count (for half-open)
    failure_window_start: Option<Instant>,  // Start of current failure window
}

// Session timeout: 1 hour of inactivity
const SESSION_TIMEOUT: Duration = Duration::from_secs(3600);

// Circuit breaker constants (hardcoded defaults)
const FAILURE_THRESHOLD: u32 = 3;           // 3 failures trigger circuit open
const FAILURE_WINDOW_SECS: u64 = 60;        // 60 second failure window
const SUCCESS_THRESHOLD: u32 = 2;           // 2 successes close circuit from half-open

// Exponential backoff constants
const INITIAL_BACKOFF_SECS: u64 = 60;       // Initial backoff 60 seconds
const MAX_BACKOFF_SECS: u64 = 600;          // Maximum backoff 10 minutes
const BACKOFF_MULTIPLIER: f64 = 2.0;        // Double each time
const JITTER_RATIO: f64 = 0.2;              // ±20% jitter

/// Calculate exponential backoff duration with jitter
fn calculate_backoff(consecutive_failures: u32, base_timeout_secs: u64) -> Duration {
    use rand::Rng;

    // Start from initial backoff or configured timeout (whichever is larger)
    let base = base_timeout_secs.max(INITIAL_BACKOFF_SECS);

    // Calculate exponential backoff: base * (multiplier ^ (failures - threshold))
    let exponent = consecutive_failures.saturating_sub(FAILURE_THRESHOLD);
    let backoff_secs = if exponent == 0 {
        base
    } else {
        let multiplied = (base as f64) * BACKOFF_MULTIPLIER.powi(exponent as i32);
        multiplied.min(MAX_BACKOFF_SECS as f64) as u64
    };

    // Add jitter: ±20% random variation
    let jitter_range = (backoff_secs as f64 * JITTER_RATIO) as u64;
    let mut rng = rand::thread_rng();
    let jitter = rng.gen_range(0..=jitter_range * 2);
    let final_backoff = backoff_secs.saturating_sub(jitter_range).saturating_add(jitter);

    Duration::from_secs(final_backoff.max(1)) // At least 1 second
}

#[derive(Clone)]
pub struct ProviderInstance {
    pub name: Arc<str>,
    pub config: Arc<dyn ProviderConfig>,
}

// ============================================================
// LoadBalancer Core Logic
// ============================================================

impl LoadBalancer {
    pub fn new(provider_name: String, instances: Vec<ProviderInstance>) -> Self {
        Self::with_client(provider_name, instances, None)
    }

    pub fn with_client(
        provider_name: String,
        instances: Vec<ProviderInstance>,
        http_client: Option<reqwest::Client>,
    ) -> Self {
        // Initialize health state
        let mut health_instances = HashMap::new();
        for inst in &instances {
            health_instances.insert(
                inst.name.to_string(),
                InstanceHealth {
                    is_healthy: true,
                    last_failure_time: None,
                    circuit_state: CircuitState::Closed,
                    consecutive_failures: 0,
                    consecutive_successes: 0,
                    failure_window_start: None,
                },
            );
        }

        Self {
            sessions: Arc::new(DashMap::new()),
            health_state: Arc::new(RwLock::new(HealthState {
                instances: health_instances,
            })),
            instances: Arc::new(instances),
            provider_name,
            http_client,
        }
    }

    /// Select instance for given API key (sticky session)
    pub async fn select_instance_for_key(
        &self,
        api_key: &str,
    ) -> Option<ProviderInstance> {
        // Step 1: Check if session exists and is not expired
        if let Some(mut session) = self.sessions.get_mut(api_key) {
            let now = Instant::now();

            // Check if session has expired
            if now.duration_since(session.last_request_time) < SESSION_TIMEOUT {
                let instance_name = session.instance_name.clone();

                // Check if the instance is still healthy
                let is_healthy = {
                    let health = self.health_state.read().await;
                    health.instances.get(&instance_name)
                        .is_some_and(|h| h.is_healthy)
                };

                if is_healthy {
                    // Update session last access time
                    session.last_request_time = now;
                    drop(session);  // Release lock

                    // Return bound instance
                    return self.get_instance_by_name(&instance_name);
                } else {
                    // Instance unhealthy, remove session and reselect
                    drop(session);
                    self.sessions.remove(api_key);

                    tracing::warn!(
                        api_key = api_key,
                        old_instance = %instance_name,
                        "Session instance unhealthy, selecting new instance"
                    );
                }
            } else {
                // Session expired, remove and reselect
                drop(session);
                self.sessions.remove(api_key);

                tracing::info!(
                    api_key = api_key,
                    "Session expired after 1 hour, selecting new instance"
                );
            }
        }

        // Step 2: New session or instance unhealthy/expired, select by priority
        let instance = self.select_healthy_instance_by_priority().await?;

        // Step 3: Create session
        let now = Instant::now();
        self.sessions.insert(
            api_key.to_string(),
            SessionInfo {
                instance_name: instance.name.to_string(),
                last_request_time: now,
            },
        );

        tracing::info!(
            api_key = api_key,
            instance = %instance.name,
            "Created new session"
        );

        Some(instance)
    }

    /// Select healthy instance by priority (lower priority number = higher priority, weighted random among same priority)
    async fn select_healthy_instance_by_priority(&self) -> Option<ProviderInstance> {
        let health = self.health_state.read().await;

        // Filter healthy and enabled instances (respect circuit breaker)
        let healthy_instances: Vec<_> = self.instances.iter()
            .filter(|inst| {
                inst.config.enabled() &&
                health.instances.get(inst.name.as_ref())
                    .is_some_and(|h| {
                        h.is_healthy && h.circuit_state != CircuitState::Open
                    })
            })
            .collect();

        if healthy_instances.is_empty() {
            return None;
        }

        // Find minimum priority (highest priority)
        let min_priority = healthy_instances.iter()
            .map(|inst| inst.config.priority())
            .min()?;

        // Get all instances with highest priority
        let top_priority: Vec<_> = healthy_instances.iter()
            .filter(|inst| inst.config.priority() == min_priority)
            .collect();

        // Weighted random selection among same priority instances
        select_by_weight(&top_priority)
    }

    /// Get the total weight of instances
    #[allow(dead_code)]
    fn get_total_weight(&self) -> u32 {
        self.instances.iter()
            .map(|inst| inst.config.weight())
            .sum()
    }

    /// Get instance by name
    pub fn get_instance_by_name(&self, name: &str) -> Option<ProviderInstance> {
        self.instances.iter()
            .find(|i| i.name.as_ref() == name)
            .cloned()
    }

    /// Get the provider name for this load balancer
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Get the count of healthy and enabled instances
    pub async fn healthy_instance_count(&self) -> usize {
        let health = self.health_state.read().await;
        self.instances.iter()
            .filter(|inst| {
                inst.config.enabled() &&
                health.instances.get(inst.name.as_ref())
                    .is_some_and(|h| h.is_healthy)
            })
            .count()
    }

    /// Mark instance failure (single failure marks unhealthy)
    pub async fn mark_instance_failure(&self, instance_name: &str) {
        let mut health = self.health_state.write().await;
        if let Some(inst_health) = health.instances.get_mut(instance_name) {
            inst_health.is_healthy = false;
            inst_health.last_failure_time = Some(Instant::now());

            tracing::warn!(
                instance = instance_name,
                "Instance marked unhealthy due to request failure"
            );
        }
    }

    /// Record a successful request (for circuit breaker)
    pub async fn record_success(&self, instance_name: &str) {
        let mut health = self.health_state.write().await;
        if let Some(h) = health.instances.get_mut(instance_name) {
            h.consecutive_failures = 0;
            h.consecutive_successes += 1;

            // Half-open state: consecutive successes reach threshold → close circuit
            if h.circuit_state == CircuitState::HalfOpen
                && h.consecutive_successes >= SUCCESS_THRESHOLD
            {
                h.circuit_state = CircuitState::Closed;
                h.is_healthy = true;
                tracing::info!(
                    instance = instance_name,
                    "✅ Circuit closed after {} consecutive successes",
                    h.consecutive_successes
                );
            }
        }
    }

    /// Record an instance failure with circuit breaker logic
    pub async fn record_failure(&self, instance_name: &str, failure_type: crate::retry::FailureType) {
        let mut health = self.health_state.write().await;
        if let Some(h) = health.instances.get_mut(instance_name) {
            let now = Instant::now();

            // Reset failure window if expired
            if let Some(window_start) = h.failure_window_start {
                if now.duration_since(window_start) > Duration::from_secs(FAILURE_WINDOW_SECS) {
                    h.consecutive_failures = 0;
                    h.failure_window_start = Some(now);
                }
            } else {
                h.failure_window_start = Some(now);
            }

            h.consecutive_failures += 1;
            h.consecutive_successes = 0;

            // Reach threshold → open circuit
            if h.consecutive_failures >= FAILURE_THRESHOLD {
                h.circuit_state = CircuitState::Open;
                h.is_healthy = false;
                h.last_failure_time = Some(now);
                tracing::warn!(
                    instance = instance_name,
                    consecutive_failures = h.consecutive_failures,
                    failure_type = ?failure_type,
                    "🔴 Circuit opened due to {} consecutive failures",
                    h.consecutive_failures
                );
            } else {
                tracing::debug!(
                    instance = instance_name,
                    consecutive_failures = h.consecutive_failures,
                    threshold = FAILURE_THRESHOLD,
                    "⚠️ Failure recorded ({}/{})",
                    h.consecutive_failures,
                    FAILURE_THRESHOLD
                );
            }
        }
    }

    /// Check if instance is available (respects circuit breaker state)
    pub async fn is_instance_available(&self, instance_name: &str) -> bool {
        let health = self.health_state.read().await;
        if let Some(h) = health.instances.get(instance_name) {
            match h.circuit_state {
                CircuitState::Closed => true,
                CircuitState::Open => false,
                CircuitState::HalfOpen => {
                    // Half-open state: allow through (controlled by health_recovery_loop)
                    true
                }
            }
        } else {
            false
        }
    }

    /// Clean up expired sessions (background task)
    pub async fn session_cleanup_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(300));  // 5 minutes

        loop {
            interval.tick().await;

            let now = Instant::now();
            let timeout = SESSION_TIMEOUT;

            self.sessions.retain(|_key, session| {
                now.duration_since(session.last_request_time) < timeout
            });

            let session_count = self.sessions.len();

            tracing::debug!(
                active_sessions = session_count,
                "Session cleanup completed"
            );
        }
    }

    /// Health check recovery loop with active health checking and exponential backoff
    pub async fn health_recovery_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            let mut health = self.health_state.write().await;
            let now = Instant::now();

            for (name, inst_health) in health.instances.iter_mut() {
                if !inst_health.is_healthy || inst_health.circuit_state == CircuitState::Open {
                    if let Some(last_failure) = inst_health.last_failure_time {
                        // Get the instance's configured base timeout
                        let base_timeout = self.instances.iter()
                            .find(|i| i.name.as_ref() == name)
                            .map(|i| i.config.failure_timeout_seconds())
                            .unwrap_or(60);

                        // Calculate backoff with exponential increase
                        let backoff_duration = calculate_backoff(
                            inst_health.consecutive_failures,
                            base_timeout
                        );

                        if now.duration_since(last_failure) >= backoff_duration {
                            // Attempt active health check before marking as healthy
                            let check_result = self.perform_active_health_check(name).await;

                            match check_result {
                                Ok(()) => {
                                    // Health check passed, transition to half-open
                                    inst_health.circuit_state = CircuitState::HalfOpen;
                                    inst_health.is_healthy = true;
                                    inst_health.consecutive_successes = 1; // First success

                                    tracing::info!(
                                        instance = name,
                                        backoff_seconds = backoff_duration.as_secs(),
                                        consecutive_failures = inst_health.consecutive_failures,
                                        "🟡 Instance passed health check, circuit half-open (testing recovery)"
                                    );
                                }
                                Err(e) => {
                                    // Health check failed, increment failures and reset timer
                                    inst_health.consecutive_failures += 1;
                                    inst_health.last_failure_time = Some(now);

                                    let next_backoff = calculate_backoff(
                                        inst_health.consecutive_failures,
                                        base_timeout
                                    );

                                    tracing::warn!(
                                        instance = name,
                                        error = %e,
                                        consecutive_failures = inst_health.consecutive_failures,
                                        next_retry_seconds = next_backoff.as_secs(),
                                        "❌ Health check failed, extending recovery time (backoff: {}s → {}s)",
                                        backoff_duration.as_secs(),
                                        next_backoff.as_secs()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Perform active health check on an instance
    ///
    /// Sends a lightweight request to the instance to verify it's actually healthy.
    /// Uses a short timeout to avoid blocking the recovery loop.
    async fn perform_active_health_check(&self, instance_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Find the instance
        let instance = self.instances.iter()
            .find(|i| i.name.as_ref() == instance_name)
            .ok_or_else(|| format!("Instance '{}' not found", instance_name))?;

        // Only perform active health check if we have an HTTP client
        let client = self.http_client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No HTTP client configured for active health check"))?;

        // Build health check URL (use /v1/models endpoint which is lightweight)
        let base_url = instance.config.base_url();
        let health_check_url = format!("{}/models", base_url.trim_end_matches('/'));

        // Create a short-lived request with timeout
        let mut request = client.get(&health_check_url);

        // Add authorization header if api_key is available
        if let Some(api_key) = instance.config.api_key() {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = tokio::time::timeout(
            Duration::from_secs(5),  // 5 second timeout for health check
            request.send()
        ).await;

        match response {
            Ok(Ok(resp)) if resp.status().is_success() => {
                // Health check passed
                Ok(())
            }
            Ok(Ok(resp)) => {
                // Non-success status code
                Err(anyhow::anyhow!("Health check returned non-success status: {}", resp.status()).into())
            }
            Ok(Err(e)) => {
                // Request error
                Err(anyhow::anyhow!("Health check request failed: {}", e).into())
            }
            Err(_) => {
                // Timeout
                Err(anyhow::anyhow!("Health check timed out after 5 seconds").into())
            }
        }
    }

    /// Get health status of all instances
    ///
    /// Returns a vector of instance health information including current status,
    /// duration in current state, and other metadata.
    pub async fn get_all_instances_health(&self) -> Vec<InstanceHealthInfo> {
        let health = self.health_state.read().await;
        let now = Instant::now();

        self.instances.iter()
            .filter(|inst| inst.config.enabled())
            .map(|inst| {
                let inst_health = health.instances.get(inst.name.as_ref());
                let is_healthy = inst_health.map(|h| h.is_healthy).unwrap_or(true);
                let last_failure = inst_health.and_then(|h| h.last_failure_time);

                // Calculate duration in current state
                let duration_secs = if is_healthy {
                    // If healthy, check if we have a last_failure time
                    // If yes, calculate duration since recovery (now - failure_time - timeout)
                    // If no, instance has never failed, duration is 0
                    last_failure.map(|t| {
                        let timeout = Duration::from_secs(inst.config.failure_timeout_seconds());
                        let recovery_time = t + timeout;
                        if now > recovery_time {
                            now.duration_since(recovery_time).as_secs()
                        } else {
                            0
                        }
                    }).unwrap_or(0)
                } else {
                    // If unhealthy, duration is time since failure
                    last_failure.map(|t| now.duration_since(t).as_secs()).unwrap_or(0)
                };

                InstanceHealthInfo {
                    provider: self.provider_name.clone(),
                    instance: inst.name.to_string(),
                    is_healthy,
                    duration_secs,
                }
            })
            .collect()
    }

    /// Migrate sessions from an old LoadBalancer to this new LoadBalancer
    ///
    /// This is called during config reload to preserve sticky sessions.
    /// Only migrates sessions that:
    /// - Have not expired (< 1 hour since last request)
    /// - Point to instances that still exist in the new config
    /// - Point to instances that are enabled
    /// - Point to instances that are healthy
    ///
    /// Returns statistics about the migration process.
    pub async fn migrate_sessions_from(&self, old_lb: &LoadBalancer) -> MigrationStats {
        let now = Instant::now();

        let mut stats = MigrationStats::default();

        // Get health state once to avoid repeated locks
        let health = self.health_state.read().await;

        // Iterate through all sessions in old load balancer
        for entry in old_lb.sessions.iter() {
            let api_key = entry.key();
            let session_info = entry.value();

            stats.total_sessions += 1;

            // Check 1: Session not expired
            if now.duration_since(session_info.last_request_time) >= SESSION_TIMEOUT {
                stats.dropped_expired += 1;
                tracing::debug!(
                    api_key = %crate::logging::sanitize_log_value(api_key),
                    instance = %session_info.instance_name,
                    "Dropping expired session during migration"
                );
                continue;
            }

            // Check 2: Instance exists in new config
            let instance = match self.get_instance_by_name(&session_info.instance_name) {
                Some(inst) => inst,
                None => {
                    stats.dropped_instance_not_found += 1;
                    tracing::debug!(
                        api_key = %crate::logging::sanitize_log_value(api_key),
                        instance = %session_info.instance_name,
                        "Dropping session: instance not found in new config"
                    );
                    continue;
                }
            };

            // Check 3: Instance is enabled
            if !instance.config.enabled() {
                stats.dropped_instance_disabled += 1;
                tracing::debug!(
                    api_key = %crate::logging::sanitize_log_value(api_key),
                    instance = %session_info.instance_name,
                    "Dropping session: instance is disabled"
                );
                continue;
            }

            // Check 4: Instance is healthy
            let is_healthy = health.instances.get(session_info.instance_name.as_str())
                .is_some_and(|h| h.is_healthy);

            if !is_healthy {
                stats.dropped_instance_unhealthy += 1;
                tracing::debug!(
                    api_key = %crate::logging::sanitize_log_value(api_key),
                    instance = %session_info.instance_name,
                    "Dropping session: instance is unhealthy"
                );
                continue;
            }

            // All checks passed - migrate the session
            self.sessions.insert(
                api_key.clone(),
                SessionInfo {
                    instance_name: session_info.instance_name.clone(),
                    last_request_time: session_info.last_request_time,
                },
            );

            stats.migrated += 1;
            tracing::debug!(
                api_key = %crate::logging::sanitize_log_value(api_key),
                instance = %session_info.instance_name,
                "Migrated session successfully"
            );
        }

        stats
    }
}

/// Instance health information for API responses
#[derive(Debug, Clone, Serialize)]
pub struct InstanceHealthInfo {
    pub provider: String,
    pub instance: String,
    pub is_healthy: bool,
    pub duration_secs: u64,
}

/// Statistics from session migration during config reload
#[derive(Debug, Clone, Default)]
pub struct MigrationStats {
    /// Total sessions in old load balancer
    pub total_sessions: usize,
    /// Sessions successfully migrated
    pub migrated: usize,
    /// Sessions dropped because instance doesn't exist
    pub dropped_instance_not_found: usize,
    /// Sessions dropped because instance is disabled
    pub dropped_instance_disabled: usize,
    /// Sessions dropped because instance is unhealthy
    pub dropped_instance_unhealthy: usize,
    /// Sessions dropped because they expired (>1 hour)
    pub dropped_expired: usize,
}

impl MigrationStats {
    /// Total sessions that were dropped (not migrated)
    pub fn total_dropped(&self) -> usize {
        self.dropped_instance_not_found
            + self.dropped_instance_disabled
            + self.dropped_instance_unhealthy
            + self.dropped_expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_instance(name: &str, priority: u32) -> ProviderInstance {
        ProviderInstance {
            name: Arc::from(name),
            config: Arc::new(crate::config::ProviderInstanceConfig {
                name: name.to_string(),
                enabled: true,
                api_key: Some("test-key".to_string()),
                base_url: "http://localhost".to_string(),
                timeout_seconds: 60,
                priority,
                failure_timeout_seconds: 60,
                weight: 100,
                auth_mode: crate::config::AuthMode::Bearer,
                oauth_provider: None,
            }),
        }
    }

    fn create_test_instance_disabled(name: &str, priority: u32) -> ProviderInstance {
        ProviderInstance {
            name: Arc::from(name),
            config: Arc::new(crate::config::ProviderInstanceConfig {
                name: name.to_string(),
                enabled: false,
                api_key: Some("test-key".to_string()),
                base_url: "http://localhost".to_string(),
                timeout_seconds: 60,
                priority,
                failure_timeout_seconds: 60,
                weight: 100,
                auth_mode: crate::config::AuthMode::Bearer,
                oauth_provider: None,
            }),
        }
    }

    #[tokio::test]
    async fn test_priority_based_selection() {
        let instances = vec![
            create_test_instance("low-priority", 2),
            create_test_instance("high-priority", 1),
        ];

        let lb = LoadBalancer::new("test".to_string(), instances);

        // Should select high-priority (priority=1)
        let selected = lb.select_healthy_instance_by_priority().await;
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().name.as_ref(), "high-priority");
    }

    #[tokio::test]
    async fn test_sticky_session() {
        let instances = vec![
            create_test_instance("instance-a", 1),
            create_test_instance("instance-b", 1),
        ];

        let lb = LoadBalancer::new("test".to_string(), instances);

        // First request creates session
        let first = lb.select_instance_for_key("test-api-key").await;
        assert!(first.is_some());
        let first_name = first.as_ref().unwrap().name.to_string();

        // Second request should get same instance (sticky)
        let second = lb.select_instance_for_key("test-api-key").await;
        assert!(second.is_some());
        assert_eq!(second.unwrap().name.as_ref(), first_name);
    }

    #[tokio::test]
    async fn test_failover_on_unhealthy() {
        let instances = vec![
            create_test_instance("primary", 1),
            create_test_instance("backup", 2),
        ];

        let lb = LoadBalancer::new("test".to_string(), instances);

        // First request goes to primary
        let first = lb.select_instance_for_key("test-key").await;
        assert_eq!(first.as_ref().unwrap().name.as_ref(), "primary");

        // Mark primary unhealthy
        lb.mark_instance_failure("primary").await;

        // Next request should failover to backup
        let second = lb.select_instance_for_key("test-key").await;
        assert_eq!(second.unwrap().name.as_ref(), "backup");
    }

    #[tokio::test]
    async fn test_migrate_sessions_basic() {
        // Create old LoadBalancer with 2 instances and 2 sessions
        let old_instances = vec![
            create_test_instance("instance-a", 1),
            create_test_instance("instance-b", 1),
        ];
        let old_lb = LoadBalancer::new("test".to_string(), old_instances);

        // Create sessions in old LoadBalancer
        old_lb.select_instance_for_key("api-key-1").await;
        old_lb.select_instance_for_key("api-key-2").await;

        // Create new LoadBalancer with same instances
        let new_instances = vec![
            create_test_instance("instance-a", 1),
            create_test_instance("instance-b", 1),
        ];
        let new_lb = LoadBalancer::new("test".to_string(), new_instances);

        // Migrate sessions
        let stats = new_lb.migrate_sessions_from(&old_lb).await;

        // Verify: both sessions migrated successfully
        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.migrated, 2);
        assert_eq!(stats.total_dropped(), 0);
    }

    #[tokio::test]
    async fn test_migrate_sessions_instance_not_found() {
        // Create old LoadBalancer with session bound to "instance-a"
        let old_instances = vec![create_test_instance("instance-a", 1)];
        let old_lb = LoadBalancer::new("test".to_string(), old_instances);
        old_lb.select_instance_for_key("api-key-1").await;

        // Create new LoadBalancer with only "instance-b" (no instance-a)
        let new_instances = vec![create_test_instance("instance-b", 1)];
        let new_lb = LoadBalancer::new("test".to_string(), new_instances);

        // Migrate sessions
        let stats = new_lb.migrate_sessions_from(&old_lb).await;

        // Verify: session dropped because instance not found
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.migrated, 0);
        assert_eq!(stats.dropped_instance_not_found, 1);
        assert_eq!(stats.total_dropped(), 1);
    }

    #[tokio::test]
    async fn test_migrate_sessions_instance_disabled() {
        // Create old LoadBalancer with enabled instance
        let old_instances = vec![create_test_instance("instance-a", 1)];
        let old_lb = LoadBalancer::new("test".to_string(), old_instances);
        old_lb.select_instance_for_key("api-key-1").await;

        // Create new LoadBalancer with same instance but disabled
        let new_instances = vec![create_test_instance_disabled("instance-a", 1)];
        let new_lb = LoadBalancer::new("test".to_string(), new_instances);

        // Migrate sessions
        let stats = new_lb.migrate_sessions_from(&old_lb).await;

        // Verify: session dropped because instance is disabled
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.migrated, 0);
        assert_eq!(stats.dropped_instance_disabled, 1);
    }

    #[tokio::test]
    async fn test_migrate_sessions_instance_unhealthy() {
        // Create old LoadBalancer with healthy instance
        let old_instances = vec![create_test_instance("instance-a", 1)];
        let old_lb = LoadBalancer::new("test".to_string(), old_instances);
        old_lb.select_instance_for_key("api-key-1").await;

        // Create new LoadBalancer with same instance
        let new_instances = vec![create_test_instance("instance-a", 1)];
        let new_lb = LoadBalancer::new("test".to_string(), new_instances);

        // Mark instance as unhealthy in new LoadBalancer
        new_lb.mark_instance_failure("instance-a").await;

        // Migrate sessions
        let stats = new_lb.migrate_sessions_from(&old_lb).await;

        // Verify: session dropped because instance is unhealthy
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.migrated, 0);
        assert_eq!(stats.dropped_instance_unhealthy, 1);
    }

    #[tokio::test]
    async fn test_migrate_sessions_expired() {
        use std::time::Duration;

        // Create old LoadBalancer
        let old_instances = vec![create_test_instance("instance-a", 1)];
        let old_lb = LoadBalancer::new("test".to_string(), old_instances);

        // Create session and manually set it to 2 hours ago (expired)
        let api_key = "api-key-1";
        let expired_time = Instant::now() - Duration::from_secs(7200); // 2 hours ago
        old_lb.sessions.insert(
            api_key.to_string(),
            SessionInfo {
                instance_name: "instance-a".to_string(),
                last_request_time: expired_time,
            },
        );

        // Create new LoadBalancer with same instance
        let new_instances = vec![create_test_instance("instance-a", 1)];
        let new_lb = LoadBalancer::new("test".to_string(), new_instances);

        // Migrate sessions
        let stats = new_lb.migrate_sessions_from(&old_lb).await;

        // Verify: session dropped because it expired (>1 hour)
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.migrated, 0);
        assert_eq!(stats.dropped_expired, 1);
    }

    #[tokio::test]
    async fn test_migrate_sessions_mixed_scenarios() {
        use std::time::Duration;

        // Create old LoadBalancer with 3 instances
        let old_instances = vec![
            create_test_instance("instance-a", 1),
            create_test_instance("instance-b", 1),
            create_test_instance("instance-c", 1),
        ];
        let old_lb = LoadBalancer::new("test".to_string(), old_instances);

        // Create 5 sessions with different scenarios:
        // 1. Valid session to instance-a (should migrate)
        old_lb.sessions.insert(
            "valid-key-1".to_string(),
            SessionInfo {
                instance_name: "instance-a".to_string(),
                last_request_time: Instant::now(),
            },
        );

        // 2. Valid session to instance-a (should migrate)
        old_lb.sessions.insert(
            "valid-key-2".to_string(),
            SessionInfo {
                instance_name: "instance-a".to_string(),
                last_request_time: Instant::now(),
            },
        );

        // 3. Session to instance-c which won't exist in new config
        old_lb.sessions.insert(
            "not-found-key".to_string(),
            SessionInfo {
                instance_name: "instance-c".to_string(),
                last_request_time: Instant::now(),
            },
        );

        // 4. Session to instance-b which will be disabled in new config
        old_lb.sessions.insert(
            "disabled-key".to_string(),
            SessionInfo {
                instance_name: "instance-b".to_string(),
                last_request_time: Instant::now(),
            },
        );

        // 5. Expired session to instance-a
        old_lb.sessions.insert(
            "expired-key".to_string(),
            SessionInfo {
                instance_name: "instance-a".to_string(),
                last_request_time: Instant::now() - Duration::from_secs(7200),
            },
        );

        // Create new LoadBalancer:
        // - instance-a: enabled and healthy
        // - instance-b: disabled
        // - instance-c: not present
        let new_instances = vec![
            create_test_instance("instance-a", 1),
            create_test_instance_disabled("instance-b", 1),
        ];
        let new_lb = LoadBalancer::new("test".to_string(), new_instances);

        // Migrate sessions
        let stats = new_lb.migrate_sessions_from(&old_lb).await;

        // Verify statistics
        assert_eq!(stats.total_sessions, 5);
        assert_eq!(stats.migrated, 2); // valid-key-1 and valid-key-2
        assert_eq!(stats.dropped_expired, 1); // expired-key
        assert_eq!(stats.dropped_instance_not_found, 1); // not-found-key
        assert_eq!(stats.dropped_instance_disabled, 1); // disabled-key
        assert_eq!(stats.total_dropped(), 3);
    }
}
