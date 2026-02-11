//! Training endpoint handlers
//!
//! This module contains handlers for training-related operations.
//! These handlers are designed to work with the adapteros-server-api state and types.
//!
//! # Integration
//!
//! These handlers expect to be integrated with adapteros-server-api which provides:
//! - `AppState` - Application state with database, services, and configuration
//! - `Claims` - Authenticated user claims
//! - `Permission` - Permission system
//! - `ApiError` / `ErrorResponse` - Error handling types
//!
//! # Handler Categories
//!
//! ## Job Management
//! - `list_training_jobs` - List jobs with filtering
//! - `get_training_job` - Get single job details
//! - `create_training_job` - Create workspace-scoped job
//! - `start_training` - Start full training job
//! - `cancel_training` - Cancel running job
//! - `retry_training` - Retry failed job
//!
//! ## Version Management
//! - `promote_version` - Promote adapter version
//! - `publish_version` - Publish with attach mode
//!
//! ## Backend & Preprocessing
//! - `get_training_backend_readiness` - Check backend availability
//! - `get_preprocess_status` - Check preprocessing cache
//! - `export_coreml_training_job` - Trigger CoreML export
//!
//! ## Metrics & Logs
//! - `get_training_logs` - Get job logs
//! - `get_training_metrics` - Get job metrics
//! - `get_training_report` - Get training report artifact
//!
//! ## Queue & Priority
//! - `get_training_queue` - Get queue status
//! - `update_training_priority` - Update job priority
//!
//! ## Templates
//! - `list_training_templates` - List templates
//! - `get_training_template` - Get template
//!
//! ## Chat Integration
//! - `get_chat_bootstrap` - Get chat bootstrap data
//! - `create_chat_from_training_job` - Create chat from job
//!
//! ## Batch Operations
//! - `batch_training_status` - Get batch status

// Re-export types
pub use crate::types::*;

// Note: The actual handler implementations are designed to be used with
// adapteros-server-api's AppState, Claims, and error handling.
//
// When this crate is integrated, the parent crate will:
// 1. Import these handlers
// 2. Wire them up in routes with the appropriate state extractors
// 3. Use the parent crate's middleware for auth, permissions, tenant isolation
//
// The handlers below document the expected signatures and logic flow.
// The actual integration happens in adapteros-server-api's finalization.rs

use adapteros_api_types::TrainingBackendCapabilities;
use adapteros_api_types::TrainingCoremlReadiness;
use adapteros_lora_worker::backend_factory::BackendCapabilities;
use adapteros_types::training::{TrainingBackendKind, TrainingBackendPolicy};

// ============================================================================
// Backend Readiness Helpers
// ============================================================================

/// Map backend capabilities to API response type
pub fn map_capabilities(capabilities: &BackendCapabilities) -> TrainingBackendCapabilities {
    TrainingBackendCapabilities {
        has_coreml: capabilities.has_coreml,
        has_ane: capabilities.has_ane,
        has_metal: capabilities.has_metal,
        has_mlx: capabilities.has_mlx,
        has_mlx_bridge: Some(capabilities.has_mlx_bridge),
        metal_device_name: capabilities.metal_device_name.clone(),
        gpu_memory_bytes: capabilities.gpu_memory_bytes,
    }
}

/// Check if a backend is available given capabilities
pub fn backend_available(
    backend: TrainingBackendKind,
    coreml_available: bool,
    capabilities: &BackendCapabilities,
    require_gpu: bool,
) -> bool {
    match backend {
        TrainingBackendKind::CoreML => coreml_available,
        TrainingBackendKind::Mlx => capabilities.has_mlx,
        TrainingBackendKind::Metal => capabilities.has_metal,
        TrainingBackendKind::Cpu => !require_gpu,
        TrainingBackendKind::Auto => {
            coreml_available || capabilities.has_mlx || capabilities.has_metal || !require_gpu
        }
    }
}

/// Choose fallback backend based on preferences and availability
pub fn choose_fallback(
    preferred: Option<TrainingBackendKind>,
    coreml_available: bool,
    capabilities: &BackendCapabilities,
    require_gpu: bool,
) -> Option<TrainingBackendKind> {
    let mut order = Vec::new();
    if let Some(pref) = preferred {
        order.push(pref);
    }
    order.extend_from_slice(&[
        TrainingBackendKind::Mlx,
        TrainingBackendKind::Metal,
        TrainingBackendKind::Cpu,
    ]);

    order
        .into_iter()
        .find(|backend| backend_available(*backend, coreml_available, capabilities, require_gpu))
}

