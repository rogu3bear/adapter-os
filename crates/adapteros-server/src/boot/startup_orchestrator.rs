//! Startup orchestrator for resilient boot sequencing.
//!
//! This module centralizes phase retries, lightweight circuit breakers, startup
//! error classification, and immutable startup audit events persisted to
//! `var/run/startup_audit.jsonl`.

use adapteros_server_api::boot_state::BootStateManager;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_base: Duration,
    pub circuit_breaker_threshold: u32,
}

impl RetryPolicy {
    pub fn with_attempts(max_attempts: u32) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            backoff_base: Duration::from_millis(400),
            circuit_breaker_threshold: 3,
        }
    }

    pub fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            backoff_base: Duration::from_millis(0),
            circuit_breaker_threshold: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StartupRecoveryPath {
    Retry,
    Degrade,
    OperatorMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupErrorTaxonomy {
    pub phase: String,
    pub code: String,
    pub recoverable: bool,
    pub recovery_path: StartupRecoveryPath,
    pub operator_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupAuditEvent {
    pub timestamp: String,
    pub phase: String,
    pub attempt: u32,
    pub event: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StartupCircuitStatus {
    pub phase: String,
    pub consecutive_failures: u32,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupSnapshot {
    pub determinism_seed_initialized: bool,
    pub replay_ready: bool,
    pub audit_events: usize,
    pub circuits: Vec<StartupCircuitStatus>,
}

#[derive(Debug, Clone, Default)]
struct CircuitState {
    consecutive_failures: u32,
    open: bool,
}

#[derive(Clone)]
pub struct StartupOrchestrator {
    boot_state: BootStateManager,
    circuits: Arc<Mutex<HashMap<String, CircuitState>>>,
    audit_events: Arc<Mutex<Vec<StartupAuditEvent>>>,
    determinism_seed_initialized: Arc<AtomicBool>,
    replay_ready: Arc<AtomicBool>,
}

impl StartupOrchestrator {
    pub fn new(boot_state: BootStateManager) -> Self {
        Self {
            boot_state,
            circuits: Arc::new(Mutex::new(HashMap::new())),
            audit_events: Arc::new(Mutex::new(Vec::new())),
            determinism_seed_initialized: Arc::new(AtomicBool::new(false)),
            replay_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn snapshot(&self) -> StartupSnapshot {
        let circuits = self
            .circuits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(phase, state)| StartupCircuitStatus {
                phase: phase.clone(),
                consecutive_failures: state.consecutive_failures,
                open: state.open,
            })
            .collect::<Vec<_>>();

        StartupSnapshot {
            determinism_seed_initialized: self.determinism_seed_initialized.load(Ordering::Relaxed),
            replay_ready: self.replay_ready.load(Ordering::Relaxed),
            audit_events: self
                .audit_events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .len(),
            circuits,
        }
    }

    pub fn mark_determinism_seed_initialized(&self, initialized: bool) {
        self.determinism_seed_initialized
            .store(initialized, Ordering::Relaxed);
        self.record_event(
            "determinism_gate",
            1,
            if initialized {
                "seed_initialized"
            } else {
                "seed_missing"
            },
            "Deterministic seed status updated",
            None,
        );
    }

    pub fn mark_replay_ready(&self, ready: bool) {
        self.replay_ready.store(ready, Ordering::Relaxed);
        self.record_event(
            "replay_gate",
            1,
            if ready {
                "replay_ready"
            } else {
                "replay_missing"
            },
            "Replay readiness status updated",
            None,
        );
    }

    pub fn ensure_runtime_gates_ready(&self) -> Result<()> {
        if !self
            .determinism_seed_initialized
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(anyhow!(
                "Determinism seed was not initialized; refusing to accept inference traffic"
            ));
        }
        if !self.replay_ready.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(anyhow!(
                "Replay machinery not ready; refusing to accept inference traffic"
            ));
        }
        Ok(())
    }

    pub async fn run_phase<T, F, Fut>(
        &self,
        phase: &'static str,
        failure_code: &'static str,
        policy: RetryPolicy,
        mut op: F,
        mut degrade_to: Option<T>,
    ) -> Result<T>
    where
        F: FnMut(u32) -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        if self.is_circuit_open(phase) {
            self.boot_state.finish_phase_err(
                phase,
                failure_code,
                Some("startup circuit breaker is open for this phase".to_string()),
            );
            return Err(anyhow!(
                "startup circuit breaker open for phase '{}'; operator action required",
                phase
            ));
        }

        self.boot_state.start_phase(phase);

        for attempt in 1..=policy.max_attempts.max(1) {
            self.record_event(
                phase,
                attempt,
                "attempt_started",
                "Startup phase attempt started",
                None,
            );

            match op(attempt).await {
                Ok(value) => {
                    self.on_phase_success(phase);
                    self.boot_state.finish_phase_ok(phase);
                    self.record_event(
                        phase,
                        attempt,
                        "attempt_succeeded",
                        "Startup phase completed",
                        None,
                    );
                    return Ok(value);
                }
                Err(error) => {
                    let taxonomy = classify_startup_error(phase, &error);
                    self.on_phase_failure(phase, policy.circuit_breaker_threshold);

                    self.record_event(
                        phase,
                        attempt,
                        "attempt_failed",
                        &taxonomy.operator_message,
                        Some(taxonomy.code.clone()),
                    );

                    let can_retry = taxonomy.recovery_path == StartupRecoveryPath::Retry
                        && taxonomy.recoverable
                        && attempt < policy.max_attempts
                        && !self.is_circuit_open(phase);

                    if can_retry {
                        let sleep_for = policy
                            .backoff_base
                            .saturating_mul(1u32 << (attempt.saturating_sub(1)));
                        warn!(
                            phase = phase,
                            attempt = attempt,
                            retry_after_ms = sleep_for.as_millis() as u64,
                            code = taxonomy.code,
                            "Startup phase failed; retrying"
                        );
                        tokio::time::sleep(sleep_for).await;
                        continue;
                    }

                    if taxonomy.recovery_path == StartupRecoveryPath::Degrade {
                        if let Some(value) = degrade_to.take() {
                            self.boot_state.record_boot_warning(
                                phase,
                                format!(
                                    "{} (code: {}, phase: {})",
                                    taxonomy.operator_message, taxonomy.code, taxonomy.phase
                                ),
                            );
                            self.boot_state.finish_phase_ok(phase);
                            self.record_event(
                                phase,
                                attempt,
                                "degraded_continue",
                                &taxonomy.operator_message,
                                Some(taxonomy.code.clone()),
                            );
                            return Ok(value);
                        }
                    }

                    self.boot_state.finish_phase_err(
                        phase,
                        failure_code,
                        Some(format!(
                            "{} (code: {})",
                            taxonomy.operator_message, taxonomy.code
                        )),
                    );
                    return Err(error);
                }
            }
        }

        Err(anyhow!(
            "phase '{}' exited without success, retry, or terminal failure",
            phase
        ))
    }

    fn on_phase_success(&self, phase: &str) {
        let mut circuits = self.circuits.lock().unwrap_or_else(|e| e.into_inner());
        let state = circuits.entry(phase.to_string()).or_default();
        state.consecutive_failures = 0;
        state.open = false;
    }

    fn on_phase_failure(&self, phase: &str, threshold: u32) {
        let mut circuits = self.circuits.lock().unwrap_or_else(|e| e.into_inner());
        let state = circuits.entry(phase.to_string()).or_default();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if state.consecutive_failures >= threshold.max(1) {
            state.open = true;
            warn!(
                phase = phase,
                consecutive_failures = state.consecutive_failures,
                threshold = threshold,
                "Startup phase circuit opened"
            );
        }
    }

    fn is_circuit_open(&self, phase: &str) -> bool {
        self.circuits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(phase)
            .map(|state| state.open)
            .unwrap_or(false)
    }

    fn record_event(
        &self,
        phase: &str,
        attempt: u32,
        event: &str,
        message: &str,
        code: Option<String>,
    ) {
        let entry = StartupAuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            phase: phase.to_string(),
            attempt,
            event: event.to_string(),
            message: message.to_string(),
            code,
        };

        self.audit_events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entry.clone());
        adapteros_server_api::middleware::audit::record_startup_audit_event(
            entry.phase.clone(),
            entry.event.clone(),
            entry.message.clone(),
            entry.code.clone(),
        );
    }
}

