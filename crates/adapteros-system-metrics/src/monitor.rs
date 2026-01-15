#![allow(unused_variables)]

//! Continuous system monitoring pipeline
//! Provides continuous monitoring of system metrics with telemetry integration,
//! policy enforcement, and alerting capabilities.
//!
//! # Determinism Guarantees
//!
//! SystemMonitor implements deterministic sampling using HKDF-derived seeds to ensure
//! reproducible telemetry behavior across runs. This enables audit trail reconstruction,
//! deterministic replay for debugging, and compliance with adapterOS Policy #2 (Determinism).
//!
//! **Key Guarantees:**
//!
//! 1. **Deterministic Sampling**: All sampling decisions (`should_sample()`) use a ChaCha20 RNG
//!    seeded via HKDF-SHA256 from a global seed with domain label "system_metrics_sampling"
//!
//! 2. **Reproducible Telemetry**: Given the same global seed, the same metrics will be sampled
//!    across multiple runs, enabling identical telemetry bundle generation
//!
//! 3. **Domain Separation**: HKDF domain label prevents seed reuse across subsystems, ensuring
//!    independent randomness streams for router, dropout, training, and metrics sampling
//!
//! 4. **Audit Trail Support**: Deterministic sampling enables telemetry bundle replay for
//!    compliance auditing and policy violation investigation
//!
//! **Global Seed Requirement:**
//!
//! SystemMonitor requires a `global_seed: &B3Hash` parameter at construction time. This seed
//! should be:
//! - Consistent across server restarts for reproducible telemetry
//! - Stored securely (e.g., in configuration or encrypted storage)
//! - Rotated periodically for security (will change sampling patterns)
//!
//! **Integration Example:**
//!
//! ```rust,ignore
//! use adapteros_core::B3Hash;
//! use adapteros_system_metrics::SystemMonitor;
//!
//! // Derive or load global seed from secure storage
//! let global_seed = B3Hash::hash(b"production_seed_v1");
//!
//! let monitor = SystemMonitor::new(telemetry_writer, config, &global_seed);
//! ```
//!
//! **Performance Characteristics:**
//!
//! - Metrics collection: ~46ms per call (dominated by OS system calls via sysinfo crate)
//! - Sampling overhead: <1µs (ChaCha20 RNG generation)
//! - Recommended collection interval: 10-60 seconds in production
//!
//! See `crates/adapteros-system-metrics/ARCHITECTURE.md` for detailed integration guide.

use crate::policy::SystemMetricsPolicy;
use crate::{MetricsConfig, SystemMetricsCollector};
use adapteros_core::{derive_seed, B3Hash, Result};
use adapteros_telemetry::{SecurityEvent, TelemetryWriter};
use parking_lot::Mutex;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// System metrics monitor
pub struct SystemMonitor {
    collector: SystemMetricsCollector,
    policy: SystemMetricsPolicy,
    telemetry_writer: Arc<TelemetryWriter>,
    config: MetricsConfig,
    last_collection: SystemTime,
    violation_count: u32,
    /// Deterministic RNG for sampling decisions (HKDF-derived seed)
    sampling_rng: Mutex<ChaCha20Rng>,
}

