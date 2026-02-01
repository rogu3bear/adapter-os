//! Unified Worker Selection
//!
//! This module provides `WorkerSelector` - a unified component that combines
//! database query, capability filtering, and health checking in a single pass.
//!
//! ## Design Goals
//!
//! 1. **Single Pass Selection**: Combines DB query + capability filter + health score
//!    instead of separate sequential operations.
//! 2. **Pre-indexed Capabilities**: Parse capabilities JSON once during worker fetch,
//!    not repeatedly during filtering.
//! 3. **Integrated Health Scoring**: Health scores are computed inline with filtering.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use adapteros_server_api::worker_selector::{WorkerSelector, WorkerRequirements};
//!
//! let selector = WorkerSelector::new(db, health_monitor);
//! let requirements = WorkerRequirements {
//!     tenant_id: "tenant-123".to_string(),
//!     manifest_hash: "abc123".to_string(),
//!     capabilities: RequiredCapabilities::for_streaming(),
//!     prefer_cache_hit: true,
//! };
//! let worker = selector.select_best_worker(&requirements).await?;
//! ```

use crate::types::InferenceRequestInternal;
use crate::worker_capabilities::{
    capability_reasons, normalize_worker_capabilities, RequiredModes, WorkerCapabilityExclusion,
};
use crate::worker_health::{WorkerHealthMonitor, WorkerHealthStatus};
use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_core::BackendKind;
use adapteros_db::workers::WorkerWithBinding;
use adapteros_db::Db;
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Required capabilities for worker selection.
///
/// This struct encapsulates all capability requirements that a worker must satisfy.
#[derive(Debug, Clone, Serialize)]
pub struct RequiredCapabilities {
    /// Required execution modes (step, bulk, logits, streaming)
    pub modes: RequiredModes,
    /// Required backend type (if any)
    pub backend: Option<BackendKind>,
    /// Whether determinism is required
    pub require_determinism: bool,
}

impl RequiredCapabilities {
    /// Create requirements for a streaming request.
    pub fn for_streaming() -> Self {
        Self {
            modes: RequiredModes::for_request(true, true),
            backend: None,
            require_determinism: false,
        }
    }

    /// Create requirements for a bulk (non-streaming) request.
    pub fn for_bulk() -> Self {
        Self {
            modes: RequiredModes::for_request(false, false),
            backend: None,
            require_determinism: false,
        }
    }

    /// Create requirements from an inference request.
    pub fn from_request(request: &InferenceRequestInternal) -> Self {
        let modes = RequiredModes::from_request(request);
        let backend = if request.allow_fallback {
            None
        } else {
            match request.backend_profile {
                Some(BackendKind::Auto) | None => None,
                Some(kind) => Some(kind),
            }
        };

        Self {
            modes,
            backend,
            require_determinism: request.require_determinism,
        }
    }

    /// Create empty requirements (any worker is acceptable).
    pub fn any() -> Self {
        Self {
            modes: RequiredModes {
                require_step: false,
                require_bulk: false,
                require_logits: false,
                require_streaming: false,
            },
            backend: None,
            require_determinism: false,
        }
    }
}

/// Requirements for worker selection.
///
/// Encapsulates all constraints for finding a compatible worker.
#[derive(Debug, Clone)]
pub struct WorkerRequirements {
    /// Tenant ID for isolation
    pub tenant_id: String,
    /// Required manifest hash
    pub manifest_hash: String,
    /// Capability requirements
    pub capabilities: RequiredCapabilities,
    /// Prefer workers with cache hits (for prefix caching)
    pub prefer_cache_hit: bool,
}

impl WorkerRequirements {
    /// Create requirements from an inference request.
    pub fn from_request(request: &InferenceRequestInternal, manifest_hash: &str) -> Self {
        Self {
            tenant_id: request.cpid.clone(),
            manifest_hash: manifest_hash.to_string(),
            capabilities: RequiredCapabilities::from_request(request),
            prefer_cache_hit: true, // Default to preferring cache hits
        }
    }

    /// Create minimal requirements for a tenant.
    pub fn for_tenant(tenant_id: &str, manifest_hash: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            manifest_hash: manifest_hash.to_string(),
            capabilities: RequiredCapabilities::any(),
            prefer_cache_hit: false,
        }
    }
}

