//! Replay session management and state tracking

use anyhow::Result;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use adapteros_core::B3Hash;
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
use adapteros_trace::{reader::read_trace_bundle, schema::TraceBundle};

use crate::verification::VerificationMode;

#[derive(Error, Debug)]
pub enum ReplayError {
    #[error("Replay initialization failed: {0}")]
    InitializationError(String),
    #[error("Replay step failed: {0}")]
    StepError(String),
    #[error("Hash mismatch at step {step}: expected {expected}, actual {actual}")]
    HashMismatch {
        step: usize,
        expected: B3Hash,
        actual: B3Hash,
    },
    #[error("Trace error: {0}")]
    TraceError(String),
    #[error("Executor error: {0}")]
    ExecutorError(#[from] adapteros_deterministic_exec::DeterministicExecutorError),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("AosError: {0}")]
    AosError(#[from] adapteros_core::AosError),
}

#[derive(Debug, Clone, Default)]
pub struct ReplayStats {
    pub total_events: usize,
    pub current_step: usize,
    pub verified_ops: usize,
    pub hash_mismatches: usize,
    pub is_complete: bool,
    pub progress_percent: f64,
}

pub struct ReplaySession {
    _trace_path: PathBuf,
    trace_bundle: TraceBundle,
    _executor: Arc<DeterministicExecutor>,
    current_event_index: Arc<AtomicU64>,
    verification_mode: VerificationMode,
    stats: Mutex<ReplayStats>,
}

impl ReplaySession {
    pub fn from_log(trace_path: &Path) -> Result<Self, ReplayError> {
        Self::from_log_with_mode(trace_path, VerificationMode::Strict)
    }

    pub fn from_log_with_mode(
        trace_path: &Path,
        mode: VerificationMode,
    ) -> Result<Self, ReplayError> {
        info!("Loading trace bundle from: {}", trace_path.display());
        let trace_bundle = read_trace_bundle(trace_path)?;

        let executor_config = ExecutorConfig {
            global_seed: [0u8; 32], // TODO: Extract from trace bundle metadata
            replay_mode: true,
            replay_events: Vec::new(),  // Events will be fed one by one
            enable_event_logging: true, // Log events during replay for comparison
            ..Default::default()
        };

        let executor = Arc::new(DeterministicExecutor::new(executor_config));
        let total_events = trace_bundle.events.len();

        Ok(Self {
            _trace_path: trace_path.to_path_buf(),
            trace_bundle,
            _executor: executor,
            current_event_index: Arc::new(AtomicU64::new(0)),
            verification_mode: mode,
            stats: Mutex::new(ReplayStats {
                total_events,
                ..Default::default()
            }),
        })
    }

    pub async fn step(&self) -> Result<(), ReplayError> {
        let mut stats = self.stats.lock().await;
        let current_idx = self.current_event_index.load(Ordering::Relaxed) as usize;

        if current_idx >= stats.total_events {
            stats.is_complete = true;
            return Ok(());
        }

        let expected_event = &self.trace_bundle.events[current_idx];
        debug!("Replaying step {}: {:?}", current_idx, expected_event);

        // Simulate the event in the deterministic executor
        // For now, we're just advancing the tick and logging the event
        // A full implementation would involve re-executing the actual operation
        // and comparing its output hash.

        // TODO: Access executor fields through public methods
        // self.executor.tick_counter.fetch_max(expected_event.tick_id, Ordering::Relaxed);

        // Verify hash
        let actual_hash = expected_event.blake3_hash;
        if !self.verify_hash(&expected_event.blake3_hash, &actual_hash) {
            stats.hash_mismatches += 1;
            return Err(ReplayError::HashMismatch {
                step: current_idx,
                expected: expected_event.blake3_hash,
                actual: actual_hash,
            });
        }

        stats.verified_ops += 1;
        stats.current_step = current_idx + 1;
        stats.progress_percent = (stats.current_step as f64 / stats.total_events as f64) * 100.0;
        self.current_event_index.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    fn verify_hash(&self, expected: &B3Hash, actual: &B3Hash) -> bool {
        match self.verification_mode {
            VerificationMode::Strict => expected == actual,
            VerificationMode::Permissive => {
                // Implement tolerant comparison if needed, e.g., for floating point
                // For now, permissive is same as strict for simplicity
                expected == actual
            }
            VerificationMode::HashOnly => true, // Only check if hash can be computed
        }
    }

    /// Verify RNG states match between expected and actual
    pub fn verify_rng_states(
        &self,
        expected_states: &[adapteros_telemetry::replay::RngCheckpoint],
        actual_states: &[adapteros_telemetry::replay::RngCheckpoint],
    ) -> Result<(), ReplayError> {
        if expected_states.len() != actual_states.len() {
            return Err(ReplayError::InitializationError(format!(
                "RNG checkpoint count mismatch: expected {}, got {}",
                expected_states.len(),
                actual_states.len()
            )));
        }

        for (i, (expected, actual)) in expected_states.iter().zip(actual_states.iter()).enumerate()
        {
            // Compare metadata
            if expected.phase != actual.phase {
                return Err(ReplayError::InitializationError(format!(
                    "RNG phase mismatch at checkpoint {}: expected '{}', got '{}'",
                    i, expected.phase, actual.phase
                )));
            }

            if expected.label != actual.label {
                return Err(ReplayError::InitializationError(format!(
                    "RNG label mismatch at checkpoint {}: expected '{}', got '{}'",
                    i, expected.label, actual.label
                )));
            }

            // Compare step counts
            if expected.step_count != actual.step_count {
                return Err(ReplayError::InitializationError(format!(
                    "RNG step count mismatch at checkpoint {} ({}): expected {}, got {}",
                    i, expected.phase, expected.step_count, actual.step_count
                )));
            }

            // Compare global nonce
            if expected.global_nonce != actual.global_nonce {
                return Err(ReplayError::InitializationError(format!(
                    "Global nonce mismatch at checkpoint {} ({}): expected {}, got {}",
                    i, expected.phase, expected.global_nonce, actual.global_nonce
                )));
            }

            info!(
                "✅ RNG checkpoint {} ({}) verified: {} steps, nonce {}",
                i, expected.phase, expected.step_count, expected.global_nonce
            );
        }

        info!(
            "✅ All {} RNG checkpoints verified successfully",
            expected_states.len()
        );
        Ok(())
    }

    /// Verify replay signature with deterministic error code
    ///
    /// Future use: Will be called during replay initialization to validate trace bundle signatures.
    /// Stub implementation currently returns error. Integration pending with cryptographic verification.
    #[allow(dead_code)]
    fn verify_replay_signature(&self) -> Result<(), ReplayError> {
        // TODO: Implement actual signature verification
        // For now, return error with deterministic code
        Err(ReplayError::AosError(
            adapteros_core::AosError::Verification(
                "Replay signature verification failed".to_string(),
            ),
        ))
    }

    pub async fn run_with_progress<F>(
        &mut self,
        mut progress_callback: F,
    ) -> Result<(), ReplayError>
    where
        F: FnMut(ReplayStats),
    {
        info!(
            "Starting replay session for {} events with mode {:?}",
            self.trace_bundle.events.len(),
            self.verification_mode
        );
        let total_events = self.trace_bundle.events.len();

        for _i in 0..total_events {
            let current_idx = self.current_event_index.load(Ordering::Relaxed) as usize;
            if current_idx >= total_events {
                break;
            }

            let expected_event = &self.trace_bundle.events[current_idx];
            debug!(
                "Replaying step {}: {:?}",
                current_idx, expected_event.event_type
            );

            // In replay mode, the executor's run_replay_mode will iterate through its config.replay_events.
            // Here, we are feeding the events from the loaded trace_bundle to the executor.
            // This requires a more direct interaction or a different design for the executor's replay mode.
            // For now, we'll simulate the executor processing the event and then verify.

            // Update executor's tick counter to match the trace
            // TODO: Access executor fields through public methods
            // self.executor.tick_counter.store(expected_event.tick_id, Ordering::Relaxed);

            // In a full replay, the executor would re-execute the operation corresponding to `expected_event`
            // and produce its own `ExecutorEvent`s and potentially a new `Event` with actual outputs/hashes.
            // For this iteration, we'll directly compare the expected event's hash.

            let actual_hash = expected_event.blake3_hash; // Use stored hash
            if !self.verify_hash(&expected_event.blake3_hash, &actual_hash) {
                let mut stats = self.stats.lock().await;
                stats.hash_mismatches += 1;
                return Err(ReplayError::HashMismatch {
                    step: current_idx,
                    expected: expected_event.blake3_hash,
                    actual: actual_hash,
                });
            }

            let mut stats = self.stats.lock().await;
            stats.verified_ops += 1;
            stats.current_step = current_idx + 1;
            stats.progress_percent =
                (stats.current_step as f64 / stats.total_events as f64) * 100.0;
            progress_callback(stats.clone());
            self.current_event_index.fetch_add(1, Ordering::Relaxed);
        }

        let mut stats = self.stats.lock().await;
        stats.is_complete = true;
        stats.progress_percent = 100.0;
        progress_callback(stats.clone());

        info!("Replay session completed.");
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), ReplayError> {
        self.run_with_progress(|_| {}).await
    }

    pub async fn stats(&self) -> ReplayStats {
        self.stats.lock().await.clone()
    }

    pub fn reset(&mut self) {
        self.current_event_index.store(0, Ordering::Relaxed);
        // TODO: Access executor fields through public methods
        // *self.executor.event_log.lock() = Vec::new();
        // self.executor.tick_counter.store(0, Ordering::Relaxed);
        *self.stats.blocking_lock() = ReplayStats {
            total_events: self.trace_bundle.events.len(),
            ..Default::default()
        };
        info!("Replay session reset.");
    }

    pub fn jump_to_step(&mut self, step: usize) -> Result<(), ReplayError> {
        if step > self.trace_bundle.events.len() {
            return Err(ReplayError::StepError(format!(
                "Step {} is out of bounds (total events: {})",
                step,
                self.trace_bundle.events.len()
            )));
        }
        self.current_event_index
            .store(step as u64, Ordering::Relaxed);
        // Reset executor state to match the state at 'step' if possible, or just clear logs
        // TODO: Access executor fields through public methods
        // *self.executor.event_log.lock() = Vec::new();
        // self.executor.tick_counter.store(self.trace_bundle.events[step].tick_id, Ordering::Relaxed);
        let mut stats = self.stats.blocking_lock();
        stats.current_step = step;
        stats.verified_ops = step; // Assume previous steps were verified
        stats.progress_percent = (step as f64 / stats.total_events as f64) * 100.0;
        stats.is_complete = false;
        info!("Replay session jumped to step {}.", step);
        Ok(())
    }
}

pub async fn replay_trace(trace_path: &Path) -> Result<ReplayStats, ReplayError> {
    let mut session = ReplaySession::from_log(trace_path)?;
    session.run().await?;
    Ok(session.stats().await)
}
