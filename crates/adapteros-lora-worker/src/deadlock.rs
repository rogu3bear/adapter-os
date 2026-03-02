//! Deadlock detection and recovery mechanisms
//!
//! Implements deadlock detection and recovery to prevent runaway processes.
//! Aligns with Determinism Ruleset #2 and Performance Ruleset #11 from policy enforcement.

use adapteros_config::effective::WorkerSafetySection;
use adapteros_core::retry_policy::{RetryManager, RetryPolicy};
use adapteros_core::{AosError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{error, warn};

#[derive(Debug, Clone, Copy)]
enum DeadlockRecoveryStatus {
    Requested,
    Failed,
    NotConfigured,
}

impl DeadlockRecoveryStatus {
    fn as_str(self) -> &'static str {
        match self {
            DeadlockRecoveryStatus::Requested => "requested",
            DeadlockRecoveryStatus::Failed => "failed",
            DeadlockRecoveryStatus::NotConfigured => "not_configured",
        }
    }

    fn exit_reason(self) -> &'static str {
        match self {
            DeadlockRecoveryStatus::Requested => "deadlock_recovery_requested",
            DeadlockRecoveryStatus::Failed => "deadlock_recovery_failed",
            DeadlockRecoveryStatus::NotConfigured => "deadlock_recovery_not_configured",
        }
    }

    fn exit_code(self) -> i32 {
        match self {
            DeadlockRecoveryStatus::Requested => 111,
            DeadlockRecoveryStatus::Failed => 112,
            DeadlockRecoveryStatus::NotConfigured => 113,
        }
    }
}

#[derive(Debug, Clone)]
struct DeadlockRecoveryOutcome {
    status: DeadlockRecoveryStatus,
    details: String,
    error: Option<String>,
}

impl DeadlockRecoveryOutcome {
    fn requested(details: String) -> Self {
        Self {
            status: DeadlockRecoveryStatus::Requested,
            details,
            error: None,
        }
    }

    fn failed(error: String) -> Self {
        Self {
            status: DeadlockRecoveryStatus::Failed,
            details: "supervisor_restart_or_handshake_failed".to_string(),
            error: Some(error),
        }
    }

    fn not_configured(details: String) -> Self {
        Self {
            status: DeadlockRecoveryStatus::NotConfigured,
            details,
            error: None,
        }
    }
}

#[derive(Debug)]
struct SupervisorHttpResponse {
    status_code: u16,
    body: String,
}

#[derive(Debug, Deserialize)]
struct SupervisorServiceStatus {
    state: Option<String>,
    restart_count: Option<u32>,
}

/// Deadlock detection configuration
#[derive(Debug, Clone)]
pub struct DeadlockConfig {
    pub check_interval: Duration,
    pub max_wait_time: Duration,
    pub max_lock_depth: usize,
    pub recovery_timeout: Duration,
}

impl Default for DeadlockConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(5),
            max_wait_time: Duration::from_secs(30),
            max_lock_depth: 10,
            recovery_timeout: Duration::from_secs(10),
        }
    }
}

impl DeadlockConfig {
    pub fn from_effective_section(worker_safety: &WorkerSafetySection) -> Self {
        Self {
            check_interval: Duration::from_secs(worker_safety.deadlock_check_interval_secs),
            max_wait_time: Duration::from_secs(worker_safety.max_wait_time_secs),
            max_lock_depth: worker_safety.max_lock_depth,
            recovery_timeout: Duration::from_secs(worker_safety.recovery_timeout_secs),
        }
    }

    pub fn from_effective_section_or_default(worker_safety: Option<&WorkerSafetySection>) -> Self {
        worker_safety
            .map(Self::from_effective_section)
            .unwrap_or_default()
    }
}

/// Lock information for deadlock detection
#[derive(Debug, Clone)]
struct LockInfo {
    thread_id: u64,
    lock_id: String,
    acquired_at: Instant,
    /// Stack trace for debugging (reserved for detailed deadlock analysis)
    _stack_trace: String,
}

/// Deadlock detector
pub struct DeadlockDetector {
    config: DeadlockConfig,
    locks: Arc<Mutex<HashMap<String, LockInfo>>>,
    thread_locks: Arc<Mutex<HashMap<u64, Vec<String>>>>,
    deadlock_count: Arc<Mutex<usize>>,
    recovery_in_progress: Arc<Mutex<bool>>,
}