/// A worker with pre-indexed capabilities for efficient filtering.
///
/// This struct holds the worker binding along with parsed capabilities,
/// avoiding repeated JSON parsing during selection.
#[derive(Debug, Clone)]
pub struct IndexedWorker {
    /// The worker binding from database
    pub binding: WorkerWithBinding,
    /// Pre-parsed and normalized capabilities
    pub capabilities: Option<WorkerCapabilities>,
    /// Pre-computed health status
    pub health_status: WorkerHealthStatus,
    /// Pre-computed average latency
    pub avg_latency_ms: f64,
}

impl IndexedWorker {
    /// Create an indexed worker from a binding with optional health monitor.
    fn from_binding(
        binding: WorkerWithBinding,
        health_monitor: Option<&WorkerHealthMonitor>,
    ) -> Self {
        // Parse capabilities once
        let capabilities = parse_capabilities(&binding);

        // Get health metrics
        let (health_status, avg_latency_ms) = health_monitor
            .and_then(|hm| hm.get_worker_metrics(&binding.id))
            .map(|m| (m.health_status, m.avg_latency_ms))
            .unwrap_or((WorkerHealthStatus::Unknown, 0.0));

        Self {
            binding,
            capabilities,
            health_status,
            avg_latency_ms,
        }
    }

    /// Check if this worker matches the required capabilities.
    fn matches_capabilities(&self, required: &RequiredCapabilities) -> Result<(), Vec<String>> {
        let reasons = capability_reasons(
            self.capabilities.as_ref(),
            &required.modes,
            required.backend,
            required.require_determinism,
        );

        if reasons.is_empty() {
            Ok(())
        } else {
            Err(reasons)
        }
    }

    /// Compute a selection score for this worker.
    ///
    /// Lower score is better. The score combines:
    /// - Health priority (healthy=0, degraded=100, unknown=200, crashed=1000)
    /// - Latency in milliseconds
    fn selection_score(&self) -> f64 {
        let health_priority = match self.health_status {
            WorkerHealthStatus::Healthy => 0.0,
            WorkerHealthStatus::Degraded => 100.0,
            WorkerHealthStatus::Unknown => 200.0,
            WorkerHealthStatus::Crashed => 1000.0,
        };

        health_priority + self.avg_latency_ms
    }
}

/// Result of worker selection including diagnostics.
#[derive(Debug)]
pub struct WorkerSelectionResult {
    /// The selected worker (if any)
    pub worker: Option<WorkerWithBinding>,
    /// Workers that were excluded and why
    pub exclusions: Vec<WorkerCapabilityExclusion>,
    /// Total candidates considered
    pub candidates_considered: usize,
    /// Candidates after capability filtering
    pub candidates_after_filter: usize,
}

/// Unified worker selector that combines DB query, capability filter, and health check.
pub struct WorkerSelector<'a> {
    db: &'a Db,
    health_monitor: Option<Arc<WorkerHealthMonitor>>,
}

impl<'a> WorkerSelector<'a> {
    /// Create a new worker selector.
    pub fn new(db: &'a Db, health_monitor: Option<Arc<WorkerHealthMonitor>>) -> Self {
        Self { db, health_monitor }
    }

    /// Select the best worker for the given requirements.
    ///
    /// This performs a single-pass selection:
    /// 1. Query database for compatible workers (manifest + tenant + health)
    /// 2. Parse capabilities and compute health scores inline
    /// 3. Filter by capability requirements
    /// 4. Select best worker by health score and latency
    ///
    /// # Returns
    ///
    /// - `Ok(worker)` - A compatible worker was found
    /// - `Err(SelectionError)` - No compatible worker found (with diagnostics)
    pub async fn select_best_worker(
        &self,
        req: &WorkerRequirements,
    ) -> Result<WorkerWithBinding, SelectionError> {
        let result = self.select_with_diagnostics(req).await?;

        result
            .worker
            .ok_or_else(|| SelectionError::NoCompatibleWorker {
                tenant_id: req.tenant_id.clone(),
                manifest_hash: req.manifest_hash.clone(),
                candidates_considered: result.candidates_considered,
                candidates_after_filter: result.candidates_after_filter,
                exclusions: result.exclusions,
            })
    }