impl SystemMonitor {
    /// Create a new system monitor with deterministic sampling
    ///
    /// # Arguments
    ///
    /// * `telemetry_writer` - Writer for telemetry events and policy violations
    /// * `config` - Metrics configuration including thresholds and sampling rate
    /// * `global_seed` - Global BLAKE3 seed for deterministic sampling
    ///
    /// # Deterministic Sampling
    ///
    /// The `global_seed` parameter enables reproducible sampling decisions via HKDF-SHA256:
    ///
    /// 1. Domain-specific seed derived: `HKDF(global_seed, "system_metrics_sampling")`
    /// 2. ChaCha20 RNG initialized with derived seed
    /// 3. All `should_sample()` calls use this deterministic RNG
    ///
    /// **Why this matters:**
    /// - Enables telemetry bundle replay with identical sampling behavior
    /// - Supports audit trail reconstruction for compliance
    /// - Complies with adapterOS Policy #2 (Determinism)
    /// - Prevents seed reuse via domain separation label
    ///
    /// **Global Seed Requirements:**
    /// - Must be a 32-byte BLAKE3 hash (validated at compile time via type system)
    /// - Should be persistent across server restarts for reproducible telemetry
    /// - Should be stored securely (config file, environment, or secrets manager)
    /// - Rotating the seed will change sampling patterns (expected behavior)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use adapteros_core::B3Hash;
    /// use adapteros_system_metrics::{SystemMonitor, MetricsConfig};
    ///
    /// // Load or derive global seed from configuration
    /// let global_seed = B3Hash::hash(config.seed_material.as_bytes());
    ///
    /// let monitor = SystemMonitor::new(
    ///     telemetry_writer,
    ///     MetricsConfig::default(),
    ///     &global_seed
    /// );
    /// ```
    ///
    /// # Domain Label
    ///
    /// Uses HKDF label: `"system_metrics_sampling"`
    ///
    /// This label must be unique across all adapterOS subsystems to prevent
    /// correlated randomness. Other labels in use:
    /// - `"router"` - K-sparse adapter selection
    /// - `"dropout"` - LoRA dropout masks
    /// - `"sampling"` - Token sampling
    /// - `"lora_trainer"` - Weight initialization
    pub fn new(
        telemetry_writer: Arc<TelemetryWriter>,
        config: MetricsConfig,
        global_seed: &B3Hash,
    ) -> Self {
        let thresholds = config.thresholds.clone();
        let policy = SystemMetricsPolicy::new(thresholds);

        // Derive domain-specific seed for system metrics sampling
        let sampling_seed = derive_seed(global_seed, "system_metrics_sampling");

        // Initialize deterministic RNG
        let rng = ChaCha20Rng::from_seed(sampling_seed);

        Self {
            collector: SystemMetricsCollector::new(),
            policy,
            telemetry_writer,
            config,
            last_collection: SystemTime::now(),
            violation_count: 0,
            sampling_rng: Mutex::new(rng),
        }
    }