impl DeadlockDetector {
    pub fn new(config: DeadlockConfig) -> Self {
        Self {
            config,
            locks: Arc::new(Mutex::new(HashMap::new())),
            thread_locks: Arc::new(Mutex::new(HashMap::new())),
            deadlock_count: Arc::new(Mutex::new(0)),
            recovery_in_progress: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn start_monitoring(&self) -> Result<()> {
        let mut interval = interval(self.config.check_interval);

        loop {
            interval.tick().await;

            if let Err(e) = self.check_for_deadlocks().await {
                error!("Deadlock detection failed: {}", e);
                // Continue monitoring even if detection fails
            }
        }
    }

    async fn check_for_deadlocks(&self) -> Result<()> {
        // Collect lock info in first scope
        let lock_infos: Vec<_> = {
            let locks = self.locks.lock().await;
            locks.values().cloned().collect()
        };
        // First lock released

        // Collect thread info in second scope
        let thread_info: HashMap<_, _> = {
            let thread_locks = self.thread_locks.lock().await;
            thread_locks.clone()
        };
        // Second lock released

        let now = Instant::now();

        // Now process without holding any locks
        for lock_info in lock_infos {
            if now.duration_since(lock_info.acquired_at) > self.config.max_wait_time {
                warn!(
                    "Lock {} held for {} seconds by thread {}",
                    lock_info.lock_id,
                    now.duration_since(lock_info.acquired_at).as_secs(),
                    lock_info.thread_id
                );

                // Check if this might be a deadlock
                if self.is_potential_deadlock(&lock_info, &thread_info) {
                    error!("Potential deadlock detected on lock {}", lock_info.lock_id);
                    self.trigger_deadlock_recovery(&lock_info.lock_id).await?;
                }
            }
        }

        Ok(())
    }

    fn is_potential_deadlock(
        &self,
        lock_info: &LockInfo,
        thread_locks: &HashMap<u64, Vec<String>>,
    ) -> bool {
        // Simple deadlock detection: check if thread is waiting for locks held by other threads
        if let Some(thread_locks) = thread_locks.get(&lock_info.thread_id) {
            // Check if any of the locks this thread is waiting for are held by other threads
            for waiting_lock in thread_locks {
                if waiting_lock != &lock_info.lock_id {
                    // This is a simplified check - in practice, you'd need more sophisticated cycle detection
                    return true;
                }
            }
        }
        false
    }

    async fn trigger_deadlock_recovery(&self, lock_id: &str) -> Result<()> {
        // Check if recovery is already in progress
        {
            let mut recovery = self.recovery_in_progress.lock().await;
            if *recovery {
                warn!("Deadlock recovery already in progress, skipping");
                return Ok(());
            }
            *recovery = true;
        }

        error!(lock_id = %lock_id, "Deadlock detected - initiating recovery");

        // Increment deadlock count for metrics
        {
            let mut count = self.deadlock_count.lock().await;
            *count += 1;
        }

        let outcome = if let Some(url) = supervisor_base_url() {
            match trigger_supervisor_restart(&url).await {
                Ok(details) => DeadlockRecoveryOutcome::requested(details),
                Err(e) => {
                    warn!(error = %e, "Supervisor restart attempt failed");
                    DeadlockRecoveryOutcome::failed(e.to_string())
                }
            }
        } else {
            warn!("Supervisor not configured (SUPERVISOR_API_URL/AOS_PANEL_PORT missing)");
            DeadlockRecoveryOutcome::not_configured(
                "missing_supervisor_api_url_or_panel_port".to_string(),
            )
        };

        write_deadlock_artifact(lock_id, &outcome);

        warn!(
            restart_status = outcome.status.as_str(),
            exit_reason = outcome.status.exit_reason(),
            "Exiting worker after deadlock recovery artifact write"
        );
        std::process::exit(outcome.status.exit_code());
    }

    pub async fn record_lock_acquisition(&self, lock_id: String, thread_id: u64) {
        let lock_info = LockInfo {
            thread_id,
            lock_id: lock_id.clone(),
            acquired_at: Instant::now(),
            _stack_trace: "".to_string(), // Would capture actual stack trace
        };

        self.locks.lock().await.insert(lock_id.clone(), lock_info);
        self.thread_locks
            .lock()
            .await
            .entry(thread_id)
            .or_insert_with(Vec::new)
            .push(lock_id);
    }

    pub async fn record_lock_release(&self, lock_id: &str, thread_id: u64) {
        self.locks.lock().await.remove(lock_id);
        self.thread_locks
            .lock()
            .await
            .entry(thread_id)
            .and_modify(|locks| locks.retain(|id| id != lock_id));
    }

    pub async fn get_deadlock_count(&self) -> usize {
        *self.deadlock_count.lock().await
    }

    pub async fn is_recovery_in_progress(&self) -> bool {
        *self.recovery_in_progress.lock().await
    }
}

fn write_deadlock_artifact(lock_id: &str, outcome: &DeadlockRecoveryOutcome) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let pid = std::process::id();

    let filename = format!("deadlock-{}-{}.log", pid, timestamp);
    let mut payload = format!(
        "timestamp={}\npid={}\nlock_id={}\nrestart_status={}\nrestart_details={}\nexit_reason={}\nexit_code={}\n",
        timestamp,
        pid,
        lock_id,
        outcome.status.as_str(),
        sanitize_artifact_value(&outcome.details),
        outcome.status.exit_reason(),
        outcome.status.exit_code()
    );
    if let Some(err) = outcome.error.as_deref() {
        payload.push_str(&format!("restart_error={}\n", sanitize_artifact_value(err)));
    }

    let candidate_dirs = [
        adapteros_core::rebase_var_path("var/deadlock"),
        std::env::temp_dir().join("adapteros-deadlock"),
    ];

    for artifact_dir in candidate_dirs {
        if let Err(e) = std::fs::create_dir_all(&artifact_dir) {
            warn!(
                error = %e,
                path = %artifact_dir.display(),
                "Failed to create deadlock artifact directory"
            );
            continue;
        }

        let artifact_path = artifact_dir.join(&filename);
        if let Err(e) = std::fs::write(&artifact_path, &payload) {
            warn!(
                error = %e,
                path = %artifact_path.display(),
                "Failed to write deadlock artifact"
            );
            continue;
        }

        return;
    }

    warn!("Deadlock artifact persistence failed across all artifact locations");
}

fn sanitize_artifact_value(value: &str) -> String {
    value
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn supervisor_base_url() -> Option<String> {
    std::env::var("SUPERVISOR_API_URL").ok().or_else(|| {
        std::env::var("AOS_PANEL_PORT")
            .ok()
            .map(|port| format!("http://127.0.0.1:{}", port))
    })
}

fn supervisor_service_id() -> Option<String> {
    std::env::var("AOS_SUPERVISOR_SERVICE_ID")
        .ok()
        .or_else(|| std::env::var("WORKER_ID").ok())
}

fn supervisor_token() -> Option<String> {
    std::env::var("SUPERVISOR_API_TOKEN")
        .ok()
        .or_else(|| std::env::var("AOS_PANEL_TOKEN").ok())
}

async fn trigger_supervisor_restart(base_url: &str) -> Result<String> {
    let Some(service_id) = supervisor_service_id() else {
        return Err(adapteros_core::AosError::Config(
            "Supervisor service ID not configured (AOS_SUPERVISOR_SERVICE_ID/WORKER_ID)"
                .to_string(),
        ));
    };

    let token = supervisor_token().ok_or_else(|| {
        adapteros_core::AosError::Config(
            "Supervisor API token not configured (SUPERVISOR_API_TOKEN/AOS_PANEL_TOKEN)"
                .to_string(),
        )
    })?;

    let restart_url = format!("{}/v1/services/restart", base_url);
    let payload = format!("{{\"service_id\":\"{}\"}}", service_id);
    let restart_policy = supervisor_retry_policy("deadlock_restart_request");
    let restart_manager = RetryManager::from_policy_defaults(&restart_policy);

    restart_manager
        .execute_with_policy(&restart_policy, || {
            let restart_url = restart_url.clone();
            let token = token.clone();
            let payload = payload.clone();
            Box::pin(async move {
                let response = perform_supervisor_http_request(
                    "POST".to_string(),
                    restart_url,
                    token,
                    Some(payload),
                )
                .await?;

                if !(200..300).contains(&response.status_code) {
                    return Err(AosError::Network(format!(
                        "Supervisor restart request rejected: status={} body={}",
                        response.status_code,
                        trim_http_body_for_error(&response.body)
                    )));
                }

                Ok(())
            })
        })
        .await?;

    let handshake_details = verify_restart_handshake(base_url, &service_id, &token).await?;
    Ok(handshake_details)
}

fn supervisor_retry_policy(service_type: &str) -> RetryPolicy {
    RetryPolicy {
        service_type: service_type.to_string(),
        deterministic_jitter: true,
        ..Default::default()
    }
}

fn trim_http_body_for_error(body: &str) -> String {
    let trimmed = body.trim();
    const MAX_ERROR_BODY: usize = 240;
    let mut chars = trimmed.chars();
    let clipped: String = chars.by_ref().take(MAX_ERROR_BODY).collect();
    if chars.next().is_none() {
        clipped
    } else {
        format!("{}...", clipped)
    }
}

async fn perform_supervisor_http_request(
    method: String,
    url: String,
    token: String,
    payload: Option<String>,
) -> Result<SupervisorHttpResponse> {
    let output = tokio::task::spawn_blocking(move || {
        let mut command = Command::new("curl");
        command
            .arg("-sS")
            .arg("-X")
            .arg(&method)
            .arg(&url)
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", token))
            .arg("-w")
            .arg("\n%{http_code}");

        if let Some(payload) = payload {
            command
                .arg("-H")
                .arg("Content-Type: application/json")
                .arg("-d")
                .arg(payload);
        }

        command.output()
    })
    .await
    .map_err(|e| AosError::Io(format!("Failed to run curl: {}", e)))?
    .map_err(|e| AosError::Io(format!("curl failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AosError::Network(format!(
            "Supervisor HTTP request failed with process status {} ({})",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let (body, code) = stdout
        .rsplit_once('\n')
        .map(|(body, code)| (body.to_string(), code.trim().to_string()))
        .unwrap_or_else(|| ("".to_string(), stdout.trim().to_string()));

    let status_code = code.parse::<u16>().map_err(|e| {
        AosError::Network(format!(
            "Failed to parse supervisor HTTP status '{}' from curl output: {}",
            code, e
        ))
    })?;

    Ok(SupervisorHttpResponse { status_code, body })
}

async fn verify_restart_handshake(base_url: &str, service_id: &str, token: &str) -> Result<String> {
    let status_url = format!("{}/v1/services/{}", base_url, service_id);
    let handshake_policy = supervisor_retry_policy("deadlock_restart_handshake");
    let handshake_manager = RetryManager::from_policy_defaults(&handshake_policy);

    let status = handshake_manager
        .execute_with_policy(&handshake_policy, || {
            let status_url = status_url.clone();
            let token = token.to_string();
            let service_id = service_id.to_string();
            Box::pin(async move {
                let response =
                    perform_supervisor_http_request("GET".to_string(), status_url, token, None)
                        .await?;

                if !(200..300).contains(&response.status_code) {
                    return Err(AosError::Network(format!(
                        "Supervisor status endpoint rejected handshake for {}: status={} body={}",
                        service_id,
                        response.status_code,
                        trim_http_body_for_error(&response.body)
                    )));
                }

                let status: SupervisorServiceStatus =
                    serde_json::from_str(&response.body).map_err(AosError::Serialization)?;
                let state = status.state.unwrap_or_else(|| "unknown".to_string());

                if !is_restart_handshake_state(&state) {
                    return Err(AosError::Network(format!(
                        "Restart handshake state mismatch for {}: {}",
                        service_id, state
                    )));
                }

                Ok((state, status.restart_count))
            })
        })
        .await?;

    Ok(format!(
        "service_id={} observed_state={} restart_count={}",
        service_id,
        status.0,
        status
            .1
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    ))
}

fn is_restart_handshake_state(state: &str) -> bool {
    matches!(state, "running" | "restarting" | "starting")
}

/// Deadlock event for telemetry
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeadlockEvent {
    pub lock_id: String,
    pub thread_id: u64,
    pub wait_time_secs: u64,
    pub recovery_triggered: bool,
    pub total_deadlocks: usize,
    pub timestamp: u64,
}

impl DeadlockEvent {
    pub fn new(
        lock_id: String,
        thread_id: u64,
        wait_time: Duration,
        recovery_triggered: bool,
        total_deadlocks: usize,
    ) -> Self {
        Self {
            lock_id,
            thread_id,
            wait_time_secs: wait_time.as_secs(),
            recovery_triggered,
            total_deadlocks,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(std::time::Duration::ZERO)
                .as_secs(),
        }
    }
}

/// Simplified deadlock-aware lock (without lifetime issues)
pub struct DeadlockAwareLock<T> {
    inner: Arc<Mutex<T>>,
    lock_id: String,
    detector: Arc<DeadlockDetector>,
}

impl<T> DeadlockAwareLock<T> {
    pub fn new(inner: T, lock_id: String, detector: Arc<DeadlockDetector>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
            lock_id,
            detector,
        }
    }

    pub async fn lock(&self) -> Result<tokio::sync::MutexGuard<'_, T>> {
        let thread_id = get_thread_id();
        self.detector
            .record_lock_acquisition(self.lock_id.clone(), thread_id)
            .await;

        // In a real implementation, would check for deadlocks here
        Ok(self.inner.lock().await)
    }
}

/// Get current thread ID (simplified implementation)
fn get_thread_id() -> u64 {
    // In a real implementation, would use platform-specific thread ID
    // For now, use a hash of the thread ID
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    std::thread::current().id().hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_config::effective::WorkerSafetySection;
    use std::time::Duration;

    #[test]
    fn test_deadlock_config_from_effective_section_configured() {
        let worker_safety = WorkerSafetySection {
            deadlock_check_interval_secs: 9,
            max_wait_time_secs: 45,
            max_lock_depth: 21,
            recovery_timeout_secs: 17,
            ..WorkerSafetySection::default()
        };

        let config = DeadlockConfig::from_effective_section_or_default(Some(&worker_safety));

        assert_eq!(config.check_interval, Duration::from_secs(9));
        assert_eq!(config.max_wait_time, Duration::from_secs(45));
        assert_eq!(config.max_lock_depth, 21);
        assert_eq!(config.recovery_timeout, Duration::from_secs(17));
    }

    #[test]
    fn test_deadlock_config_from_effective_section_fallback() {
        let config = DeadlockConfig::from_effective_section_or_default(None);
        let defaults = DeadlockConfig::default();

        assert_eq!(config.check_interval, defaults.check_interval);
        assert_eq!(config.max_wait_time, defaults.max_wait_time);
        assert_eq!(config.max_lock_depth, defaults.max_lock_depth);
        assert_eq!(config.recovery_timeout, defaults.recovery_timeout);
    }

    #[tokio::test]
    async fn test_deadlock_detector_creation() {
        let config = DeadlockConfig::default();
        let detector = DeadlockDetector::new(config);

        assert_eq!(detector.get_deadlock_count().await, 0);
        assert!(!detector.is_recovery_in_progress().await);
    }

    #[tokio::test]
    async fn test_lock_tracking() {
        let config = DeadlockConfig::default();
        let detector = DeadlockDetector::new(config);

        detector
            .record_lock_acquisition("test_lock".to_string(), 1)
            .await;
        detector.record_lock_release("test_lock", 1).await;

        // Should not panic
        assert_eq!(detector.get_deadlock_count().await, 0);
    }

    #[tokio::test]
    async fn test_deadlock_aware_lock() {
        let config = DeadlockConfig::default();
        let detector = Arc::new(DeadlockDetector::new(config));

        let lock = DeadlockAwareLock::new(42, "test_lock".to_string(), detector.clone());
        let guard = lock
            .lock()
            .await
            .expect("Test lock acquisition should succeed");

        assert_eq!(*guard, 42);
        // Guard will be dropped here, releasing the lock
    }

    #[test]
    fn test_deadlock_event_creation() {
        let event =
            DeadlockEvent::new("test_lock".to_string(), 1, Duration::from_secs(30), true, 1);

        assert_eq!(event.lock_id, "test_lock");
        assert_eq!(event.thread_id, 1);
        assert_eq!(event.wait_time_secs, 30);
        assert!(event.recovery_triggered);
        assert_eq!(event.total_deadlocks, 1);
        assert!(event.timestamp > 0);
    }
}
