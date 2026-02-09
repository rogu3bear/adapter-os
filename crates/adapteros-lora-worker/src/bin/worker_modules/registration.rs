use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use adapteros_api_types::workers::{WorkerCapabilities, WorkerHeartbeatRequest};
use tracing::{error, info, warn};

// Schema and API versions for worker registration
const SCHEMA_VERSION: &str = "1.0";
const API_VERSION: &str = "1.0";

/// Worker registration result
pub struct RegistrationResult {
    pub heartbeat_interval_secs: u32,
    pub kv_quota_bytes: Option<u64>,
    pub kv_residency_policy_id: Option<String>,
}

pub struct RegistrationParams<'a> {
    pub cp_url: &'a str,
    pub worker_id: &'a str,
    pub tenant_id: &'a str,
    pub plan_id: &'a str,
    pub manifest_hash: &'a str,
    pub backend: &'a str,
    pub model_hash: &'a str,
    pub tokenizer_hash_b3: &'a str,
    pub tokenizer_vocab_size: u32,
    pub uds_path: &'a str,
    pub capabilities: &'a [String],
    pub capabilities_detail: &'a WorkerCapabilities,
    pub strict_mode: bool,
}

/// Register worker with control plane
///
/// Returns registration result on success.
/// Returns error message on rejection or communication failure.
fn register_with_cp(
    params: &RegistrationParams,
) -> std::result::Result<RegistrationResult, String> {
    let registration = serde_json::json!({
        "worker_id": params.worker_id,
        "tenant_id": params.tenant_id,
        "plan_id": params.plan_id,
        "manifest_hash": params.manifest_hash,
        "backend": params.backend,
        "model_hash": params.model_hash,
        "tokenizer_hash_b3": params.tokenizer_hash_b3,
        "tokenizer_vocab_size": params.tokenizer_vocab_size,
        "schema_version": SCHEMA_VERSION,
        "api_version": API_VERSION,
        "pid": std::process::id() as i32,
        "uds_path": params.uds_path,
        "capabilities": params.capabilities,
        "capabilities_detail": params.capabilities_detail,
        "strict_mode": params.strict_mode
    });

    let url = format!("{}/v1/workers/register", params.cp_url);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();

    match agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send(registration.to_string().as_bytes())
    {
        Ok(response) => {
            let body = response.into_body().read_to_string().unwrap_or_default();
            match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(json) => {
                    let accepted = json["accepted"].as_bool().unwrap_or(false);
                    let heartbeat = json["heartbeat_interval_secs"].as_u64().unwrap_or(30) as u32;
                    let kv_quota_bytes = json["kv_quota_bytes"].as_u64();
                    let kv_residency_policy_id =
                        json["kv_residency_policy_id"].as_str().map(String::from);

                    // Check for strict mode mismatch between worker and control plane
                    let cp_strict = json["cp_strict_mode"].as_bool().unwrap_or(false);
                    if params.strict_mode != cp_strict {
                        warn!(
                            worker_strict = params.strict_mode,
                            cp_strict = cp_strict,
                            "Strict mode mismatch between worker and control plane. \
                             This may cause inconsistent error handling behavior."
                        );
                    }

                    if !accepted {
                        let reason = json["rejection_reason"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string();
                        Err(reason)
                    } else {
                        Ok(RegistrationResult {
                            heartbeat_interval_secs: heartbeat,
                            kv_quota_bytes,
                            kv_residency_policy_id,
                        })
                    }
                }
                Err(e) => Err(format!("Invalid response: {}", e)),
            }
        }
        Err(e) => Err(format!("HTTP error: {}", e)),
    }
}