    /// Select worker with full diagnostics.
    ///
    /// Returns the selection result including all exclusion reasons,
    /// useful for debugging worker selection issues.
    pub async fn select_with_diagnostics(
        &self,
        req: &WorkerRequirements,
    ) -> Result<WorkerSelectionResult, SelectionError> {
        // Step 1: Query database for compatible workers
        let workers = self
            .db
            .list_compatible_workers_for_tenant(&req.manifest_hash, &req.tenant_id)
            .await
            .map_err(|e| SelectionError::DatabaseError(e.to_string()))?;

        let candidates_considered = workers.len();
        debug!(
            tenant_id = %req.tenant_id,
            manifest_hash = %req.manifest_hash,
            candidates = candidates_considered,
            "Fetched worker candidates from database"
        );

        // Step 2: Index workers with capabilities and health
        let health_monitor_ref = self.health_monitor.as_deref();
        let indexed: Vec<IndexedWorker> = workers
            .into_iter()
            .map(|w| IndexedWorker::from_binding(w, health_monitor_ref))
            .collect();

        // Step 3: Filter by capabilities and collect exclusions
        let mut compatible: Vec<IndexedWorker> = Vec::new();
        let mut exclusions: Vec<WorkerCapabilityExclusion> = Vec::new();

        for worker in indexed {
            match worker.matches_capabilities(&req.capabilities) {
                Ok(()) => {
                    // Skip crashed workers
                    if worker.health_status == WorkerHealthStatus::Crashed {
                        debug!(
                            worker_id = %worker.binding.id,
                            "Worker excluded: crashed"
                        );
                        exclusions.push(WorkerCapabilityExclusion {
                            worker_id: worker.binding.id.clone(),
                            backend: worker.binding.backend.clone(),
                            reasons: vec!["worker_crashed".to_string()],
                            capabilities: worker.capabilities.clone(),
                        });
                    } else {
                        compatible.push(worker);
                    }
                }
                Err(reasons) => {
                    debug!(
                        worker_id = %worker.binding.id,
                        tenant_id = %worker.binding.tenant_id,
                        backend = %worker.binding.backend.as_deref().unwrap_or("unknown"),
                        reasons = ?reasons,
                        "Worker excluded by capability requirements"
                    );
                    exclusions.push(WorkerCapabilityExclusion {
                        worker_id: worker.binding.id.clone(),
                        backend: worker.binding.backend.clone(),
                        reasons,
                        capabilities: worker.capabilities.clone(),
                    });
                }
            }
        }

        let candidates_after_filter = compatible.len();

        if compatible.is_empty() {
            return Ok(WorkerSelectionResult {
                worker: None,
                exclusions,
                candidates_considered,
                candidates_after_filter: 0,
            });
        }

        // Step 4: Sort by selection score and pick best
        compatible.sort_by(|a, b| {
            a.selection_score()
                .partial_cmp(&b.selection_score())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.binding.id.cmp(&b.binding.id)) // Tie-break by ID for determinism
        });

        let selected = compatible.into_iter().next().map(|w| w.binding);

        if let Some(ref worker) = selected {
            debug!(
                worker_id = %worker.id,
                tenant_id = %req.tenant_id,
                manifest_hash = %worker.manifest_hash_b3.as_deref().unwrap_or("none"),
                "Selected best worker"
            );
        }

