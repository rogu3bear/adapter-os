//! Configurable retry strategies for different service types
//!
//! Provides predefined retry strategies optimized for different types of operations
//! and service characteristics.

use crate::retry_policy::{RetryPolicy, RetryBudgetConfig};
use crate::circuit_breaker::CircuitBreakerConfig;
use std::time::Duration;

/// Service type enumeration for retry strategy selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceType {
    /// Fast, low-latency operations (API calls, cache lookups)
    FastApi,
    /// Database operations (queries, transactions)
    Database,
    /// Network operations (HTTP requests, external APIs)
    Network,
    /// File system operations (reads, writes)
    FileSystem,
    /// Model loading and inference operations
    ModelInference,
    /// Background processing tasks
    BackgroundTask,
    /// Critical system operations
    CriticalSystem,
    /// Batch processing operations
    BatchProcessing,
}

impl ServiceType {
    /// Get the string representation for metrics
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceType::FastApi => "fast_api",
            ServiceType::Database => "database",
            ServiceType::Network => "network",
            ServiceType::FileSystem => "filesystem",
            ServiceType::ModelInference => "model_inference",
            ServiceType::BackgroundTask => "background_task",
            ServiceType::CriticalSystem => "critical_system",
            ServiceType::BatchProcessing => "batch_processing",
        }
    }
}

/// Retry strategy configuration
#[derive(Debug, Clone)]
pub struct RetryStrategy {
    /// Service type this strategy is optimized for
    pub service_type: ServiceType,
    /// Base retry policy
    pub policy: RetryPolicy,
    /// Whether this strategy includes circuit breaker
    pub use_circuit_breaker: bool,
    /// Whether this strategy includes retry budget
    pub use_budget: bool,
    /// Human-readable description
    pub description: String,
}

impl RetryStrategy {
    /// Create a retry strategy for fast API operations
    pub fn fast_api() -> Self {
        Self {
            service_type: ServiceType::FastApi,
            policy: RetryPolicy::fast(ServiceType::FastApi.as_str()),
            use_circuit_breaker: true,
            use_budget: true,
            description: "Fast retry strategy for low-latency API operations".to_string(),
        }
    }

    /// Create a retry strategy for database operations
    pub fn database() -> Self {
        Self {
            service_type: ServiceType::Database,
            policy: RetryPolicy::database(ServiceType::Database.as_str()),
            use_circuit_breaker: true,
            use_budget: true,
            description: "Conservative retry strategy for database operations".to_string(),
        }
    }

    /// Create a retry strategy for network operations
    pub fn network() -> Self {
        Self {
            service_type: ServiceType::Network,
            policy: RetryPolicy::network(ServiceType::Network.as_str()),
            use_circuit_breaker: true,
            use_budget: true,
            description: "Network-aware retry strategy for external API calls".to_string(),
        }
    }

    /// Create a retry strategy for file system operations
    pub fn filesystem() -> Self {
        Self {
            service_type: ServiceType::FileSystem,
            policy: RetryPolicy {
                max_attempts: 3,
                base_delay: Duration::from_millis(200),
                max_delay: Duration::from_secs(5),
                backoff_factor: 2.0,
                jitter: true,
                deterministic_jitter: false,
                circuit_breaker: Some(CircuitBreakerConfig {
                    failure_threshold: 5,
                    success_threshold: 2,
                    timeout_ms: 30000,
                    half_open_max_requests: 3,
                }),
                budget: Some(RetryBudgetConfig {
                    max_concurrent_retries: 20,
                    max_retry_rate_per_second: 50.0,
                    budget_window: Duration::from_secs(60),
                    max_budget_tokens: 200,
                }),
                service_type: ServiceType::FileSystem.as_str().to_string(),
            },
            use_circuit_breaker: true,
            use_budget: true,
            description: "File system retry strategy with conservative limits".to_string(),
        }
    }

    /// Create a retry strategy for model inference operations
    pub fn model_inference() -> Self {
        Self {
            service_type: ServiceType::ModelInference,
            policy: RetryPolicy {
                max_attempts: 2, // Limited retries for expensive operations
                base_delay: Duration::from_millis(500),
                max_delay: Duration::from_secs(10),
                backoff_factor: 2.0,
                jitter: true,
                deterministic_jitter: true, // Use deterministic jitter for reproducibility
                circuit_breaker: Some(CircuitBreakerConfig {
                    failure_threshold: 3,
                    success_threshold: 2,
                    timeout_ms: 120000,
                    half_open_max_requests: 1, // Very limited concurrent requests
                }),
                budget: Some(RetryBudgetConfig {
                    max_concurrent_retries: 5, // Very limited concurrent retries
                    max_retry_rate_per_second: 10.0,
                    budget_window: Duration::from_secs(60),
                    max_budget_tokens: 50,
                }),
                service_type: ServiceType::ModelInference.as_str().to_string(),
            },
            use_circuit_breaker: true,
            use_budget: true,
            description: "Conservative retry strategy for expensive model inference".to_string(),
        }
    }

