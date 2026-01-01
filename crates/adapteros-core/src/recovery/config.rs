//! Recovery orchestrator configuration
//!
//! Defines configuration types for the recovery orchestrator.

use crate::circuit_breaker::CircuitBreakerConfig;
use crate::retry_policy::{RetryBudgetConfig, RetryPolicy};

/// Configuration for the recovery orchestrator
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Service identifier for metrics and circuit breaker registry
    pub service_name: String,

    /// Retry policy configuration
    pub retry_policy: RetryPolicy,

    /// Whether to use the global circuit breaker registry
    ///
    /// If true, the orchestrator looks up a circuit breaker by service_name
    /// in the global registry. If false, uses a local circuit breaker.
    pub use_global_circuit_breaker: bool,

    /// Whether SingleFlight deduplication is enabled
    ///
    /// When enabled, concurrent requests with the same key will share
    /// a single execution, reducing load during cache misses.
    pub enable_singleflight: bool,

    /// Fallback configuration
    pub fallback: Option<FallbackConfig>,

    /// Telemetry configuration
    pub telemetry: TelemetryConfig,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            service_name: "default".to_string(),
            retry_policy: RetryPolicy::default(),
            use_global_circuit_breaker: false,
            enable_singleflight: false,
            fallback: None,
            telemetry: TelemetryConfig::default(),
        }
    }
}

impl RecoveryConfig {
    /// Create a new config with the given service name
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    /// Create config for fast operations (low latency, fewer retries)
    pub fn fast(service_name: impl Into<String>) -> Self {
        let name = service_name.into();
        Self {
            service_name: name.clone(),
            retry_policy: RetryPolicy::fast(&name),
            ..Default::default()
        }
    }

    /// Create config for database operations
    pub fn database(service_name: impl Into<String>) -> Self {
        let name = service_name.into();
        Self {
            service_name: name.clone(),
            retry_policy: RetryPolicy::database(&name),
            ..Default::default()
        }
    }

    /// Create config for network operations
    pub fn network(service_name: impl Into<String>) -> Self {
        let name = service_name.into();
        Self {
            service_name: name.clone(),
            retry_policy: RetryPolicy::network(&name),
            ..Default::default()
        }
    }

    /// Create config for slow/expensive operations
    pub fn slow(service_name: impl Into<String>) -> Self {
        let name = service_name.into();
        Self {
            service_name: name.clone(),
            retry_policy: RetryPolicy::slow(&name),
            ..Default::default()
        }
    }

    /// Enable the global circuit breaker registry
    pub fn with_global_circuit_breaker(mut self) -> Self {
        self.use_global_circuit_breaker = true;
        self
    }

    /// Enable SingleFlight deduplication
    pub fn with_singleflight(mut self) -> Self {
        self.enable_singleflight = true;
        self
    }

    /// Configure fallback behavior
    pub fn with_fallback(mut self, fallback: FallbackConfig) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Enable fallback on all failure conditions
    pub fn with_fallback_always(mut self) -> Self {
        self.fallback = Some(FallbackConfig::always());
        self
    }

    /// Configure telemetry
    pub fn with_telemetry(mut self, telemetry: TelemetryConfig) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Set deterministic jitter for reproducible operations
    pub fn deterministic_jitter(mut self, enabled: bool) -> Self {
        self.retry_policy.deterministic_jitter = enabled;
        self
    }
}

/// Configuration for fallback behavior
#[derive(Debug, Clone)]
pub struct FallbackConfig {
    /// Invoke fallback when all retries are exhausted
    pub on_exhausted: bool,

    /// Invoke fallback when circuit breaker is open
    pub on_circuit_open: bool,

    /// Invoke fallback when retry budget is exhausted
    pub on_budget_exhausted: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            on_exhausted: true,
            on_circuit_open: false,
            on_budget_exhausted: false,
        }
    }
}

impl FallbackConfig {
    /// Create config that only falls back on retry exhaustion
    pub fn on_exhausted_only() -> Self {
        Self::default()
    }

    /// Create config that falls back on all failure conditions
    pub fn always() -> Self {
        Self {
            on_exhausted: true,
            on_circuit_open: true,
            on_budget_exhausted: true,
        }
    }

    /// Create config that never falls back
    pub fn never() -> Self {
        Self {
            on_exhausted: false,
            on_circuit_open: false,
            on_budget_exhausted: false,
        }
    }

    /// Check if any fallback condition is enabled
    pub fn is_enabled(&self) -> bool {
        self.on_exhausted || self.on_circuit_open || self.on_budget_exhausted
    }
}

/// Configuration for telemetry/observability
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Enable structured logging of recovery events
    pub log_events: bool,

    /// Include timing information in telemetry
    pub include_timing: bool,

    /// Log level for recovery events (trace, debug, info, warn)
    pub log_level: LogLevel,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_events: true,
            include_timing: true,
            log_level: LogLevel::Debug,
        }
    }
}

impl TelemetryConfig {
    /// Create config with minimal logging
    pub fn minimal() -> Self {
        Self {
            log_events: false,
            include_timing: false,
            log_level: LogLevel::Warn,
        }
    }

    /// Create config with verbose logging
    pub fn verbose() -> Self {
        Self {
            log_events: true,
            include_timing: true,
            log_level: LogLevel::Trace,
        }
    }
}

/// Log level for telemetry events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
}

/// Configuration specifically for the circuit breaker within recovery
#[derive(Debug, Clone, Default)]
pub struct RecoveryCircuitBreakerConfig {
    /// Base circuit breaker configuration
    pub config: CircuitBreakerConfig,

    /// Whether to use the global registry
    pub use_global_registry: bool,
}

/// Configuration specifically for retry budget within recovery
#[derive(Debug, Clone, Default)]
pub struct RecoveryBudgetConfig {
    /// Base budget configuration
    pub config: RetryBudgetConfig,

    /// Whether to share budget across orchestrator instances
    pub shared: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_recovery_config_presets() {
        let fast = RecoveryConfig::fast("api");
        assert_eq!(fast.service_name, "api");
        assert!(fast.retry_policy.base_delay < Duration::from_millis(100));

        let slow = RecoveryConfig::slow("batch");
        assert!(slow.retry_policy.max_attempts >= 5);

        let db = RecoveryConfig::database("sqlite");
        assert!(db.retry_policy.max_delay <= Duration::from_secs(15));
    }

    #[test]
    fn test_recovery_config_builder() {
        let config = RecoveryConfig::new("test")
            .with_global_circuit_breaker()
            .with_singleflight()
            .with_fallback_always()
            .deterministic_jitter(true);

        assert!(config.use_global_circuit_breaker);
        assert!(config.enable_singleflight);
        assert!(config.fallback.unwrap().on_circuit_open);
        assert!(config.retry_policy.deterministic_jitter);
    }

    #[test]
    fn test_fallback_config() {
        let always = FallbackConfig::always();
        assert!(always.is_enabled());
        assert!(always.on_exhausted);
        assert!(always.on_circuit_open);
        assert!(always.on_budget_exhausted);

        let never = FallbackConfig::never();
        assert!(!never.is_enabled());

        let default = FallbackConfig::default();
        assert!(default.is_enabled());
        assert!(default.on_exhausted);
        assert!(!default.on_circuit_open);
    }

    #[test]
    fn test_telemetry_config() {
        let minimal = TelemetryConfig::minimal();
        assert!(!minimal.log_events);

        let verbose = TelemetryConfig::verbose();
        assert!(verbose.log_events);
        assert_eq!(verbose.log_level, LogLevel::Trace);
    }
}