        Ok(WorkerSelectionResult {
            worker: selected,
            exclusions,
            candidates_considered,
            candidates_after_filter,
        })
    }

    /// Select worker with retry logic.
    ///
    /// Retries selection with exponential backoff for transient failures.
    /// Used by InferenceCore for production-grade worker selection.
    ///
    /// # Parameters
    ///
    /// - `req`: Worker requirements
    /// - `max_attempts`: Maximum number of attempts (default 3)
    /// - `base_delay`: Initial delay between retries (default 2s)
    /// - `max_elapsed`: Maximum total time for retries (default 30s)
    pub async fn select_with_retry(
        &self,
        req: &WorkerRequirements,
        max_attempts: u32,
        base_delay: std::time::Duration,
        max_elapsed: std::time::Duration,
    ) -> Result<WorkerWithBinding, SelectionError> {
        use std::time::Instant;

        let deadline = Instant::now() + max_elapsed;
        let mut attempt: u32 = 0;
        let mut delay = base_delay;

        loop {
            attempt += 1;
            let remaining = deadline.saturating_duration_since(Instant::now());

            match self.select_best_worker(req).await {
                Ok(worker) => {
                    if attempt > 1 {
                        info!(
                            tenant_id = %req.tenant_id,
                            worker_id = %worker.id,
                            attempt = attempt,
                            "Selected compatible worker after retry"
                        );
                    }
                    return Ok(worker);
                }
                Err(e) => {
                    let should_retry =
                        e.is_retryable() && attempt < max_attempts && !remaining.is_zero();

                    if should_retry {
                        warn!(
                            attempt = attempt,
                            max_attempts = max_attempts,
                            delay_ms = delay.as_millis() as u64,
                            remaining_budget_ms = remaining.as_millis() as u64,
                            tenant_id = %req.tenant_id,
                            error = %e,
                            "Worker selection failed, retrying"
                        );

                        let actual_delay = delay.min(remaining);
                        tokio::time::sleep(actual_delay).await;
                        delay *= 2;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }
}

/// Error type for worker selection failures.
#[derive(Debug, thiserror::Error)]
pub enum SelectionError {
    /// Database query failed
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// No compatible worker found
    #[error("No compatible worker found for tenant '{tenant_id}' with manifest '{manifest_hash}': {candidates_considered} candidates considered, {candidates_after_filter} after filtering")]
    NoCompatibleWorker {
        tenant_id: String,
        manifest_hash: String,
        candidates_considered: usize,
        candidates_after_filter: usize,
        exclusions: Vec<WorkerCapabilityExclusion>,
    },
}

impl SelectionError {
    /// Check if this error is retryable.
    ///
    /// Database errors and "no healthy workers" are retryable.
    /// Capability mismatches are not retryable.
    pub fn is_retryable(&self) -> bool {
        match self {
            SelectionError::DatabaseError(_) => true,
            SelectionError::NoCompatibleWorker {
                candidates_considered,
                exclusions,
                ..
            } => {
                // Retryable if no candidates at all (workers might be starting up)
                // Not retryable if candidates exist but were excluded by capabilities
                *candidates_considered == 0
                    || exclusions
                        .iter()
                        .all(|e| e.reasons.iter().any(|r| r == "worker_crashed"))
            }
        }
    }

    /// Get exclusion details for error reporting.
    pub fn exclusion_details(&self) -> Option<serde_json::Value> {
        match self {
            SelectionError::NoCompatibleWorker { exclusions, .. } if !exclusions.is_empty() => {
                Some(serde_json::json!({
                    "excluded_workers": exclusions,
                }))
            }
            _ => None,
        }
    }
}

/// Parse worker capabilities from the binding.
///
/// This is a unified parsing function that handles:
/// - Structured WorkerCapabilities JSON
/// - Legacy capability arrays
/// - Backend label derivation as fallback
fn parse_capabilities(binding: &WorkerWithBinding) -> Option<WorkerCapabilities> {
    // Try structured capabilities JSON first
    if let Some(ref raw_json) = binding.capabilities_json {
        if let Ok(caps) = serde_json::from_str::<WorkerCapabilities>(raw_json) {
            return Some(normalize_worker_capabilities(caps));
        }

        // Try legacy array format
        if let Ok(list) = serde_json::from_str::<Vec<String>>(raw_json) {
            if let Some(caps) = derive_from_list_and_backend(&list, binding.backend.as_deref()) {
                return Some(caps);
            }
        }
    }

    // Fall back to deriving from backend label
    binding.backend.as_deref().and_then(derive_from_backend)
}

/// Derive capabilities from a backend list and optional backend label.
fn derive_from_list_and_backend(
    list: &[String],
    backend_label: Option<&str>,
) -> Option<WorkerCapabilities> {
    if let Some(label) = backend_label {
        return derive_from_backend(label);
    }

    for entry in list {
        if let Some(caps) = derive_from_backend(entry) {
            return Some(caps);
        }
    }

    None
}

/// Derive capabilities from a backend label.
fn derive_from_backend(label: &str) -> Option<WorkerCapabilities> {
    let normalized = match label.to_ascii_lowercase().as_str() {
        "mlxbridge" | "mlx-bridge" | "bridge" => "bridge",
        "mlx" => "mlx",
        "metal" => "metal",
        "coreml" | "core-ml" => "coreml",
        "cpu" => "cpu",
        _ => return None,
    };

    let (supports_step, supports_bulk, supports_logits, supports_streaming) = match normalized {
        "bridge" => (false, true, false, false),
        "mlx" | "metal" | "coreml" => (true, false, true, true),
        _ => (false, false, false, false),
    };

    let implementation = if normalized == "bridge" {
        Some("mlx_subprocess".to_string())
    } else {
        None
    };

    let gpu_backward = normalized == "mlx";
    let multi_backend = matches!(normalized, "mlx" | "bridge");

    Some(WorkerCapabilities {
        backend_kind: normalized.to_string(),
        implementation,
        supports_step,
        supports_bulk,
        supports_logits,
        supports_streaming,
        gpu_backward,
        multi_backend,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_worker(
        id: &str,
        capabilities_json: Option<&str>,
        backend: Option<&str>,
    ) -> WorkerWithBinding {
        WorkerWithBinding {
            id: id.to_string(),
            tenant_id: "test-tenant".to_string(),
            node_id: "node-1".to_string(),
            plan_id: "plan-1".to_string(),
            uds_path: format!("/var/run/aos/test/{}.sock", id),
            pid: Some(1234),
            status: "healthy".to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_seen_at: None,
            manifest_hash_b3: Some("test-manifest".to_string()),
            backend: backend.map(|s| s.to_string()),
            model_hash_b3: None,
            capabilities_json: capabilities_json.map(|s| s.to_string()),
            schema_version: Some("1.0.0".to_string()),
            api_version: None,
            registered_at: None,
            health_status: Some("healthy".to_string()),
        }
    }

    #[test]
    fn test_parse_capabilities_structured() {
        let caps_json = r#"{"backend_kind":"mlx","supports_step":true,"supports_bulk":false,"supports_logits":true,"supports_streaming":true,"gpu_backward":true,"multi_backend":true}"#;
        let worker = make_worker("w1", Some(caps_json), Some("mlx"));
        let caps = parse_capabilities(&worker).expect("should parse");
        assert_eq!(caps.backend_kind, "mlx");
        assert!(caps.supports_step);
        assert!(caps.supports_streaming);
    }

    #[test]
    fn test_parse_capabilities_from_backend() {
        let worker = make_worker("w1", None, Some("mlx"));
        let caps = parse_capabilities(&worker).expect("should derive from backend");
        assert_eq!(caps.backend_kind, "mlx");
        assert!(caps.supports_step);
    }

    #[test]
    fn test_parse_capabilities_bridge() {
        let worker = make_worker("w1", None, Some("bridge"));
        let caps = parse_capabilities(&worker).expect("should derive bridge caps");
        assert_eq!(caps.backend_kind, "bridge");
        assert!(!caps.supports_step);
        assert!(caps.supports_bulk);
    }

    #[test]
    fn test_indexed_worker_matches_streaming() {
        let worker = make_worker("w1", None, Some("mlx"));
        let indexed = IndexedWorker::from_binding(worker, None);

        let streaming_req = RequiredCapabilities::for_streaming();
        assert!(indexed.matches_capabilities(&streaming_req).is_ok());

        let bulk_req = RequiredCapabilities::for_bulk();
        assert!(indexed.matches_capabilities(&bulk_req).is_ok());
    }

    #[test]
    fn test_indexed_worker_rejects_bridge_for_streaming() {
        let worker = make_worker("w1", None, Some("bridge"));
        let indexed = IndexedWorker::from_binding(worker, None);

        let streaming_req = RequiredCapabilities::for_streaming();
        let result = indexed.matches_capabilities(&streaming_req);
        assert!(result.is_err());
    }

    #[test]
    fn test_selection_score_ordering() {
        let healthy = IndexedWorker {
            binding: make_worker("w1", None, Some("mlx")),
            capabilities: Some(derive_from_backend("mlx").unwrap()),
            health_status: WorkerHealthStatus::Healthy,
            avg_latency_ms: 10.0,
        };

        let degraded = IndexedWorker {
            binding: make_worker("w2", None, Some("mlx")),
            capabilities: Some(derive_from_backend("mlx").unwrap()),
            health_status: WorkerHealthStatus::Degraded,
            avg_latency_ms: 5.0,
        };

        // Healthy should score lower (better) than degraded despite lower latency
        assert!(healthy.selection_score() < degraded.selection_score());
    }

    #[test]
    fn test_selection_error_retryable() {
        let db_error = SelectionError::DatabaseError("connection failed".to_string());
        assert!(db_error.is_retryable());

        let no_candidates = SelectionError::NoCompatibleWorker {
            tenant_id: "t1".to_string(),
            manifest_hash: "m1".to_string(),
            candidates_considered: 0,
            candidates_after_filter: 0,
            exclusions: vec![],
        };
        assert!(no_candidates.is_retryable());

        let capability_mismatch = SelectionError::NoCompatibleWorker {
            tenant_id: "t1".to_string(),
            manifest_hash: "m1".to_string(),
            candidates_considered: 2,
            candidates_after_filter: 0,
            exclusions: vec![WorkerCapabilityExclusion {
                worker_id: "w1".to_string(),
                backend: Some("bridge".to_string()),
                reasons: vec!["mode_streaming_required".to_string()],
                capabilities: None,
            }],
        };
        assert!(!capability_mismatch.is_retryable());
    }
}
