//! K reduction coordination protocol between memory and lifecycle managers
//!
//! This module defines a shared protocol for coordinating K reduction decisions
//! across the memory pressure manager and lifecycle manager. It ensures both
//! systems agree on when and how to reduce K (number of active adapters).
//!
//! # Design Principles
//!
//! 1. **Consensus-based**: Both managers must agree on K reduction
//! 2. **Memory-driven**: Memory pressure initiates K reduction request
//! 3. **Lifecycle-aware**: Lifecycle manager determines feasibility and implements
//! 4. **Observable**: All decisions logged and telemetrized
//! 5. **Reversible**: K reduction is temporary and can be reversed
//! 6. **Deadlock-safe**: Timeout mechanism and lock ordering prevent deadlocks
//! 7. **Instrumented**: Telemetry events track all phases of K reduction

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// K reduction request initiated by memory pressure manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionRequest {
    /// Request ID for tracking
    pub request_id: String,
    /// Proposed new K value
    pub target_k: usize,
    /// Current K value
    pub current_k: usize,
    /// Current memory pressure level (0-1, 1=critical)
    pub pressure_level: f32,
    /// Bytes needed to be freed
    pub bytes_to_free: u64,
    /// Current memory headroom percentage
    pub headroom_pct: f32,
    /// Timestamp when request was created
    pub created_at: u128,
    /// Optional reason for reduction
    pub reason: String,
}

impl KReductionRequest {
    /// Create a new K reduction request
    pub fn new(
        target_k: usize,
        current_k: usize,
        pressure_level: f32,
        bytes_to_free: u64,
        headroom_pct: f32,
        reason: String,
    ) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            target_k,
            current_k,
            pressure_level,
            bytes_to_free,
            headroom_pct,
            created_at,
            reason,
        }
    }

    /// Check if K reduction is valid (target < current)
    pub fn is_valid(&self) -> bool {
        self.target_k > 0 && self.target_k < self.current_k
    }

    /// Calculate estimated memory freed by K reduction
    pub fn estimated_memory_freed(&self, avg_adapter_size: u64) -> u64 {
        let adapters_to_remove = self.current_k.saturating_sub(self.target_k);
        (adapters_to_remove as u64).saturating_mul(avg_adapter_size)
    }
}

/// K reduction response from lifecycle manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionResponse {
    /// Request ID being responded to
    pub request_id: String,
    /// Whether reduction was approved
    pub approved: bool,
    /// New K value if approved, otherwise original K
    pub new_k: usize,
    /// Adapters that will be unloaded (if approved)
    pub adapters_to_unload: Vec<u16>,
    /// Reason for approval/rejection
    pub reason: String,
    /// Estimated memory freed
    pub estimated_freed: u64,
    /// Timestamp of response
    pub created_at: u128,
}

impl KReductionResponse {
    /// Create an approval response
    pub fn approve(
        request_id: String,
        new_k: usize,
        adapters_to_unload: Vec<u16>,
        estimated_freed: u64,
        reason: String,
    ) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        Self {
            request_id,
            approved: true,
            new_k,
            adapters_to_unload,
            reason,
            estimated_freed,
            created_at,
        }
    }

    /// Create a rejection response
    pub fn reject(request_id: String, current_k: usize, reason: String) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        Self {
            request_id,
            approved: false,
            new_k: current_k,
            adapters_to_unload: Vec::new(),
            reason,
            estimated_freed: 0,
            created_at,
        }
    }
}

/// K reduction decision record (for history/audit)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionDecision {
    /// Request that led to this decision
    pub request: KReductionRequest,
    /// Response from lifecycle manager
    pub response: KReductionResponse,
    /// Whether decision was actually executed
    pub executed: bool,
    /// Actual memory freed
    pub actual_freed: Option<u64>,
    /// Failure reason if execution failed
    pub failure_reason: Option<String>,
}

impl KReductionDecision {
    /// Create a new decision record
    pub fn new(request: KReductionRequest, response: KReductionResponse) -> Self {
        Self {
            request,
            response,
            executed: false,
            actual_freed: None,
            failure_reason: None,
        }
    }