    /// Create a retry strategy for background tasks
    pub fn background_task() -> Self {
        Self {
            service_type: ServiceType::BackgroundTask,
            policy: RetryPolicy {
                max_attempts: 5,
                base_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(300), // 5 minutes
                backoff_factor: 2.0,
                jitter: true,
                deterministic_jitter: false,
                circuit_breaker: Some(CircuitBreakerConfig {
                    failure_threshold: 10,
                    success_threshold: 3,
                    timeout_ms: 600000, // 10 minutes
                    half_open_max_requests: 5,
                }),
                budget: Some(RetryBudgetConfig {
                    max_concurrent_retries: 100,
                    max_retry_rate_per_second: 20.0,
                    budget_window: Duration::from_secs(300),
                    max_budget_tokens: 1000,
                }),
                service_type: ServiceType::BackgroundTask.as_str().to_string(),
            },
            use_circuit_breaker: true,
            use_budget: true,
            description: "Patient retry strategy for background processing tasks".to_string(),
        }
    }

    /// Create a retry strategy for critical system operations
    pub fn critical_system() -> Self {
        Self {
            service_type: ServiceType::CriticalSystem,
            policy: RetryPolicy {
                max_attempts: 10, // Many retries for critical operations
                base_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(60),
                backoff_factor: 1.5,
                jitter: false, // No jitter for predictable critical operations
                deterministic_jitter: false,
                circuit_breaker: Some(CircuitBreakerConfig {
                    failure_threshold: 20,
                    success_threshold: 5,
                    timeout_ms: 300000,
                    half_open_max_requests: 10,
                }),
                budget: Some(RetryBudgetConfig {
                    max_concurrent_retries: 50,
                    max_retry_rate_per_second: 100.0,
                    budget_window: Duration::from_secs(60),
                    max_budget_tokens: 500,
                }),
                service_type: ServiceType::CriticalSystem.as_str().to_string(),
            },
            use_circuit_breaker: true,
            use_budget: true,
            description: "Aggressive retry strategy for critical system operations".to_string(),
        }
    }

    /// Create a retry strategy for batch processing operations
    pub fn batch_processing() -> Self {
        Self {
            service_type: ServiceType::BatchProcessing,
            policy: RetryPolicy {
                max_attempts: 3,
                base_delay: Duration::from_secs(5),
                max_delay: Duration::from_secs(300),
                backoff_factor: 2.0,
                jitter: true,
                deterministic_jitter: false,
                circuit_breaker: Some(CircuitBreakerConfig {
                    failure_threshold: 5,
                    success_threshold: 2,
                    timeout_ms: 1800000, // 30 minutes
                    half_open_max_requests: 2,
                }),
                budget: Some(RetryBudgetConfig {
                    max_concurrent_retries: 10,
                    max_retry_rate_per_second: 5.0,
                    budget_window: Duration::from_secs(3600), // 1 hour
                    max_budget_tokens: 200,
                }),
                service_type: ServiceType::BatchProcessing.as_str().to_string(),
            },
            use_circuit_breaker: true,
            use_budget: true,
            description: "Batch processing retry strategy with long timeouts".to_string(),
        }
    }

    /// Create a custom retry strategy
    pub fn custom(service_type: ServiceType, policy: RetryPolicy) -> Self {
        let use_circuit_breaker = policy.circuit_breaker.is_some();
        let use_budget = policy.budget.is_some();
        Self {
            service_type,
            policy: policy.clone(),
            use_circuit_breaker,
            use_budget,
            description: format!("Custom retry strategy for {:?}", service_type),
        }
    }

    /// Get the effective retry policy, optionally disabling circuit breaker or budget
    pub fn effective_policy(&self, use_circuit_breaker: bool, use_budget: bool) -> RetryPolicy {
        let mut policy = self.policy.clone();

        if !use_circuit_breaker {
            policy.circuit_breaker = None;
        }

        if !use_budget {
            policy.budget = None;
        }

        policy
    }
}

/// Registry of retry strategies
#[derive(Debug, Clone)]
pub struct RetryStrategyRegistry {
    strategies: std::collections::HashMap<ServiceType, RetryStrategy>,
}

impl Default for RetryStrategyRegistry {
    fn default() -> Self {
        let mut strategies = std::collections::HashMap::new();

        strategies.insert(ServiceType::FastApi, RetryStrategy::fast_api());
        strategies.insert(ServiceType::Database, RetryStrategy::database());
        strategies.insert(ServiceType::Network, RetryStrategy::network());
        strategies.insert(ServiceType::FileSystem, RetryStrategy::filesystem());
        strategies.insert(ServiceType::ModelInference, RetryStrategy::model_inference());
        strategies.insert(ServiceType::BackgroundTask, RetryStrategy::background_task());
        strategies.insert(ServiceType::CriticalSystem, RetryStrategy::critical_system());
        strategies.insert(ServiceType::BatchProcessing, RetryStrategy::batch_processing());

        Self { strategies }
    }
}

