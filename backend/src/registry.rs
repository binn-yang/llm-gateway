use crate::load_balancer::LoadBalancer;
use crate::provider_trait::LlmProvider;
use std::collections::HashMap;
use std::sync::Arc;

/// A registered provider with its LlmProvider implementation and LoadBalancer.
pub struct RegisteredProvider {
    pub provider: Arc<dyn LlmProvider>,
    pub load_balancer: Arc<LoadBalancer>,
}

/// Central registry mapping provider names to their implementations.
///
/// Replaces `HashMap<Provider, Arc<LoadBalancer>>` with string-keyed registry
/// that also carries the LlmProvider trait object for each provider.
pub struct ProviderRegistry {
    providers: HashMap<String, RegisteredProvider>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider with its implementation and load balancer.
    pub fn register(
        &mut self,
        name: String,
        provider: Arc<dyn LlmProvider>,
        load_balancer: Arc<LoadBalancer>,
    ) {
        self.providers.insert(
            name,
            RegisteredProvider {
                provider,
                load_balancer,
            },
        );
    }

    /// Get a registered provider by name.
    pub fn get(&self, name: &str) -> Option<&RegisteredProvider> {
        self.providers.get(name)
    }

    /// Check if a provider is registered.
    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// Iterate over all registered providers.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &RegisteredProvider)> {
        self.providers.iter()
    }

    /// Get all provider names.
    pub fn provider_names(&self) -> Vec<&String> {
        self.providers.keys().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
