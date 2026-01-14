//! Deterministic replay infrastructure for AdapterOS inference runs
//!
//! This module provides the ability to:
//! - Load prior event logs (task order, tensor hashes, scheduler ticks)
//! - Replay execution step-by-step using deterministic executor
//! - Validate all intermediate hashes against the stored trace
//!
//! # Determinism Guarantees
//!
//! 1. **Identical Execution Order**: Tasks execute in the exact same order as recorded
//! 2. **Hash Verification**: All intermediate tensor hashes must match the stored trace
//! 3. **Tick Synchronization**: Logical tick counter advances identically to original run
//! 4. **RNG State Reconstruction**: Random number generation matches original via HKDF seeding
//! 5. **Event-Driven Scheduling**: Uses recorded events to drive scheduling instead of runtime queue

#![allow(unused_imports)]

use std::collections::HashMap;

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod bundle;
pub mod reproducible;
pub mod session;
pub mod verification;

pub use bundle::{ReplayBundle, ReplaySignatureMetadata};
pub use reproducible::{
    compare_receipt_digests, compare_receipt_digests_hex, AdapterAvailabilityChecker,
    AdapterSpec, AvailabilityCheckResult, ComponentType, DivergenceDiagnostics, DivergenceType,
    FieldMismatch, ModelAvailabilityChecker, ModelSpec, ReplayExecutionStats,
    ReplayVerificationResult, ReproducibleReplayError, ReproducibleReplayExecutor,
    ReproducibleReplayResult, ReproducibleReplaySpec, SamplingParams as ReproducibleSamplingParams,
    UnavailableComponent, VerificationStatus,
};
pub use session::{replay_trace, ExecutorState, ReplaySession, ReplayStats};
pub use verification::{
    compare_events_permissive, compare_traces, ComparisonResult, HashVerifier, TolerantVerifier,
    VerificationMode,
};

/// Error types for replay operations
#[derive(Error, Debug)]
pub enum ReplayError {
    #[error("Failed to load replay bundle: {path}")]
    BundleLoadFailed { path: String },
    #[error("Replay divergence at step {step}: expected {expected}, got {actual}")]
    DivergenceDetected {
        step: usize,
        expected: B3Hash,
        actual: B3Hash,
    },
    #[error("Event sequence mismatch: expected {expected} events, got {actual}")]
    EventSequenceMismatch { expected: usize, actual: usize },
    #[error("Hash verification failed: {reason}")]
    HashVerificationFailed { reason: String },
    #[error("Executor error: {error}")]
    ExecutorError { error: String },
    #[error("Invalid replay state: {reason}")]
    InvalidState { reason: String },
}

/// Result type for replay operations
pub type ReplayResult<T> = std::result::Result<T, ReplayError>;

/// Replay state tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayState {
    /// Current step index in the event sequence
    pub current_step: usize,
    /// Total number of steps in the replay
    pub total_steps: usize,
    /// Current logical tick counter
    pub current_tick: u64,
    /// Global seed for deterministic execution
    pub global_seed: B3Hash,
    /// Map of operation IDs to their expected hashes
    pub expected_hashes: HashMap<String, B3Hash>,
    /// Whether replay is complete
    pub is_complete: bool,
    /// Number of verified operations
    pub verified_ops: usize,
}

impl ReplayState {
    /// Create a new replay state
    pub fn new(total_steps: usize, global_seed: B3Hash) -> Self {
        Self {
            current_step: 0,
            total_steps,
            current_tick: 0,
            global_seed,
            expected_hashes: HashMap::new(),
            is_complete: false,
            verified_ops: 0,
        }
    }

    /// Advance to the next step
    pub fn advance_step(&mut self) {
        self.current_step += 1;
        if self.current_step >= self.total_steps {
            self.is_complete = true;
        }
    }

    /// Get progress percentage
    pub fn progress_percent(&self) -> f64 {
        if self.total_steps == 0 {
            0.0
        } else {
            (self.current_step as f64 / self.total_steps as f64) * 100.0
        }
    }
}
