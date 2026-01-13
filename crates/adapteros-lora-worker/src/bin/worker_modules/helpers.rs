use adapteros_core::identity::IdentityEnvelope;
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEvent as UnifiedTelemetryEvent};
use adapteros_telemetry::TelemetryWriter;
use adapteros_lora_worker::panic_utils::{
    build_fatal_payload, extract_panic_message, format_panic_location, truncate_backtrace,
};
use std::sync::OnceLock;
use tracing::{warn, info};

/// Worker identity for panic hook access
#[derive(Debug, Clone)]
pub struct WorkerIdentity {
    pub worker_id: String,
    pub cp_url: String,
    pub tenant_id: String,
}

// Worker panic hook support for fatal error reporting
// Global state for panic hook (must be static for panic handler access)
pub static WORKER_IDENTITY: OnceLock<WorkerIdentity> = OnceLock::new();
pub static WORKER_TELEMETRY: OnceLock<TelemetryWriter> = OnceLock::new();

/// Set up panic hook to report fatal errors to the control plane
pub fn setup_panic_hook() {
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

        if let Some(writer) = WORKER_TELEMETRY.get() {
            let identity_snapshot = WORKER_IDENTITY.get().cloned();
            let _ = std::panic::catch_unwind(|| {
                let (tenant_id, worker_id) = identity_snapshot
                    .as_ref()
                    .map(|id| (id.tenant_id.clone(), id.worker_id.clone()))
                    .unwrap_or_else(|| ("system".to_string(), "unknown".to_string()));
                let location_meta = location.clone();
                let message_meta = message.clone();
                let identity = IdentityEnvelope::new(
                    tenant_id,
                    "worker".to_string(),
                    "panic".to_string(),
                    "1.0".to_string(),
                );
                let event = UnifiedTelemetryEvent {
                    id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
                    timestamp: chrono::Utc::now(),
                    event_type: EventType::SystemError.as_str().to_string(),
                    level: LogLevel::Critical,
                    message: "Worker panic captured".to_string(),
                    component: Some("aos-worker".to_string()),
                    identity,
                    user_id: None,
                    metadata: Some(serde_json::json!({
                        "worker_id": worker_id,
                        "location": location_meta,
                        "message": message_meta,
                    })),
                    trace_id: None,
                    span_id: None,
                    hash: None,
                    sampling_rate: None,
                };
                let _ = writer.log_event(event);
                let _ = writer.flush_with_timeout(std::time::Duration::from_secs(2));
            });
        }

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

pub async fn shutdown_worker_telemetry(timeout: std::time::Duration) {
    let Some(writer) = WORKER_TELEMETRY.get() else {
        return;
    };
    let writer = writer.clone();
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = writer.shutdown_with_timeout(timeout) {
            warn!(error = %e, "Telemetry shutdown did not complete cleanly");
        }
    })
    .await;
}

use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_config::parse_bool;
use adapteros_core::{B3Hash, Result};
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_worker::{
    backend_factory::{detect_capabilities as detect_backend_capabilities, get_model_cache},
    model_handle_cache::ModelHandle,
    model_key::{ModelCacheIdentity, ModelKey},
};
use adapteros_lora_worker::backend_factory::BackendChoice;
use std::sync::Arc;

pub fn dev_no_auth_enabled() -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }

    match std::env::var("AOS_DEV_NO_AUTH") {
        Ok(raw) => match parse_bool(&raw) {
            Ok(value) => value,
            Err(err) => {
                warn!(error = %err, value = %raw, "Invalid AOS_DEV_NO_AUTH value");
                false
            }
        },
        Err(_) => false,
    }
}

/// Detect backend capabilities based on compiled features
pub fn detect_capabilities(backend_choice: &str) -> Vec<String> {
    let mut caps = vec![];

    // Check if MLX is actually compiled into this binary
    #[cfg(feature = "multi-backend")]
    let has_mlx_support = true;
    #[cfg(not(feature = "multi-backend"))]
    let has_mlx_support = false;

    // Add backend capability based on requested backend AND compiled features
    match backend_choice.to_lowercase().as_str() {
        "mock" => caps.push("mock".to_string()),
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

pub fn build_capabilities_detail(backend_choice: BackendChoice) -> WorkerCapabilities {
    let backend_kind = match backend_choice {
        BackendChoice::MlxBridge => "bridge",
        BackendChoice::Mlx => "mlx",
        BackendChoice::CoreML => "coreml",
        BackendChoice::Metal => "metal",
        BackendChoice::CPU => "cpu",
        BackendChoice::Auto => "auto",
    };

    let (supports_step, supports_bulk, supports_logits, supports_streaming) = match backend_choice {
        BackendChoice::MlxBridge => (false, true, false, false),
        BackendChoice::Mlx | BackendChoice::CoreML | BackendChoice::Metal => {
            (true, false, true, true)
        }
        _ => (false, false, false, false),
    };

    let implementation = if matches!(backend_choice, BackendChoice::MlxBridge) {
        Some("mlx_subprocess".to_string())
    } else {
        None
    };

    let runtime_caps = detect_backend_capabilities();
    let multi_backend = cfg!(feature = "multi-backend");
    let gpu_backward = multi_backend && runtime_caps.has_mlx;

    WorkerCapabilities {
        backend_kind: backend_kind.to_string(),
        implementation,
        supports_step,
        supports_bulk,
        supports_logits,
        supports_streaming,
        gpu_backward,
        multi_backend,
    }
}

pub fn mock_capabilities_detail() -> WorkerCapabilities {
    WorkerCapabilities {
        backend_kind: "mock".to_string(),
        implementation: Some("mock_kernels".to_string()),
        supports_step: true,
        supports_bulk: false,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: false,
        multi_backend: false,
    }
}

pub fn setup_mock_base_model_cache(manifest_hash: &B3Hash, cache_budget_bytes: u64) -> Result<()> {
    let cache = match get_model_cache() {
        Ok(cache) => cache,
        Err(e) => {
            warn!(error = %e, "Mock backend: model cache unavailable, skipping pin setup");
            return Ok(());
        }
    };

    let cache_key = ModelKey::new(
        BackendType::Mock,
        *manifest_hash,
        ModelCacheIdentity::for_backend(BackendType::Mock),
    );
    cache.set_base_model_key(&cache_key);

    let max_bytes = cache.max_memory_bytes().min(cache_budget_bytes);
    let mut mock_bytes = max_bytes / 8;
    if mock_bytes == 0 {
        mock_bytes = 1024 * 1024;
    }
    if mock_bytes > 64 * 1024 * 1024 {
        mock_bytes = 64 * 1024 * 1024;
    }
    if mock_bytes > max_bytes && max_bytes > 0 {
        mock_bytes = max_bytes;
    }

    let load = || {
        Ok((
            ModelHandle::Metal(Arc::new(vec![0u8; mock_bytes as usize])),
            mock_bytes,
        ))
    };

    if cache.base_model_pin_enabled() {
        cache.get_or_load_base_model(&cache_key, load)?;
    } else {
        cache.get_or_load(&cache_key, load)?;
    }

    info!(
        pinned = cache.is_pinned(&cache_key),
        mock_bytes, "Mock backend: base model cached"
    );

    Ok(())
}