    /// Start continuous monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!(
            "Starting system metrics monitoring with interval: {} secs",
            self.config.collection_interval_secs
        );

        let mut interval = interval(Duration::from_secs(self.config.collection_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.collect_and_process_metrics().await {
                error!("Failed to collect system metrics: {}", e);
                self.violation_count += 1;

                // Log the error as a security event
                if let Err(telemetry_err) =
                    self.telemetry_writer
                        .log_security_event(SecurityEvent::PolicyViolation {
                            policy: "system_monitoring".to_string(),
                            violation_type: "collection_failure".to_string(),
                            details: e.to_string(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        })
                {
                    error!("Failed to log monitoring error: {}", telemetry_err);
                }
            } else {
                // Reset violation count on successful collection
                self.violation_count = 0;
            }

            // Check if we should stop monitoring due to too many violations
            if self.violation_count >= 10 {
                error!(
                    "Too many monitoring violations ({}), stopping monitor",
                    self.violation_count
                );
                break;
            }
        }

        Ok(())
    }

    /// Collect and process system metrics
    async fn collect_and_process_metrics(&mut self) -> Result<()> {
        let metrics = self.collector.collect_metrics();

        // Log metrics to telemetry if sampling criteria met
        if self.should_sample() {
            let event = crate::telemetry::SystemMetricsEvent::from_metrics(&metrics);
            self.telemetry_writer.log("system.metrics", event)?;
        }

        // Check policy thresholds
        if let Err(e) = self.policy.check_thresholds(&metrics) {
            warn!("Policy threshold violation: {}", e);

            // Log threshold violation
            let violation = crate::telemetry::ThresholdViolationEvent::new(
                "system_metrics".to_string(),
                0.0, // Would be set to actual violating metric value
                0.0, // Would be set to actual threshold value
                "warning".to_string(),
            );

            self.telemetry_writer
                .log("system.threshold_violation", violation)?;

            // Log as security event if critical
            if self.policy.get_health_status(&metrics)
                == crate::policy::SystemHealthStatus::Critical
            {
                self.telemetry_writer
                    .log_security_event(SecurityEvent::PolicyViolation {
                        policy: "performance".to_string(),
                        violation_type: "threshold_exceeded".to_string(),
                        details: e.to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    })?;
            }
        }

        // Check memory headroom policy
        let headroom = self.collector.headroom_pct();
        if let Err(e) = self.policy.check_memory_headroom(headroom) {
            warn!("Memory headroom violation: {}", e);

            self.telemetry_writer
                .log_security_event(SecurityEvent::PolicyViolation {
                    policy: "memory".to_string(),
                    violation_type: "insufficient_headroom".to_string(),
                    details: e.to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })?;
        }

        self.last_collection = SystemTime::now();
        debug!("System metrics collected successfully");

        Ok(())
    }

    /// Check if we should sample this collection
    ///
    /// Uses deterministic HKDF-derived RNG for reproducible sampling decisions.
    ///
    /// # Why Deterministic Sampling?
    ///
    /// Deterministic sampling enables several critical capabilities:
    ///
    /// 1. **Audit Trail Reconstruction**: Regulators or compliance teams can replay
    ///    telemetry bundles with identical sampling behavior to verify system state
    ///    at specific points in time.
    ///
    /// 2. **Deterministic Replay for Debugging**: When investigating incidents,
    ///    engineers can replay execution with the same seed to reproduce exact
    ///    telemetry sampling patterns and identify issues.
    ///
    /// 3. **Policy Compliance**: Satisfies adapterOS Policy #2 (Determinism) which
    ///    requires all randomness to be seeded and reproducible for regulatory
    ///    compliance in industries like healthcare and finance.
    ///
    /// 4. **Telemetry Bundle Integrity**: Given the same global seed, telemetry
    ///    bundles will have bit-identical sampling decisions, enabling cryptographic
    ///    verification via BLAKE3 hashing.
    ///
    /// # Implementation Details
    ///
    /// - RNG: ChaCha20 (cryptographically secure, deterministic)
    /// - Seeding: HKDF-SHA256 with label "system_metrics_sampling"
    /// - Thread Safety: Mutex-protected for concurrent access
    /// - Sampling Rate: Configured via `config.sampling_rate` (0.0 to 1.0)
    ///
    /// # Performance
    ///
    /// - RNG generation: <1µs (negligible overhead)
    /// - Mutex contention: Minimal (sampling happens once per collection interval)
    ///
    /// # Example Behavior
    ///
    /// ```ignore
    /// // With sampling_rate = 0.5 and seed "abc123":
    /// should_sample() // → true  (deterministic based on seed)
    /// should_sample() // → false (next value in RNG sequence)
    /// should_sample() // → true  (predictable pattern)
    ///
    /// // Same seed, same pattern every time:
    /// // Run 1: [true, false, true, ...]
    /// // Run 2: [true, false, true, ...]  ← identical
    /// ```
    fn should_sample(&self) -> bool {
        use rand::Rng;
        let mut rng = self.sampling_rng.lock();
        rng.gen::<f32>() < self.config.sampling_rate
    }

    /// Get current system health status
    pub fn get_health_status(&mut self) -> crate::policy::SystemHealthStatus {
        let metrics = self.collector.collect_metrics();
        self.policy.get_health_status(&metrics)
    }

    /// Get current metrics
    pub fn get_current_metrics(&mut self) -> crate::SystemMetrics {
        self.collector.collect_metrics()
    }

    /// Get violation count
    pub fn get_violation_count(&self) -> u32 {
        self.violation_count
    }

    /// Reset violation count
    pub fn reset_violation_count(&mut self) {
        self.violation_count = 0;
    }
}

/// System monitoring service
pub struct SystemMonitoringService {
    monitor: Option<SystemMonitor>,
    config: MetricsConfig,
}

