//! aos-worker binary - Standalone inference worker
//!
//! This binary provides a UDS-based inference server that can be spawned
//! by the node agent or run standalone for development/testing.
//!
//! Usage:
//!   aos-worker --uds-path var/run/worker.sock --manifest manifests/qwen32b-coder-mlx.yaml \
//!              --model-path /var/models/Llama-3.2-3B-Instruct-4bit --manifest-hash <HASH>

mod worker_modules;

use adapteros_core::{identity::IdentityEnvelope, Result};
use adapteros_telemetry::unified_events::{
    EventType, LogLevel, TelemetryEvent as UnifiedTelemetryEvent,
};
use tracing::error;
use worker_modules::{
    error_to_exit_code, run_worker, shutdown_worker_telemetry, EXIT_SUCCESS, WORKER_IDENTITY,
    WORKER_TELEMETRY,
};

fn main() -> Result<()> {
    // CRITICAL: Initialize MLX BEFORE tokio runtime starts
    // This ensures the MLX C library's Metal device is initialized before any
    // other Metal operations that might come from tokio or other async code.
    #[cfg(feature = "mlx")]
    {
        // Try to initialize MLX early, but don't fail the worker if it doesn't work
        // The backend selection logic will handle unavailability gracefully
        match adapteros_lora_mlx_ffi::mlx_runtime_init() {
            Ok(()) => {
                let impl_name = adapteros_lora_mlx_ffi::mlx_selected_implementation()
                    .map(|imp| imp.as_str())
                    .unwrap_or("unknown");
                eprintln!(
                    "MLX runtime initialized early (before tokio, impl: {})",
                    impl_name
                );
            }
            Err(e) => eprintln!("MLX early init failed (will use fallback backend): {}", e),
        }
    }

    // Now start the tokio runtime
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime")
        .block_on(async {
            // Run the actual worker logic and map errors to exit codes
            let result = run_worker().await;
            let exit_code = match &result {
                Ok(()) => EXIT_SUCCESS,
                Err(e) => {
                    let exit_code = error_to_exit_code(e);
                    error!(
                        error = %e,
                        exit_code = exit_code,
                        "Worker exiting with error"
                    );
                    exit_code
                }
            };

            if let Err(e) = result {
                if let Some(writer) = WORKER_TELEMETRY.get() {
                    let identity_snapshot = WORKER_IDENTITY.get().cloned();
                    let (tenant_id, worker_id) = identity_snapshot
                        .as_ref()
                        .map(|id| (id.tenant_id.clone(), id.worker_id.clone()))
                        .unwrap_or_else(|| ("system".to_string(), "unknown".to_string()));
                    let identity = IdentityEnvelope::new(
                        tenant_id,
                        "worker".to_string(),
                        "shutdown".to_string(),
                        "1.0".to_string(),
                    );
                    let event = UnifiedTelemetryEvent {
                        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
                        timestamp: chrono::Utc::now(),
                        event_type: EventType::SystemError.as_str().to_string(),
                        level: LogLevel::Error,
                        message: "Worker exiting with error".to_string(),
                        component: Some("aos-worker".to_string()),
                        identity,
                        user_id: None,
                        metadata: Some(serde_json::json!({
                            "worker_id": worker_id,
                            "error": e.to_string(),
                        })),
                        trace_id: None,
                        span_id: None,
                        hash: None,
                        sampling_rate: None,
                    };
                    let _ = writer.log_event(event);
                }
            }
            shutdown_worker_telemetry(std::time::Duration::from_secs(2)).await;
            std::process::exit(exit_code);
        })
}
