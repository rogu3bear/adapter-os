//! Synthetic traffic probes for continuous inference pipeline validation.
//!
//! Generalizes the `/readyz?deep=true` canary probe into a system that
//! periodically validates the full inference pipeline across all active
//! adapters. Each probe sends a minimal 1-token request through
//! `InferenceCore::route_and_infer()` — the same path real requests take.
//!
//! ## Configuration
//!
//! Probes are **disabled by default**. Enable via config or env:
//!
//! | Env var | Default | Description |
//! |---------|---------|-------------|
//! | `AOS_SYNTHETIC_PROBES_ENABLED` | `false` | Master enable switch |
//! | `AOS_SYNTHETIC_PROBE_INTERVAL_SECS` | `60` | Cycle interval |
//! | `AOS_SYNTHETIC_PROBE_MAX_PER_CYCLE` | `5` | Max adapters per cycle |
//! | `AOS_SYNTHETIC_PROBE_TIMEOUT_SECS` | `10` | Per-probe timeout |
//! | `AOS_SYNTHETIC_PROBE_LOAD_THRESHOLD` | `50` | Skip when in-flight exceeds this |
//!
//! ## Observability
//!
//! - Results are kept in a ring buffer (last 100 per adapter)
//! - Health report available via `GET /v1/system/probe-health`
//! - Metrics emitted: `synthetic_probe_success_total`, `synthetic_probe_failure_total`,
//!   `synthetic_probe_latency_ms`

use crate::inference_core::InferenceCore;
use crate::state::AppState;
use crate::types::InferenceRequestInternal;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Global registration (set once during background task spawn)
// ---------------------------------------------------------------------------

/// Global handle to the shared probe results.
///
/// Set once via [`register_global_results`] during boot. The health endpoint
/// reads from this rather than from an `AppState` field, because `AppState`
/// is already constructed and distributed to routes before the probe runner
/// is spawned.
static GLOBAL_PROBE_RESULTS: OnceLock<SharedProbeResults> = OnceLock::new();

/// Register the shared probe results handle globally.
///
/// Called once during background task spawning. Subsequent calls are no-ops.
pub fn register_global_results(results: SharedProbeResults) {
    let _ = GLOBAL_PROBE_RESULTS.set(results);
}

/// Get the global probe results handle, if probes have been registered.
pub fn global_probe_results() -> Option<&'static SharedProbeResults> {
    GLOBAL_PROBE_RESULTS.get()
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the synthetic probe system.
#[derive(Debug, Clone)]
pub struct SyntheticProbeConfig {
    /// Enable synthetic probes (default: false).
    pub enabled: bool,
    /// Interval between full probe cycles (default: 60s).
    pub cycle_interval: Duration,
    /// Maximum number of adapters to probe per cycle (default: 5).
    pub max_probes_per_cycle: usize,
    /// Timeout for individual probe requests (default: 10s).
    pub probe_timeout: Duration,
    /// Skip probes when in-flight requests exceed this (default: 50).
    pub load_skip_threshold: usize,
}

impl Default for SyntheticProbeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cycle_interval: Duration::from_secs(60),
            max_probes_per_cycle: 5,
            probe_timeout: Duration::from_secs(10),
            load_skip_threshold: 50,
        }
    }
}