/// Choose auto backend based on availability
pub fn choose_auto_backend(
    coreml_available: bool,
    capabilities: &BackendCapabilities,
    require_gpu: bool,
) -> Option<TrainingBackendKind> {
    [
        TrainingBackendKind::CoreML,
        TrainingBackendKind::Mlx,
        TrainingBackendKind::Metal,
        TrainingBackendKind::Cpu,
    ]
    .into_iter()
    .find(|backend| backend_available(*backend, coreml_available, capabilities, require_gpu))
}

/// Get reason why CoreML is unavailable
pub fn coreml_unavailable_reason(
    capabilities: &BackendCapabilities,
    coreml: &TrainingCoremlReadiness,
) -> String {
    if capabilities.has_coreml && !coreml.ane_available {
        "ane_unavailable".to_string()
    } else {
        "coreml_unavailable".to_string()
    }
}

/// Plan backend readiness based on request and capabilities
pub fn plan_backend_readiness(
    requested_backend: TrainingBackendKind,
    backend_policy: TrainingBackendPolicy,
    coreml_fallback: Option<TrainingBackendKind>,
    require_gpu: bool,
    capabilities: &BackendCapabilities,
    coreml: &TrainingCoremlReadiness,
) -> BackendPlan {
    let coreml_available = coreml.available && coreml.ane_available;
    let mut resolved = requested_backend;
    let mut fallback_backend = None;
    let mut fallback_reason = None;
    let mut ready = true;
    let mut warnings = Vec::new();

    match backend_policy {
        TrainingBackendPolicy::CoremlOnly => {
            if coreml_available {
                resolved = TrainingBackendKind::CoreML;
            } else {
                ready = false;
                fallback_reason = Some("coreml_required_unavailable".to_string());
                warnings.push(
                    "CoreML is required but unavailable; training will block until ANE/GPU is ready"
                        .to_string(),
                );
            }
        }
        TrainingBackendPolicy::CoremlElseFallback => {
            if coreml_available {
                resolved = TrainingBackendKind::CoreML;
            } else if let Some(fallback) =
                choose_fallback(coreml_fallback, coreml_available, capabilities, require_gpu)
            {
                resolved = fallback;
                fallback_backend = Some(fallback);
                fallback_reason = Some("coreml_policy_fallback".to_string());
                warnings.push(format!(
                    "CoreML unavailable; falling back to {}",
                    fallback.as_str()
                ));
            } else {
                ready = false;
                fallback_reason = Some("coreml_policy_no_backend".to_string());
                warnings.push(
                    "CoreML unavailable and no fallback backend available for training".to_string(),
                );
            }
        }
        TrainingBackendPolicy::Auto => match requested_backend {
            TrainingBackendKind::CoreML => {
                if coreml_available {
                    resolved = TrainingBackendKind::CoreML;
                } else if let Some(fallback) =
                    choose_fallback(coreml_fallback, coreml_available, capabilities, require_gpu)
                {
                    resolved = fallback;
                    fallback_backend = Some(fallback);
                    fallback_reason = Some(coreml_unavailable_reason(capabilities, coreml));
                    warnings.push(format!(
                        "CoreML unavailable; using {} fallback",
                        fallback.as_str()
                    ));
                } else {
                    ready = false;
                    fallback_reason = Some(coreml_unavailable_reason(capabilities, coreml));
                    warnings.push(
                        "CoreML unavailable and no fallback backend available for training"
                            .to_string(),
                    );
                }
            }
            TrainingBackendKind::Auto => {
                if let Some(auto_backend) =
                    choose_auto_backend(coreml_available, capabilities, require_gpu)
                {
                    resolved = auto_backend;
                    if auto_backend != TrainingBackendKind::CoreML && coreml_available {
                        warnings.push(format!(
                            "CoreML available but auto-selected {} based on policy",
                            auto_backend.as_str()
                        ));
                    }
                } else {
                    ready = false;
                    fallback_reason = Some("no_backend_available".to_string());
                    warnings.push(
                        "No compatible backend available for training on this host".to_string(),
                    );
                }
            }
            other => {
                if !backend_available(other, coreml_available, capabilities, require_gpu) {
                    ready = false;
                    fallback_reason = Some("requested_backend_unavailable".to_string());
                    warnings.push(format!(
                        "Requested backend {} is unavailable on this host",
                        other.as_str()
                    ));
                }
            }
        },
    }

    if ready && !backend_available(resolved, coreml_available, capabilities, require_gpu) {
        ready = false;
        warnings.push(format!(
            "Resolved backend {} is not available after capability detection",
            resolved.as_str()
        ));
        fallback_reason.get_or_insert_with(|| "resolved_backend_unavailable".to_string());
    }

    BackendPlan {
        resolved_backend: resolved,
        fallback_backend,
        fallback_reason,
        ready,
        warnings,
    }
}

