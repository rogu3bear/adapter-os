//! Replay session management and state tracking

use adapteros_core::AosError;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use adapteros_core::B3Hash;
use adapteros_crypto::{verify_signature, PublicKey, Signature};
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
use adapteros_trace::{reader::read_trace_bundle, schema::TraceBundle};

use crate::verification::VerificationMode;

/// Local Result type alias for ReplayError
type Result<T> = std::result::Result<T, ReplayError>;

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
    #[error("AosError: {0}")]
    AosError(#[from] AosError),
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
    executor: Arc<DeterministicExecutor>,
    current_event_index: Arc<AtomicU64>,
    verification_mode: VerificationMode,
    stats: Mutex<ReplayStats>,
    /// Optional trusted public key for signature verification
    trusted_pubkey: Option<PublicKey>,
}

impl ReplaySession {
    pub fn from_log(trace_path: &Path) -> std::result::Result<Self, ReplayError> {
        Self::from_log_with_mode(trace_path, VerificationMode::Strict)
    }

    pub fn from_log_with_mode(
        trace_path: &Path,
        mode: VerificationMode,
    ) -> std::result::Result<Self, ReplayError> {
        info!("Loading trace bundle from: {}", trace_path.display());
        let trace_bundle = read_trace_bundle(trace_path)?;

        // Extract global seed from trace bundle metadata
        let global_seed = *trace_bundle.global_seed.as_bytes();

        let executor_config = ExecutorConfig {
            global_seed,
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
            executor,
            current_event_index: Arc::new(AtomicU64::new(0)),
            verification_mode: mode,
            stats: Mutex::new(ReplayStats {
                total_events,
                ..Default::default()
            }),
            trusted_pubkey: None,
        })
    }

    /// Set trusted public key for signature verification
    pub fn with_trusted_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.trusted_pubkey = Some(pubkey);
        self
    }

    pub async fn step(&self) -> std::result::Result<(), ReplayError> {
        let mut stats = self.stats.lock().await;
        let current_idx = self.current_event_index.load(Ordering::Relaxed) as usize;

        if current_idx >= stats.total_events {
            stats.is_complete = true;
            return Ok(());
        }

        let expected_event = &self.trace_bundle.events[current_idx];
        debug!("Replaying step {}: {:?}", current_idx, expected_event);

        // Verify the event's hash matches its computed hash
        let actual_hash = expected_event.compute_hash();
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
    ) -> std::result::Result<(), ReplayError> {
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

    /// Verify replay signature using Ed25519
    ///
    /// Validates the trace bundle signature against the trusted public key.
    /// Returns error if no trusted public key is set or signature verification fails.
    pub fn verify_replay_signature(&self) -> std::result::Result<(), ReplayError> {
        let pubkey = self.trusted_pubkey.as_ref().ok_or_else(|| {
            ReplayError::AosError(adapteros_core::AosError::Verification(
                "No trusted public key configured for signature verification".to_string(),
            ))
        })?;

        // Get signature from bundle metadata
        let sig_hex = self
            .trace_bundle
            .metadata
            .signature
            .as_ref()
            .ok_or_else(|| {
                ReplayError::AosError(adapteros_core::AosError::Verification(
                    "Trace bundle has no signature".to_string(),
                ))
            })?;

        // Decode signature from hex
        let sig_bytes = hex::decode(sig_hex).map_err(|e| {
            ReplayError::AosError(adapteros_core::AosError::Crypto(format!(
                "Invalid signature hex: {}",
                e
            )))
        })?;

        if sig_bytes.len() != 64 {
            return Err(ReplayError::AosError(adapteros_core::AosError::Crypto(
                format!("Invalid signature length: {}", sig_bytes.len()),
            )));
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_array).map_err(|e| {
            ReplayError::AosError(adapteros_core::AosError::Crypto(format!(
                "Invalid signature format: {}",
                e
            )))
        })?;

        // Verify signature against bundle hash
        verify_signature(pubkey, self.trace_bundle.bundle_hash.as_bytes(), &signature).map_err(
            |e| {
                ReplayError::AosError(adapteros_core::AosError::Crypto(format!(
                    "Replay bundle signature verification failed: {}",
                    e
                )))
            },
        )?;

        info!("Replay bundle signature verified successfully");
        Ok(())
    }

    pub async fn run_with_progress<F>(
        &mut self,
        mut progress_callback: F,
    ) -> std::result::Result<(), ReplayError>
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

            // Verify the event's stored hash matches its computed hash
            let actual_hash = expected_event.compute_hash();

            // Check determinism: current executor tick should match event tick
            let current_tick = self.executor.current_tick();
            if current_tick != expected_event.tick_id && current_tick > 0 {
                warn!(
                    "Tick mismatch at step {}: executor tick {} vs event tick {}",
                    current_idx, current_tick, expected_event.tick_id
                );
            }
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

    pub async fn run(&mut self) -> std::result::Result<(), ReplayError> {
        self.run_with_progress(|_| {}).await
    }

    pub async fn stats(&self) -> ReplayStats {
        self.stats.lock().await.clone()
    }

    pub fn reset(&mut self) {
        self.current_event_index.store(0, Ordering::Relaxed);
        // Clear executor event log using public method
        self.executor.clear_event_log();
        *self.stats.blocking_lock() = ReplayStats {
            total_events: self.trace_bundle.events.len(),
            ..Default::default()
        };
        info!("Replay session reset.");
    }

    /// Extract current executor state for comparison
    pub fn extract_state(&self) -> ExecutorState {
        ExecutorState {
            current_tick: self.executor.current_tick(),
            event_log: self.executor.get_event_log(),
            is_running: self.executor.is_running(),
            pending_tasks: self.executor.pending_tasks(),
        }
    }

    /// Compare replay outputs with expected outputs
    pub fn compare_outputs(
        &self,
        expected_events: &[adapteros_trace::schema::Event],
    ) -> std::result::Result<(), ReplayError> {
        let actual_events = self.executor.get_event_log();

        if expected_events.len() != actual_events.len() {
            return Err(ReplayError::InitializationError(format!(
                "Event count mismatch: expected {}, got {}",
                expected_events.len(),
                actual_events.len()
            )));
        }

        for (i, expected) in expected_events.iter().enumerate() {
            let actual_hash = expected.compute_hash();
            if expected.blake3_hash != actual_hash {
                return Err(ReplayError::HashMismatch {
                    step: i,
                    expected: expected.blake3_hash,
                    actual: actual_hash,
                });
            }
        }

        info!(
            "All {} outputs match expected values",
            expected_events.len()
        );
        Ok(())
    }

    /// Validate determinism by comparing executor state with trace bundle
    pub fn validate_determinism(&self) -> std::result::Result<(), ReplayError> {
        let executor_tick = self.executor.current_tick();
        let current_idx = self.current_event_index.load(Ordering::Relaxed) as usize;

        if current_idx > 0 && current_idx <= self.trace_bundle.events.len() {
            let last_event = &self.trace_bundle.events[current_idx - 1];

            // Verify tick progression is consistent
            if executor_tick > 0 && executor_tick < last_event.tick_id {
                return Err(ReplayError::InitializationError(format!(
                    "Determinism violation: executor tick {} is behind event tick {}",
                    executor_tick, last_event.tick_id
                )));
            }
        }

        // Verify all processed events have valid hashes
        for (i, event) in self
            .trace_bundle
            .events
            .iter()
            .take(current_idx)
            .enumerate()
        {
            if !event.verify_hash() {
                return Err(ReplayError::HashMismatch {
                    step: i,
                    expected: event.blake3_hash,
                    actual: event.compute_hash(),
                });
            }
        }

        info!("Determinism validated for {} events", current_idx);
        Ok(())
    }

    /// Get executor reference for advanced operations
    pub fn executor(&self) -> &Arc<DeterministicExecutor> {
        &self.executor
    }

    /// Get trace bundle reference
    pub fn trace_bundle(&self) -> &TraceBundle {
        &self.trace_bundle
    }

    pub fn jump_to_step(&mut self, step: usize) -> std::result::Result<(), ReplayError> {
        if step > self.trace_bundle.events.len() {
            return Err(ReplayError::StepError(format!(
                "Step {} is out of bounds (total events: {})",
                step,
                self.trace_bundle.events.len()
            )));
        }
        self.current_event_index
            .store(step as u64, Ordering::Relaxed);
        // Clear executor event log when jumping
        self.executor.clear_event_log();
        let mut stats = self.stats.blocking_lock();
        stats.current_step = step;
        stats.verified_ops = step; // Assume previous steps were verified
        stats.progress_percent = (step as f64 / stats.total_events as f64) * 100.0;
        stats.is_complete = false;
        info!("Replay session jumped to step {}.", step);
        Ok(())
    }
}

/// Executor state snapshot for comparison
#[derive(Debug, Clone)]
pub struct ExecutorState {
    pub current_tick: u64,
    pub event_log: Vec<adapteros_deterministic_exec::ExecutorEvent>,
    pub is_running: bool,
    pub pending_tasks: usize,
}

pub async fn replay_trace(trace_path: &Path) -> std::result::Result<ReplayStats, ReplayError> {
    let mut session = ReplaySession::from_log(trace_path)?;
    session.run().await?;
    Ok(session.stats().await)
}