    /// Mark decision as executed successfully
    pub fn mark_executed(&mut self, actual_freed: u64) {
        self.executed = true;
        self.actual_freed = Some(actual_freed);
    }

    /// Mark decision as failed
    pub fn mark_failed(&mut self, reason: String) {
        self.executed = false;
        self.failure_reason = Some(reason);
    }
}

/// Trait for K reduction decision making
pub trait KReductionDecisionMaker: Send + Sync {
    /// Evaluate a K reduction request
    fn evaluate_request(&self, request: &KReductionRequest) -> KReductionResponse;

    /// Check if further K reduction is possible
    fn can_reduce_further(&self, current_k: usize, min_k: usize) -> bool {
        current_k > min_k
    }

    /// Calculate recommended K reduction
    fn calculate_target_k(
        &self,
        current_k: usize,
        pressure_level: f32,
        available_adapters: usize,
    ) -> usize {
        // Reduce by ~10% for each pressure unit
        let reduction_factor = (pressure_level * 10.0).min(0.9);
        let target = ((current_k as f32) * (1.0 - reduction_factor)) as usize;
        target.max(1).min(available_adapters)
    }
}

/// Default K reduction decision maker (used by lifecycle manager)
pub struct DefaultKReductionDecisionMaker {
    /// Minimum K value (never go below this)
    pub min_k: usize,
    /// Critical pressure threshold (0-1)
    pub critical_threshold: f32,
}

impl DefaultKReductionDecisionMaker {
    /// Create a new decision maker
    pub fn new(min_k: usize, critical_threshold: f32) -> Self {
        Self {
            min_k,
            critical_threshold,
        }
    }
}

impl KReductionDecisionMaker for DefaultKReductionDecisionMaker {
    fn evaluate_request(&self, request: &KReductionRequest) -> KReductionResponse {
        // Validate request
        if !request.is_valid() {
            return KReductionResponse::reject(
                request.request_id.clone(),
                request.current_k,
                "Invalid K reduction request: target >= current".to_string(),
            );
        }

        // Check if we can reduce further
        if request.target_k < self.min_k {
            return KReductionResponse::reject(
                request.request_id.clone(),
                request.current_k,
                format!("Cannot reduce K below minimum: {}", self.min_k),
            );
        }

        // Only approve if pressure is high enough to justify K reduction
        if request.pressure_level < self.critical_threshold {
            return KReductionResponse::reject(
                request.request_id.clone(),
                request.current_k,
                format!(
                    "Pressure level {:.2} below critical threshold {:.2}",
                    request.pressure_level, self.critical_threshold
                ),
            );
        }

        // Approve the reduction
        let adapters_to_unload = (request.target_k..request.current_k)
            .map(|i| i as u16)
            .collect();

        let estimated_freed = request.estimated_memory_freed(1024 * 1024); // Assume 1MB per adapter

        KReductionResponse::approve(
            request.request_id.clone(),
            request.target_k,
            adapters_to_unload,
            estimated_freed,
            format!(
                "K reduction approved: {} -> {}, pressure: {:.2}%",
                request.current_k,
                request.target_k,
                request.pressure_level * 100.0
            ),
        )
    }
}

/// Timeout configuration for K reduction operations
#[derive(Debug, Clone)]
pub struct KReductionTimeoutConfig {
    /// Timeout for K reduction request processing (milliseconds)
    pub request_timeout_ms: u64,
    /// Timeout for lifecycle evaluation (milliseconds)
    pub evaluation_timeout_ms: u64,
    /// Timeout for adapter unload execution (milliseconds)
    pub execution_timeout_ms: u64,
}

impl Default for KReductionTimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout_ms: 5000,     // 5 seconds
            evaluation_timeout_ms: 10000, // 10 seconds
            execution_timeout_ms: 15000,  // 15 seconds
        }
    }
}