/// Build CoreML readiness information from capabilities
pub fn build_coreml_readiness(capabilities: &BackendCapabilities) -> TrainingCoremlReadiness {
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    {
        use adapteros_lora_worker::backend_factory::resolve_coreml_backend_settings;

        fn coreml_compute_units_label(units: adapteros_lora_worker::ComputeUnits) -> String {
            match units {
                adapteros_lora_worker::ComputeUnits::CpuOnly => "cpu_only",
                adapteros_lora_worker::ComputeUnits::CpuAndGpu => "cpu_and_gpu",
                adapteros_lora_worker::ComputeUnits::CpuAndNeuralEngine => "cpu_and_ne",
                adapteros_lora_worker::ComputeUnits::All => "all",
            }
            .to_string()
        }

        let settings = resolve_coreml_backend_settings();
        TrainingCoremlReadiness {
            available: capabilities.has_coreml,
            gpu_available: settings.gpu_available,
            ane_available: settings.ane_available,
            compute_units_preference: Some(settings.preference.as_str().to_string()),
            compute_units_effective: Some(coreml_compute_units_label(settings.compute_units)),
            gpu_used: settings.gpu_used,
            ane_used: settings.ane_used,
            production_mode: settings.production_mode,
        }
    }

    #[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
    {
        TrainingCoremlReadiness {
            available: capabilities.has_coreml,
            gpu_available: capabilities.has_metal,
            ane_available: capabilities.has_ane,
            compute_units_preference: None,
            compute_units_effective: None,
            gpu_used: false,
            ane_used: false,
            production_mode: false,
        }
    }
}

// ============================================================================
// Metric Constants
// ============================================================================

/// Metric name for jobs rejected due to lineage requirements
pub const METRIC_LINEAGE_REQUIRED: &str = "training_jobs_rejected_lineage_required";

/// Metric name for jobs rejected due to trust blocked state
pub const METRIC_TRUST_BLOCKED: &str = "training_jobs_rejected_trust_blocked";

/// Metric name for jobs rejected due to trust needs approval state
pub const METRIC_TRUST_NEEDS_APPROVAL: &str = "training_jobs_rejected_trust_needs_approval";

// ============================================================================
// Validation Helpers
// ============================================================================

use adapteros_api_types::training::TrainingConfigRequest;
use adapteros_db::adapter_repositories::AdapterRepository;

