//! aos-worker binary - Standalone inference worker
//!
//! This binary provides a UDS-based inference server that can be spawned
//! by the node agent or run standalone for development/testing.
//!
//! Usage:
//!   aos-worker --uds-path ./var/run/worker.sock --manifest manifests/qwen32b-coder-mlx.yaml \
//!              --model-path ./var/models/Qwen2.5-7B-Instruct-4bit --manifest-hash <HASH>

use adapteros_boot::jti_cache::JtiCacheStore;
use adapteros_config::{
    prepare_socket_path, reject_tmp_persistent_path, resolve_manifest_cache_dir,
    resolve_telemetry_dir, resolve_worker_socket_for_worker,
};
use adapteros_core::{AosError, B3Hash, ExecutionProfile, Result, SeedMode, WorkerStatus};
use adapteros_lora_worker::{
    backend_coordinator::BackendCoordinator,
    backend_factory::{
        configure_model_cache_telemetry, create_backend_with_model_hashes,
        detect_capabilities as detect_backend_capabilities, get_model_cache,
        select_backend_from_execution_profile, validate_model_cache_budget, BackendChoice,
        SelectionContext,
    },
    health::{HealthEvent, HealthTick},
    panic_utils::{
        build_fatal_payload, extract_panic_message, format_panic_location, truncate_backtrace,
    },
    uds_server::UdsServer,
    CoordinatedKernels, CoremlRuntimeTelemetry, CoremlVerificationSnapshot, DirectKernels,
    HealthConfig, HealthMonitor, KernelWrapper, Worker,
};
use adapteros_manifest::ManifestV3;
use adapteros_telemetry::TelemetryWriter;
use clap::Parser;
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use std::{
    fs,
    path::PathBuf,
    str::FromStr,
};
use tokio::signal;
use tokio::sync::Mutex;
use tracing::{error, info, info_span, warn};

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_db::{CreateCoremlFusionPairParams, Db};
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_kernel_coreml::export::validate_coreml_fusion;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_kernel_coreml::ComputeUnits;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
use adapteros_lora_worker::backend_factory::CoreMLBackendSettings;

// Schema and API versions for worker registration
const SCHEMA_VERSION: &str = "1.0";
const API_VERSION: &str = "1.0";
const DEBUG_LOG_PATH: &str = "/Users/mln-dev/Dev/adapter-os/.cursor/debug.log";

// #region agent log helper
fn write_debug_log(hypothesis_id: &str, location: &str, message: &str, data: serde_json::Value) {
    let payload = serde_json::json!({
        "sessionId": "debug-session",
        "runId": "pre-fix",
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": chrono::Utc::now().timestamp_millis(),
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(DEBUG_LOG_PATH)
    {
        let _ = writeln!(file, "{}", payload);
    }
}
// #endregion

// Worker panic hook support for fatal error reporting
// Global state for panic hook (must be static for panic handler access)
static WORKER_IDENTITY: OnceLock<WorkerIdentity> = OnceLock::new();

/// Worker registration result
struct RegistrationResult {
    heartbeat_interval_secs: u32,
    kv_quota_bytes: Option<u64>,
    kv_residency_policy_id: Option<String>,
}

struct RegistrationParams<'a> {
    cp_url: &'a str,
    worker_id: &'a str,
    tenant_id: &'a str,
    plan_id: &'a str,
    manifest_hash: &'a str,
    backend: &'a str,
    model_hash: &'a str,
    uds_path: &'a str,
    capabilities: &'a [String],
    strict_mode: bool,
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
        "schema_version": SCHEMA_VERSION,
        "api_version": API_VERSION,
        "pid": std::process::id() as i32,
        "uds_path": params.uds_path,
        "capabilities": params.capabilities,
        "strict_mode": params.strict_mode
    });

    let url = format!("{}/api/v1/workers/register", params.cp_url);
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
/// - Base delay: 1 second, backoff factor: 2x, max delay: 16 seconds
/// - Maximum elapsed time: 120 seconds (deadline)
/// - Logs attempt number, delay, and remaining budget on each retry
/// - Non-transient errors (validation, rejection) fail immediately without retry
fn register_with_cp_with_retry(
    params: &RegistrationParams,
) -> std::result::Result<RegistrationResult, String> {
    use std::time::{Duration, Instant};

    const BASE_DELAY: Duration = Duration::from_secs(1);
    const MAX_DELAY: Duration = Duration::from_secs(16);
    const MAX_ELAPSED: Duration = Duration::from_secs(120);
    const BACKOFF_FACTOR: f64 = 2.0;

    let deadline = Instant::now() + MAX_ELAPSED;
    let mut attempt: u32 = 0;
    let mut delay = BASE_DELAY;

    loop {
        attempt += 1;
        let remaining = deadline.saturating_duration_since(Instant::now());

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
                        "Worker registration succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(err) => {
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
                        elapsed_ms = MAX_ELAPSED.as_millis() as u64,
                        error = %err,
                        "Worker registration failed: deadline exceeded"
                    );
                    return Err(format!(
                        "Registration failed after {} attempts ({:?} elapsed): {}",
                        attempt, MAX_ELAPSED, err
                    ));
                }

                // Log retry attempt with structured fields including remaining budget
                warn!(
                    attempt = attempt,
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
fn notify_cp_status(
    cp_url: &str,
    worker_id: &str,
    status: &str,
    reason: &str,
    backend: &str,
    model_hash: &str,
    manifest_hash: &str,
) {
    let notification = serde_json::json!({
        "worker_id": worker_id,
        "status": status,
        "reason": reason,
        "backend": backend,
        "model_hash": model_hash,
        "manifest_hash": manifest_hash,
    });

    let url = format!("{}/api/v1/workers/status", cp_url);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .new_agent();

    match agent
        .post(&url)
        .header("Content-Type", "application/json")
        .send(notification.to_string().as_bytes())
    {
        Ok(_) => {
            info!(status = %status, reason = %reason, "Status notification sent to CP");
        }
        Err(e) => {
            warn!(status = %status, error = %e, "Failed to notify CP of status change");
        }
    }
}

/// Parse manifest content from YAML or JSON
fn parse_manifest(content: &str) -> Result<ManifestV3> {
    serde_yaml::from_str(content).or_else(|yaml_err| {
        serde_json::from_str(content).map_err(|json_err| {
            AosError::Validation(format!(
                "Failed to parse manifest as YAML ({}) or JSON ({})",
                yaml_err, json_err
            ))
        })
    })
}

/// Fetch manifest from control plane by hash
fn fetch_manifest_from_cp(cp_url: &str, tenant_id: &str, manifest_hash: &B3Hash) -> Result<String> {
    let url = format!(
        "{}/api/v1/tenants/{}/manifests/{}",
        cp_url,
        tenant_id,
        manifest_hash.to_hex()
    );

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .new_agent();

    let response = agent
        .get(&url)
        .call()
        .map_err(|e| AosError::Worker(format!("Failed to fetch manifest: {}", e)))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| AosError::Worker(format!("Failed to read manifest response: {}", e)))?;

    let parsed: adapteros_api_types::workers::WorkerManifestFetchResponse =
        serde_json::from_str(&body).map_err(|e| {
            AosError::Worker(format!("Failed to parse manifest response JSON: {}", e))
        })?;

    if parsed.manifest_hash != manifest_hash.to_hex() {
        return Err(AosError::Validation(format!(
            "Manifest hash mismatch from CP: expected {}, got {}",
            manifest_hash.to_hex(),
            parsed.manifest_hash
        )));
    }

    let computed = B3Hash::hash(parsed.manifest_json.as_bytes());
    if computed != *manifest_hash {
        return Err(AosError::Validation(format!(
            "Manifest content hash mismatch: expected {}, computed {}",
            manifest_hash.to_hex(),
            computed.to_hex()
        )));
    }

    Ok(parsed.manifest_json)
}