pub fn classify_startup_error(phase: &str, error: &anyhow::Error) -> StartupErrorTaxonomy {
    let msg = error.to_string();
    let lowered = msg.to_ascii_lowercase();

    if lowered.contains("timeout")
        || lowered.contains("temporar")
        || lowered.contains("connection reset")
        || lowered.contains("unavailable")
        || lowered.contains("would block")
    {
        return StartupErrorTaxonomy {
            phase: phase.to_string(),
            code: "STARTUP_TRANSIENT_DEPENDENCY".to_string(),
            recoverable: true,
            recovery_path: StartupRecoveryPath::Retry,
            operator_message: format!(
                "Transient dependency issue while executing '{}'; retrying with backoff",
                phase
            ),
        };
    }

    if phase == "startup_recovery" || phase == "metrics_init" {
        return StartupErrorTaxonomy {
            phase: phase.to_string(),
            code: "STARTUP_DEGRADED_COMPONENT".to_string(),
            recoverable: false,
            recovery_path: StartupRecoveryPath::Degrade,
            operator_message: format!(
                "Non-critical startup component '{}' failed; continuing in degraded mode",
                phase
            ),
        };
    }

    let code = if lowered.contains("determin")
        || lowered.contains("replay")
        || lowered.contains("invariant")
    {
        "STARTUP_DETERMINISM_GATE"
    } else if lowered.contains("migration") || lowered.contains("database") {
        "STARTUP_DATABASE_GATE"
    } else if lowered.contains("model server") || lowered.contains("worker") {
        "STARTUP_WORKER_GATE"
    } else {
        "STARTUP_OPERATOR_ACTION_REQUIRED"
    };

    StartupErrorTaxonomy {
        phase: phase.to_string(),
        code: code.to_string(),
        recoverable: false,
        recovery_path: StartupRecoveryPath::OperatorMessage,
        operator_message: format!("Startup phase '{}' failed: {}", phase, msg),
    }
}

pub fn log_startup_snapshot(snapshot: &StartupSnapshot) {
    info!(
        determinism_seed_initialized = snapshot.determinism_seed_initialized,
        replay_ready = snapshot.replay_ready,
        circuit_count = snapshot.circuits.len(),
        startup_audit_events = snapshot.audit_events,
        "Startup orchestrator snapshot"
    );
}