impl RetryStrategyRegistry {
    /// Create a new registry with default strategies
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a retry strategy for the given service type
    pub fn get(&self, service_type: ServiceType) -> Option<&RetryStrategy> {
        self.strategies.get(&service_type)
    }

    /// Register a custom retry strategy
    pub fn register(&mut self, strategy: RetryStrategy) {
        self.strategies.insert(strategy.service_type, strategy);
    }

    /// Get all registered service types
    pub fn service_types(&self) -> Vec<ServiceType> {
        self.strategies.keys().cloned().collect()
    }

    /// Create a retry manager configured for the given service type
    pub fn create_manager(
        &self,
        service_type: ServiceType,
        use_circuit_breaker: bool,
        use_budget: bool,
    ) -> Option<crate::RetryManager> {
        self.get(service_type).map(|strategy| {
            let policy = strategy.effective_policy(use_circuit_breaker, use_budget);

            // Note: Budget configuration would need to be handled at manager creation time
            // For now, we just return the basic manager
            if let Some(cb_config) = &policy.circuit_breaker {
                crate::RetryManager::with_circuit_breaker(cb_config.clone())
            } else {
                crate::RetryManager::new()
            }
        })
    }
}

/// Global registry instance
static REGISTRY: std::sync::OnceLock<RetryStrategyRegistry> = std::sync::OnceLock::new();

/// Get the global retry strategy registry
pub fn global_registry() -> &'static RetryStrategyRegistry {
    REGISTRY.get_or_init(RetryStrategyRegistry::default)
}

/// Get a retry strategy for the given service type from the global registry
pub fn get_strategy(service_type: ServiceType) -> Option<&'static RetryStrategy> {
    global_registry().get(service_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_as_str() {
        assert_eq!(ServiceType::FastApi.as_str(), "fast_api");
        assert_eq!(ServiceType::Database.as_str(), "database");
        assert_eq!(ServiceType::Network.as_str(), "network");
    }

    #[test]
    fn test_retry_strategy_creation() {
        let strategy = RetryStrategy::fast_api();
        assert_eq!(strategy.service_type, ServiceType::FastApi);
        assert!(strategy.use_circuit_breaker);
        assert!(strategy.use_budget);
        assert_eq!(strategy.policy.max_attempts, 3);
    }

    #[test]
    fn test_registry_default_strategies() {
        let registry = RetryStrategyRegistry::new();

        assert!(registry.get(ServiceType::FastApi).is_some());
        assert!(registry.get(ServiceType::Database).is_some());
        assert!(registry.get(ServiceType::Network).is_some());
        assert!(registry.get(ServiceType::ModelInference).is_some());

        let service_types = registry.service_types();
        assert_eq!(service_types.len(), 8); // All service types should be registered
    }

    #[test]
    fn test_custom_strategy() {
        let policy = RetryPolicy::fast("custom");
        let strategy = RetryStrategy::custom(ServiceType::FastApi, policy);

        assert_eq!(strategy.service_type, ServiceType::FastApi);
        assert_eq!(strategy.policy.service_type, "custom");
    }

    #[test]
    fn test_effective_policy_modifications() {
        let strategy = RetryStrategy::fast_api();

        // With circuit breaker and budget
        let policy1 = strategy.effective_policy(true, true);
        assert!(policy1.circuit_breaker.is_some());
        assert!(policy1.budget.is_some());

        // Without circuit breaker
        let policy2 = strategy.effective_policy(false, true);
        assert!(policy2.circuit_breaker.is_none());
        assert!(policy2.budget.is_some());

        // Without budget
        let policy3 = strategy.effective_policy(true, false);
        assert!(policy3.circuit_breaker.is_some());
        assert!(policy3.budget.is_none());

        // Without both
        let policy4 = strategy.effective_policy(false, false);
        assert!(policy4.circuit_breaker.is_none());
        assert!(policy4.budget.is_none());
    }

    #[test]
    fn test_global_registry() {
        let strategy = get_strategy(ServiceType::Database);
        assert!(strategy.is_some());
        assert_eq!(strategy.unwrap().service_type, ServiceType::Database);
    }

    #[test]
    fn test_strategy_configurations() {
        // Test that different strategies have appropriate configurations
        let fast_api = RetryStrategy::fast_api();
        let model_inference = RetryStrategy::model_inference();
        let background = RetryStrategy::background_task();

        // Fast API should have quick retries
        assert_eq!(fast_api.policy.max_attempts, 3);
        assert_eq!(fast_api.policy.base_delay, Duration::from_millis(50));

        // Model inference should have limited retries due to cost
        assert_eq!(model_inference.policy.max_attempts, 2);
        assert_eq!(model_inference.policy.base_delay, Duration::from_millis(500));

        // Background tasks should have patient retries
        assert_eq!(background.policy.max_attempts, 5);
        assert_eq!(background.policy.base_delay, Duration::from_secs(1));
    }
}