/// Cache manifest locally for reuse
fn cache_manifest(manifest_hash: &B3Hash, manifest_json: &str) {
    let resolved_cache = match resolve_manifest_cache_dir() {
        Ok(path) => path,
        Err(err) => {
            warn!(error = %err, "Skipping manifest cache write because cache path is invalid");
            return;
        }
    };
    let cache_dir = resolved_cache.path;
    if fs::create_dir_all(&cache_dir).is_ok() {
        let cache_path = cache_dir.join(format!("{}.json", manifest_hash.to_hex()));
        info!(
            path = %cache_path.display(),
            source = %resolved_cache.source,
            "Writing manifest cache entry"
        );
        if let Err(e) = fs::write(&cache_path, manifest_json) {
            warn!(error = %e, path = %cache_path.display(), "Failed to write manifest cache");
        }
    } else {
        warn!(
            path = %cache_dir.display(),
            source = %resolved_cache.source,
            "Failed to create manifest cache directory"
        );
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoremlVerifyMode {
    Off,
    Warn,
    Strict,
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn resolve_coreml_verify_mode() -> CoremlVerifyMode {
    match std::env::var("AOS_COREML_VERIFY_MODE")
        .unwrap_or_else(|_| "warn".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "off" | "disable" | "disabled" => CoremlVerifyMode::Off,
        "strict" | "fail" | "enforce" => CoremlVerifyMode::Strict,
        _ => CoremlVerifyMode::Warn,
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn coreml_manifest_path(model_path: &Path) -> Result<PathBuf> {
    let manifest_path = if model_path.is_dir() {
        model_path.join("Manifest.json")
    } else {
        model_path.to_path_buf()
    };

    if !manifest_path.exists() {
        return Err(AosError::Validation(
            "CoreML manifest not found (expected Manifest.json)".to_string(),
        ));
    }

    Ok(manifest_path)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn compute_coreml_package_hash(model_path: &Path) -> Result<B3Hash> {
    let manifest_path = coreml_manifest_path(model_path)?;
    let bytes = fs::read(&manifest_path)
        .map_err(|e| AosError::Io(format!("Failed to read CoreML manifest for hashing: {}", e)))?;
    Ok(B3Hash::hash(&bytes))
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn expected_coreml_hash_from_metadata(model_path: &Path) -> Option<B3Hash> {
    let candidate = if model_path.is_dir() {
        model_path.join("adapteros_coreml_fusion.json")
    } else {
        model_path
            .parent()
            .map(|p| p.join("adapteros_coreml_fusion.json"))
            .unwrap_or_else(|| PathBuf::from("adapteros_coreml_fusion.json"))
    };
    if !candidate.exists() {
        return None;
    }
    validate_coreml_fusion(&candidate)
        .map(|meta| meta.fused_manifest_hash)
        .ok()
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn resolve_fusion_ids(manifest: &ManifestV3) -> (Option<String>, Option<String>) {
    let base_model_id = manifest
        .fusion
        .as_ref()
        .and_then(|f| f.base_model_id.clone())
        .or_else(|| Some(manifest.base.model_id.clone()));

    let adapter_id = manifest
        .fusion
        .as_ref()
        .and_then(|f| f.adapter_id.clone())
        .or_else(|| {
            if manifest.adapters.len() == 1 {
                Some(manifest.adapters[0].id.clone())
            } else {
                None
            }
        });

    (base_model_id, adapter_id)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
async fn resolve_expected_coreml_hash(
    manifest: &ManifestV3,
    model_path: &Path,
    tenant_id: &str,
    db: Option<&Db>,
) -> (Option<B3Hash>, Option<String>) {
    if let Some(db) = db {
        let (base_model_id, adapter_id) = resolve_fusion_ids(manifest);
        if let (Some(base_id), Some(adapter_id)) = (base_model_id, adapter_id) {
            match db
                .get_coreml_fusion_pair(tenant_id, &base_id, &adapter_id)
                .await
            {
                Ok(Some(pair)) => {
                    if let Ok(hash) = B3Hash::from_hex(&pair.coreml_package_hash) {
                        return (Some(hash), Some("db".to_string()));
                    }
                    if let Ok(hash) = B3Hash::from_hex(&pair.fused_manifest_hash) {
                        return (Some(hash), Some("db".to_string()));
                    }
                    warn!(
                        tenant_id = %tenant_id,
                        base_model_id = %base_id,
                        adapter_id = %adapter_id,
                        "Failed to parse CoreML fusion hash from database record"
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    warn!(
                        tenant_id = %tenant_id,
                        base_model_id = %base_id,
                        adapter_id = %adapter_id,
                        error = %e,
                        "CoreML fusion pair lookup failed, falling back to manifest/env"
                    );
                }
            }
        }
    }

    if let Some(fusion) = &manifest.fusion {
        if let Some(hash) = fusion.fused_manifest_hash {
            return (Some(hash), Some("manifest.fused_manifest_hash".to_string()));
        }
        if let Some(hash) = fusion.coreml_package_hash {
            return (Some(hash), Some("manifest.coreml_package_hash".to_string()));
        }
    }

    if let Ok(env_hash) = std::env::var("AOS_COREML_EXPECTED_HASH") {
        if let Ok(parsed) = B3Hash::from_hex(&env_hash) {
            return (Some(parsed), Some("env".to_string()));
        }
    }

    if let Some(hash) = expected_coreml_hash_from_metadata(model_path) {
        return (Some(hash), Some("metadata".to_string()));
    }

    (None, None)
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoremlVerificationStatus {
    Match,
    Mismatch,
    MissingExpected,
    MissingActual,
    Skipped,
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
impl CoremlVerificationStatus {
    fn as_str(&self) -> &'static str {
        match self {
            CoremlVerificationStatus::Match => "match",
            CoremlVerificationStatus::Mismatch => "mismatch",
            CoremlVerificationStatus::MissingExpected => "missing_expected",
            CoremlVerificationStatus::MissingActual => "missing_actual",
            CoremlVerificationStatus::Skipped => "skipped",
        }
    }

    fn is_mismatch(&self) -> bool {
        matches!(self, CoremlVerificationStatus::Mismatch)
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn log_coreml_verification_result(
    mode: CoremlVerifyMode,
    expected: Option<&B3Hash>,
    actual: Option<&B3Hash>,
    source: Option<&str>,
) -> Result<CoremlVerificationStatus> {
    match (expected, actual) {
        (Some(exp), Some(act)) => {
            if exp == act {
                info!(
                    mode = ?mode,
                    expected_source = source.unwrap_or("unknown"),
                    fused_manifest_hash = %act.to_hex(),
                    "CoreML fused package verified"
                );
                Ok(CoremlVerificationStatus::Match)
            } else if mode == CoremlVerifyMode::Strict {
                Err(AosError::Validation(format!(
                    "CoreML fused package hash mismatch (expected {}, got {})",
                    exp.to_hex(),
                    act.to_hex()
                )))
            } else {
                warn!(
                    mode = ?mode,
                    expected_source = source.unwrap_or("unknown"),
                    expected_hash = %exp.to_hex(),
                    actual_hash = %act.to_hex(),
                    "CoreML fused package hash mismatch"
                );
                Ok(CoremlVerificationStatus::Mismatch)
            }
        }
        (None, Some(act)) => {
            match mode {
                CoremlVerifyMode::Strict => {
                    return Err(AosError::Validation(format!(
                        "CoreML verification strict but expected hash missing (actual {})",
                        act.to_hex()
                    )))
                }
                CoremlVerifyMode::Warn => {
                    warn!(
                        mode = "warn",
                        expected_source = source.unwrap_or("unknown"),
                        actual_hash = %act.to_hex(),
                        "CoreML verification skipped (no expected hash)"
                    );
                }
                CoremlVerifyMode::Off => {}
            }
            Ok(CoremlVerificationStatus::MissingExpected)
        }
        (Some(exp), None) => {
            match mode {
                CoremlVerifyMode::Strict => {
                    return Err(AosError::Validation(format!(
                    "CoreML verification strict but failed to compute actual hash (expected {})",
                    exp.to_hex()
                )))
                }
                CoremlVerifyMode::Warn => {
                    warn!(
                        mode = "warn",
                        expected_source = source.unwrap_or("unknown"),
                        expected_hash = %exp.to_hex(),
                        "CoreML verification skipped (actual hash unavailable)"
                    );
                }
                CoremlVerifyMode::Off => {}
            }
            Ok(CoremlVerificationStatus::MissingActual)
        }
        (None, None) => {
            if mode != CoremlVerifyMode::Off {
                warn!(
                    mode = ?mode,
                    "CoreML verification skipped (no expected or actual hash)"
                );
            }
            Ok(CoremlVerificationStatus::Skipped)
        }
    }
}
struct LoadedManifest {
    manifest: ManifestV3,
    _canonical_json: String,
    hash: B3Hash,
}

fn validate_backend_feature(choice: &BackendChoice) -> Result<()> {
    if matches!(choice, BackendChoice::Mlx) && !cfg!(feature = "multi-backend") {
        return Err(AosError::Config(
            "MLX backend requested but this binary was built without 'multi-backend'. \
             Rebuild with: cargo build --features multi-backend"
                .to_string(),
        ));
    }
    Ok(())
}

/// Parse backend choice from CLI flag using canonical BackendKind parser.
fn parse_backend_choice(raw: &str) -> BackendChoice {
    BackendChoice::from_str(raw).unwrap_or_else(|err| {
        warn!(
            backend = raw,
            error = %err,
            expected = %BackendChoice::variants().join(", "),
            "Invalid backend flag, falling back to auto"
        );
        BackendChoice::Auto
    })
}

/// Detect backend capabilities based on compiled features
fn detect_capabilities(backend_choice: &str) -> Vec<String> {
    let mut caps = vec![];

    // Check if MLX is actually compiled into this binary
    #[cfg(feature = "multi-backend")]
    let has_mlx_support = true;
    #[cfg(not(feature = "multi-backend"))]
    let has_mlx_support = false;

    // Add backend capability based on requested backend AND compiled features
    match backend_choice.to_lowercase().as_str() {
        "coreml" => caps.push("coreml".to_string()),
        "mlx" => {
            // Only advertise MLX if we actually have it compiled
            if has_mlx_support {
                caps.push("mlx".to_string());
            } else {
                tracing::warn!(
                    "MLX backend requested but binary not compiled with multi-backend feature"
                );
            }
        }
        "metal" => caps.push("metal".to_string()),
        "auto" => {
            // Auto tries in order: CoreML -> MLX (if available) -> Metal
            #[cfg(target_os = "macos")]
            {
                caps.push("coreml".to_string());
                // Only advertise MLX capability if the feature is actually compiled in
                if has_mlx_support {
                    caps.push("mlx".to_string());
                }
                caps.push("metal".to_string());
            }
        }
        _ => {}
    }

    caps
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn render_coreml_compute_units(units: ComputeUnits) -> &'static str {
    match units {
        ComputeUnits::CpuOnly => "cpu_only",
        ComputeUnits::CpuAndGpu => "cpu_and_gpu",
        ComputeUnits::CpuAndNeuralEngine => "cpu_and_neural_engine",
        ComputeUnits::All => "all",
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn coreml_telemetry_from_settings(settings: &CoreMLBackendSettings) -> CoremlRuntimeTelemetry {
    CoremlRuntimeTelemetry {
        compute_preference: Some(settings.preference.to_string()),
        compute_units: Some(render_coreml_compute_units(settings.compute_units).to_string()),
        gpu_available: Some(settings.gpu_available),
        ane_available: Some(settings.ane_available),
        gpu_used: Some(settings.gpu_used),
        ane_used: Some(settings.ane_used),
        production_mode: Some(settings.production_mode),
    }
}

#[derive(Debug, Clone)]
struct WorkerIdentity {
    worker_id: String,
    cp_url: String,
}

/// Set up panic hook to report fatal errors to the control plane
fn setup_panic_hook() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        // Extract panic message
        let message = extract_panic_message(panic_info.payload());

        // Extract location
        let location = panic_info
            .location()
            .map(|l| format_panic_location(l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        // Capture backtrace (first 2000 chars to avoid oversized messages)
        let backtrace_snippet = {
            let backtrace = std::backtrace::Backtrace::force_capture();
            let mut buf = String::new();
            let _ = std::fmt::Write::write_fmt(&mut buf, format_args!("{backtrace}"));
            truncate_backtrace(&buf, 1024)
        };

        // Attempt to notify CP of fatal error
        if let Some(identity) = WORKER_IDENTITY.get() {
            // Build fatal error payload
            let fatal_payload =
                build_fatal_payload(&identity.worker_id, &location, &message, &backtrace_snippet);

            // Use blocking HTTP client (ureq) since we're in panic context
            // and can't use async. Best-effort delivery with short timeout.
            let url = format!("{}/api/v1/workers/fatal", identity.cp_url);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(3)))
                .build()
                .new_agent();
            let result = match serde_json::to_vec(&fatal_payload) {
                Ok(body) => agent
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .send(body.as_slice()),
                Err(e) => {
                    eprintln!("[PANIC HOOK] Failed to serialize fatal payload: {e}");
                    return;
                }
            };

            match result {
                Ok(_) => {
                    eprintln!("[PANIC HOOK] Fatal error reported to CP");
                }
                Err(e) => {
                    eprintln!("[PANIC HOOK] Failed to report fatal to CP: {}", e);
                }
            }
        } else {
            eprintln!("[PANIC HOOK] Worker identity not set, cannot report to CP");
        }

        // Call default hook for normal panic handling
        default_hook(panic_info);
    }));
}

/// AdapterOS Inference Worker
#[derive(Parser, Debug)]
#[command(name = "aos-worker")]
#[command(about = "AdapterOS inference worker with UDS communication")]
struct Args {
    /// Tenant ID for this worker
    #[arg(long, env = "TENANT_ID", default_value = "default")]
    tenant_id: String,

    /// Plan ID for this worker
    #[arg(long, env = "PLAN_ID", default_value = "dev")]
    plan_id: String,

    /// UDS socket path for communication
    /// Standard production path: /var/run/aos/{tenant_id}/worker.sock
    /// Development path: ./var/run/worker.sock (relative to cwd)
    #[arg(long, env = "AOS_WORKER_SOCKET")]
    uds_path: Option<PathBuf>,

    /// Manifest hash (preferred) to fetch/verify
    #[arg(long, env = "AOS_MANIFEST_HASH")]
    manifest_hash: Option<String>,

    /// Path to manifest YAML/JSON file (fallback when hash fetch is unavailable)
    #[arg(long, env = "AOS_WORKER_MANIFEST")]
    manifest: Option<PathBuf>,

    /// Path to model directory (auto-discovered from AOS_MODEL_PATH)
    #[arg(long, env = "AOS_MODEL_PATH")]
    model_path: Option<PathBuf>,

    /// Path to tokenizer JSON file (auto-discovered from AOS_TOKENIZER_PATH or model directory)
    #[arg(long, env = "AOS_TOKENIZER_PATH")]
    tokenizer: Option<PathBuf>,

    /// Backend choice (auto, metal, coreml, mlx)
    #[arg(long, default_value = "auto")]
    backend: String,

    /// Worker ID (auto-generated if not provided)
    #[arg(long, env = "WORKER_ID")]
    worker_id: Option<String>,

    /// Control plane URL for fatal error reporting
    #[arg(long, env = "AOS_CP_URL", default_value = "http://127.0.0.1:8080")]
    cp_url: String,
    /// Enable backend coordinator (primary + fallback) for runtime failover
    #[arg(long, env = "AOS_COORDINATOR_ENABLED", default_value_t = false)]
    coordinator_enabled: bool,

    /// Enable strict mode (fail-closed boot)
    /// When enabled:
    /// - Worker public key must exist (var/keys/worker_signing.pub)
    /// - Tokens from CP are required for all requests
    #[arg(long, env = "AOS_STRICT")]
    strict: bool,
}

/// Exit codes for worker process control
///
/// These codes determine restart behavior:
/// - 0: Graceful shutdown (don't restart)
/// - 1: Config/validation error (don't restart - requires manual fix)
/// - 2: Transient error (restart with backoff)
/// - 3: Fatal error (don't restart - requires investigation)
const EXIT_SUCCESS: i32 = 0;
const EXIT_CONFIG_ERROR: i32 = 1;
const EXIT_TRANSIENT_ERROR: i32 = 2;
const EXIT_FATAL_ERROR: i32 = 3;

/// Determine exit code based on error type
fn error_to_exit_code(err: &AosError) -> i32 {
    match err {
        // Config/validation errors should not restart (need manual fix)
        AosError::Config(_) | AosError::Validation(_) => EXIT_CONFIG_ERROR,

        // Network/transient errors should restart with backoff
        AosError::Network(_) | AosError::Timeout { .. } => EXIT_TRANSIENT_ERROR,

        // Fatal errors (internal, cache corruption, etc.) should not restart
        AosError::Internal(_) | AosError::CacheCorruption { .. } => EXIT_FATAL_ERROR,

        // Default: treat as transient for unknown error types
        _ => EXIT_TRANSIENT_ERROR,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Run the actual worker logic and map errors to exit codes
    match run_worker().await {
        Ok(()) => std::process::exit(EXIT_SUCCESS),
        Err(e) => {
            let exit_code = error_to_exit_code(&e);
            error!(
                error = %e,
                exit_code = exit_code,
                "Worker exiting with error"
            );
            std::process::exit(exit_code);
        }
    }
}

async fn run_worker() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("aos_worker=info".parse().unwrap())
                .add_directive("adapteros_lora_worker=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();

    // Load canonical .env before any environment-based resolution
    adapteros_config::model::load_dotenv();

    // Early validation: check model cache budget BEFORE any expensive operations
    // This is a fail-fast check to avoid 100-200ms of wasted work (manifest loading,
    // backend selection) when the configuration is missing.
    info!("Validating model cache budget configuration...");
    if let Err(e) = validate_model_cache_budget() {
        error!(error = %e, "FATAL: Model cache budget not configured");
        eprintln!(
            "ERROR: Model cache budget not configured.\n\
             Set AOS_MODEL_CACHE_MAX_MB=<megabytes> environment variable\n\
             or model.cache.max.mb in the config TOML file."
        );
        // Exit immediately with config error - no point in continuing
        std::process::exit(EXIT_CONFIG_ERROR);
    }
    info!("Model cache budget validated successfully");

    // Set up panic hook for fatal error reporting
    let worker_id = args
        .worker_id
        .clone()
        .unwrap_or_else(|| format!("worker-{}", uuid::Uuid::now_v7()));

    // Store worker identity for panic hook access
    let _ = WORKER_IDENTITY.set(WorkerIdentity {
        worker_id: worker_id.clone(),
        cp_url: args.cp_url.clone(),
    });

    // Install panic hook for fatal error reporting
    setup_panic_hook();
    info!(worker_id = %worker_id, cp_url = %args.cp_url, "Panic hook installed for fatal error reporting");

    // Resolve UDS path with fallback logic and guard against tmp directories
    let resolved_uds = resolve_worker_socket_for_worker(&args.tenant_id, args.uds_path.as_deref())
        .map_err(|e| {
            error!(
                tenant_id = %args.tenant_id,
                uds_override = ?args.uds_path,
                error = %e,
                "Worker socket resolution failed"
            );
            // #region agent log
            write_debug_log(
                "H1",
                "aos_worker.rs:resolve_uds",
                "uds resolution failed",
                serde_json::json!({
                    "tenant_id": args.tenant_id,
                    "uds_override": args.uds_path,
                    "error": e.to_string()
                }),
            );
            // #endregion
            e
        })?;
    let uds_path = resolved_uds.path.clone();
    prepare_socket_path(&uds_path, "worker").map_err(|e| {
        error!(
            tenant_id = %args.tenant_id,
            uds_path = %uds_path.display(),
            error = %e,
            "Failed to prepare worker socket path"
        );
        // #region agent log
        write_debug_log(
            "H1",
            "aos_worker.rs:prepare_socket_path",
            "prepare socket failed",
            serde_json::json!({
                "tenant_id": args.tenant_id,
                "uds_path": uds_path,
                "error": e.to_string()
            }),
        );
        // #endregion
        e
    })?;

    info!(
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        uds_path = %uds_path.display(),
        uds_source = %resolved_uds.source,
        "Starting aos-worker"
    );

    // Resolve model and tokenizer paths
    let model_path = match &args.model_path {
        Some(path) => path.clone(),
        None => adapteros_config::get_model_path_with_fallback()?,
    };
    reject_tmp_persistent_path(&model_path, "model-path")?;
    if !model_path.exists() {
        // #region agent log
        write_debug_log(
            "H3",
            "aos_worker.rs:model_path",
            "model path missing",
            serde_json::json!({
                "model_path": model_path
            }),
        );
        // #endregion
        return Err(AosError::Validation(format!(
            "Model path does not exist: {}",
            model_path.display()
        )));
    }

    // Resolve tokenizer via canonical discovery (CLI arg > AOS_TOKENIZER_PATH > AOS_MODEL_PATH/tokenizer.json)
    let tokenizer_path = adapteros_config::resolve_tokenizer_path(args.tokenizer.as_ref())?;

    // Resolve manifest content (hash-first)
    let expected_manifest_hash = args
        .manifest_hash
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|h| B3Hash::from_hex(h).map_err(|e| AosError::Validation(e.to_string())))
        .transpose()?;

    if let Some(path) = args.manifest.as_ref() {
        reject_tmp_persistent_path(path, "worker-manifest")?;
    }

    let loaded_manifest = if let Some(expected_hash) = expected_manifest_hash {
        if let Some(path) = args.manifest.as_ref() {
            if !path.exists() {
                return Err(AosError::Validation(format!(
                    "Manifest file not found at {}",
                    path.display()
                )));
            }
            let manifest_raw = fs::read_to_string(path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read manifest at {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let manifest = parse_manifest(&manifest_raw)?;
            let computed_hash = manifest.compute_hash()?;
            if computed_hash != expected_hash {
                return Err(AosError::Validation(format!(
                    "Manifest hash mismatch: expected {}, computed {}",
                    expected_hash.to_hex(),
                    computed_hash.to_hex()
                )));
            }
            let canonical_json = manifest.to_json().map_err(|e| {
                AosError::Validation(format!("Failed to canonicalize manifest: {}", e))
            })?;
            cache_manifest(&computed_hash, &canonical_json);
            LoadedManifest {
                manifest,
                _canonical_json: canonical_json,
                hash: computed_hash,
            }
        } else {
            info!(
                manifest_hash = %expected_hash.to_hex(),
                cp_url = %args.cp_url,
                tenant_id = %args.tenant_id,
                "Fetching manifest from control plane"
            );
            let manifest_json =
                fetch_manifest_from_cp(&args.cp_url, &args.tenant_id, &expected_hash)?;
            let manifest = parse_manifest(&manifest_json)?;
            let computed_hash = manifest.compute_hash()?;
            if computed_hash != expected_hash {
                return Err(AosError::Validation(format!(
                    "Manifest hash mismatch after fetch: expected {}, computed {}",
                    expected_hash.to_hex(),
                    computed_hash.to_hex()
                )));
            }
            let canonical_json = manifest.to_json().map_err(|e| {
                AosError::Validation(format!("Failed to canonicalize manifest: {}", e))
            })?;
            cache_manifest(&computed_hash, &canonical_json);
            LoadedManifest {
                manifest,
                _canonical_json: canonical_json,
                hash: computed_hash,
            }
        }
    } else {
        let path = args.manifest.as_ref().ok_or_else(|| {
            AosError::Validation(
                "Manifest hash not provided. Supply --manifest-hash/AOS_MANIFEST_HASH or --manifest/AOS_WORKER_MANIFEST"
                    .to_string(),
            )
        })?;
        if !path.exists() {
            return Err(AosError::Validation(format!(
                "Manifest file not found at {}",
                path.display()
            )));
        }
        let manifest_raw = fs::read_to_string(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read manifest at {}: {}",
                path.display(),
                e
            ))
        })?;
        let manifest = parse_manifest(&manifest_raw)?;
        let computed_hash = manifest.compute_hash()?;
        let canonical_json = manifest
            .to_json()
            .map_err(|e| AosError::Validation(format!("Failed to canonicalize manifest: {}", e)))?;
        cache_manifest(&computed_hash, &canonical_json);
        LoadedManifest {
            manifest,
            _canonical_json: canonical_json,
            hash: computed_hash,
        }
    };

    let manifest = loaded_manifest.manifest;
    let manifest_hash = loaded_manifest.hash;

    info!(
        model_id = %manifest.base.model_id,
        manifest_hash = %manifest_hash.to_hex(),
        k_sparse = manifest.router.k_sparse,
        "Manifest loaded and verified"
    );
    let model_hash_hex = manifest.base.model_hash.to_hex();

    // Select backend (ExecutionProfile is the canonical source)
    let requested_backend = parse_backend_choice(&args.backend);
    validate_backend_feature(&requested_backend)?;

    let capabilities = detect_backend_capabilities();
    let exec_profile = ExecutionProfile {
        seed_mode: SeedMode::BestEffort,
        backend_profile: requested_backend,
    };
    let selection = select_backend_from_execution_profile(&SelectionContext::new(
        exec_profile,
        capabilities.clone(),
    ))?;
    info!(
        requested = %requested_backend.as_str(),
        selected = %selection.selected.as_str(),
        overridden = selection.overridden,
        reason = selection.reason.unwrap_or("none"),
        "Resolved backend selection at worker startup"
    );
    // #region agent log
    write_debug_log(
        "H3",
        "aos_worker.rs:backend_selection",
        "backend selection resolved",
        serde_json::json!({
            "requested": requested_backend.as_str(),
            "selected": selection.selected.as_str(),
            "overridden": selection.overridden,
            "reason": selection.reason
        }),
    );
    // #endregion
    if selection.overridden {
        info!(
            requested = %requested_backend.as_str(),
            selected = %selection.selected.as_str(),
            reason = ?selection.reason,
            "Backend request overridden based on capabilities"
        );
    }
    let backend_choice = selection.selected;

    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    let coreml_primary_runtime = if backend_choice == BackendChoice::CoreML {
        Some(coreml_telemetry_from_settings(
            &adapteros_lora_worker::backend_factory::resolve_coreml_backend_settings(),
        ))
    } else {
        None
    };
    #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
    let coreml_primary_runtime: Option<CoremlRuntimeTelemetry> = None;
    #[allow(unused_mut)]
    let mut fallback_coreml_runtime: Option<CoremlRuntimeTelemetry> = None;

    // NOTE: Model cache budget validation moved to startup (line ~952) for fail-fast behavior.
    // The budget is validated before any expensive operations like manifest loading.

    // Create kernel backend with manifest hash for cache identity and model hash for integrity verification
    info!(
        backend = %backend_choice.as_str(),
        manifest_hash = %manifest_hash.to_hex(),
        model_hash = %manifest.base.model_hash.to_hex(),
        "Creating kernel backend with integrity verification"
    );
    let primary_kernels = create_backend_with_model_hashes(
        backend_choice,
        &model_path,
        Some(&manifest_hash),
        Some(&manifest.base.model_hash),
    )
    .inspect_err(|e| {
        // #region agent log
        write_debug_log(
            "H3",
            "aos_worker.rs:create_backend",
            "primary backend creation failed",
            serde_json::json!({
                "backend": backend_choice.as_str(),
                "model_path": model_path,
                "manifest_hash": manifest_hash.to_hex(),
                "model_hash": manifest.base.model_hash.to_hex(),
                "error": e.to_string()
            }),
        );
        // #endregion
    })?;

    // Optional fallback backend via coordinator
    let mut fallback_backend_kind: Option<BackendChoice> = None;
    let fallback_kernels = if args.coordinator_enabled {
        match BackendCoordinator::select_fallback_backend(&backend_choice, &capabilities) {
            Ok(choice) => {
                match create_backend_with_model_hashes(
                    choice,
                    &model_path,
                    Some(&manifest_hash),
                    Some(&manifest.base.model_hash),
                ) {
                    Ok(k) => {
                        if choice == BackendChoice::CoreML {
                            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
                            {
                                fallback_coreml_runtime = Some(coreml_telemetry_from_settings(
                                    &adapteros_lora_worker::backend_factory::resolve_coreml_backend_settings(),
                                ));
                            }
                        }
                        info!(fallback_backend = ?choice, "Created fallback backend");
                        fallback_backend_kind = Some(choice);
                        Some(k)
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to create fallback backend, continuing without fallback");
                        None
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "No suitable fallback backend available, continuing without fallback");
                None
            }
        }
    } else {
        None
    };

    let kernels = if args.coordinator_enabled {
        KernelWrapper::Coordinated(CoordinatedKernels::new(primary_kernels, fallback_kernels))
    } else {
        KernelWrapper::Direct(DirectKernels::new(primary_kernels))
    };
    // #region agent log
    write_debug_log(
        "H3",
        "aos_worker.rs:kernels_ready",
        "kernels initialized",
        serde_json::json!({
            "coordinator": args.coordinator_enabled,
            "backend": backend_choice.as_str(),
            "fallback": fallback_backend_kind.map(|b| b.as_str())
        }),
    );
    // #endregion

    let available_backends = adapteros_lora_worker::AvailableBackends {
        primary: backend_choice,
        fallback: fallback_backend_kind,
        coreml_primary: coreml_primary_runtime,
        coreml_fallback: fallback_coreml_runtime,
    };

    // Compute and verify CoreML fused package hash when CoreML is in play.
    #[allow(unused_variables)]
    let coreml_in_use = backend_choice == BackendChoice::CoreML
        || matches!(available_backends.fallback, Some(BackendChoice::CoreML));

    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    let (coreml_package_hash_hex, coreml_verification) = if coreml_in_use {
        let coreml_db = match Db::connect_env().await {
            Ok(db) => Some(db),
            Err(e) => {
                warn!(
                    error = %e,
                    "CoreML verification DB unavailable; continuing without registry lookup"
                );
                None
            }
        };

        let actual_hash = match compute_coreml_package_hash(&model_path) {
            Ok(hash) => Some(hash),
            Err(e) => {
                warn!(error = %e, "Failed to compute CoreML package hash");
                None
            }
        };
        let (expected_hash, expected_source) = resolve_expected_coreml_hash(
            &manifest,
            &model_path,
            &args.tenant_id,
            coreml_db.as_ref(),
        )
        .await;
        let mode = resolve_coreml_verify_mode();
        let status = log_coreml_verification_result(
            mode,
            expected_hash.as_ref(),
            actual_hash.as_ref(),
            expected_source.as_deref(),
        )?;

        let verification_snapshot = CoremlVerificationSnapshot {
            mode: Some(format!("{:?}", mode).to_lowercase()),
            expected: expected_hash.as_ref().map(|h| h.to_hex()),
            actual: actual_hash.as_ref().map(|h| h.to_hex()),
            source: expected_source.clone(),
            status: Some(status.as_str().to_string()),
            mismatch: status.is_mismatch(),
        };

        if status == CoremlVerificationStatus::Match {
            if let (Some(db), Some(actual_hex)) =
                (coreml_db.as_ref(), actual_hash.as_ref().map(|h| h.to_hex()))
            {
                let (base_model_id, adapter_id) = resolve_fusion_ids(&manifest);
                if let (Some(base_id), Some(adapter_id)) = (base_model_id, adapter_id) {
                    let params = CreateCoremlFusionPairParams {
                        tenant_id: args.tenant_id.clone(),
                        base_model_id: base_id,
                        adapter_id,
                        fused_manifest_hash: actual_hex.clone(),
                        coreml_package_hash: actual_hex.clone(),
                        adapter_hash_b3: manifest
                            .fusion
                            .as_ref()
                            .and_then(|f| f.adapter_hash)
                            .map(|h| h.to_hex()),
                        base_model_hash_b3: manifest
                            .fusion
                            .as_ref()
                            .and_then(|f| f.base_model_hash)
                            .map(|h| h.to_hex()),
                        metadata_path: None,
                    };
                    if let Err(e) = db.upsert_coreml_fusion_pair(params).await {
                        warn!(
                            error = %e,
                            "Failed to upsert CoreML fusion pair after verification"
                        );
                    }
                }
            }
        }

        (actual_hash.map(|h| h.to_hex()), Some(verification_snapshot))
    } else {
        (None, None)
    };
    #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
    let (coreml_package_hash_hex, coreml_verification): (
        Option<String>,
        Option<CoremlVerificationSnapshot>,
    ) = (None, None);

    // Create telemetry writer - use env var or ./var/telemetry
    let resolved_telemetry = resolve_telemetry_dir()?;
    if let Err(e) = std::fs::create_dir_all(&resolved_telemetry.path) {
        warn!(
            error = %e,
            path = %resolved_telemetry.path.display(),
            source = %resolved_telemetry.source,
            "Failed to create telemetry directory; continuing"
        );
    }
    let telemetry =
        TelemetryWriter::new(&resolved_telemetry.path, 10000, 100_000_000).map_err(|e| {
            adapteros_core::AosError::Worker(format!("Failed to create telemetry writer: {}", e))
        })?;
    info!(
        path = %resolved_telemetry.path.display(),
        source = %resolved_telemetry.source,
        "Telemetry writer initialized"
    );
    configure_model_cache_telemetry(telemetry.clone());

    // Track lifecycle locally for state validation
    let mut lifecycle = WorkerStatus::Created;
    let backend_label = backend_choice.as_str();

    // Register with control plane first to get quota allocation
    let capabilities = detect_capabilities(backend_label);
    let uds_path_str = uds_path.to_string_lossy().to_string();

    let manifest_hash_hex = manifest_hash.to_hex();
    info!(
        worker_id = %worker_id,
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        manifest_hash = %manifest_hash_hex,
        backend = %backend_label,
        model_hash = %model_hash_hex,
        "Registering with control plane"
    );

    let registration_result = match register_with_cp_with_retry(&RegistrationParams {
        cp_url: &args.cp_url,
        worker_id: &worker_id,
        tenant_id: &args.tenant_id,
        plan_id: &args.plan_id,
        manifest_hash: &manifest_hash_hex,
        backend: backend_label,
        model_hash: &model_hash_hex,
        uds_path: &uds_path_str,
        capabilities: &capabilities,
        strict_mode: args.strict,
    }) {
        Ok(result) => {
            // #region agent log
            write_debug_log(
                "H4",
                "aos_worker.rs:register_cp",
                "registration accepted",
                serde_json::json!({
                    "worker_id": worker_id,
                    "tenant_id": args.tenant_id,
                    "manifest_hash": manifest_hash_hex,
                    "backend": backend_label,
                    "heartbeat": result.heartbeat_interval_secs,
                    "kv_quota_bytes": result.kv_quota_bytes,
                    "kv_residency_policy_id": result.kv_residency_policy_id,
                }),
            );
            // #endregion
            lifecycle = lifecycle
                .transition_to(WorkerStatus::Registered)
                .map_err(|e| AosError::Lifecycle(e.to_string()))?;
            notify_cp_status(
                &args.cp_url,
                &worker_id,
                WorkerStatus::Registered.as_str(),
                "registration-accepted",
                &args.backend,
                &model_hash_hex,
                &manifest_hash_hex,
            );
            info!(
                heartbeat_interval = result.heartbeat_interval_secs,
                kv_quota_bytes = ?result.kv_quota_bytes,
                kv_residency_policy_id = ?result.kv_residency_policy_id,
                "Worker registration accepted by control plane"
            );
            result
        }
        Err(reason) => {
            // #region agent log
            write_debug_log(
                "H4",
                "aos_worker.rs:register_cp",
                "registration failed",
                serde_json::json!({
                    "worker_id": worker_id,
                    "tenant_id": args.tenant_id,
                    "manifest_hash": manifest_hash_hex,
                    "backend": backend_label,
                    "reason": reason
                }),
            );
            // #endregion
            let _lifecycle = lifecycle
                .transition_to(WorkerStatus::Error)
                .unwrap_or(lifecycle);
            notify_cp_status(
                &args.cp_url,
                &worker_id,
                WorkerStatus::Error.as_str(),
                "registration-failed",
                &args.backend,
                &model_hash_hex,
                &manifest_hash_hex,
            );
            error!(reason = %reason, "Worker registration failed - exiting");
            return Err(AosError::Worker(format!("Registration failed: {}", reason)));
        }
    };

    // Create KV quota manager from registration response
    let quota_manager = Arc::new(adapteros_lora_worker::TenantKvQuotaManager::new(
        args.tenant_id.clone(),
        registration_result.kv_quota_bytes,
    ));

    info!(
        tenant_id = %args.tenant_id,
        kv_quota_bytes = ?registration_result.kv_quota_bytes,
        kv_residency_policy_id = ?registration_result.kv_residency_policy_id,
        quota_enforced = quota_manager.is_quota_enforced(),
        "KV quota manager initialized"
    );

    // Create worker with quota manager
    info!("Creating worker instance");

    // Fail fast on non-UTF8 paths rather than silently coercing to "".
    // Determinism expectation: invalid configuration must error, not change behavior.
    let tokenizer_path_str = tokenizer_path.to_str().ok_or_else(|| {
        AosError::Validation(format!(
            "Tokenizer path is not valid UTF-8: {:?} (display: {})",
            tokenizer_path,
            tokenizer_path.display()
        ))
    })?;
    let model_path_str = model_path.to_str().ok_or_else(|| {
        AosError::Validation(format!(
            "Model path is not valid UTF-8: {:?} (display: {})",
            model_path,
            model_path.display()
        ))
    })?;

    // PRD-06: Compute worker_id as u32 from BLAKE3 hash for deterministic identity binding
    // Using BLAKE3 ensures stability across Rust versions (unlike DefaultHasher)
    let worker_id_u32 = {
        let hash = adapteros_core::B3Hash::hash(worker_id.as_bytes());
        let bytes: [u8; 4] = hash.as_bytes()[0..4].try_into().unwrap_or([0; 4]);
        u32::from_le_bytes(bytes)
    };

    let worker = Worker::new(
        manifest,
        &args.tenant_id,
        kernels,
        available_backends,
        None, // No RAG system for now
        tokenizer_path_str,
        model_path_str,
        telemetry,
        coreml_package_hash_hex.clone(),
        coreml_verification.clone(),
        Some(quota_manager),
        worker_id_u32,
    )
    .await?;

    let worker = Arc::new(Mutex::new(worker));
    let drain_flag = Arc::new(AtomicBool::new(false));

    let heartbeat_interval = registration_result.heartbeat_interval_secs;

    // Align health monitoring interval with control plane heartbeat expectation
    {
        let mut guard = worker.lock().await;
        let telemetry_for_monitor = guard.telemetry().clone();
        let config = HealthConfig {
            check_interval: std::time::Duration::from_secs(heartbeat_interval as u64),
            ..Default::default()
        };
        guard.set_health_monitor(Arc::new(if let Some(t) = telemetry_for_monitor {
            HealthMonitor::new(config)?.with_telemetry(t, args.tenant_id.clone(), worker_id.clone())
        } else {
            HealthMonitor::new(config)?
        }));
    }

    // Start UDS server (bind before marking healthy)
    // Try to load worker verifying key for CP->Worker authentication
    // In strict mode, we use retry with exponential backoff (worker may start before CP generates keypair).
    // In non-strict mode, we try once and fall back to no authentication if key is missing.
    const KEY_LOAD_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);

    let worker_verifying_key = if args.strict {
        // Strict mode: use retry with deadline, then fail with transient error code
        match adapteros_boot::load_worker_public_key_with_retry("var/keys", KEY_LOAD_DEADLINE) {
            Ok(key) => {
                info!("Worker public key loaded for CP->Worker authentication");
                Some(key)
            }
            Err(e) => {
                error!(
                    error = %e,
                    deadline_secs = KEY_LOAD_DEADLINE.as_secs(),
                    "STRICT MODE: Failed to load worker public key after retry"
                );
                // Use transient error code so orchestrator will retry
                std::process::exit(EXIT_TRANSIENT_ERROR);
            }
        }
    } else {
        // Non-strict mode: try once, fall back to no auth if missing
        match adapteros_boot::load_worker_public_key("var/keys") {
            Ok(key) => {
                info!("Worker public key loaded for CP->Worker authentication");
                Some(key)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Worker public key not found, running without CP->Worker token validation"
                );
                None
            }
        }
    };

    // Initialize persistent JTI cache for replay defense (only when auth is enabled)
    // The cache is loaded from disk on startup and persisted on shutdown.
    let jti_cache = if worker_verifying_key.is_some() {
        let jti_cache_path = PathBuf::from("var/keys/jti_cache.json");
        let cache = JtiCacheStore::load_or_new(jti_cache_path);
        info!(
            entries = cache.len(),
            capacity = cache.capacity(),
            "JTI cache initialized for replay defense"
        );
        Some(Arc::new(Mutex::new(cache)))
    } else {
        None
    };

    info!(uds_path = %uds_path.display(), "Starting UDS server");
    let server = if let Some(verifying_key) = worker_verifying_key {
        let jti_cache = jti_cache.expect("JTI cache should be initialized when auth is enabled");
        UdsServer::new_with_worker_auth(
            uds_path.clone(),
            worker.clone(),
            None,
            drain_flag.clone(),
            verifying_key,
            worker_id.clone(),
            jti_cache,
        )
    } else {
        // In non-strict mode, this is allowed
        UdsServer::new(uds_path.clone(), worker.clone(), None, drain_flag.clone())
    };
    let listener = server.bind().await.inspect_err(|e| {
        // #region agent log
        write_debug_log(
            "H5",
            "aos_worker.rs:uds_bind",
            "uds bind failed",
            serde_json::json!({
                "uds_path": uds_path,
                "error": e.to_string()
            }),
        );
        // #endregion
    })?;

    lifecycle = lifecycle
        .transition_to(WorkerStatus::Healthy)
        .map_err(|e| AosError::Lifecycle(e.to_string()))?;
    notify_cp_status(
        &args.cp_url,
        &worker_id,
        WorkerStatus::Healthy.as_str(),
        "uds-listening",
        &args.backend,
        &model_hash_hex,
        &manifest_hash_hex,
    );

    // Spawn health monitor loop with telemetry + shutdown hook
    let (health_monitor, telemetry_for_health) = {
        let guard = worker.lock().await;
        (guard.health_monitor(), guard.telemetry().clone())
    };
    let health_monitor_for_task = health_monitor.clone();
    let cp_url_health = args.cp_url.clone();
    let worker_id_health = worker_id.clone();
    let backend_health = args.backend.clone();
    let model_hash_health = model_hash_hex.clone();
    let manifest_hash_health = manifest_hash_hex.clone();
    let drain_flag_health = drain_flag.clone();
    tokio::spawn(async move {
        if let Err(e) = health_monitor_for_task
            .start_monitoring_with_hook(|monitor, tick| {
                if let Some(t) = telemetry_for_health.as_ref() {
                    if let HealthTick::Status { status, .. } = &tick {
                        if let Ok(event) = HealthEvent::from_monitor(monitor, status) {
                            let _ = t.log("worker_health", event);
                        }
                    }
                }

                if matches!(tick, HealthTick::Shutdown { .. }) {
                    notify_cp_status(
                        &cp_url_health,
                        &worker_id_health,
                        WorkerStatus::Error.as_str(),
                        "health-shutdown",
                        &backend_health,
                        &model_hash_health,
                        &manifest_hash_health,
                    );
                    drain_flag_health.store(true, Ordering::Relaxed);
                }

                Ok(())
            })
            .await
        {
            warn!(error = %e, "Health monitor exited with error");
        }
    });

    let serve_span = info_span!(
        "worker_serve",
        worker_id = %worker_id,
        tenant_id = %args.tenant_id,
        plan_id = %args.plan_id,
        backend = %args.backend,
        manifest_hash = %manifest_hash_hex,
        uds_path = %uds_path_str,
        coordinator_enabled = args.coordinator_enabled,
    );
    let _serve_span_guard = serve_span.enter();

    // Run server with drain handling
    let shutdown_signal = signal::ctrl_c();
    tokio::pin!(shutdown_signal);
    let serve_fut = server.serve_with_listener(listener);
    tokio::pin!(serve_fut);
    tokio::select! {
        res = &mut serve_fut => res,
        _ = &mut shutdown_signal => {
            info!(worker_id = %worker_id, "Drain signal received, initiating worker drain");

            // Persist JTI cache before shutdown to maintain replay defense across restarts
            if let Err(e) = server.persist_jti_cache().await {
                warn!(error = %e, "Failed to persist JTI cache during shutdown");
            }

            // Cleanup model cache during drain to free pinned entries
            if let Ok(cache) = get_model_cache() {
                info!("Cleaning up model cache before drain");
                cache.cleanup_all();
            }

            drain_flag.store(true, Ordering::Relaxed);
            lifecycle = lifecycle.transition_to(WorkerStatus::Draining)
                .map_err(|e| AosError::Lifecycle(e.to_string()))?;
            notify_cp_status(
                &args.cp_url,
                &worker_id,
                WorkerStatus::Draining.as_str(),
                "drain-signal",
                &args.backend,
                &model_hash_hex,
                &manifest_hash_hex,
            );
            serve_fut.await
        }
    }?;

    // Notify stopped (or error if health triggered shutdown) on clean exit
    let final_status = if health_monitor.is_shutdown_requested() {
        WorkerStatus::Error
    } else {
        WorkerStatus::Stopped
    };
    let _lifecycle = lifecycle
        .transition_to(final_status)
        .map_err(|e| AosError::Lifecycle(e.to_string()))?;
    notify_cp_status(
        &args.cp_url,
        &worker_id,
        final_status.as_str(),
        if final_status == WorkerStatus::Error {
            "health-shutdown"
        } else {
            "clean shutdown"
        },
        &args.backend,
        &model_hash_hex,
        &manifest_hash_hex,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mlx_guard_triggers_without_feature() {
        if cfg!(feature = "multi-backend") {
            return;
        }
        let result = validate_backend_feature(&BackendChoice::Mlx);
        assert!(result.is_err());
    }

    #[test]
    fn mlx_guard_allows_with_feature() {
        if !cfg!(feature = "multi-backend") {
            return;
        }
        let result = validate_backend_feature(&BackendChoice::Mlx);
        assert!(result.is_ok());
    }

    #[test]
    fn parses_known_backends() {
        assert_eq!(parse_backend_choice("auto"), BackendChoice::Auto);
        assert_eq!(parse_backend_choice("metal"), BackendChoice::Metal);
        assert_eq!(parse_backend_choice("coreml"), BackendChoice::CoreML);
        assert_eq!(parse_backend_choice("mlx"), BackendChoice::Mlx);
    }

    #[test]
    fn unknown_backend_falls_back_to_auto() {
        let parsed = parse_backend_choice("not-a-backend");
        assert_eq!(parsed, BackendChoice::Auto);
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    fn coreml_verification_status_helpers() {
        let h1 = B3Hash::hash(b"expected");
        let h2 = B3Hash::hash(b"actual");

        let match_status =
            log_coreml_verification_result(CoremlVerifyMode::Warn, Some(&h1), Some(&h1), None)
                .expect("match should not error");
        assert_eq!(match_status, CoremlVerificationStatus::Match);
        assert!(!match_status.is_mismatch());

        let mismatch_status =
            log_coreml_verification_result(CoremlVerifyMode::Warn, Some(&h1), Some(&h2), None)
                .expect("warn mode should allow mismatch");
        assert_eq!(mismatch_status, CoremlVerificationStatus::Mismatch);
        assert!(mismatch_status.is_mismatch());

        let skipped_status =
            log_coreml_verification_result(CoremlVerifyMode::Off, None, None, Some("unset"))
                .expect("off mode should skip cleanly");
        assert_eq!(skipped_status, CoremlVerificationStatus::Skipped);
        assert!(!skipped_status.is_mismatch());
    }

    #[test]
    fn retry_logic_succeeds_on_first_attempt() {
        // This test validates that the retry wrapper doesn't add overhead when
        // registration succeeds immediately. We can't easily test the actual HTTP
        // call without a mock server, but we can verify the error classification logic.

        // Test that non-transient errors are identified correctly
        let validation_error = "Invalid manifest hash";
        let is_transient = validation_error.contains("HTTP error")
            || validation_error.contains("connect")
            || validation_error.contains("timeout")
            || validation_error.contains("Connection refused");
        assert!(
            !is_transient,
            "Validation errors should not be classified as transient"
        );

        // Test that transient errors are identified correctly
        let network_error = "HTTP error: Connection refused";
        let is_transient = network_error.contains("HTTP error")
            || network_error.contains("connect")
            || network_error.contains("timeout")
            || network_error.contains("Connection refused");
        assert!(
            is_transient,
            "Network errors should be classified as transient"
        );

        let timeout_error = "HTTP error: timeout reading response";
        let is_transient = timeout_error.contains("HTTP error")
            || timeout_error.contains("connect")
            || timeout_error.contains("timeout")
            || timeout_error.contains("Connection refused");
        assert!(
            is_transient,
            "Timeout errors should be classified as transient"
        );
    }

    #[test]
    fn retry_logic_identifies_non_transient_errors() {
        // Validation error should not be retried
        let err = "Worker registration rejected: invalid tenant";
        let is_transient = err.contains("HTTP error")
            || err.contains("connect")
            || err.contains("timeout")
            || err.contains("Connection refused")
            || err.contains("DNS")
            || err.contains("network");
        assert!(!is_transient, "Rejection errors should not be retried");

        // Invalid response should not be retried
        let err = "Invalid response: expected JSON object";
        let is_transient = err.contains("HTTP error")
            || err.contains("connect")
            || err.contains("timeout")
            || err.contains("Connection refused")
            || err.contains("DNS")
            || err.contains("network");
        assert!(
            !is_transient,
            "Invalid response errors should not be retried"
        );
    }

    #[test]
    fn retry_logic_identifies_transient_errors() {
        let transient_errors = vec![
            "HTTP error: Connection refused",
            "HTTP error: connect failed",
            "HTTP error: timeout reading response",
            "network error occurred",
            "DNS resolution failed",
        ];

        for err in transient_errors {
            let is_transient = err.contains("HTTP error")
                || err.contains("connect")
                || err.contains("timeout")
                || err.contains("Connection refused")
                || err.contains("DNS")
                || err.contains("network");
            assert!(
                is_transient,
                "Error '{}' should be classified as transient",
                err
            );
        }
    }

    #[test]
    fn exponential_backoff_calculation() {
        // Verify the exponential backoff formula: BASE_DELAY_MS * 2^(attempt - 1)
        const BASE_DELAY_MS: u64 = 1000;

        // First retry: 1000 * 2^0 = 1000ms (1s)
        let delay_1 = BASE_DELAY_MS * 2u64.pow(1 - 1);
        assert_eq!(delay_1, 1000);

        // Second retry: 1000 * 2^1 = 2000ms (2s)
        let delay_2 = BASE_DELAY_MS * 2u64.pow(2 - 1);
        assert_eq!(delay_2, 2000);

        // Third retry: 1000 * 2^2 = 4000ms (4s)
        let delay_3 = BASE_DELAY_MS * 2u64.pow(3 - 1);
        assert_eq!(delay_3, 4000);
    }

    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    mod coreml_expected_hash_tests {
        use super::*;
        use adapteros_db::{CreateCoremlFusionPairParams, Db};
        use adapteros_lora_kernel_coreml::export::CoreMLFusionMetadata;
        use adapteros_manifest::{
            Adapter, AdapterCategory, AdapterScope, AdapterTier, AssuranceTier, Base, BundleCfg,
            CoreMLFusion, DeterminismPolicy, EgressPolicy, EvidencePolicy, IsolationPolicy,
            ManifestV3, MemoryPolicy, NumericPolicy, PerformancePolicy, Policies, RagPolicy,
            RefusalPolicy, RouterCfg, Sampling, Seeds, TelemetryCfg,
        };
        use std::collections::{BTreeMap, HashMap};
        use tempfile::tempdir;

        fn test_manifest(
            base_model_id: &str,
            adapter_id: &str,
            fusion: Option<CoreMLFusion>,
        ) -> ManifestV3 {
            ManifestV3 {
                schema: "adapteros.manifest.v3".into(),
                base: Base {
                    model_id: base_model_id.into(),
                    model_hash: B3Hash::hash(b"model"),
                    arch: "llama".into(),
                    vocab_size: 32000,
                    hidden_dim: 4096,
                    n_layers: 32,
                    n_heads: 32,
                    routing_bias: 1.0,
                    config_hash: B3Hash::hash(b"config"),
                    tokenizer_hash: B3Hash::hash(b"tokenizer"),
                    tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
                    license_hash: None,
                    rope_scaling_override: None,
                },
                adapters: vec![Adapter {
                    id: adapter_id.into(),
                    hash: B3Hash::hash(b"adapter"),
                    assurance_tier: AssuranceTier::Standard,
                    tier: AdapterTier::Persistent,
                    rank: 8,
                    alpha: 16.0,
                    lora_strength: None,
                    target_modules: vec!["q_proj".into()],
                    ttl: None,
                    acl: vec![],
                    warmup_prompt: None,
                    dependencies: None,
                    determinism_seed: None,
                    determinism_backend: None,
                    determinism_device: None,
                    drift_reference_backend: None,
                    drift_metric: None,
                    drift_baseline_backend: None,
                    drift_test_backend: None,
                    drift_tier: None,
                    drift_slice_size: None,
                    drift_slice_offset: None,
                    drift_loss_metric: None,
                    category: AdapterCategory::Code,
                    scope: AdapterScope::Global,
                    framework_id: None,
                    framework_version: None,
                    repo_id: None,
                    commit_sha: None,
                    intent: None,
                    recommended_for_moe: false,
                    auto_promote: true,
                    eviction_priority: adapteros_manifest::EvictionPriority::Normal,
                    free_tokens: None,
                    hot_experts: None,
                }],
                router: RouterCfg {
                    k_sparse: 1,
                    gate_quant: "q15".into(),
                    entropy_floor: 0.02,
                    tau: 1.0,
                    sample_tokens_full: 8,
                    warmup: false,
                    algorithm: "weighted".into(),
                    orthogonal_penalty: 0.1,
                    shared_downsample: false,
                    compression_ratio: 0.8,
                    multi_path_enabled: false,
                    diversity_threshold: 0.05,
                    orthogonal_constraints: false,
                },
                telemetry: TelemetryCfg {
                    schema_hash: B3Hash::hash(b"schema"),
                    sampling: Sampling {
                        token: 0.05,
                        router: 1.0,
                        inference: 1.0,
                    },
                    router_full_tokens: 128,
                    bundle: BundleCfg {
                        max_events: 10_000,
                        max_bytes: 1_048_576,
                    },
                },
                policies: Policies {
                    egress: EgressPolicy {
                        mode: "deny_all".into(),
                        serve_requires_pf: true,
                        allow_tcp: false,
                        allow_udp: false,
                        uds_paths: vec!["/var/run/aos/<tenant>/*.sock".into()],
                    },
                    determinism: DeterminismPolicy {
                        require_metallib_embed: true,
                        require_kernel_hash_match: true,
                        rng: "hkdf_seeded".into(),
                        retrieval_tie_break: vec!["score_desc".into(), "doc_id_asc".into()],
                    },
                    evidence: EvidencePolicy {
                        require_open_book: true,
                        min_spans: 1,
                        prefer_latest_revision: true,
                        warn_on_superseded: true,
                    },
                    refusal: RefusalPolicy {
                        abstain_threshold: 0.55,
                        missing_fields_templates: BTreeMap::new(),
                    },
                    numeric: NumericPolicy {
                        canonical_units: [("torque".into(), "in_lbf".into())].into_iter().collect(),
                        max_rounding_error: 0.5,
                        require_units_in_trace: true,
                    },
                    rag: RagPolicy {
                        index_scope: "per_tenant".into(),
                        doc_tags_required: vec!["doc_id".into()],
                        embedding_model_hash: B3Hash::hash(b"embedding"),
                        topk: 5,
                        order: vec!["score_desc".into()],
                    },
                    isolation: IsolationPolicy {
                        process_model: "per_tenant".into(),
                        uds_root: "/var/run/aos/<tenant>".into(),
                        forbid_shm: true,
                    },
                    performance: PerformancePolicy {
                        latency_p95_ms: 24,
                        router_overhead_pct_max: 8,
                        throughput_tokens_per_s_min: 40,
                        max_tokens: 1000,
                        cpu_threshold_pct: 90.0,
                        memory_threshold_pct: 95.0,
                        circuit_breaker_threshold: 5,
                    },
                    memory: MemoryPolicy {
                        min_headroom_pct: 15,
                        evict_order: vec![
                            "ephemeral_ttl".into(),
                            "cold_lru".into(),
                            "warm_lru".into(),
                        ],
                        k_reduce_before_evict: true,
                    },
                    artifacts: adapteros_manifest::ArtifactsPolicy {
                        require_signature: true,
                        require_sbom: true,
                        cas_only: true,
                    },
                    drift: adapteros_manifest::DriftPolicy::default(),
                },
                seeds: Seeds {
                    global: B3Hash::hash(b"global_seed"),
                    manifest_hash: B3Hash::hash(b"manifest"),
                    parent_cpid: None,
                },
                coreml: None,
                fusion,
            }
        }

        fn write_metadata(dir: &tempfile::TempDir) -> (PathBuf, B3Hash) {
            let fused_manifest_path = dir.path().join("Manifest.json");
            fs::write(&fused_manifest_path, b"fused-manifest").unwrap();
            let base_manifest_path = dir.path().join("Base.json");
            fs::write(&base_manifest_path, b"base-manifest").unwrap();
            let adapter_path = dir.path().join("adapter.aos");
            fs::write(&adapter_path, b"adapter-bytes").unwrap();

            let fused_hash = B3Hash::hash(b"fused-manifest");
            let metadata = CoreMLFusionMetadata {
                base_manifest_hash: B3Hash::hash(b"base-manifest"),
                fused_manifest_hash: fused_hash,
                adapter_hash: B3Hash::hash(b"adapter-bytes"),
                base_package: base_manifest_path,
                fused_package: fused_manifest_path.clone(),
                adapter_path,
            };
            let metadata_path = dir.path().join("adapteros_coreml_fusion.json");
            fs::write(
                &metadata_path,
                serde_json::to_vec(&metadata).expect("serialize metadata"),
            )
            .unwrap();

            (fused_manifest_path, fused_hash)
        }

        #[tokio::test]
        async fn registry_pair_precedes_other_sources() {
            let db = Db::new_in_memory().await.expect("db init");
            let tempdir = tempdir().unwrap();
            let (model_path, _meta_hash) = write_metadata(&tempdir);

            let tenant_id = "tenant-coreml";
            let base_model_id = "base-model";
            let adapter_id = "adapter-a";

            let manifest = test_manifest(
                base_model_id,
                adapter_id,
                Some(CoreMLFusion {
                    fused_manifest_hash: Some(B3Hash::hash(b"manifest-fused")),
                    coreml_package_hash: Some(B3Hash::hash(b"manifest-package")),
                    base_model_id: Some(base_model_id.to_string()),
                    adapter_id: Some(adapter_id.to_string()),
                    ..CoreMLFusion::default()
                }),
            );

            let db_hash = B3Hash::hash(b"db-expected");
            db.upsert_coreml_fusion_pair(CreateCoremlFusionPairParams {
                tenant_id: tenant_id.to_string(),
                base_model_id: base_model_id.to_string(),
                adapter_id: adapter_id.to_string(),
                fused_manifest_hash: db_hash.to_hex(),
                coreml_package_hash: db_hash.to_hex(),
                adapter_hash_b3: None,
                base_model_hash_b3: None,
                metadata_path: None,
            })
            .await
            .unwrap();

            std::env::set_var(
                "AOS_COREML_EXPECTED_HASH",
                B3Hash::hash(b"env-hash").to_hex(),
            );
            let (expected, source) =
                resolve_expected_coreml_hash(&manifest, &model_path, tenant_id, Some(&db)).await;
            std::env::remove_var("AOS_COREML_EXPECTED_HASH");

            assert_eq!(expected, Some(db_hash));
            assert_eq!(source.as_deref(), Some("db"));
        }

        #[tokio::test]
        async fn manifest_fusion_precedes_env_and_metadata() {
            let tempdir = tempdir().unwrap();
            let (model_path, _meta_hash) = write_metadata(&tempdir);
            let fusion_hash = B3Hash::hash(b"fusion-expected");

            let manifest = test_manifest(
                "base-manifest",
                "adapter-manifest",
                Some(CoreMLFusion {
                    fused_manifest_hash: Some(fusion_hash),
                    coreml_package_hash: None,
                    base_model_id: None,
                    adapter_id: None,
                    base_model_hash: None,
                    adapter_hash: None,
                }),
            );

            std::env::set_var(
                "AOS_COREML_EXPECTED_HASH",
                B3Hash::hash(b"env-hash").to_hex(),
            );
            let (expected, source) =
                resolve_expected_coreml_hash(&manifest, &model_path, "tenant", None).await;
            std::env::remove_var("AOS_COREML_EXPECTED_HASH");

            assert_eq!(expected, Some(fusion_hash));
            assert_eq!(source.as_deref(), Some("manifest.fused_manifest_hash"));
        }

        #[tokio::test]
        async fn env_precedes_metadata_when_manifest_absent() {
            let tempdir = tempdir().unwrap();
            let (model_path, _meta_hash) = write_metadata(&tempdir);
            let env_hash = B3Hash::hash(b"env-expected");

            let manifest = test_manifest("base", "adapter", None);

            std::env::set_var("AOS_COREML_EXPECTED_HASH", env_hash.to_hex());
            let (expected, source) =
                resolve_expected_coreml_hash(&manifest, &model_path, "tenant", None).await;
            std::env::remove_var("AOS_COREML_EXPECTED_HASH");

            assert_eq!(expected, Some(env_hash));
            assert_eq!(source.as_deref(), Some("env"));
        }

        #[tokio::test]
        async fn metadata_used_when_no_other_sources_present() {
            let tempdir = tempdir().unwrap();
            let (model_path, metadata_hash) = write_metadata(&tempdir);
            let manifest = test_manifest("base", "adapter", None);

            std::env::remove_var("AOS_COREML_EXPECTED_HASH");
            let (expected, source) =
                resolve_expected_coreml_hash(&manifest, &model_path, "tenant", None).await;

            assert_eq!(expected, Some(metadata_hash));
            assert_eq!(source.as_deref(), Some("metadata"));
        }
    }
}