/// Deadlock detection context for lock ordering
#[derive(Debug, Clone)]
pub struct LockOrderingContext {
    /// Lock acquisition timestamp
    pub acquired_at: Instant,
    /// Lock owner/context identifier
    pub owner: String,
    /// Expected lock acquisition time (microseconds)
    pub expected_duration_us: u64,
}

/// K reduction operation status with timeout tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KReductionStatus {
    /// Request initiated but not yet processed
    Pending,
    /// Request being evaluated by lifecycle manager
    Evaluating,
    /// Request approved, waiting for execution
    Approved,
    /// Request being executed (adapters being unloaded)
    Executing,
    /// Operation completed successfully
    Completed,
    /// Operation failed
    Failed,
    /// Operation timed out
    TimedOut,
    /// Deadlock detected and recovered
    DeadlockRecovered,
}

/// K reduction coordinator - manages protocol between managers with timeout and deadlock prevention
pub struct KReductionCoordinator {
    decision_maker: Arc<dyn KReductionDecisionMaker>,
    decision_history: Arc<parking_lot::RwLock<Vec<KReductionDecision>>>,
    max_history_size: usize,
    /// Timeout configuration
    timeout_config: KReductionTimeoutConfig,
    /// Pending requests with timestamp for timeout tracking
    pending_requests:
        Arc<parking_lot::RwLock<std::collections::HashMap<String, (Instant, KReductionStatus)>>>,
    /// Lock acquisition tracking for deadlock detection
    lock_ordering: Arc<parking_lot::RwLock<Vec<LockOrderingContext>>>,
}

impl KReductionCoordinator {
    /// Create a new K reduction coordinator with default timeout config
    pub fn new(decision_maker: Arc<dyn KReductionDecisionMaker>, max_history_size: usize) -> Self {
        Self::with_config(
            decision_maker,
            max_history_size,
            KReductionTimeoutConfig::default(),
        )
    }

    /// Create a new K reduction coordinator with custom timeout config
    pub fn with_config(
        decision_maker: Arc<dyn KReductionDecisionMaker>,
        max_history_size: usize,
        timeout_config: KReductionTimeoutConfig,
    ) -> Self {
        Self {
            decision_maker,
            decision_history: Arc::new(parking_lot::RwLock::new(Vec::new())),
            max_history_size,
            timeout_config,
            pending_requests: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            lock_ordering: Arc::new(parking_lot::RwLock::new(Vec::new())),
        }
    }

    /// Process a K reduction request with timeout protection
    pub fn process_request(&self, request: KReductionRequest) -> KReductionResponse {
        let request_id = request.request_id.clone();

        // Record pending request for timeout tracking
        {
            let mut pending = self.pending_requests.write();
            pending.insert(
                request_id.clone(),
                (Instant::now(), KReductionStatus::Pending),
            );
        }

        debug!(
            request_id = %request_id,
            target_k = request.target_k,
            current_k = request.current_k,
            pressure_level = request.pressure_level,
            "Processing K reduction request"
        );

        // Check for deadlock condition before evaluation
        if self.check_and_handle_deadlock(&request_id) {
            warn!(
                request_id = %request_id,
                "Deadlock detected during request processing, aborting"
            );

            // Update status
            {
                let mut pending = self.pending_requests.write();
                if let Some((_, status)) = pending.get_mut(&request_id) {
                    *status = KReductionStatus::DeadlockRecovered;
                }
            }

            return KReductionResponse::reject(
                request_id,
                request.current_k,
                "K reduction aborted: deadlock detected".to_string(),
            );
        }

        // Mark as evaluating
        {
            let mut pending = self.pending_requests.write();
            if let Some((_, status)) = pending.get_mut(&request_id) {
                *status = KReductionStatus::Evaluating;
            }
        }

        let eval_start = Instant::now();
        let response = self.decision_maker.evaluate_request(&request);
        let eval_duration = eval_start.elapsed();

        // Check for evaluation timeout
        if eval_duration > Duration::from_millis(self.timeout_config.evaluation_timeout_ms) {
            warn!(
                request_id = %request_id,
                duration_ms = eval_duration.as_millis(),
                timeout_ms = self.timeout_config.evaluation_timeout_ms,
                "K reduction evaluation exceeded timeout"
            );

            // Update status
            {
                let mut pending = self.pending_requests.write();
                if let Some((_, status)) = pending.get_mut(&request_id) {
                    *status = KReductionStatus::TimedOut;
                }
            }

            return KReductionResponse::reject(
                request_id,
                request.current_k,
                format!(
                    "K reduction evaluation timeout: took {}ms, limit {}ms",
                    eval_duration.as_millis(),
                    self.timeout_config.evaluation_timeout_ms
                ),
            );
        }

        if response.approved {
            info!(
                request_id = %response.request_id,
                new_k = response.new_k,
                adapters_to_unload = response.adapters_to_unload.len(),
                estimated_freed = response.estimated_freed,
                eval_duration_ms = eval_duration.as_millis(),
                "K reduction request approved"
            );

            // Update status
            {
                let mut pending = self.pending_requests.write();
                if let Some((_, status)) = pending.get_mut(&request_id) {
                    *status = KReductionStatus::Approved;
                }
            }
        } else {
            warn!(
                request_id = %response.request_id,
                reason = %response.reason,
                eval_duration_ms = eval_duration.as_millis(),
                "K reduction request rejected"
            );

            // Update status
            {
                let mut pending = self.pending_requests.write();
                if let Some((_, status)) = pending.get_mut(&request_id) {
                    *status = KReductionStatus::Failed;
                }
            }
        }

        response
    }