impl SystemMonitoringService {
    /// Create a new monitoring service
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            monitor: None,
            config,
        }
    }

    /// Start the monitoring service
    ///
    /// # Arguments
    /// * `telemetry_writer` - Writer for telemetry events
    /// * `global_seed` - Global seed for deterministic sampling
    pub async fn start(
        &mut self,
        telemetry_writer: Arc<TelemetryWriter>,
        global_seed: &B3Hash,
    ) -> Result<()> {
        let mut monitor = SystemMonitor::new(telemetry_writer, self.config.clone(), global_seed);

        info!("Starting system monitoring service");
        monitor.start_monitoring().await?;

        Ok(())
    }

    /// Stop the monitoring service
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(monitor) = &mut self.monitor {
            info!("Stopping system monitoring service");
            // The monitor will stop when the start_monitoring loop exits
        }
        Ok(())
    }

    /// Get service status
    pub fn is_running(&self) -> bool {
        self.monitor.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::TelemetryWriter;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn test_telemetry_writer() -> (TempDir, Arc<TelemetryWriter>) {
        let dir = TempDir::new_in(".").expect("test telemetry dir");
        let writer = TelemetryWriter::new(dir.path(), 1000, 1024 * 1024)
            .expect("Test telemetry writer creation should succeed");
        (dir, Arc::new(writer))
    }

    #[tokio::test]
    async fn test_monitor_creation() {
        let config = MetricsConfig::default();
        let (_dir, telemetry_writer) = test_telemetry_writer();
        let test_seed = B3Hash::hash(b"test_seed");
        let monitor = SystemMonitor::new(telemetry_writer, config, &test_seed);

        assert_eq!(monitor.get_violation_count(), 0);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = MetricsConfig::default();
        let (_dir, telemetry_writer) = test_telemetry_writer();
        let test_seed = B3Hash::hash(b"test_seed");
        let mut monitor = SystemMonitor::new(telemetry_writer, config, &test_seed);

        let metrics = monitor.get_current_metrics();
        assert!(metrics.cpu_usage >= 0.0 && metrics.cpu_usage <= 100.0);
        assert!(metrics.memory_usage >= 0.0 && metrics.memory_usage <= 100.0);
    }

    #[tokio::test]
    async fn test_health_status() {
        let config = MetricsConfig::default();
        let (_dir, telemetry_writer) = test_telemetry_writer();
        let test_seed = B3Hash::hash(b"test_seed");
        let mut monitor = SystemMonitor::new(telemetry_writer, config, &test_seed);

        let status = monitor.get_health_status();
        // Status should be one of the valid health statuses
        assert!(matches!(
            status,
            crate::policy::SystemHealthStatus::Healthy
                | crate::policy::SystemHealthStatus::Warning
                | crate::policy::SystemHealthStatus::Critical
        ));
    }

    #[tokio::test]
    async fn test_deterministic_sampling() {
        // Test that same seed produces same sampling decisions
        let config = MetricsConfig {
            sampling_rate: 0.5, // 50% sampling rate for variability
            ..Default::default()
        };

        let seed = B3Hash::hash(b"determinism_test_seed");
        let (_dir1, telemetry_writer1) = test_telemetry_writer();
        let (_dir2, telemetry_writer2) = test_telemetry_writer();

        let monitor1 = SystemMonitor::new(telemetry_writer1, config.clone(), &seed);
        let monitor2 = SystemMonitor::new(telemetry_writer2, config.clone(), &seed);

        // Generate 100 sampling decisions from each monitor
        let mut decisions1 = Vec::new();
        let mut decisions2 = Vec::new();

        for _ in 0..100 {
            decisions1.push(monitor1.should_sample());
            decisions2.push(monitor2.should_sample());
        }

        // Same seed should produce identical sampling decisions
        assert_eq!(
            decisions1, decisions2,
            "Same seed should produce identical sampling decisions"
        );

        // Verify that we got some true and some false (with 50% rate, highly unlikely to be all same)
        let true_count = decisions1.iter().filter(|&&x| x).count();
        assert!(
            true_count > 20 && true_count < 80,
            "With 50% sampling rate, expect roughly half true: got {}/100",
            true_count
        );
    }

    #[tokio::test]
    async fn test_different_seeds_different_sampling() {
        // Test that different seeds produce different sampling decisions
        let config = MetricsConfig {
            sampling_rate: 0.5,
            ..Default::default()
        };

        let seed1 = B3Hash::hash(b"seed_1");
        let seed2 = B3Hash::hash(b"seed_2");

        let (_dir1, telemetry_writer1) = test_telemetry_writer();
        let (_dir2, telemetry_writer2) = test_telemetry_writer();

        let monitor1 = SystemMonitor::new(telemetry_writer1, config.clone(), &seed1);
        let monitor2 = SystemMonitor::new(telemetry_writer2, config.clone(), &seed2);

        // Generate sampling decisions
        let mut decisions1 = Vec::new();
        let mut decisions2 = Vec::new();

        for _ in 0..100 {
            decisions1.push(monitor1.should_sample());
            decisions2.push(monitor2.should_sample());
        }

        // Different seeds should produce different decisions (highly unlikely to be identical)
        assert_ne!(
            decisions1, decisions2,
            "Different seeds should produce different sampling decisions"
        );
    }
}