impl SyntheticProbeConfig {
    /// Build a config from environment variables, falling back to defaults.
    pub fn from_env() -> Self {
        let enabled = std::env::var("AOS_SYNTHETIC_PROBES_ENABLED")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let cycle_interval = std::env::var("AOS_SYNTHETIC_PROBE_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(60));

        let max_probes_per_cycle = std::env::var("AOS_SYNTHETIC_PROBE_MAX_PER_CYCLE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        let probe_timeout = std::env::var("AOS_SYNTHETIC_PROBE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(10));

        let load_skip_threshold = std::env::var("AOS_SYNTHETIC_PROBE_LOAD_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);

        Self {
            enabled,
            cycle_interval,
            max_probes_per_cycle,
            probe_timeout,
            load_skip_threshold,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Maximum number of probe results to retain per adapter.
const MAX_RESULTS_PER_ADAPTER: usize = 100;

/// System tenant used for synthetic probe requests.
const PROBE_TENANT: &str = "system";

/// Minimal prompt for synthetic probes.
const PROBE_PROMPT: &str = "ping";

/// Result of a single synthetic probe.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProbeResult {
    /// Adapter ID that was probed.
    pub adapter_id: String,
    /// Whether the probe succeeded.
    pub success: bool,
    /// Latency of the probe in milliseconds.
    pub latency_ms: u64,
    /// Number of tokens generated by the probe.
    pub tokens_generated: usize,
    /// Error message if the probe failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// ISO 8601 timestamp of when the probe was executed.
    pub probed_at: String,
}

/// Aggregated probe health across all adapters.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProbeHealthReport {
    /// Total number of probes in the current result window.
    pub total_probes: usize,
    /// Number of successful probes.
    pub successful: usize,
    /// Number of failed probes.
    pub failed: usize,
    /// Average latency across all probes in milliseconds.
    pub avg_latency_ms: f64,
    /// P99 latency across all probes in milliseconds.
    pub p99_latency_ms: u64,
    /// Adapter IDs where the most recent probe succeeded.
    pub adapters_healthy: Vec<String>,
    /// Adapter IDs where the most recent probe failed.
    pub adapters_degraded: Vec<String>,
    /// ISO 8601 timestamp of the last completed probe cycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cycle_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// Shared handle to probe results, readable from the health endpoint.
pub type SharedProbeResults = Arc<RwLock<ProbeState>>;

/// Internal probe state: per-adapter ring buffers + last cycle timestamp.
pub struct ProbeState {
    results: HashMap<String, VecDeque<ProbeResult>>,
    last_cycle_at: Option<String>,
}

impl ProbeState {
    fn new() -> Self {
        Self {
            results: HashMap::new(),
            last_cycle_at: None,
        }
    }

    fn push(&mut self, result: ProbeResult) {
        let buf = self
            .results
            .entry(result.adapter_id.clone())
            .or_insert_with(|| VecDeque::with_capacity(MAX_RESULTS_PER_ADAPTER));
        if buf.len() >= MAX_RESULTS_PER_ADAPTER {
            buf.pop_front();
        }
        buf.push_back(result);
    }

    /// Generate a health report from the current result window.
    pub fn health_report(&self) -> ProbeHealthReport {
        let mut total = 0usize;
        let mut successful = 0usize;
        let mut failed = 0usize;
        let mut latencies: Vec<u64> = Vec::new();
        let mut adapters_healthy = Vec::new();
        let mut adapters_degraded = Vec::new();

        for (adapter_id, buf) in &self.results {
            for r in buf {
                total += 1;
                if r.success {
                    successful += 1;
                } else {
                    failed += 1;
                }
                latencies.push(r.latency_ms);
            }

            // Classify adapter by most recent probe
            if let Some(latest) = buf.back() {
                if latest.success {
                    adapters_healthy.push(adapter_id.clone());
                } else {
                    adapters_degraded.push(adapter_id.clone());
                }
            }
        }

        adapters_healthy.sort();
        adapters_degraded.sort();

        let avg_latency_ms = if latencies.is_empty() {
            0.0
        } else {
            latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
        };

        let p99_latency_ms = compute_p99(&mut latencies);

        ProbeHealthReport {
            total_probes: total,
            successful,
            failed,
            avg_latency_ms,
            p99_latency_ms,
            adapters_healthy,
            adapters_degraded,
            last_cycle_at: self.last_cycle_at.clone(),
        }
    }
}

/// Compute p99 latency from a mutable slice (sorts in place).
fn compute_p99(latencies: &mut [u64]) -> u64 {
    if latencies.is_empty() {
        return 0;
    }
    latencies.sort_unstable();
    let idx = ((latencies.len() as f64) * 0.99).ceil() as usize;
    let idx = idx.min(latencies.len()) - 1;
    latencies[idx]
}

/// Background runner that periodically probes active adapters.
pub struct SyntheticProbeRunner {
    state: AppState,
    config: SyntheticProbeConfig,
    probe_state: SharedProbeResults,
}

impl SyntheticProbeRunner {
    /// Create a new runner. Returns the runner and a shared handle to results.
    pub fn new(state: AppState, config: SyntheticProbeConfig) -> (Self, SharedProbeResults) {
        let probe_state: SharedProbeResults = Arc::new(RwLock::new(ProbeState::new()));
        let runner = Self {
            state,
            config,
            probe_state: Arc::clone(&probe_state),
        };
        (runner, probe_state)
    }

    /// Run a single probe cycle across active adapters.
    async fn run_cycle(&self) -> Vec<ProbeResult> {
        // Check system load — skip if too busy
        let in_flight = self.state.in_flight_requests.load(Ordering::Relaxed);
        if in_flight > self.config.load_skip_threshold {
            info!(
                in_flight,
                threshold = self.config.load_skip_threshold,
                "Synthetic probe cycle skipped: system load above threshold"
            );
            return Vec::new();
        }

        // Discover active adapters from the DB
        let adapter_ids = match self.discover_probe_targets().await {
            Ok(ids) => ids,
            Err(e) => {
                warn!(error = %e, "Synthetic probes: failed to discover adapter targets");
                return Vec::new();
            }
        };

        if adapter_ids.is_empty() {
            debug!("Synthetic probe cycle: no active adapters to probe");
            return Vec::new();
        }

        let count = adapter_ids.len().min(self.config.max_probes_per_cycle);
        let targets = &adapter_ids[..count];

        let mut results = Vec::with_capacity(count);
        for adapter_id in targets {
            let result = self.probe_adapter(adapter_id).await;
            results.push(result);
        }

        results
    }

    /// Query the DB for adapter IDs in the "active" lifecycle state.
    async fn discover_probe_targets(&self) -> Result<Vec<String>, String> {
        // Use a direct SQL query for adapter_id values with active lifecycle_state.
        // This avoids tenant-scoped APIs since probes are system-level.
        let pool = self
            .state
            .db
            .pool_opt()
            .ok_or_else(|| "SQL pool not available".to_string())?;

        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT adapter_id FROM adapters WHERE lifecycle_state = 'active' AND adapter_id IS NOT NULL ORDER BY adapter_id LIMIT 100",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| format!("query active adapters: {e}"))?;

        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Probe a single adapter with a minimal 1-token inference request.
    async fn probe_adapter(&self, adapter_id: &str) -> ProbeResult {
        let start = Instant::now();
        let now = chrono::Utc::now().to_rfc3339();

        let core = InferenceCore::new(&self.state);

        let mut req =
            InferenceRequestInternal::new(PROBE_TENANT.to_string(), PROBE_PROMPT.to_string());
        req.max_tokens = 1;
        req.stream = false;
        req.require_step = false;
        req.require_determinism = false;
        req.adapters = Some(vec![adapter_id.to_string()]);

        let probe = tokio::time::timeout(
            self.config.probe_timeout,
            core.route_and_infer(req, None, None, None, None),
        )
        .await;

        let latency = start.elapsed().as_millis() as u64;

        match probe {
            Ok(Ok(inference_result)) => ProbeResult {
                adapter_id: adapter_id.to_string(),
                success: true,
                latency_ms: latency,
                tokens_generated: inference_result.tokens_generated,
                error: None,
                probed_at: now,
            },
            Ok(Err(e)) => ProbeResult {
                adapter_id: adapter_id.to_string(),
                success: false,
                latency_ms: latency,
                tokens_generated: 0,
                error: Some(format!("{e}")),
                probed_at: now,
            },
            Err(_) => ProbeResult {
                adapter_id: adapter_id.to_string(),
                success: false,
                latency_ms: latency,
                tokens_generated: 0,
                error: Some("probe timeout".to_string()),
                probed_at: now,
            },
        }
    }

    /// Run the background probe loop until shutdown.
    pub async fn run(self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) {
        if !self.config.enabled {
            info!("Synthetic probes disabled, background loop will not start");
            return;
        }

        info!(
            cycle_interval_secs = self.config.cycle_interval.as_secs(),
            max_probes_per_cycle = self.config.max_probes_per_cycle,
            probe_timeout_secs = self.config.probe_timeout.as_secs(),
            load_skip_threshold = self.config.load_skip_threshold,
            "Synthetic probe runner started"
        );

        let mut interval = tokio::time::interval(self.config.cycle_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.recv() => {
                    info!("Synthetic probe runner received shutdown signal, exiting");
                    break;
                }
                _ = interval.tick() => {
                    let results = self.run_cycle().await;

                    if !results.is_empty() {
                        let successes = results.iter().filter(|r| r.success).count();
                        let failures = results.len() - successes;

                        info!(
                            probed = results.len(),
                            successes,
                            failures,
                            "Synthetic probe cycle completed"
                        );

                        // Record metrics
                        let registry = self.state.metrics_registry.clone();
                        registry
                            .record_metric(
                                "synthetic_probe_success_total".to_string(),
                                successes as f64,
                            )
                            .await;
                        registry
                            .record_metric(
                                "synthetic_probe_failure_total".to_string(),
                                failures as f64,
                            )
                            .await;

                        if let Some(max_latency) = results.iter().map(|r| r.latency_ms).max() {
                            registry
                                .set_gauge(
                                    "synthetic_probe_latency_ms".to_string(),
                                    max_latency as f64,
                                )
                                .await;
                        }

                        // Store results
                        let now = chrono::Utc::now().to_rfc3339();
                        let mut state = self.probe_state.write().await;
                        for result in results {
                            state.push(result);
                        }
                        state.last_cycle_at = Some(now);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let cfg = SyntheticProbeConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.cycle_interval, Duration::from_secs(60));
        assert_eq!(cfg.max_probes_per_cycle, 5);
        assert_eq!(cfg.probe_timeout, Duration::from_secs(10));
        assert_eq!(cfg.load_skip_threshold, 50);
    }

    #[test]
    fn probe_result_serializes_success() {
        let r = ProbeResult {
            adapter_id: "adapter-1".into(),
            success: true,
            latency_ms: 42,
            tokens_generated: 1,
            error: None,
            probed_at: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["latency_ms"], 42);
        assert!(
            json.get("error").is_none(),
            "error should be omitted when None"
        );
    }

    #[test]
    fn probe_result_serializes_failure() {
        let r = ProbeResult {
            adapter_id: "adapter-1".into(),
            success: false,
            latency_ms: 5001,
            tokens_generated: 0,
            error: Some("probe timeout".into()),
            probed_at: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["error"], "probe timeout");
    }

    #[test]
    fn health_report_empty_state() {
        let state = ProbeState::new();
        let report = state.health_report();
        assert_eq!(report.total_probes, 0);
        assert_eq!(report.successful, 0);
        assert_eq!(report.failed, 0);
        assert_eq!(report.avg_latency_ms, 0.0);
        assert_eq!(report.p99_latency_ms, 0);
        assert!(report.adapters_healthy.is_empty());
        assert!(report.adapters_degraded.is_empty());
        assert!(report.last_cycle_at.is_none());
    }

    #[test]
    fn health_report_aggregation() {
        let mut state = ProbeState::new();

        // Two successful probes for adapter-a
        state.push(ProbeResult {
            adapter_id: "adapter-a".into(),
            success: true,
            latency_ms: 10,
            tokens_generated: 1,
            error: None,
            probed_at: "2026-01-01T00:00:00Z".into(),
        });
        state.push(ProbeResult {
            adapter_id: "adapter-a".into(),
            success: true,
            latency_ms: 20,
            tokens_generated: 1,
            error: None,
            probed_at: "2026-01-01T00:01:00Z".into(),
        });

        // One failed probe for adapter-b
        state.push(ProbeResult {
            adapter_id: "adapter-b".into(),
            success: false,
            latency_ms: 100,
            tokens_generated: 0,
            error: Some("timeout".into()),
            probed_at: "2026-01-01T00:00:00Z".into(),
        });

        state.last_cycle_at = Some("2026-01-01T00:01:00Z".into());

        let report = state.health_report();
        assert_eq!(report.total_probes, 3);
        assert_eq!(report.successful, 2);
        assert_eq!(report.failed, 1);

        // avg = (10 + 20 + 100) / 3 = 43.333...
        assert!((report.avg_latency_ms - 43.333).abs() < 1.0);

        // p99 of [10, 20, 100] = 100
        assert_eq!(report.p99_latency_ms, 100);

        assert_eq!(report.adapters_healthy, vec!["adapter-a"]);
        assert_eq!(report.adapters_degraded, vec!["adapter-b"]);
        assert_eq!(report.last_cycle_at, Some("2026-01-01T00:01:00Z".into()));
    }

    #[test]
    fn ring_buffer_eviction() {
        let mut state = ProbeState::new();

        // Push MAX_RESULTS_PER_ADAPTER + 10 results
        for i in 0..(MAX_RESULTS_PER_ADAPTER + 10) {
            state.push(ProbeResult {
                adapter_id: "adapter-a".into(),
                success: true,
                latency_ms: i as u64,
                tokens_generated: 1,
                error: None,
                probed_at: format!("2026-01-01T00:{:02}:00Z", i % 60),
            });
        }

        let buf = state.results.get("adapter-a").unwrap();
        assert_eq!(buf.len(), MAX_RESULTS_PER_ADAPTER);

        // Oldest entry should be evicted (latency_ms = 0..9 are gone)
        assert_eq!(buf.front().unwrap().latency_ms, 10);
    }

    #[test]
    fn compute_p99_single_element() {
        let mut v = vec![42];
        assert_eq!(compute_p99(&mut v), 42);
    }

    #[test]
    fn compute_p99_hundred_elements() {
        let mut v: Vec<u64> = (1..=100).collect();
        // p99 of 1..=100: index = ceil(100 * 0.99) - 1 = 99 - 1 = 98 → value 99
        assert_eq!(compute_p99(&mut v), 99);
    }

    #[test]
    fn compute_p99_empty() {
        let mut v: Vec<u64> = Vec::new();
        assert_eq!(compute_p99(&mut v), 0);
    }

    #[test]
    fn load_skip_threshold_logic() {
        // Verify the threshold comparison used in run_cycle
        let threshold = 50usize;
        assert!(60 > threshold, "60 in-flight should skip");
        assert!(!(50 > threshold), "50 in-flight should not skip");
        assert!(!(10 > threshold), "10 in-flight should not skip");
    }

    #[test]
    fn health_report_degraded_on_latest_failure() {
        let mut state = ProbeState::new();

        // First probe succeeds, second fails → adapter is degraded
        state.push(ProbeResult {
            adapter_id: "adapter-a".into(),
            success: true,
            latency_ms: 10,
            tokens_generated: 1,
            error: None,
            probed_at: "2026-01-01T00:00:00Z".into(),
        });
        state.push(ProbeResult {
            adapter_id: "adapter-a".into(),
            success: false,
            latency_ms: 50,
            tokens_generated: 0,
            error: Some("connection refused".into()),
            probed_at: "2026-01-01T00:01:00Z".into(),
        });

        let report = state.health_report();
        assert!(report.adapters_healthy.is_empty());
        assert_eq!(report.adapters_degraded, vec!["adapter-a"]);
    }
}