/// Register worker with control plane with retry logic for transient failures
///
/// Uses exponential backoff with a hard deadline to prevent both
/// "panic and die" and "spin forever" behaviors.
///
/// # Retry Behavior
///
/// Worker Registration Backoff (ANCHOR, AUDIT, RECTIFY)
///
/// - **ANCHOR**: Enforces a deadline with exponential backoff up to 5-minute delay
/// - **AUDIT**: Logs attempt number, delay, remaining budget, and consecutive failures
/// - **RECTIFY**: Circuit breaker stops retries after `MAX_CONSECUTIVE_FAILURES`
///
/// Configuration:
/// - Base delay: 1 second, backoff factor: 2x, max delay: 5 minutes
/// - Maximum elapsed time: 10 minutes (deadline)
/// - Circuit breaker: stops after 10 consecutive failures to prevent infinite retry loops
/// - Non-transient errors (validation, rejection) fail immediately without retry
pub fn register_with_cp_with_retry(
    params: &RegistrationParams,
) -> std::result::Result<RegistrationResult, String> {
    use std::time::{Duration, Instant};

    // ANCHOR: Registration backoff constants aligned with critical_system strategy
    const BASE_DELAY: Duration = Duration::from_secs(1);
    const MAX_DELAY: Duration = Duration::from_secs(300); // 5 minutes cap (plan requirement)
    const MAX_ELAPSED: Duration = Duration::from_secs(600); // 10 minute deadline
    const BACKOFF_FACTOR: f64 = 2.0;
    const MAX_CONSECUTIVE_FAILURES: u32 = 10; // Circuit breaker threshold

    let deadline = Instant::now() + MAX_ELAPSED;
    let mut attempt: u32 = 0;
    let mut delay = BASE_DELAY;
    let mut consecutive_failures: u32 = 0; // RECTIFY: Circuit breaker counter

    loop {
        attempt += 1;
        let remaining = deadline.saturating_duration_since(Instant::now());

        // RECTIFY: Circuit breaker - stop after too many consecutive failures
        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            error!(
                attempt = attempt,
                consecutive_failures = consecutive_failures,
                "Worker registration circuit breaker triggered - too many consecutive failures"
            );
            return Err(format!(
                "Registration circuit breaker triggered after {} consecutive failures",
                consecutive_failures
            ));
        }

        // Check if we've exceeded the deadline (after first attempt)
        if remaining.is_zero() && attempt > 1 {
            return Err(format!(
                "Registration failed after {} attempts ({:?} elapsed): deadline exceeded",
                attempt - 1,
                MAX_ELAPSED
            ));
        }

        match register_with_cp(params) {
            Ok(result) => {
                if attempt > 1 {
                    info!(
                        attempt = attempt,
                        consecutive_failures = consecutive_failures,
                        "Worker registration succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(err) => {
                consecutive_failures += 1; // AUDIT: Track consecutive failures

                // Check if error is transient (network/HTTP errors) or non-transient (validation/rejection)
                let is_transient = err.contains("HTTP error")
                    || err.contains("connect")
                    || err.contains("timeout")
                    || err.contains("Connection refused")
                    || err.contains("DNS")
                    || err.contains("network");

                if !is_transient {
                    // Non-transient errors (validation, rejection) should fail immediately
                    warn!(
                        attempt = attempt,
                        consecutive_failures = consecutive_failures,
                        error = %err,
                        "Worker registration failed with non-transient error, not retrying"
                    );
                    return Err(err);
                }

                // Check if we have time budget remaining
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    error!(
                        attempt = attempt,
                        consecutive_failures = consecutive_failures,
                        elapsed_ms = MAX_ELAPSED.as_millis() as u64,
                        error = %err,
                        "Worker registration failed: deadline exceeded"
                    );
                    return Err(format!(
                        "Registration failed after {} attempts ({:?} elapsed): {}",
                        attempt, MAX_ELAPSED, err
                    ));
                }

                // AUDIT: Log retry attempt with structured fields including consecutive failures
                warn!(
                    attempt = attempt,
                    consecutive_failures = consecutive_failures,
                    delay_ms = delay.as_millis() as u64,
                    remaining_budget_ms = remaining.as_millis() as u64,
                    error = %err,
                    "Worker registration failed with transient error, will retry"
                );

                // Sleep for the delay (capped by remaining budget)
                let actual_delay = delay.min(remaining);
                std::thread::sleep(actual_delay);

                // Calculate next delay with exponential backoff
                delay = Duration::from_millis(((delay.as_millis() as f64) * BACKOFF_FACTOR) as u64)
                    .min(MAX_DELAY);
            }
        }
    }
}

/// Notify control plane of worker status change
pub fn notify_cp_status(
    cp_url: &str,
    worker_id: &str,
    status: &str,
    reason: &str,
    backend: &str,
    model_hash: &str,
    manifest_hash: &str,
    tokenizer_hash_b3: &str,
    tokenizer_vocab_size: u32,
) {
    notify_cp_status_inner(
        cp_url,
        worker_id,
        status,
        reason,
        backend,
        model_hash,
        manifest_hash,
        tokenizer_hash_b3,
        tokenizer_vocab_size,
        1, // single attempt for non-critical notifications
    );
}

