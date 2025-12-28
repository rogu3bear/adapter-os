//! Global Circuit Breaker Registry
//!
//! Provides a shared registry for circuit breakers to prevent duplicate
//! circuits for the same service across different retry managers.

use crate::circuit_breaker::{CircuitBreakerConfig, StandardCircuitBreaker};
use dashmap::DashMap;
use std::sync::{Arc, OnceLock};

/// Global circuit breaker registry singleton
static REGISTRY: OnceLock<CircuitBreakerRegistry> = OnceLock::new();

/// Registry for sharing circuit breakers across retry managers.
/// Ensures that multiple retry managers hitting the same service
/// share failure state instead of each maintaining independent circuits.
pub struct CircuitBreakerRegistry {
    breakers: DashMap<String, Arc<StandardCircuitBreaker>>,
}

impl CircuitBreakerRegistry {
    /// Get the global circuit breaker registry instance
    pub fn global() -> &'static Self {
        REGISTRY.get_or_init(|| Self {
            breakers: DashMap::new(),
        })
    }

    /// Get or create a circuit breaker for the given service name.
    /// If a breaker with this name already exists, returns the existing one.
    /// This ensures all callers share the same circuit breaker state.
    pub fn get_or_create(
        &self,
        name: &str,
        config: CircuitBreakerConfig,
    ) -> Arc<StandardCircuitBreaker> {
        self.breakers
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(StandardCircuitBreaker::new(name.to_string(), config)))
            .clone()
    }

    /// Get an existing circuit breaker by name without creating one.
    pub fn get(&self, name: &str) -> Option<Arc<StandardCircuitBreaker>> {
        self.breakers.get(name).map(|r| r.clone())
    }

    /// Check if a circuit breaker exists for the given name.
    pub fn contains(&self, name: &str) -> bool {
        self.breakers.contains_key(name)
    }

    /// Get the number of registered circuit breakers.
    pub fn len(&self) -> usize {
        self.breakers.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.breakers.is_empty()
    }

    /// List all registered circuit breaker names.
    pub fn names(&self) -> Vec<String> {
        self.breakers.iter().map(|r| r.key().clone()).collect()
    }

    /// Remove a circuit breaker from the registry.
    /// Returns the removed breaker if it existed.
    pub fn remove(&self, name: &str) -> Option<Arc<StandardCircuitBreaker>> {
        self.breakers.remove(name).map(|(_, v)| v)
    }

    /// Clear all circuit breakers from the registry.
    /// Primarily useful for testing.
    pub fn clear(&self) {
        self.breakers.clear();
    }
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self {
            breakers: DashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_get_or_create() {
        let registry = CircuitBreakerRegistry::default();
        let config = CircuitBreakerConfig::default();

        // First call creates the breaker
        let breaker1 = registry.get_or_create("test-service", config.clone());
        assert_eq!(breaker1.name(), "test-service");

        // Second call returns the same breaker
        let breaker2 = registry.get_or_create("test-service", config);
        assert!(Arc::ptr_eq(&breaker1, &breaker2));
    }

    #[test]
    fn test_registry_get() {
        let registry = CircuitBreakerRegistry::default();
        let config = CircuitBreakerConfig::default();

        // Get returns None for non-existent breaker
        assert!(registry.get("non-existent").is_none());

        // Create a breaker
        let _ = registry.get_or_create("test-service", config);

        // Now get returns Some
        assert!(registry.get("test-service").is_some());
    }

    #[test]
    fn test_registry_remove() {
        let registry = CircuitBreakerRegistry::default();
        let config = CircuitBreakerConfig::default();

        let _ = registry.get_or_create("test-service", config);
        assert!(registry.contains("test-service"));

        let removed = registry.remove("test-service");
        assert!(removed.is_some());
        assert!(!registry.contains("test-service"));
    }

    #[test]
    fn test_registry_names() {
        let registry = CircuitBreakerRegistry::default();
        let config = CircuitBreakerConfig::default();

        registry.get_or_create("service-a", config.clone());
        registry.get_or_create("service-b", config);

        let names = registry.names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"service-a".to_string()));
        assert!(names.contains(&"service-b".to_string()));
    }
}
