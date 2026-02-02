//! Request timeout mechanisms
//!
//! Implements timeout protection to prevent runaway processes.
//! Aligns with Memory Ruleset #12 and Performance Ruleset #11 from policy enforcement.
//!
//! For circuit breaker functionality, use `adapteros_core::CircuitBreaker` or
//! `adapteros_core::StandardCircuitBreaker`.

use adapteros_config::{WorkerSafetyConfig, WorkerSafetySection};
use adapteros_core::{AosError, Result};
use std::time::Duration;
use tokio::time::timeout;

/// Timeout configuration per request type
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub inference_timeout: Duration,
    pub evidence_timeout: Duration,
    pub router_timeout: Duration,
    pub policy_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            inference_timeout: Duration::from_secs(30),
            evidence_timeout: Duration::from_secs(5),
            router_timeout: Duration::from_millis(100),
            policy_timeout: Duration::from_millis(50),
        }
    }
}

impl TimeoutConfig {
    /// Create a TimeoutConfig from the worker safety configuration (types.rs).
    ///
    /// This reads timeout values from the `[worker.safety]` section in cp.toml:
    /// - `inference_timeout_secs` -> inference_timeout
    /// - `evidence_timeout_secs` -> evidence_timeout
    /// - `router_timeout_ms` -> router_timeout
    /// - `policy_timeout_ms` -> policy_timeout
    pub fn from_config(config: &WorkerSafetyConfig) -> Self {
        Self {
            inference_timeout: Duration::from_secs(config.inference_timeout_secs),
            evidence_timeout: Duration::from_secs(config.evidence_timeout_secs),
            router_timeout: Duration::from_millis(config.router_timeout_ms),
            policy_timeout: Duration::from_millis(config.policy_timeout_ms),
        }
    }

    /// Create a TimeoutConfig from the effective config section.
    ///
    /// This reads timeout values from the `[worker.safety]` section in cp.toml
    /// via the effective configuration system:
    /// - `inference_timeout_secs` -> inference_timeout
    /// - `evidence_timeout_secs` -> evidence_timeout
    /// - `router_timeout_ms` -> router_timeout
    /// - `policy_timeout_ms` -> policy_timeout
    pub fn from_effective_section(section: &WorkerSafetySection) -> Self {
        Self {
            inference_timeout: Duration::from_secs(section.inference_timeout_secs),
            evidence_timeout: Duration::from_secs(section.evidence_timeout_secs),
            router_timeout: Duration::from_millis(section.router_timeout_ms),
            policy_timeout: Duration::from_millis(section.policy_timeout_ms),
        }
    }
}

/// Timeout wrapper for async operations
pub struct TimeoutWrapper {
    config: TimeoutConfig,
}

impl TimeoutWrapper {
    pub fn new(config: TimeoutConfig) -> Self {
        Self { config }
    }

    /// Wrap an async operation with timeout
    pub async fn with_timeout<F, T>(&self, operation: F, timeout_duration: Duration) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        timeout(timeout_duration, operation)
            .await
            .map_err(|_| AosError::Worker("Operation timeout".to_string()))?
    }

    /// Wrap inference with timeout
    pub async fn infer_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.inference_timeout)
            .await
    }

    /// Wrap evidence retrieval with timeout
    pub async fn evidence_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.evidence_timeout)
            .await
    }

    /// Wrap router operation with timeout
    pub async fn router_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.router_timeout)
            .await
    }

    /// Wrap policy check with timeout
    pub async fn policy_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.policy_timeout)
            .await
    }
}

/// Timeout event for telemetry
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimeoutEvent {
    pub operation_type: String,
    pub timeout_duration_ms: u64,
    pub actual_duration_ms: u64,
    pub timed_out: bool,
    pub timestamp: u64,
}

impl TimeoutEvent {
    pub fn new(
        operation_type: String,
        timeout_duration: Duration,
        actual_duration: Duration,
        timed_out: bool,
    ) -> Self {
        Self {
            operation_type,
            timeout_duration_ms: timeout_duration.as_millis() as u64,
            actual_duration_ms: actual_duration.as_millis() as u64,
            timed_out,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_timeout_wrapper() {
        let wrapper = TimeoutWrapper::new(TimeoutConfig::default());

        // Test successful operation
        let result = wrapper
            .with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok("success")
                },
                Duration::from_millis(100),
            )
            .await;

        assert!(result.is_ok());

        // Test timeout
        let result = wrapper
            .with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok("success")
                },
                Duration::from_millis(100),
            )
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AosError::Worker(msg) if msg.contains("timeout")));
    }
}