/// Notify control plane of worker status change with retry.
///
/// Use this for critical transitions (e.g. `healthy`) where a missed notification
/// causes the worker to be silently excluded from routing.
pub fn notify_cp_status_with_retry(
    cp_url: &str,
    worker_id: &str,
    status: &str,
    reason: &str,
    backend: &str,
    model_hash: &str,
    manifest_hash: &str,
    tokenizer_hash_b3: &str,
    tokenizer_vocab_size: u32,
    max_attempts: u32,
) {
    notify_cp_status_inner(
        cp_url,
        worker_id,
        status,
        reason,
        backend,
        model_hash,
        manifest_hash,
        tokenizer_hash_b3,
        tokenizer_vocab_size,
        max_attempts,
    );
}

fn notify_cp_status_inner(
    cp_url: &str,
    worker_id: &str,
    status: &str,
    reason: &str,
    backend: &str,
    model_hash: &str,
    manifest_hash: &str,
    tokenizer_hash_b3: &str,
    tokenizer_vocab_size: u32,
    max_attempts: u32,
) {
    let notification = serde_json::json!({
        "worker_id": worker_id,
        "status": status,
        "reason": reason,
        "backend": backend,
        "model_hash": model_hash,
        "manifest_hash": manifest_hash,
        "tokenizer_hash_b3": tokenizer_hash_b3,
        "tokenizer_vocab_size": tokenizer_vocab_size,
    });

    let url = format!("{}/v1/workers/status", cp_url);
    let body = notification.to_string();

    for attempt in 1..=max_attempts {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build()
            .new_agent();

        match agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send(body.as_bytes())
        {
            Ok(_) => {
                info!(status = %status, reason = %reason, attempt, "Status notification sent to CP");
                return;
            }
            Err(e) => {
                if attempt < max_attempts {
                    let backoff = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
                    warn!(
                        status = %status,
                        error = %e,
                        attempt,
                        max_attempts,
                        backoff_ms = backoff.as_millis() as u64,
                        "Failed to notify CP of status change, retrying"
                    );
                    std::thread::sleep(backoff);
                } else {
                    error!(
                        status = %status,
                        error = %e,
                        attempt,
                        "Failed to notify CP of status change after all attempts — \
                         worker may not receive routed traffic"
                    );
                }
            }
        }
    }
}

/// Spawn a background heartbeat loop that periodically notifies the control plane.
///
/// Uses `std::thread::spawn` + `ureq` to match the existing blocking convention
/// in this module (registration and status notifications are all blocking).
///
/// The loop runs until `drain_flag` is set to `true`, at which point it exits.
pub fn spawn_heartbeat_loop(
    cp_url: String,
    worker_id: String,
    heartbeat_interval_secs: u32,
    drain_flag: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("worker-heartbeat".into())
        .spawn(move || {
            let url = format!("{}/v1/workers/heartbeat", cp_url);
            let interval = std::time::Duration::from_secs(heartbeat_interval_secs as u64);

            info!(
                worker_id = %worker_id,
                interval_secs = heartbeat_interval_secs,
                "Heartbeat loop started"
            );

            loop {
                std::thread::sleep(interval);

                if drain_flag.load(Ordering::Relaxed) {
                    info!(worker_id = %worker_id, "Heartbeat loop exiting: drain flag set");
                    break;
                }

                // Server requires `status` and `timestamp` (see WorkerHeartbeatRequest).
                let req = WorkerHeartbeatRequest {
                    worker_id: worker_id.clone(),
                    status: "healthy".to_string(),
                    memory_usage_pct: None,
                    adapters_loaded: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    cache_used_mb: None,
                    cache_max_mb: None,
                    cache_pinned_entries: None,
                    cache_active_entries: None,
                    tokenizer_hash_b3: None,
                    tokenizer_vocab_size: None,
                    coreml_failure_stage: None,
                    coreml_failure_reason: None,
                };
                let body = match serde_json::to_string(&req) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(
                            worker_id = %worker_id,
                            error = %e,
                            "Failed to serialize heartbeat request (will retry next interval)"
                        );
                        continue;
                    }
                };

                let agent = ureq::Agent::config_builder()
                    .timeout_global(Some(std::time::Duration::from_secs(5)))
                    .build()
                    .new_agent();

                match agent
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .send(body.as_bytes())
                {
                    Ok(_) => {
                        // Heartbeat acknowledged
                    }
                    Err(e) => {
                        warn!(
                            worker_id = %worker_id,
                            error = %e,
                            "Heartbeat failed (will retry next interval)"
                        );
                    }
                }
            }
        })
        .expect("failed to spawn heartbeat thread")
}