    /// Check for deadlock condition using lock acquisition time threshold
    fn check_and_handle_deadlock(&self, request_id: &str) -> bool {
        let lock_ordering = self.lock_ordering.read();

        // Check if any lock acquisition is taking suspiciously long (> 5 seconds)
        let now = Instant::now();
        const DEADLOCK_THRESHOLD_MS: u64 = 5000;

        for lock_ctx in lock_ordering.iter() {
            let acquisition_time = now.duration_since(lock_ctx.acquired_at);
            if acquisition_time > Duration::from_millis(DEADLOCK_THRESHOLD_MS) {
                warn!(
                    request_id = request_id,
                    owner = %lock_ctx.owner,
                    held_for_ms = acquisition_time.as_millis(),
                    threshold_ms = DEADLOCK_THRESHOLD_MS,
                    "Potential deadlock: lock held too long"
                );
                return true;
            }
        }

        false
    }

    /// Record lock acquisition for deadlock detection
    pub fn record_lock_acquisition(&self, owner: String, expected_duration_us: u64) {
        let mut lock_ordering = self.lock_ordering.write();
        lock_ordering.push(LockOrderingContext {
            acquired_at: Instant::now(),
            owner,
            expected_duration_us,
        });

        // Trim old entries (keep last 100)
        if lock_ordering.len() > 100 {
            lock_ordering.remove(0);
        }
    }

    /// Get current K reduction status
    pub fn get_status(&self, request_id: &str) -> Option<KReductionStatus> {
        let pending = self.pending_requests.read();
        pending.get(request_id).map(|(_, status)| status.clone())
    }

    /// Check and report timed out requests
    pub fn check_timeouts(&self) -> Vec<String> {
        let mut pending = self.pending_requests.write();
        let mut timed_out = Vec::new();
        let now = Instant::now();

        pending.retain(|request_id, (start_time, status)| {
            let elapsed = now.duration_since(*start_time);
            let timeout = match status {
                KReductionStatus::Evaluating => {
                    Duration::from_millis(self.timeout_config.evaluation_timeout_ms)
                }
                KReductionStatus::Executing => {
                    Duration::from_millis(self.timeout_config.execution_timeout_ms)
                }
                _ => Duration::from_millis(self.timeout_config.request_timeout_ms),
            };

            if elapsed > timeout {
                warn!(
                    request_id = request_id,
                    status = ?status,
                    elapsed_ms = elapsed.as_millis(),
                    timeout_ms = timeout.as_millis(),
                    "K reduction request timed out"
                );
                timed_out.push(request_id.clone());
                false // Remove from pending
            } else {
                true // Keep in pending
            }
        });

        timed_out
    }

