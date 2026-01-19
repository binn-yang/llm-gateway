use crate::config::{AnthropicInstanceConfig, ProviderInstanceConfig};
use dashmap::DashMap;
use rand::seq::SliceRandom;
use rand::Rng;
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

struct InstanceHealth {
    is_healthy: bool,                 // Whether instance is healthy
    last_failure_time: Option<Instant>,  // Last failure time (for recovery)
}

#[derive(Clone)]
pub struct ProviderInstance {
    pub name: Arc<str>,               // Shared string to avoid clone
    pub config: ProviderInstanceConfigEnum,
}

#[derive(Clone)]
pub enum ProviderInstanceConfigEnum {
    Generic(Arc<ProviderInstanceConfig>),
    Anthropic(Arc<AnthropicInstanceConfig>),
}

// Helper trait to access common fields
impl ProviderInstanceConfigEnum {
    pub fn enabled(&self) -> bool {
        match self {
            Self::Generic(c) => c.enabled,
            Self::Anthropic(c) => c.enabled,
        }
    }

    pub fn priority(&self) -> u32 {
        match self {
            Self::Generic(c) => c.priority,
            Self::Anthropic(c) => c.priority,
        }
    }

    pub fn failure_timeout_seconds(&self) -> u64 {
        match self {
            Self::Generic(c) => c.failure_timeout_seconds,
            Self::Anthropic(c) => c.failure_timeout_seconds,
        }
    }

    pub fn weight(&self) -> u32 {
        match self {
            Self::Generic(c) => c.weight,
            Self::Anthropic(c) => c.weight,
        }
    }

    pub fn api_key(&self) -> &str {
        match self {
            Self::Generic(c) => &c.api_key,
            Self::Anthropic(c) => &c.api_key,
        }
    }

    pub fn base_url(&self) -> &str {
        match self {
            Self::Generic(c) => &c.base_url,
            Self::Anthropic(c) => &c.base_url,
        }
    }

    pub fn timeout_seconds(&self) -> u64 {
        match self {
            Self::Generic(c) => c.timeout_seconds,
            Self::Anthropic(c) => c.timeout_seconds,
        }
    }
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
                },
            );

            // Initialize instance health metric to 1 (healthy)
            crate::metrics::update_instance_health(&provider_name, &inst.name, true);
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
        const SESSION_TIMEOUT: Duration = Duration::from_secs(3600);  // 1 hour

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
                        .map_or(false, |h| h.is_healthy)
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

        // Filter healthy and enabled instances
        let healthy_instances: Vec<_> = self.instances.iter()
            .filter(|inst| {
                inst.config.enabled() &&
                health.instances.get(inst.name.as_ref())
                    .map_or(false, |h| h.is_healthy)
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

    /// Get the count of healthy and enabled instances

    /// Get instance by name
    fn get_instance_by_name(&self, name: &str) -> Option<ProviderInstance> {
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
                    .map_or(false, |h| h.is_healthy)
            })
            .count()
    }

    /// Mark instance failure (single failure marks unhealthy)
    pub async fn mark_instance_failure(&self, instance_name: &str) {
        let mut health = self.health_state.write().await;
        if let Some(inst_health) = health.instances.get_mut(instance_name) {
            inst_health.is_healthy = false;
            inst_health.last_failure_time = Some(Instant::now());

            // Update instance health metric to 0 (unhealthy)
            crate::metrics::update_instance_health(&self.provider_name, instance_name, false);

            tracing::warn!(
                instance = instance_name,
                "Instance marked unhealthy due to request failure"
            );
        }
    }

    /// Clean up expired sessions (background task)
    pub async fn session_cleanup_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(300));  // 5 minutes

        loop {
            interval.tick().await;

            let now = Instant::now();
            let timeout = Duration::from_secs(3600);  // 1 hour no request considered expired

            self.sessions.retain(|_key, session| {
                now.duration_since(session.last_request_time) < timeout
            });

            let session_count = self.sessions.len();

            // Update session count metric
            crate::metrics::update_session_count(&self.provider_name, session_count);

            tracing::debug!(
                active_sessions = session_count,
                "Session cleanup completed"
            );
        }
    }

    /// Health check recovery loop with active health checking
    pub async fn health_recovery_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            let mut health = self.health_state.write().await;
            let now = Instant::now();

            for (name, inst_health) in health.instances.iter_mut() {
                if !inst_health.is_healthy {
                    if let Some(last_failure) = inst_health.last_failure_time {
                        // Get the instance's configured timeout
                        let timeout = self.instances.iter()
                            .find(|i| i.name.as_ref() == name)
                            .map(|i| Duration::from_secs(i.config.failure_timeout_seconds()))
                            .unwrap_or(Duration::from_secs(60));

                        if now.duration_since(last_failure) >= timeout {
                            // Attempt active health check before marking as healthy
                            let check_result = self.perform_active_health_check(name).await;

                            match check_result {
                                Ok(()) => {
                                    // Health check passed, mark as healthy
                                    inst_health.is_healthy = true;

                                    // Update instance health metric to 1 (healthy)
                                    crate::metrics::update_instance_health(&self.provider_name, name, true);

                                    tracing::info!(
                                        instance = name,
                                        timeout_seconds = timeout.as_secs(),
                                        "Instance passed active health check and recovered"
                                    );
                                }
                                Err(e) => {
                                    // Health check failed, extend recovery time
                                    inst_health.last_failure_time = Some(Instant::now());

                                    tracing::warn!(
                                        instance = name,
                                        error = %e,
                                        "Active health check failed, extending recovery time"
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
        let response = tokio::time::timeout(
            Duration::from_secs(5),  // 5 second timeout for health check
            client
                .get(&health_check_url)
                .header("Authorization", format!("Bearer {}", instance.config.api_key()))
                .send()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_instance(name: &str, priority: u32) -> ProviderInstance {
        ProviderInstance {
            name: Arc::from(name),
            config: ProviderInstanceConfigEnum::Generic(Arc::new(ProviderInstanceConfig {
                name: name.to_string(),
                enabled: true,
                api_key: "test-key".to_string(),
                base_url: "http://localhost".to_string(),
                timeout_seconds: 60,
                priority,
                failure_timeout_seconds: 60,
                weight: 100,
            })),
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
}