/// Validate training request against guardrails
pub fn validate_training_guardrails(
    config: &TrainingConfigRequest,
    repo: &AdapterRepository,
    base_model: &adapteros_db::models::Model,
    dataset_version: Option<&adapteros_db::training_datasets::TrainingDatasetVersion>,
) -> Result<(), GuardrailError> {
    if let Err(errors) = config.validate() {
        return Err(GuardrailError {
            code: "INVALID_CONFIG",
            message: errors.join("; "),
        });
    }

    if config.targets.is_empty() {
        return Err(GuardrailError {
            code: "INVALID_CONFIG",
            message: "targets must not be empty".to_string(),
        });
    }

    if let Some(split) = config.validation_split {
        if !(0.0..=0.5).contains(&split) {
            return Err(GuardrailError {
                code: "INVALID_CONFIG",
                message: "validation_split must be between 0.0 and 0.5".to_string(),
            });
        }
    }

    if let Some(repo_base) = repo.base_model_id.as_deref() {
        if repo_base != base_model.id {
            return Err(GuardrailError {
                code: "BASE_MODEL_MISMATCH",
                message: format!(
                    "Repository base_model_id {} does not match loaded model {}",
                    repo_base, base_model.id
                ),
            });
        }
    }

    if let Some(version) = dataset_version {
        if version.trust_state == "blocked" || version.trust_state == "needs_approval" {
            return Err(GuardrailError {
                code: "DATASET_UNTRUSTED",
                message: format!(
                    "Dataset version {} trust_state={} blocks training",
                    version.id, version.trust_state
                ),
            });
        }

        if let Some(manifest_json) = version.manifest_json.as_deref() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(manifest_json) {
                if let Some(m_base) = value.get("base_model_id").and_then(|v| v.as_str()) {
                    if m_base != base_model.id {
                        return Err(GuardrailError {
                            code: "BASE_MODEL_MISMATCH",
                            message: format!(
                                "Dataset manifest base_model_id {} does not match model {}",
                                m_base, base_model.id
                            ),
                        });
                    }
                }
                if let Some(hash) = value.get("base_model_hash_b3").and_then(|v| v.as_str()) {
                    if hash != base_model.hash_b3 {
                        return Err(GuardrailError {
                            code: "BASE_MODEL_HASH_MISMATCH",
                            message: format!(
                                "Dataset manifest base_model_hash_b3 {} does not match model {}",
                                hash, base_model.hash_b3
                            ),
                        });
                    }
                }
                if let Some(tok_hash) = value.get("tokenizer_hash_b3").and_then(|v| v.as_str()) {
                    if tok_hash != base_model.tokenizer_hash_b3 {
                        return Err(GuardrailError {
                            code: "TOKENIZER_HASH_MISMATCH",
                            message: format!(
                                "Dataset manifest tokenizer_hash_b3 {} does not match model {}",
                                tok_hash, base_model.tokenizer_hash_b3
                            ),
                        });
                    }
                }
            }
        }
    }

    if matches!(
        (
            config.preferred_backend,
            config.backend_policy.unwrap_or_default(),
            config.coreml_training_fallback
        ),
        (
            Some(TrainingBackendKind::CoreML),
            TrainingBackendPolicy::CoremlOnly,
            None
        )
    ) {
        return Err(GuardrailError {
            code: "BACKEND_UNAVAILABLE",
            message: "CoreML requested without fallback; fail fast when unavailable".to_string(),
        });
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(coreml: bool, ane: bool, metal: bool, mlx: bool) -> BackendCapabilities {
        BackendCapabilities {
            has_coreml: coreml,
            has_ane: ane,
            has_metal: metal,
            has_mlx: mlx,
            ..Default::default()
        }
    }

    #[test]
    fn backend_readiness_prefers_coreml_when_available() {
        let capabilities = caps(true, true, true, true);
        let coreml = build_coreml_readiness(&capabilities);
        let plan = plan_backend_readiness(
            TrainingBackendKind::CoreML,
            TrainingBackendPolicy::Auto,
            None,
            false,
            &capabilities,
            &coreml,
        );

        assert!(plan.ready);
        assert_eq!(plan.resolved_backend, TrainingBackendKind::CoreML);
        assert!(plan.fallback_backend.is_none());
        assert!(plan.fallback_reason.is_none());
    }

    #[test]
    fn backend_readiness_falls_back_when_coreml_missing() {
        let capabilities = caps(false, false, true, true);
        let coreml = build_coreml_readiness(&capabilities);
        let plan = plan_backend_readiness(
            TrainingBackendKind::CoreML,
            TrainingBackendPolicy::Auto,
            Some(TrainingBackendKind::Mlx),
            false,
            &capabilities,
            &coreml,
        );

        assert!(plan.ready);
        assert_eq!(plan.resolved_backend, TrainingBackendKind::Mlx);
        assert_eq!(plan.fallback_backend, Some(TrainingBackendKind::Mlx));
        assert_eq!(plan.fallback_reason.as_deref(), Some("coreml_unavailable"));
    }

    #[test]
    fn backend_readiness_blocks_when_policy_requires_coreml() {
        let capabilities = caps(false, false, false, false);
        let coreml = build_coreml_readiness(&capabilities);
        let plan = plan_backend_readiness(
            TrainingBackendKind::CoreML,
            TrainingBackendPolicy::CoremlOnly,
            None,
            true,
            &capabilities,
            &coreml,
        );

        assert!(!plan.ready);
        assert_eq!(
            plan.fallback_reason.as_deref(),
            Some("coreml_required_unavailable")
        );
    }
}
