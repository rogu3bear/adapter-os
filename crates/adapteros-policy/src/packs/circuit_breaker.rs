//! Circuit Breaker Policy Pack
//!
//! Defines service-specific circuit breaker configurations and thresholds.
//! Provides default configurations for critical services like database, network, and inference.

use crate::registry::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{CircuitBreakerConfig, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Circuit breaker policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerPolicy {
    /// Default circuit breaker configuration
    pub default_config: CircuitBreakerConfig,
    /// Service-specific overrides
    pub service_configs: HashMap<String, CircuitBreakerConfig>,
}

impl Default for CircuitBreakerPolicy {
    fn default() -> Self {
        let mut service_configs = HashMap::new();

        // Database service - more lenient due to connection pooling
        service_configs.insert(
            "database".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 10,     // More failures before opening
                success_threshold: 3,      // Standard success threshold
                timeout_ms: 30000,         // 30 seconds before retry
                half_open_max_requests: 5, // Limited concurrent requests
            },
        );

        // Inference service - strict to prevent model corruption
        service_configs.insert(
            "inference".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 3,      // Quick to open on failures
                success_threshold: 5,      // More successes needed to close
                timeout_ms: 60000,         // 1 minute timeout
                half_open_max_requests: 2, // Very limited in half-open
            },
        );

        // Network service - moderate thresholds
        service_configs.insert(
            "network".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 5,      // Standard threshold
                success_threshold: 3,      // Standard success threshold
                timeout_ms: 45000,         // 45 seconds timeout
                half_open_max_requests: 3, // Moderate concurrent requests
            },
        );

        // Router service - critical for request routing
        service_configs.insert(
            "router".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 3,      // Quick to open
                success_threshold: 3,      // Standard success threshold
                timeout_ms: 30000,         // 30 seconds timeout
                half_open_max_requests: 2, // Limited concurrent requests
            },
        );

        // Memory management - very strict
        service_configs.insert(
            "memory".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 2,      // Very quick to open
                success_threshold: 5,      // Many successes needed
                timeout_ms: 120000,        // 2 minutes timeout (memory issues take time to resolve)
                half_open_max_requests: 1, // Only one request at a time
            },
        );

        Self {
            default_config: CircuitBreakerConfig::default(),
            service_configs,
        }
    }
}

impl CircuitBreakerPolicy {
    /// Get circuit breaker configuration for a specific service
    pub fn config_for_service(&self, service: &str) -> CircuitBreakerConfig {
        self.service_configs
            .get(service)
            .cloned()
            .unwrap_or_else(|| self.default_config.clone())
    }

    /// Validate circuit breaker configuration
    pub fn validate(&self) -> Result<()> {
        // Validate default config
        self.validate_config(&self.default_config, "default")?;

        // Validate service-specific configs
        for (service, config) in &self.service_configs {
            self.validate_config(config, service)?;
        }

        Ok(())
    }

    /// Validate a single circuit breaker configuration
    fn validate_config(&self, config: &CircuitBreakerConfig, context: &str) -> Result<()> {
        if config.failure_threshold == 0 {
            return Err(adapteros_core::AosError::Config(format!(
                "Circuit breaker failure_threshold for {} must be > 0",
                context
            )));
        }

        if config.success_threshold == 0 {
            return Err(adapteros_core::AosError::Config(format!(
                "Circuit breaker success_threshold for {} must be > 0",
                context
            )));
        }

        if config.timeout_ms == 0 {
            return Err(adapteros_core::AosError::Config(format!(
                "Circuit breaker timeout_ms for {} must be > 0",
                context
            )));
        }

        if config.half_open_max_requests == 0 {
            return Err(adapteros_core::AosError::Config(format!(
                "Circuit breaker half_open_max_requests for {} must be > 0",
                context
            )));
        }

        // Sanity checks for reasonable values
        if config.timeout_ms > 300_000 { // 5 minutes max
            return Err(adapteros_core::AosError::Config(format!(
                "Circuit breaker timeout_ms for {} is too high: {}ms (max: 300000ms)",
                context, config.timeout_ms
            )));
        }

        if config.failure_threshold > 100 {
            return Err(adapteros_core::AosError::Config(format!(
                "Circuit breaker failure_threshold for {} is too high: {} (max: 100)",
                context, config.failure_threshold
            )));
        }

        Ok(())
    }
}

impl Policy for CircuitBreakerPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::CircuitBreaker
    }

    fn name(&self) -> &'static str {
        "Circuit Breaker Policy"
    }

    fn severity(&self) -> Severity {
        Severity::High
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        // Validate the circuit breaker configuration
        self.validate()?;
        Ok(Audit::passed(PolicyId::CircuitBreaker))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = CircuitBreakerPolicy::default();

        // Test default config
        assert_eq!(policy.default_config.failure_threshold, 5);
        assert_eq!(policy.default_config.success_threshold, 3);
        assert_eq!(policy.default_config.timeout_ms, 60000);
        assert_eq!(policy.default_config.half_open_max_requests, 10);

        // Test service-specific configs
        let db_config = policy.config_for_service("database");
        assert_eq!(db_config.failure_threshold, 10);

        let inference_config = policy.config_for_service("inference");
        assert_eq!(inference_config.failure_threshold, 3);
        assert_eq!(inference_config.half_open_max_requests, 2);

        let unknown_config = policy.config_for_service("unknown");
        assert_eq!(unknown_config.failure_threshold, 5); // Should use default
    }

    #[test]
    fn test_validation() {
        let policy = CircuitBreakerPolicy::default();
        assert!(policy.validate().is_ok());

        // Test invalid config
        let mut invalid_policy = CircuitBreakerPolicy::default();
        invalid_policy.default_config.failure_threshold = 0;
        assert!(invalid_policy.validate().is_err());
    }

    #[test]
    fn test_service_configs_override() {
        let policy = CircuitBreakerPolicy::default();

        // Known services should have specific configs
        assert!(policy.service_configs.contains_key("database"));
        assert!(policy.service_configs.contains_key("inference"));
        assert!(policy.service_configs.contains_key("network"));
        assert!(policy.service_configs.contains_key("router"));
        assert!(policy.service_configs.contains_key("memory"));
    }
}