    /// Record a K reduction decision
    pub fn record_decision(&self, decision: KReductionDecision) {
        let mut history = self.decision_history.write();
        history.push(decision);

        // Trim history if it exceeds max size
        if history.len() > self.max_history_size {
            history.remove(0);
        }
    }

    /// Get K reduction decision history
    pub fn get_history(&self) -> Vec<KReductionDecision> {
        self.decision_history.read().clone()
    }

    /// Get statistics about K reduction decisions
    pub fn get_stats(&self) -> KReductionStats {
        let history = self.decision_history.read();
        let total_decisions = history.len();
        let approved = history.iter().filter(|d| d.response.approved).count();
        let executed = history.iter().filter(|d| d.executed).count();
        let total_freed: u64 = history.iter().filter_map(|d| d.actual_freed).sum();

        KReductionStats {
            total_decisions,
            approved,
            executed,
            total_memory_freed: total_freed,
            approval_rate: if total_decisions > 0 {
                approved as f32 / total_decisions as f32
            } else {
                0.0
            },
            execution_rate: if approved > 0 {
                executed as f32 / approved as f32
            } else {
                0.0
            },
        }
    }

    /// Clear decision history
    pub fn clear_history(&self) {
        self.decision_history.write().clear();
    }
}

/// Statistics about K reduction decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KReductionStats {
    /// Total number of K reduction decisions
    pub total_decisions: usize,
    /// Number of approved decisions
    pub approved: usize,
    /// Number of executed decisions
    pub executed: usize,
    /// Total memory freed from K reduction
    pub total_memory_freed: u64,
    /// Approval rate (0-1)
    pub approval_rate: f32,
    /// Execution rate (0-1)
    pub execution_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_k_reduction_request_creation() {
        let request = KReductionRequest::new(
            8,
            10,
            0.85,
            1024 * 1024,
            10.0,
            "Memory pressure high".to_string(),
        );

        assert_eq!(request.target_k, 8);
        assert_eq!(request.current_k, 10);
        assert!(request.is_valid());
    }

    #[test]
    fn test_k_reduction_invalid_request() {
        let request = KReductionRequest::new(10, 8, 0.85, 1024 * 1024, 10.0, "Invalid".to_string());

        assert!(!request.is_valid());
    }

    #[test]
    fn test_default_decision_maker_approval() {
        let maker = DefaultKReductionDecisionMaker::new(2, 0.70);
        let request = KReductionRequest::new(
            8,
            10,
            0.85,
            1024 * 1024,
            10.0,
            "Memory pressure high".to_string(),
        );

        let response = maker.evaluate_request(&request);
        assert!(response.approved);
        assert_eq!(response.new_k, 8);
    }

    #[test]
    fn test_default_decision_maker_rejection_low_pressure() {
        let maker = DefaultKReductionDecisionMaker::new(2, 0.70);
        let request =
            KReductionRequest::new(8, 10, 0.50, 1024 * 1024, 20.0, "Low pressure".to_string());

        let response = maker.evaluate_request(&request);
        assert!(!response.approved);
    }

    #[test]
    fn test_k_reduction_coordinator() {
        let maker = Arc::new(DefaultKReductionDecisionMaker::new(2, 0.70));
        let coordinator = KReductionCoordinator::new(maker, 100);

        let request = KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test".to_string());

        let response = coordinator.process_request(request);
        assert!(response.approved);

        let stats = coordinator.get_stats();
        assert_eq!(stats.total_decisions, 0); // Not recorded yet
    }

    #[test]
    fn test_k_reduction_decision_execution() {
        let mut decision = KReductionDecision::new(
            KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test".to_string()),
            KReductionResponse::approve(
                "req1".to_string(),
                8,
                vec![8, 9],
                2048 * 1024,
                "Approved".to_string(),
            ),
        );

        assert!(!decision.executed);
        decision.mark_executed(2048 * 1024);
        assert!(decision.executed);
        assert_eq!(decision.actual_freed, Some(2048 * 1024));
    }
}
