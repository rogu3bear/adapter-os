#![allow(dead_code)]
#![allow(unused_mut)]
#![allow(unused_imports)]
#![allow(clippy::await_holding_lock)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::cloned_ref_to_slice_refs)]
//! This crate provides:
//! - Core worker implementation for ML inference
//! - Resource limiting and timeout management
//! - Circuit breaker patterns for fault tolerance
//! - Memory management and adapter loading
//! - Telemetry and metrics collection
//!
//! # Examples
//!
//! Basic usage showing the Worker structure and inference request types:
//!
//! ```ignore
//! use adapteros_lora_worker::{InferenceRequest, RequestType};
//!
//! // Create an inference request
//! let request = InferenceRequest {
//!     cpid: "test-cpid".to_string(),
//!     prompt: "Hello, world!".to_string(),
//!     max_tokens: 100,
//!     request_id: None,
//!     run_envelope: None,
//!     require_evidence: false,
//!     request_type: RequestType::Normal,
//!     stack_id: None,
//!     stack_version: None,
//!     temperature: None,
//!     top_k: None,
//!     top_p: None,
//!     seed: None,
//!     router_seed: None,
//!     seed_mode: None,
//!     request_seed: None,
//!     fusion_interval: None,
//!     backend_profile: None,
//!     pinned_adapter_ids: None,
//!     determinism_mode: "strict".to_string(),
//!     strict_mode: false,
//!     effective_adapter_ids: None,
//!     placement: None,
//!     routing_policy: None,
//! };
//!
//! assert_eq!(request.max_tokens, 100);
//! ```
//!
//! For full Worker usage with inference, see the integration tests.

// use crate::active_learning;
use crate::adapter_hotswap::adapter_id_to_u16;
use crate::device_placement::{
    DeviceKind, LaneDescriptor, PlacementDecision, PlacementEngine, TelemetryCollector,
};
use crate::inference_management::{
    InferenceCancelGuard, InferenceCancelRegistry, InferenceCancelToken,
};
use crate::memory::MemoryPressureLevel;
use crate::request_pinner::RequestPinner;
use crate::response_types::TokenUsage;
use crate::router_bridge::decision_to_router_ring_with_active_ids_and_strengths;
use adapteros_api_types::inference::{
    FusionIntervalTrace, RouterDecisionChainEntry, RouterDecisionHash, RouterModelType, RunReceipt,
    STOP_Q15_DENOM,
};
use adapteros_config::{
    resolve_index_root, try_effective_config, ModelConfig, PlacementConfig, PlacementMode,
    PlacementWeights,
};
use adapteros_core::constants::DEFAULT_ADAPTER_CACHE_SIZE;
use adapteros_core::prefix_kv_key::compute_tokenizer_manifest_hash;
use adapteros_core::{
    compute_adapter_config_hash,
    determinism::{DeterminismContext, DeterminismSource},
    determinism_violation_event, emit_observability_event, AosError, B3Hash, BackendKind,
    CircuitBreaker as CircuitBreakerTrait, CircuitBreakerConfig, DeterminismViolationKind,
    EquipmentProfile, FusionInterval, ReceiptGenerator, RepoAdapterPaths, Result, RoutingRecord,
    SeedMode, StandardCircuitBreaker,
};
use adapteros_db::{
    Db, SqlTraceSink, TraceCancellation, TraceFinalization, TraceSink, TraceStart, TraceTokenInput,
};
use adapteros_lora_kernel_api::{attestation::DeterminismLevel, FusedKernels, IoBuffers};
use adapteros_lora_router::{
    constants::PINNED_BOOST, features::CodeFeatures, policy_mask::PolicyMask, AbstainContext,
    AdapterInfo, Router, RouterDeterminismConfig,
};
use adapteros_model_hub::manifest::ManifestV3;
use adapteros_policy::{PolicyEngine, RefusalResponse};
use adapteros_retrieval::rag::RagSystem;
use adapteros_telemetry::{CriticalComponentMetrics, TelemetryWriter};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::{CancelSource, CancellationState};
use base64::Engine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const DETERMINISM_ATTESTATION_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeterminismAttestationPayload {
    schema_version: u8,
    report: adapteros_lora_kernel_api::attestation::DeterminismReport,
}

fn encode_determinism_attestation(
    report: &adapteros_lora_kernel_api::attestation::DeterminismReport,
) -> Result<Vec<u8>> {
    let payload = DeterminismAttestationPayload {
        schema_version: DETERMINISM_ATTESTATION_SCHEMA_VERSION,
        report: report.clone(),
    };
    Ok(serde_json::to_vec(&payload)?)
}

/// Capture equipment profile at worker initialization.
///
/// This captures processor ID, MLX version, and ANE version for binding
/// into cryptographic receipts per Patent 3535886.0002.
fn capture_equipment_profile() -> Option<EquipmentProfile> {
    let processor_id = detect_processor_id().unwrap_or_else(|| "unknown".to_string());
    let mlx_version = detect_mlx_version().unwrap_or_else(|| "unknown".to_string());
    let ane_version = detect_ane_version();

    Some(EquipmentProfile::compute(
        &processor_id,
        &mlx_version,
        ane_version.as_deref(),
    ))
}

/// Detect processor identifier (chip model + stepping).
fn detect_processor_id() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
            .ok()?;

        if output.status.success() {
            let chip = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !chip.is_empty() {
                return Some(chip);
            }
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Detect MLX framework version.
fn detect_mlx_version() -> Option<String> {
    // Check environment variable first (set by boot sequence or config)
    if let Ok(version) = std::env::var("MLX_VERSION") {
        return Some(version);
    }

    // Use compile-time version from adapteros-core
    Some(adapteros_core::version::VERSION.to_string())
}

/// Detect Apple Neural Engine version based on chip generation.
fn detect_ane_version() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
            .ok()?;

        if output.status.success() {
            let soc = String::from_utf8_lossy(&output.stdout);
            let ane_gen = if soc.contains("M4") {
                "ANEv4-38core"
            } else if soc.contains("M3") {
                "ANEv3-16core"
            } else if soc.contains("M2") {
                "ANEv2-16core"
            } else if soc.contains("M1") {
                "ANEv1-16core"
            } else {
                return None;
            };
            return Some(ane_gen.to_string());
        }
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub mod active_learning;
pub mod adapter_hotswap;
pub mod adapter_integrity;
pub mod ane_embedder;
pub mod anomaly_detection;
pub mod backend_coordinator;
pub mod backend_factory;
pub mod backoff;
pub mod backpressure;
pub mod base_model_state;
pub mod cache_prefix_lookup;
pub mod chaos_mode;
pub mod contact_discovery;
pub mod conv_pipeline;
pub mod deadlock;
pub mod deterministic_rng;
pub mod device_placement;
pub mod directory_adapters;
pub mod embeddings;
pub mod ephemeral_adapters;
pub mod evidence;
pub mod execution;
pub mod export;
pub mod filter_engine;
pub mod framework_adapters;
pub mod galaxy_loader;
pub mod generation;
pub mod health;
mod inference_management;
pub mod inference_metrics;
pub mod inference_pause;
pub mod inference_pipeline;
#[cfg(feature = "model-server")]
pub mod kernel_wrapper_model_server;
pub mod kv_quota;
pub mod kvcache;
pub mod launcher;
pub mod lifecycle_state;
pub mod limiter;
pub mod linter_runner;
pub mod llm_backend;
pub mod memory;
pub mod metrics;
#[cfg(feature = "mlx-bridge")]
pub mod mlx_subprocess_bridge;
pub mod model_handle_cache;
pub mod model_key;
pub mod model_loader;
#[cfg(feature = "model-server")]
pub mod model_server_client;
pub mod panic_utils;
pub mod patch_generator;
pub mod patch_telemetry;
pub mod patch_validator;
pub mod prefix_kv_cache;
pub mod reasoning_router;
pub mod request_pinner;
pub mod resource_monitor;
pub mod review_trigger;
pub mod router_bridge;
pub mod services;
pub mod signal;
pub mod stop_controller;
pub mod telemetry_adapter;
pub mod telemetry_lora;
pub mod test_executor;
pub mod timeout;
pub mod tokenizer;
pub mod training;
pub mod uds_server;
pub mod vision_adapter;
pub mod vision_lora;

struct TraceDb {
    db: Arc<Db>,
    sink_unavailable_logged: bool,
}

impl std::fmt::Debug for TraceDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TraceDb")
            .field("sink_unavailable_logged", &self.sink_unavailable_logged)
            .finish()
    }
}

impl TraceDb {
    async fn connect(tenant_id: &str, worker_id: u32) -> Option<Self> {
        if std::env::var("AOS_TRACE_DB_DISABLED").is_ok() {
            info!(
                tenant_id = %tenant_id,
                worker_id = %worker_id,
                "Trace DB disabled via environment; continuing without trace sink"
            );
            return None;
        }

        match Db::from_config().await {
            Ok(db) => {
                if let Err(e) = db.migrate().await {
                    warn!(tenant_id = %tenant_id, worker_id = %worker_id, error = %e, "Trace DB migration failed; disabling trace sink");
                    None
                } else {
                    Some(Self {
                        db: Arc::new(db),
                        sink_unavailable_logged: false,
                    })
                }
            }
            Err(e) => {
                warn!(tenant_id = %tenant_id, worker_id = %worker_id, error = %e, "Trace DB unavailable; disabling trace sink");
                None
            }
        }
    }

    async fn create_sink(
        &mut self,
        start: TraceStart,
        trace_flush_every: usize,
    ) -> Option<SqlTraceSink> {
        match SqlTraceSink::new(self.db.clone(), start, trace_flush_every).await {
            Ok(sink) => Some(sink),
            Err(e) => {
                if !self.sink_unavailable_logged {
                    warn!(
                        error = %e,
                        "Trace sink unavailable; continuing without persistence"
                    );
                    self.sink_unavailable_logged = true;
                }
                None
            }
        }
    }
}

pub const MAX_REASONING_SWAPS: usize = 50;

#[derive(Debug, Clone)]
struct ReasoningSwapGuard {
    max_swaps: usize,
    swaps: usize,
}

impl ReasoningSwapGuard {
    fn new(max_swaps: usize) -> Self {
        Self {
            max_swaps,
            swaps: 0,
        }
    }

    fn record_swap(&mut self) -> Result<()> {
        self.swaps = self.swaps.saturating_add(1);
        if self.swaps >= self.max_swaps {
            return Err(AosError::ReasoningLoop(format!(
                "Reasoning swap limit of {} reached (possible infinite loop)",
                self.max_swaps
            )));
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.swaps
    }
}

// Refactored modules - extracted from lib.rs
pub mod adapter_operations;
pub mod determinism;
pub mod kernel_wrapper;
pub mod patch_generation;
pub mod placement_engine;
pub mod request_types;
pub mod response_types;
pub mod routing_utilities;
pub mod training_management;
pub mod worker_utilities;

pub use adapter_hotswap::{
    AdapterCacheIdentity, AdapterCommand, AdapterCommandResult, AdapterTable, GpuFingerprint,
    HotSwapManager, HotSwapManagerNoKernel, Stack, StackCheckpoint,
};
// Re-export CircuitState for downstream consumers
pub use adapteros_core::CircuitState;
pub use adapteros_lora_router::filter_decision_by_policy;
pub use adapteros_retrieval::rag::DocIndexImpl;
pub use adapteros_retrieval::rag::SymbolIndexImpl;
pub use adapteros_retrieval::rag::TestIndexImpl;
pub use anomaly_detection::{
    AnomalyDetectionConfig, AnomalyDetector, AnomalyScore, DetectionAlgorithm,
};
pub use backend_factory::{
    create_backend, create_backend_cached, create_backend_from_config, create_backend_with_model,
    BackendChoice,
};
pub use backoff::{BackoffConfig, CircuitBreaker as BackoffCircuitBreaker};
pub use backpressure::{
    BackpressureGate, BackpressurePermit, BackpressureStats, DEFAULT_MAX_CONCURRENT,
};
pub use conv_pipeline::{
    ActivationKind, ConvPipeline, ConvPipelineConfig, ImageBatch, PoolingStrategy,
    VisionArchitecture,
};
pub use deadlock::{DeadlockConfig, DeadlockDetector};
pub use deterministic_rng::{DeterministicRng, RngFactory};
pub use directory_adapters::{DirectoryAdapterManager, DirectoryAdapterSpec, PathActivationRule};
pub use ephemeral_adapters::{EphemeralAdapterManager, EphemeralAdapterSpec};
pub use export::{
    run_coreml_export, verify_coreml_export, ComputeUnits, CoreMLExportJob, CoreMLExportRecord,
};
pub use filter_engine::{FilterConfig, FilterEngine, FilterKind};
pub use framework_adapters::{FrameworkAdapterManager, FrameworkAdapterSpec};
pub use generation::Generator;
pub use health::{HealthConfig, HealthMonitor, ProcessHealthStatus as HealthStatus};
pub use inference_metrics::{
    AdapterStats, InferenceMeasurement, InferenceMetrics, InferenceMetricsCollector,
};
pub use kv_quota::{KvQuotaUsage, KvReservation, TenantKvQuotaManager};
pub use kvcache::{KvCache, SequenceGuard};
pub use limiter::{ResourceGuard, ResourceLimiter, ResourceLimits};
pub use linter_runner::{
    LintIssue, LintSeverity, LinterConfig, LinterResult, LinterRunner, LinterType,
};
pub use llm_backend::{create_llm_backend, LlmBackendType, LocalLlmBackend, LocalLlmConfig};
pub use memory::UmaPressureMonitor as MemoryMonitor;
#[cfg(feature = "mlx-bridge")]
pub use mlx_subprocess_bridge::{GenerationResult, MLXSubprocessBridge, MlxBridgeConfig};
// Re-export MLX runtime functions from mlx-ffi for server/cli usage
#[cfg(feature = "multi-backend")]
pub use adapteros_lora_mlx_ffi::{
    mlx_runtime_init, mlx_runtime_shutdown, mlx_selected_implementation, mlx_version,
    MlxImplementation,
};
pub use cache_prefix_lookup::{
    cache_prefix_lookup, cache_prefix_lookup_with_tensors, CacheEntryHandle, CacheLookupConfig,
    CacheLookupResult, CacheLookupWithTensors, CacheMissReason,
};
pub use model_handle_cache::{
    CacheStats, CachedModelEntry, ModelHandle, ModelHandleCache, DEFAULT_MAX_PINNED_ENTRIES,
};
pub use model_key::{FusionMode, ModelCacheIdentityV2, ModelKey, QuantizationMode};
pub use model_loader::{ModelInfo, ModelLoader, QwenModel, QwenModelConfig, TransformerLayer};
pub use prefix_kv_cache::{PrefixKvCache, PrefixKvCacheStats, PrefixKvEntry};
pub use stop_controller::{StopController, StopDecision};
pub use telemetry_adapter::{
    SignalChannel, SignalSample, TelemetryAdapter, TelemetryAdapterConfig, TelemetryAdapterMetrics,
    TelemetryOutput,
};
pub use telemetry_lora::{
    load_telemetry_lora, TelemetryLoraRegistry, TelemetryLoraWeights, TelemetryMergePlan,
    TelemetryTask,
};
pub use test_executor::{TestExecutor, TestFailure, TestFramework, TestResult};
pub use timeout::{TimeoutConfig, TimeoutWrapper};
pub use training::{
    AdapterManifest, AdapterPackager, DatasetGenerator, LoRAQuantizer, LoRAWeights,
    MicroLoRATrainer, PackagedAdapter, QuantizedLoRAWeights, TrainingBackend, TrainingConfig,
    TrainingExample, TrainingResult,
};
pub use vision_adapter::{
    ColorSpace, VisionAdapter, VisionAdapterConfig, VisionAdapterMetrics, VisionBatch,
};
pub use vision_lora::{
    load_vision_lora, VisionLoraRegistry, VisionLoraWeights, VisionMergePlan, VisionTask,
};

// Re-export Model Server client for workers connecting to shared model server
#[cfg(feature = "model-server")]
pub use model_server_client::{ModelServerClient, ModelServerClientConfig};

// Re-export kernel-level determinism policy enforcement (Patent 3535886.0002 Claim 5)
pub use adapteros_policy::packs::determinism::{
    DeterminismConfig as KernelDeterminismConfig, DeterminismPolicy as KernelDeterminismPolicy,
    EnforcementMode as KernelEnforcementMode, OperationValidation as KernelOperationValidation,
};

#[cfg(test)]
pub mod tests;

#[cfg(test)]
mod internal_tests {
    use super::*;

    #[test]
    fn preload_guard_blocks_on_critical_pressure() {
        let err =
            ensure_preload_allowed(MemoryPressureLevel::Critical, MemoryPressureLevel::Critical)
                .unwrap_err();

        match err {
            AosError::MemoryPressure(msg) => {
                assert!(msg.contains("Memory pressure critical, cannot load more adapters"))
            }
            other => panic!("expected memory pressure error, got {:?}", other),
        }
    }

    #[test]
    fn preload_guard_allows_after_recovery() {
        let res = ensure_preload_allowed(MemoryPressureLevel::Critical, MemoryPressureLevel::Low);
        assert!(res.is_ok());
    }

    #[test]
    fn baseline_backend_resolution_prefers_requested() {
        let resolved = resolve_baseline_backend(Some(BackendKind::Metal), BackendKind::Mlx);
        assert_eq!(resolved, BackendKind::Metal);

        let resolved = resolve_baseline_backend(None, BackendKind::Mlx);
        assert_eq!(resolved, BackendKind::Mlx);

        let resolved = resolve_baseline_backend(Some(BackendKind::Auto), BackendKind::CoreML);
        assert_eq!(resolved, BackendKind::CoreML);
    }

    #[test]
    fn determinism_level_mapping_for_prod_backends() {
        assert_eq!(
            declared_determinism_level(BackendKind::Mlx),
            DeterminismLevel::BitExact
        );
        assert_eq!(
            declared_determinism_level(BackendKind::Metal),
            DeterminismLevel::BitExact
        );
        assert_eq!(
            declared_determinism_level(BackendKind::CoreML),
            DeterminismLevel::BoundedTolerance
        );
        assert_eq!(
            declared_determinism_level(BackendKind::MlxBridge),
            DeterminismLevel::None
        );
    }

    #[test]
    fn determinism_downgrade_detection() {
        assert!(is_determinism_downgrade(
            DeterminismLevel::BitExact,
            DeterminismLevel::None
        ));
        assert!(is_determinism_downgrade(
            DeterminismLevel::BoundedTolerance,
            DeterminismLevel::None
        ));
        assert!(!is_determinism_downgrade(
            DeterminismLevel::BoundedTolerance,
            DeterminismLevel::BitExact
        ));
        assert!(!is_determinism_downgrade(
            DeterminismLevel::BitExact,
            DeterminismLevel::BitExact
        ));
    }
}

// Kernel wrapper types moved to kernel_wrapper.rs
pub use kernel_wrapper::{
    BackendLane, CoordinatedKernels, DirectKernels, KernelWrapper, StrictnessControl,
};

/// Enforce guardrails for adapter preload under memory pressure.
fn ensure_preload_allowed(
    pressure_before: MemoryPressureLevel,
    pressure_after: MemoryPressureLevel,
) -> Result<()> {
    if matches!(pressure_after, MemoryPressureLevel::Critical) {
        return Err(AosError::MemoryPressure(
            "Memory pressure critical, cannot load more adapters".to_string(),
        ));
    }

    // If we started at critical but recovered, allow the load to proceed.
    tracing::debug!(
        pressure_before = ?pressure_before,
        pressure_after = ?pressure_after,
        "Preload allowed after memory pressure check"
    );
    Ok(())
}

// DirectKernels, CoordinatedKernels, KernelWrapper and all impls moved to kernel_wrapper.rs

// InferenceRequest, default_determinism_mode, default_utf8_healing, strict_mode_enabled
// moved to request_types.rs - see re-exports below

// Backend resolution helpers (will move to backend_resolution.rs in Phase 2)
fn resolve_baseline_backend(
    requested_backend: Option<BackendKind>,
    resolved_backend: BackendKind,
) -> BackendKind {
    match requested_backend {
        Some(BackendKind::Auto) | None => resolved_backend,
        Some(backend) => backend,
    }
}

fn declared_determinism_level(backend: BackendKind) -> DeterminismLevel {
    match backend {
        BackendKind::Mlx => DeterminismLevel::BitExact,
        BackendKind::CoreML => DeterminismLevel::BoundedTolerance,
        BackendKind::MlxBridge => DeterminismLevel::None,
        BackendKind::Metal => DeterminismLevel::BitExact,
        BackendKind::CPU => DeterminismLevel::None,
        BackendKind::Auto => DeterminismLevel::None,
        // Model Server inherits determinism from the remote server's backend
        BackendKind::ModelServer => DeterminismLevel::BitExact,
    }
}

/// Normalize backend identifiers to canonical categories for telemetry and display.
///
/// Maps implementation-specific backend names to abstract categories:
/// - "native": MLX-based backends (mlx, mlx-bridge) - primary inference path
/// - "accelerated": Hardware-accelerated backends (coreml, metal) - ANE/GPU
/// - "cpu": CPU-only execution
/// - "unknown": Unrecognized backend identifiers
///
/// This normalization enables consistent telemetry aggregation and UI display
/// across different backend implementations.
pub fn normalize_backend_id(backend: &str) -> &'static str {
    let normalized = backend.trim().to_ascii_lowercase();
    let normalized = normalized.replace(['-', '_'], "");

    match normalized.as_str() {
        // MLX variants -> native
        "mlx" | "mlxffi" | "mlxbridge" | "subprocess" => "native",
        // Hardware-accelerated backends
        "coreml" | "ane" | "metal" => "accelerated",
        // CPU-only
        "cpu" | "cpuonly" => "cpu",
        // Auto delegates to runtime selection, categorize as native since MLX is default
        "auto" | "autodev" | "default" => "native",
        // Unknown backend
        _ => "unknown",
    }
}

fn q15_from_unit(value: f32) -> i16 {
    let clamped = value.clamp(0.0, 1.0);
    (clamped * STOP_Q15_DENOM)
        .round()
        .clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn compute_cached_prefix_digest(
    tokens: &[u32],
    tokenizer_hash: &B3Hash,
    cached_len: u32,
) -> Option<B3Hash> {
    if cached_len == 0 {
        return None;
    }
    let len = cached_len as usize;
    let mut buf = Vec::with_capacity(32 + 4 + len * 4);
    buf.extend_from_slice(tokenizer_hash.as_bytes());
    buf.extend_from_slice(&cached_len.to_le_bytes());
    for t in tokens.iter().take(len) {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    Some(B3Hash::hash(&buf))
}

fn compute_retrieval_digests(
    evidence: &[EvidenceRef],
    tenant_id: &str,
) -> (Option<B3Hash>, Option<B3Hash>) {
    if evidence.is_empty() {
        return (None, None);
    }

    let mut entry_hashes = Vec::with_capacity(evidence.len());
    let mut order_buf = Vec::with_capacity(evidence.len() * 64);

    for (idx, e) in evidence.iter().enumerate() {
        let mut entry_buf = Vec::with_capacity(e.doc_id.len() + e.rev.len() + 32 + 8);
        entry_buf.extend_from_slice(e.doc_id.as_bytes());
        entry_buf.extend_from_slice(e.rev.as_bytes());
        entry_buf.extend_from_slice(e.span_hash.as_bytes());
        entry_buf.extend_from_slice(&e.score.to_le_bytes());
        let entry_hash = B3Hash::hash(&entry_buf);
        entry_hashes.push(entry_hash);

        order_buf.extend_from_slice(&(idx as u32).to_le_bytes());
        order_buf.extend_from_slice(entry_hash.as_bytes());
    }

    let merkle_root = adapteros_core::receipt_merkle::batch_receipts(tenant_id, &entry_hashes)
        .map(|b| b.merkle_root)
        .ok();
    let order_digest = Some(B3Hash::hash(&order_buf));

    (merkle_root, order_digest)
}

fn is_determinism_downgrade(baseline: DeterminismLevel, candidate: DeterminismLevel) -> bool {
    candidate < baseline
}

struct ValidatedPrompt {
    #[allow(dead_code)]
    formatted_prompt: String,
    tokens: Vec<u32>,
}

struct RequestValidator<'a> {
    tokenizer: &'a QwenTokenizer,
    max_seq_len: usize,
}

impl<'a> RequestValidator<'a> {
    fn new(tokenizer: &'a QwenTokenizer, max_seq_len: usize) -> Self {
        Self {
            tokenizer,
            max_seq_len,
        }
    }

    fn validate(&self, prompt: &str) -> Result<ValidatedPrompt> {
        const MAX_PROMPT_BYTES: usize = 1_000_000;

        if prompt.trim().is_empty() {
            return Err(AosError::Validation("Prompt cannot be empty".to_string()));
        }

        if prompt.len() > MAX_PROMPT_BYTES {
            return Err(AosError::Validation("Prompt exceeds 1MB limit".to_string()));
        }

        let formatted_prompt = self.tokenizer.apply_chat_template(prompt);
        let token_ids = self
            .tokenizer
            .encode(&formatted_prompt)
            .map_err(|e| AosError::Validation(format!("Prompt tokenization failed: {}", e)))?;

        if token_ids.is_empty() {
            return Err(AosError::Validation(
                "Prompt produced no tokens".to_string(),
            ));
        }

        if token_ids.len() > self.max_seq_len {
            return Err(AosError::Validation(format!(
                "Prompt too long: {} tokens exceeds context window of {}",
                token_ids.len(),
                self.max_seq_len
            )));
        }

        Ok(ValidatedPrompt {
            formatted_prompt,
            tokens: token_ids,
        })
    }
}

/// In strict mode, ensure router decision chain has matching gates and adapters.
fn enforce_strict_router_chain(
    strict_mode: bool,
    base_only_request: bool,
    chain: &[RouterDecisionChainEntry],
) -> Result<()> {
    if !strict_mode || base_only_request {
        return Ok(());
    }

    for entry in chain {
        if entry.gates_q15.is_empty() {
            return Err(AosError::DeterminismViolation(
                "strict mode requires gates_q15 for every routed token".to_string(),
            ));
        }
        if entry.gates_q15.len() != entry.adapter_ids.len() {
            return Err(AosError::DeterminismViolation(
                "strict mode requires gates_q15 length to match adapter_ids".to_string(),
            ));
        }
    }

    Ok(())
}

// RequestType and PatchProposalRequest moved to request_types.rs
// Re-exported via pub use request_types::{...} below

/// Placement decision trace entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementTraceEntry {
    pub step: usize,
    pub lane: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f32>,
    pub utilization: f32,
}

/// Cached CoreML verification snapshot for observability.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoremlVerificationSnapshot {
    /// Verification mode (off/warn/strict) in effect when the check ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Expected fused CoreML package hash (hex) if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// Actual fused CoreML package hash (hex) if computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// Source of the expected hash (db/manifest/env/metadata/none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Terminal verification status label (match/mismatch/missing_expected/missing_actual/skipped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Convenience flag for mismatch detection.
    #[serde(default)]
    pub mismatch: bool,
}

/// Structured error details for inference failures
///
/// This enum provides typed error information for specific failure modes,
/// allowing clients to programmatically handle different error cases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum InferenceErrorDetails {
    /// Model cache budget exceeded during eviction
    ///
    /// This error occurs when the model cache cannot free enough memory
    /// to accommodate a new model load, typically because entries are
    /// pinned (base models) or active (in-flight inference).
    #[serde(rename = "cache_budget_exceeded")]
    CacheBudgetExceeded {
        /// Memory needed in megabytes
        needed_mb: u64,
        /// Memory freed during eviction attempt in megabytes
        freed_mb: u64,
        /// Number of pinned entries that blocked eviction
        pinned_count: usize,
        /// Number of active entries that blocked eviction
        active_count: usize,
        /// Maximum cache budget in megabytes
        max_mb: u64,
        /// Optional model key identifier (for diagnostics)
        model_key: Option<String>,
    },
    /// Generic worker error (fallback for unstructured errors)
    #[serde(rename = "worker_error")]
    WorkerError {
        /// Error message
        message: String,
    },
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: ResponseTrace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_receipt: Option<RunReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<RefusalResponse>,
    /// Patch proposal if requested
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_proposal: Option<PatchProposalResponse>,
    /// Stack ID for telemetry correlation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_version: Option<i64>,
    /// Normalized backend identifier for deterministic responses.
    ///
    /// This field contains a canonical identifier that is consistent across
    /// hardware configurations to enable deterministic replay/comparison:
    /// - `"native"` for MLX variants (mlx, mlxbridge)
    /// - `"accelerated"` for Apple hardware acceleration (coreml, metal)
    /// - `"cpu"` for CPU-only execution
    ///
    /// Use `backend_raw` for observability/telemetry when the actual backend matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
    /// Raw backend identifier for telemetry/observability.
    ///
    /// Contains the device-specific backend name (e.g., "mlx", "coreml", "metal")
    /// for debugging and telemetry purposes. For deterministic comparison, use
    /// the normalized `backend_used` field instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_raw: Option<String>,
    /// Backend version/build identifier (e.g., crate/FFI version)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_version: Option<String>,
    /// Whether backend fallback occurred during execution
    #[serde(default)]
    pub fallback_triggered: bool,
    /// Requested CoreML compute preference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_preference: Option<String>,
    /// CoreML compute units actually used (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_compute_units: Option<String>,
    /// Whether CoreML leveraged GPU for this inference (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_gpu_used: Option<bool>,
    /// Hash of the fused CoreML package manifest used (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_package_hash: Option<String>,
    /// Expected hash used for verification, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_expected_package_hash: Option<String>,
    /// Whether the computed hash mismatched the expected value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_hash_mismatch: Option<bool>,
    /// Backend selected after fallback (if different from requested)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_backend: Option<String>,
    /// Determinism mode applied after resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub determinism_mode_applied: Option<String>,
    /// Pinned adapters that were unavailable (CHAT-PIN-02)
    ///
    /// These are pinned adapter IDs that were not present in the worker's
    /// loaded adapter set (manifest.adapters). Returned for UI warning display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_pinned_adapters: Option<Vec<String>>,
    /// Routing fallback mode when pinned adapters are unavailable (PRD-6A)
    ///
    /// - `None`: All pinned adapters were available (or no pins configured)
    /// - `Some("partial")`: Some pinned adapters unavailable, using available pins + stack
    /// - `Some("stack_only")`: All pinned adapters unavailable, routing uses stack only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_routing_fallback: Option<String>,
    /// Placement decisions per token (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_trace: Option<Vec<PlacementTraceEntry>>,

    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    /// Stop reason code explaining why generation terminated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_code: Option<adapteros_api_types::inference::StopReasonCode>,
    /// Token index at which the stop decision was made
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason_token_index: Option<u32>,
    /// BLAKE3 digest of the StopPolicySpec used (hex encoded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_policy_digest_b3: Option<String>,

    /// Structured error details when status indicates failure
    ///
    /// Contains typed error information for specific failure modes.
    /// This field is `None` for successful responses or unstructured errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_details: Option<InferenceErrorDetails>,
}

/// Patch proposal response with patches and citations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposalResponse {
    pub proposal_id: String,
    pub rationale: String,
    pub patches: Vec<FilePatchResponse>,
    pub citations: Vec<CitationResponse>,
    pub confidence: f32,
}

/// File patch in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatchResponse {
    pub file_path: String,
    pub hunks: Vec<PatchHunkResponse>,
}

/// Patch hunk in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunkResponse {
    pub start_line: usize,
    pub end_line: usize,
    pub old_content: String,
    pub new_content: String,
}

/// Citation in response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationResponse {
    pub source_type: String,
    pub reference: String,
    pub relevance: f32,
}

/// Response trace with evidence and router decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTrace {
    pub cpid: String,
    pub plan_id: String,
    pub evidence: Vec<EvidenceRef>,
    pub router_summary: RouterSummary,
    pub token_count: usize,
    /// Detailed router decisions per step (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decisions: Option<Vec<adapteros_api_types::inference::RouterDecision>>,
    /// Cryptographically chained router decisions (per-token)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_decision_chain:
        Option<Vec<adapteros_api_types::inference::RouterDecisionChainEntry>>,
    /// Fusion interval boundaries and fused tensor hashes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fusion_intervals: Option<Vec<FusionIntervalTrace>>,
    /// Model type for this trace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_type: Option<adapteros_api_types::inference::RouterModelType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub doc_id: String,
    pub rev: String,
    pub span_hash: B3Hash,
    pub score: f32,
}

// Re-export RouterSummary and summarize_router_usage from refactored modules
pub use response_types::{RouterSummary, StreamToken, WorkerStreamEvent};
pub use routing_utilities::summarize_router_usage;
// Import internal fusion helpers for use in lib.rs
use routing_utilities::fusion_intervals_for_mode;

// Re-export request/response types from refactored modules
pub use request_types::{
    CancelTrainingRequest, CancelTrainingResponse, InferenceRequest, PatchProposalRequest,
    RequestType,
};
// Import internal helpers for use within lib.rs
use request_types::strict_mode_enabled;

struct PlacementState {
    engine: PlacementEngine,
    telemetry: TelemetryCollector,
    lanes: Vec<LaneDescriptor>,
}

impl PlacementState {
    fn new(
        engine: PlacementEngine,
        telemetry: TelemetryCollector,
        lanes: Vec<LaneDescriptor>,
    ) -> Self {
        Self {
            engine,
            telemetry,
            lanes,
        }
    }

    fn decide(&mut self) -> Option<PlacementDecision> {
        let snapshot = self.telemetry.snapshot();
        self.engine.choose_lane(&self.lanes, &snapshot)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementReplay {
    pub mode: String,
    pub weights: PlacementWeights,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<PlacementTraceEntry>,
}

fn device_kind_from_name(name: &str) -> DeviceKind {
    let lower = name.to_ascii_lowercase();
    if lower.contains("coreml") || lower.contains("ane") || lower.contains("neural") {
        DeviceKind::Ane
    } else if lower.contains("metal") || lower.contains("gpu") {
        DeviceKind::Gpu
    } else {
        DeviceKind::Cpu
    }
}

fn resolve_worker_adapter_paths() -> RepoAdapterPaths {
    RepoAdapterPaths::from_env_and_config(None)
}

/// CoreML runtime telemetry captured for replay/logging
#[derive(Debug, Clone, Default)]
pub struct CoremlRuntimeTelemetry {
    pub compute_preference: Option<String>,
    pub compute_units: Option<String>,
    pub gpu_available: Option<bool>,
    pub ane_available: Option<bool>,
    pub gpu_used: Option<bool>,
    pub ane_used: Option<bool>,
    pub production_mode: Option<bool>,
}

/// Backends available in this worker (primary + optional fallback).
#[derive(Debug, Clone)]
pub struct AvailableBackends {
    pub primary: BackendKind,
    pub fallback: Option<BackendKind>,
    pub coreml_primary: Option<CoremlRuntimeTelemetry>,
    pub coreml_fallback: Option<CoremlRuntimeTelemetry>,
}

impl AvailableBackends {
    fn contains(&self, backend: BackendKind) -> bool {
        self.primary == backend || self.fallback == Some(backend)
    }

    fn lane_for(&self, backend: BackendKind) -> BackendLane {
        if self.fallback == Some(backend) {
            BackendLane::Fallback
        } else {
            BackendLane::Primary
        }
    }

    /// Check if CoreML with ANE is available in this worker.
    ///
    /// Returns true if either primary or fallback is CoreML and has ANE telemetry.
    fn has_coreml_with_ane(&self) -> bool {
        let coreml_available =
            self.primary == BackendKind::CoreML || self.fallback == Some(BackendKind::CoreML);
        let ane_available = self.coreml_primary.is_some() || self.coreml_fallback.is_some();
        coreml_available && ane_available
    }

    /// Check if MLX is available in this worker.
    fn has_mlx(&self) -> bool {
        self.primary == BackendKind::Mlx
            || self.primary == BackendKind::MlxBridge
            || self.fallback == Some(BackendKind::Mlx)
            || self.fallback == Some(BackendKind::MlxBridge)
    }

    /// Check if Metal is available in this worker.
    fn has_metal(&self) -> bool {
        self.primary == BackendKind::Metal || self.fallback == Some(BackendKind::Metal)
    }

    /// Resolve backend based on reasoning mode.
    ///
    /// When reasoning_mode is enabled, prefers CoreML for ANE-accelerated determinism.
    /// Otherwise, prefers MLX for streaming flexibility.
    /// Falls through to the provided default if preferred backend unavailable.
    ///
    /// Returns (resolved_backend, reasoning_triggered, reason)
    fn resolve_for_reasoning(
        &self,
        requested: Option<BackendKind>,
        reasoning_mode: bool,
    ) -> (BackendKind, bool, &'static str) {
        // If explicit backend requested, honor it
        if let Some(explicit) = requested {
            return (explicit, false, "explicit_backend_requested");
        }

        // Reasoning mode: prefer CoreML for ANE determinism
        if reasoning_mode && self.has_coreml_with_ane() {
            return (BackendKind::CoreML, true, "reasoning_mode_coreml_preferred");
        }

        // Default: prefer MLX for streaming
        if self.has_mlx() {
            if self.primary == BackendKind::Mlx || self.primary == BackendKind::MlxBridge {
                return (self.primary, false, "streaming_mode_mlx_default");
            }
            if let Some(fallback) = self.fallback {
                if fallback == BackendKind::Mlx || fallback == BackendKind::MlxBridge {
                    return (fallback, false, "streaming_mode_mlx_fallback");
                }
            }
        }

        // Fallback to primary
        (self.primary, false, "primary_backend_default")
    }
}

#[cfg(test)]
mod adapter_path_tests {
    use super::resolve_worker_adapter_paths;
    use std::path::PathBuf;

    #[test]
    fn worker_paths_respect_env_override() {
        std::env::set_var("AOS_ADAPTERS_ROOT", "var/test-adapters-repo");
        let paths = resolve_worker_adapter_paths();
        // Paths are absolutized by resolve_adapter_roots_from_strings, so check suffix
        assert!(
            paths.repo_root.ends_with("var/test-adapters-repo"),
            "Expected path to end with 'var/test-adapters-repo', got {:?}",
            paths.repo_root
        );
        std::env::remove_var("AOS_ADAPTERS_ROOT");
    }
}

#[cfg(test)]
mod available_backends_tests {
    use super::{AvailableBackends, BackendKind, CoremlRuntimeTelemetry};

    fn make_mlx_primary_with_coreml_fallback() -> AvailableBackends {
        AvailableBackends {
            primary: BackendKind::Mlx,
            fallback: Some(BackendKind::CoreML),
            coreml_primary: None,
            coreml_fallback: Some(CoremlRuntimeTelemetry::default()),
        }
    }

    fn make_coreml_primary() -> AvailableBackends {
        AvailableBackends {
            primary: BackendKind::CoreML,
            fallback: Some(BackendKind::Metal),
            coreml_primary: Some(CoremlRuntimeTelemetry::default()),
            coreml_fallback: None,
        }
    }

    fn make_mlx_only() -> AvailableBackends {
        AvailableBackends {
            primary: BackendKind::Mlx,
            fallback: Some(BackendKind::Metal),
            coreml_primary: None,
            coreml_fallback: None,
        }
    }

    #[test]
    fn reasoning_mode_selects_coreml_when_available() {
        let backends = make_mlx_primary_with_coreml_fallback();
        let (resolved, triggered, reason) = backends.resolve_for_reasoning(None, true);

        assert_eq!(resolved, BackendKind::CoreML);
        assert!(triggered);
        assert_eq!(reason, "reasoning_mode_coreml_preferred");
    }

    #[test]
    fn non_reasoning_mode_prefers_mlx() {
        let backends = make_mlx_primary_with_coreml_fallback();
        let (resolved, triggered, reason) = backends.resolve_for_reasoning(None, false);

        assert_eq!(resolved, BackendKind::Mlx);
        assert!(!triggered);
        assert_eq!(reason, "streaming_mode_mlx_default");
    }

    #[test]
    fn explicit_backend_overrides_reasoning() {
        let backends = make_mlx_primary_with_coreml_fallback();
        let (resolved, triggered, reason) =
            backends.resolve_for_reasoning(Some(BackendKind::Metal), true);

        assert_eq!(resolved, BackendKind::Metal);
        assert!(!triggered);
        assert_eq!(reason, "explicit_backend_requested");
    }

    #[test]
    fn reasoning_mode_falls_back_when_coreml_unavailable() {
        let backends = make_mlx_only();
        let (resolved, triggered, reason) = backends.resolve_for_reasoning(None, true);

        // Should fall back to MLX since CoreML/ANE not available
        assert_eq!(resolved, BackendKind::Mlx);
        assert!(!triggered);
        assert_eq!(reason, "streaming_mode_mlx_default");
    }

    #[test]
    fn coreml_primary_honors_reasoning_mode() {
        let backends = make_coreml_primary();
        let (resolved, triggered, reason) = backends.resolve_for_reasoning(None, true);

        assert_eq!(resolved, BackendKind::CoreML);
        assert!(triggered);
        assert_eq!(reason, "reasoning_mode_coreml_preferred");
    }

    #[test]
    fn has_coreml_with_ane_checks_telemetry() {
        let with_ane = make_mlx_primary_with_coreml_fallback();
        let without_ane = make_mlx_only();

        assert!(with_ane.has_coreml_with_ane());
        assert!(!without_ane.has_coreml_with_ane());
    }

    #[test]
    fn has_mlx_checks_variants() {
        let mlx_primary = make_mlx_only();
        let coreml_primary = make_coreml_primary();

        assert!(mlx_primary.has_mlx());
        assert!(!coreml_primary.has_mlx());
    }
}

use crate::adapter_integrity::{AdapterIntegrityVerifier, ExpectedAdapterMetadata};
use crate::embeddings::EmbeddingModel;
use crate::evidence::EvidenceRetriever;
use crate::tokenizer::QwenTokenizer;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;

/// Worker for running inference with comprehensive safety mechanisms
pub struct Worker<K: FusedKernels + StrictnessControl + Send + Sync + 'static> {
    manifest: ManifestV3,
    policy: PolicyEngine,
    router: Router,
    rag: Option<RagSystem>,
    tenant_namespace: adapteros_lora_rag::IndexNamespaceId,
    /// Kernels wrapped in Arc<Mutex<>> for shared access with workflows
    kernels: Arc<tokio::sync::Mutex<K>>,
    memory_monitor: Arc<MemoryMonitor>,
    tokenizer: Arc<QwenTokenizer>,
    generator: Generator,
    max_seq_len: usize,
    embedding_model: Arc<EmbeddingModel>,
    evidence_retriever: Option<EvidenceRetriever>,
    /// KV cache for transformer attention with generation tracking
    kv_cache: Arc<StdMutex<KvCache>>,
    /// KV quota manager for receipt telemetry (optional)
    kv_quota_manager: Option<Arc<TenantKvQuotaManager>>,
    /// Prefix KV cache for reusing prefill computation on repeated prefixes
    prefix_kv_cache: Arc<prefix_kv_cache::PrefixKvCache>,
    /// MoE-specific prefix cache for expert pre-warming and free tokens
    #[cfg(feature = "mlx-bridge")]
    moe_prefix_cache: Arc<crate::moe_prefix_cache::MoEPrefixCache>,
    /// Last stack hash for change detection (reserved for stack caching)
    _last_stack_hash: RwLock<Option<B3Hash>>,
    /// Backends available to this worker (primary + optional fallback)
    available_backends: AvailableBackends,
    /// Hash of the fused CoreML package manifest (if applicable)
    coreml_package_hash: Option<String>,
    /// Cached CoreML verification snapshot for observability/debug.
    coreml_verification: Option<CoremlVerificationSnapshot>,
    // Safety mechanisms
    _timeout_config: TimeoutConfig,
    _timeout_wrapper: TimeoutWrapper,
    circuit_breaker: StandardCircuitBreaker,
    pub(crate) resource_limiter: Arc<ResourceLimiter>,
    _deadlock_detector: DeadlockDetector,
    health_monitor: Arc<HealthMonitor>,
    telemetry: Option<TelemetryWriter>,
    trace_db: Option<TraceDb>,
    trace_sink_missing_warned: bool,
    trace_flush_every: usize,
    placement_template: Option<(PlacementConfig, Vec<LaneDescriptor>)>,
    // Lifecycle management
    profiler: adapteros_telemetry::profiler::AdapterProfiler,
    lifecycle: Arc<Mutex<adapteros_lora_lifecycle::LifecycleManager>>,
    // Hot-swap management
    hotswap: Arc<HotSwapManager<K>>,
    // Retirement task management
    retirement_handle: Option<tokio::task::JoinHandle<()>>,
    /// Background persistence task handle
    persistence_handle: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: watch::Sender<()>,
    /// Active training jobs with their cancellation tokens
    pub active_training_jobs: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    /// Active inference requests with cancellation tracking
    inference_cancellations: Arc<InferenceCancelRegistry>,
    /// Worker ID for identity tracking (PRD-06)
    worker_id: u32,
    /// KV residency policy identifier (receipt telemetry)
    kv_residency_policy_id: Option<String>,
    /// Critical component metrics for Prometheus export
    critical_metrics: Option<Arc<CriticalComponentMetrics>>,
    /// Inference pause registry for human-in-the-loop review protocol
    pause_registry: Option<Arc<inference_pause::InferencePauseRegistry>>,
    /// Equipment profile for cryptographic receipt binding (Patent 3535886.0002)
    equipment_profile: Option<EquipmentProfile>,
}

impl<K: FusedKernels + StrictnessControl + Send + Sync + 'static> Worker<K> {
    /// Create a new worker with comprehensive safety mechanisms
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        manifest: ManifestV3,
        tenant_id: &str,
        kernels: K,
        available_backends: AvailableBackends,
        rag: Option<RagSystem>,
        tokenizer_path: &str,
        model_path: &str,
        telemetry: TelemetryWriter,
        coreml_package_hash: Option<String>,
        coreml_verification: Option<CoremlVerificationSnapshot>,
        quota_manager: Option<Arc<TenantKvQuotaManager>>,
        kv_residency_policy_id: Option<String>,
        adapter_cache_bytes: Option<u64>,
        worker_id: u32,
    ) -> Result<Self> {
        // Initialize determinism guards first
        init_determinism_guards()?;

        let policy = PolicyEngine::new(manifest.policies.clone());

        // Create router from manifest
        let router_seed = adapteros_core::derive_seed(&manifest.seeds.global, "router");
        let mut router = Router::new(
            vec![1.0; manifest.adapters.len()],
            manifest.router.k_sparse,
            manifest.router.tau,
            manifest.router.entropy_floor,
            router_seed,
        )?;
        router.set_routing_bias(manifest.base.routing_bias);
        router.set_model_is_moe(manifest.base.arch.to_ascii_lowercase().contains("moe"));

        let memory_monitor = Arc::new(MemoryMonitor::new(
            manifest.policies.memory.min_headroom_pct,
            Some(telemetry.clone()),
        ));

        // Initialize safety mechanisms from config (falls back to defaults if config not loaded)
        let timeout_config = if let Some(cfg) = try_effective_config() {
            tracing::debug!(
                inference_timeout_secs = cfg.worker_safety.inference_timeout_secs,
                evidence_timeout_secs = cfg.worker_safety.evidence_timeout_secs,
                router_timeout_ms = cfg.worker_safety.router_timeout_ms,
                policy_timeout_ms = cfg.worker_safety.policy_timeout_ms,
                "Loaded timeout config from cp.toml [worker.safety]"
            );
            TimeoutConfig::from_effective_section(&cfg.worker_safety)
        } else {
            tracing::debug!("Effective config not initialized, using default timeout config");
            TimeoutConfig::default()
        };
        let timeout_wrapper = TimeoutWrapper::new(timeout_config.clone());

        // Initialize circuit breaker from config (falls back to defaults if config not loaded)
        let (cb_failure_threshold, cb_timeout_ms) = if let Some(cfg) = try_effective_config() {
            (
                cfg.worker_safety.circuit_breaker_threshold,
                cfg.worker_safety.circuit_breaker_timeout_secs * 1000,
            )
        } else {
            (5, 60000)
        };
        let circuit_breaker = StandardCircuitBreaker::new(
            "worker".to_string(),
            CircuitBreakerConfig {
                failure_threshold: cb_failure_threshold as usize,
                timeout_ms: cb_timeout_ms,
                ..Default::default()
            },
        );
        let resource_limiter = Arc::new(ResourceLimiter::new(ResourceLimits::from_env()));
        let deadlock_detector = DeadlockDetector::new(DeadlockConfig::default());
        let health_monitor = Arc::new(HealthMonitor::new(HealthConfig::default())?.with_telemetry(
            telemetry.clone(),
            tenant_id.to_string(),
            "worker".to_string(),
        ));

        // Load tokenizer
        let tokenizer = Arc::new(QwenTokenizer::from_file(tokenizer_path)?);

        // Create generator with deterministic seed and step-level reproducibility
        let gen_seed = adapteros_core::derive_seed(&manifest.seeds.global, "generation");
        let generator = Generator::new(gen_seed)?
            .with_temperature(0.7)
            .with_top_p(0.9)
            .with_deterministic();

        // Resolve context window from model config (fall back to env/dev fixture)
        let max_seq_len = match ModelConfig::from_config_json(Path::new(model_path)) {
            Ok(cfg) => cfg.max_seq_len,
            Err(err) => {
                warn!(
                    error = %err,
                    "Failed to parse model config.json for max_seq_len, falling back to env"
                );
                match ModelConfig::from_env() {
                    Ok(cfg) => cfg.max_seq_len,
                    Err(env_err) => {
                        warn!(
                            error = %env_err,
                            "Failed to load model config from env, using dev fixture max_seq_len"
                        );
                        ModelConfig::dev_fixture().max_seq_len
                    }
                }
            }
        };

        // Load embedding model with tokenizer for proper text encoding
        let embedding_model = Arc::new(EmbeddingModel::from_model_path_with_tokenizer(
            model_path,
            manifest.base.vocab_size as usize,
            manifest.base.hidden_dim as usize,
            tokenizer.clone(),
        )?);

        // Initialize evidence retriever with real implementation if RAG is available
        let evidence_retriever = if let Some(ref _rag_system) = rag {
            use crate::evidence::*;
            use adapteros_retrieval::rag::EvidenceIndexManager;

            // Create evidence index manager for the tenant
            let index_root = resolve_index_root()?;
            let indices_root = index_root.path.join(tenant_id);
            if let Some(parent) = indices_root.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            tracing::info!(
                path = %indices_root.display(),
                source = %index_root.source,
                "Initializing evidence index manager"
            );
            let evidence_manager = Arc::new(Mutex::new(
                EvidenceIndexManager::new(
                    indices_root,
                    tenant_id.to_string(),
                    Some(embedding_model.clone()),
                )
                .await?,
            ));

            Some(EvidenceRetriever::new(evidence_manager))
        } else {
            None
        };

        let kv_quota_manager = quota_manager.clone();
        // Initialize kv_cache with Arc<StdMutex<>> for interior mutability
        let kv_cache = Arc::new(StdMutex::new(KvCache::new_with_quota(
            adapteros_core::constants::BYTES_PER_GB,
            quota_manager,
        ))); // 1GB default

        // Initialize prefix KV cache for reusing prefill computation on repeated prefixes
        // Budget: 2GB for prefix KV tensors (can be tuned via env var)
        let prefix_kv_cache_bytes = std::env::var("AOS_PREFIX_KV_CACHE_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2 * adapteros_core::constants::BYTES_PER_GB);
        let prefix_kv_cache = Arc::new(prefix_kv_cache::PrefixKvCache::new(prefix_kv_cache_bytes));

        // Initialize MoE prefix cache (512MB default budget for metadata)
        #[cfg(feature = "mlx-bridge")]
        let moe_prefix_cache = Arc::new(crate::moe_prefix_cache::MoEPrefixCache::new(
            512 * 1024 * 1024,
        ));

        // Load MoE cache snapshot if exists
        let adapter_paths = resolve_worker_adapter_paths();
        #[allow(unused_variables)]
        let adapters_path = adapter_paths.repo_root.join(tenant_id);
        #[cfg(feature = "mlx-bridge")]
        let moe_cache_path = adapters_path.join("moe_cache.json");
        #[cfg(feature = "mlx-bridge")]
        if let Err(e) = moe_prefix_cache.load_snapshot(&moe_cache_path) {
            warn!(path = %moe_cache_path.display(), error = %e, "Failed to load MoE cache snapshot");
        } else {
            info!(path = %moe_cache_path.display(), "Loaded MoE cache snapshot");
        }

        let (shutdown_tx, _shutdown_rx) = watch::channel(());

        // Spawn persistence task (every 5 minutes)
        #[cfg(feature = "mlx-bridge")]
        let persistence_handle = {
            let persistence_cache = moe_prefix_cache.clone();
            let persistence_path = moe_cache_path.clone();
            let mut shutdown_rx = shutdown_tx.subscribe();
            Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            info!("Persistence task received shutdown signal");
                            break;
                        }
                        _ = interval.tick() => {
                            if let Err(e) = persistence_cache.save_snapshot(&persistence_path) {
                                warn!(path = %persistence_path.display(), error = %e, "Failed to background save MoE cache");
                            }
                        }
                    }
                }
            }))
        };
        #[cfg(not(feature = "mlx-bridge"))]
        let persistence_handle = None;

        let last_stack_hash = RwLock::new(None);

        // Initialize profiler
        let adapter_names: Vec<String> = manifest.adapters.iter().map(|a| a.id.clone()).collect();
        let profiler = adapteros_telemetry::profiler::AdapterProfiler::new(
            adapter_names.clone(),
            Some(telemetry.clone()),
        );

        let expected_metadata: HashMap<String, ExpectedAdapterMetadata> = manifest
            .adapters
            .iter()
            .map(|adapter| {
                (
                    adapter.id.clone(),
                    ExpectedAdapterMetadata {
                        tier: Some(adapter.tier),
                        scope: Some(adapter.scope.clone()),
                    },
                )
            })
            .collect();

        // Initialize lifecycle manager
        // Use centralized adapter path resolution (ENV > Default)
        // Note: Worker doesn't have access to server config, so ENV variables are the standard way to configure paths
        let adapter_paths = resolve_worker_adapter_paths();
        let adapters_path = adapter_paths.repo_root.join(tenant_id);
        let _ = std::fs::create_dir_all(&adapters_path);

        // Build adapter hashes map from manifest
        let adapter_hashes: std::collections::HashMap<String, adapteros_core::B3Hash> = manifest
            .adapters
            .iter()
            .map(|a| (a.id.clone(), a.hash))
            .collect();

        let lifecycle = Arc::new(Mutex::new(adapteros_lora_lifecycle::LifecycleManager::new(
            adapter_names,
            adapter_hashes,
            &manifest.policies,
            adapters_path.clone(),
            Some(telemetry.clone()),
            manifest.router.k_sparse,
        )));

        {
            let lifecycle_guard = lifecycle.lock().await;
            lifecycle_guard
                .set_expected_base_model(&manifest.base.model_id, manifest.base.model_hash);
        }

        // Placement configuration (CPU/GPU/ANE lane steering)
        let placement_template = if cfg!(feature = "telemetry-sysinfo") {
            let placement_cfg = PlacementConfig::from_env();
            let lane_names = kernels.lane_names();
            let mut lanes = vec![LaneDescriptor {
                lane: BackendLane::Primary,
                kind: device_kind_from_name(&lane_names.0),
                name: lane_names.0.clone(),
            }];
            if let Some(fallback_name) = lane_names.1 {
                lanes.push(LaneDescriptor {
                    lane: BackendLane::Fallback,
                    kind: device_kind_from_name(&fallback_name),
                    name: fallback_name,
                });
            }
            if placement_cfg.mode != PlacementMode::Off && lanes.len() > 1 {
                Some((placement_cfg.clone(), lanes))
            } else {
                None
            }
        } else {
            None
        };

        // Create shared kernels Arc for both Worker and HotSwapManager
        let kernels_arc = Arc::new(tokio::sync::Mutex::new(kernels));

        let integrity = Arc::new(AdapterIntegrityVerifier::new(
            tenant_id.to_string(),
            manifest.base.model_id.clone(),
            expected_metadata,
        ));

        let hotswap = Arc::new(HotSwapManager::new_with_kernels(
            kernels_arc.clone(),
            adapter_paths.repo_root.clone(),
            tenant_id.to_string(),
            integrity,
            Some(Arc::new(telemetry.clone())),
            Some(memory_monitor.clone()),
        ));

        let adapter_cache_budget_bytes = adapter_cache_bytes.unwrap_or(DEFAULT_ADAPTER_CACHE_SIZE);
        if adapter_cache_budget_bytes == 0 {
            return Err(AosError::Validation(
                "Adapter cache budget bytes must be greater than zero".to_string(),
            ));
        }
        hotswap.set_adapter_cache_budget_bytes(Some(adapter_cache_budget_bytes));

        // Initialize critical component metrics (shared between hotswap and worker)
        let critical_metrics = match CriticalComponentMetrics::new() {
            Ok(metrics) => {
                let metrics_arc = Arc::new(metrics);
                hotswap.set_adapter_cache_metrics(metrics_arc.clone());
                Some(metrics_arc)
            }
            Err(e) => {
                warn!(error = %e, "Failed to initialize critical component metrics");
                None
            }
        };

        hotswap.set_cache_identity(AdapterCacheIdentity {
            base_manifest_hash: Some(manifest.seeds.manifest_hash),
            backend_type: available_backends.primary.as_str().to_string(),
            kernel_version_id: adapteros_core::version::VERSION.to_string(),
            tenant_id: Some(tenant_id.to_string()),
            adapter_dir_hash: None,
        });

        // Retirement task management
        let retirement_handle = Some(
            hotswap
                .clone()
                .start_retirement_task(shutdown_tx.subscribe()),
        );

        // Initialize active training jobs tracking
        let active_training_jobs = Arc::new(RwLock::new(HashMap::new()));
        let inference_cancellations = Arc::new(InferenceCancelRegistry::new());

        let trace_flush_every = std::env::var("AOS_TRACE_FLUSH_EVERY")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(|v: usize| v.max(1))
            .unwrap_or(1);

        let trace_db = TraceDb::connect(tenant_id, worker_id).await;

        Ok(Self {
            manifest,
            policy,
            router,
            rag,
            tenant_namespace: tenant_id.to_string(),
            kernels: kernels_arc.clone(),
            memory_monitor,
            tokenizer,
            generator,
            max_seq_len,
            embedding_model,
            evidence_retriever,
            kv_cache,
            kv_quota_manager,
            prefix_kv_cache,
            #[cfg(feature = "mlx-bridge")]
            moe_prefix_cache,
            _last_stack_hash: last_stack_hash,
            available_backends,
            coreml_package_hash,
            coreml_verification,
            _timeout_config: timeout_config,
            _timeout_wrapper: timeout_wrapper,
            circuit_breaker,
            resource_limiter,
            _deadlock_detector: deadlock_detector,
            health_monitor,
            telemetry: Some(telemetry),
            trace_db,
            trace_sink_missing_warned: false,
            trace_flush_every,
            placement_template,
            profiler,
            lifecycle,
            hotswap,
            retirement_handle,
            persistence_handle,
            shutdown_tx,
            active_training_jobs,
            inference_cancellations,
            worker_id,
            kv_residency_policy_id,
            critical_metrics,
            pause_registry: None,
            equipment_profile: capture_equipment_profile(),
        })
    }

    /// Set inference pause registry for human-in-the-loop review protocol
    pub fn with_pause_registry(
        mut self,
        registry: Arc<inference_pause::InferencePauseRegistry>,
    ) -> Self {
        self.pause_registry = Some(registry);
        self
    }

    /// Start background GPU verification task
    ///
    /// Spawns a background task that periodically verifies GPU buffer integrity.
    /// This should be called after Worker::new() to enable automatic verification.
    ///
    /// # Parameters
    /// - `interval_secs`: How often to run verification (default: 300 seconds / 5 minutes)
    ///
    /// Note: Background monitoring is acceptable as tokio::spawn per AGENTS.md,
    /// but using deterministic spawn for consistency where possible
    pub fn start_gpu_verification_task(&self, interval_secs: u64) -> tokio::task::JoinHandle<()> {
        let kernels = self.kernels.clone();
        let lifecycle = self.lifecycle.clone();
        let telemetry = self.telemetry.clone();
        let tenant_id = self.tenant_namespace.clone();
        let worker_id = self.worker_id;

        // Background monitoring task - acceptable as tokio::spawn per AGENTS.md
        // Using tokio::spawn for background monitoring tasks
        tokio::spawn(async move {
            use crate::backoff::{BackoffConfig, CircuitBreaker as BackoffCircuitBreaker};

            let backoff = BackoffConfig::new(
                tokio::time::Duration::from_secs(5),
                tokio::time::Duration::from_secs(300),
                2.0,
                5,
            );
            let circuit_breaker =
                BackoffCircuitBreaker::new(10, tokio::time::Duration::from_secs(600));
            let mut consecutive_failures = 0u32;

            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                // Check circuit breaker state
                if circuit_breaker.is_open() {
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        worker_id = %worker_id,
                        failure_count = circuit_breaker.failure_count(),
                        "GPU verification circuit breaker is open, pausing"
                    );
                    tokio::time::sleep(circuit_breaker.reset_timeout()).await;
                    continue;
                }

                // Get loaded adapters from lifecycle
                let loaded_adapters = {
                    let lifecycle_guard = lifecycle.lock().await;
                    lifecycle_guard.get_loaded_adapters()
                };

                if loaded_adapters.is_empty() {
                    continue; // Skip if no adapters loaded
                }

                let mut had_errors = false;

                // Verify each loaded adapter
                for (adapter_id_u16, adapter_id, _state) in &loaded_adapters {
                    let mut kernels_lock = kernels.lock().await;

                    // Verify GPU buffers
                    match kernels_lock.verify_adapter_buffers(*adapter_id_u16) {
                        Ok((buffer_size, first_sample, last_sample, mid_sample)) => {
                            // Create GPU fingerprint
                            #[cfg(target_os = "macos")]
                            {
                                use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;
                                let gpu_fp = GpuBufferFingerprint::new(
                                    buffer_size,
                                    &first_sample,
                                    &last_sample,
                                    &mid_sample,
                                );

                                // Verify against baseline
                                match kernels_lock.verify_gpu_fingerprint(
                                    *adapter_id_u16,
                                    buffer_size,
                                    &gpu_fp.checkpoint_hash.to_hex(),
                                ) {
                                    Ok(true) => {
                                        tracing::debug!(
                                            adapter_id = %adapter_id,
                                            "GPU integrity verification passed"
                                        );
                                    }
                                    Ok(false) => {
                                        had_errors = true;
                                        tracing::error!(
                                            tenant_id = %tenant_id,
                                            worker_id = %worker_id,
                                            adapter_id = %adapter_id,
                                            "GPU buffer fingerprint mismatch detected - taking corrective action"
                                        );

                                        // Log critical telemetry event
                                        if let Some(ref t) = telemetry {
                                            let _ = t.log(
                                                "gpu_integrity_failure",
                                                serde_json::json!({
                                                    "adapter_id": adapter_id,
                                                    "reason": "fingerprint_mismatch",
                                                    "action": "adapter_unloaded",
                                                    "severity": "critical"
                                                }),
                                            );
                                        }

                                        // Corrective action: Unload corrupted adapter from kernels
                                        if let Err(unload_err) =
                                            kernels_lock.detach_adapter(*adapter_id_u16)
                                        {
                                            tracing::error!(
                                                tenant_id = %tenant_id,
                                                worker_id = %worker_id,
                                                adapter_id = %adapter_id,
                                                error = %unload_err,
                                                "Failed to unload corrupted adapter"
                                            );
                                        } else {
                                            tracing::info!(
                                                tenant_id = %tenant_id,
                                                worker_id = %worker_id,
                                                adapter_id = %adapter_id,
                                                "Successfully unloaded corrupted adapter from GPU"
                                            );
                                        }

                                        // Mark adapter for manual review in lifecycle
                                        tracing::warn!(
                                            tenant_id = %tenant_id,
                                            worker_id = %worker_id,
                                            adapter_id = %adapter_id,
                                            "Adapter marked for manual review - will not be automatically reloaded"
                                        );
                                    }
                                    Err(e) => {
                                        had_errors = true;
                                        tracing::error!(
                                            tenant_id = %tenant_id,
                                            worker_id = %worker_id,
                                            adapter_id = %adapter_id,
                                            error = %e,
                                            "GPU verification failed"
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!(
                                adapter_id = %adapter_id,
                                error = %e,
                                "Failed to verify adapter GPU buffers (may not be loaded)"
                            );
                        }
                    }
                }

                // Track success/failure for circuit breaker and backoff
                if had_errors {
                    circuit_breaker.record_failure();
                    consecutive_failures += 1;

                    // Apply backoff on errors
                    let delay = backoff.next_delay(consecutive_failures);
                    tracing::warn!(
                        delay_ms = delay.as_millis(),
                        consecutive_failures = consecutive_failures,
                        "Applying backoff to GPU verification after errors"
                    );
                    tokio::time::sleep(delay).await;

                    // Extended backoff if we've exceeded max retries
                    if backoff.should_give_up(consecutive_failures) {
                        tracing::error!(
                            "GPU verification has failed {} times, entering extended backoff",
                            consecutive_failures
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
                        consecutive_failures = 0;
                    }
                } else {
                    circuit_breaker.record_success();
                    consecutive_failures = 0;
                }
            }
        })
    }

    /// Resolve runtime metadata (coreml + fallback) for the active backend lane.
    fn runtime_metadata_for_response(
        &self,
        fallback_triggered: bool,
    ) -> (Option<CoremlRuntimeTelemetry>, Option<String>) {
        let active_backend = if fallback_triggered {
            self.available_backends
                .fallback
                .unwrap_or(self.available_backends.primary)
        } else {
            self.available_backends.primary
        };

        let coreml = if active_backend == BackendKind::CoreML {
            if fallback_triggered {
                self.available_backends
                    .coreml_fallback
                    .clone()
                    .or_else(|| self.available_backends.coreml_primary.clone())
            } else {
                self.available_backends.coreml_primary.clone()
            }
        } else {
            None
        };

        let fallback_backend = if fallback_triggered {
            self.available_backends
                .fallback
                .map(|bk| bk.as_str().to_string())
        } else {
            None
        };

        (coreml, fallback_backend)
    }

    /// Normalized backend identifier for deterministic responses.
    ///
    /// Returns the normalized backend identifier that is consistent across hardware
    /// configurations, enabling deterministic replay/comparison. The normalization
    /// mapping is defined in `BackendKind::normalized_id()`:
    /// - MLX variants → "native"
    /// - CoreML/Metal → "accelerated"
    /// - CPU → "cpu"
    fn backend_used_for_response(&self, fallback_triggered: bool) -> String {
        let backend = if fallback_triggered {
            self.available_backends
                .fallback
                .unwrap_or(self.available_backends.primary)
        } else {
            self.available_backends.primary
        };
        backend.normalized_id().to_string()
    }

    /// Raw backend identifier for telemetry/observability.
    ///
    /// Returns the device-specific backend name (e.g., "mlx", "coreml", "metal")
    /// for debugging and telemetry purposes. For deterministic comparison, use
    /// `backend_used_for_response()` instead.
    fn backend_raw_for_response(&self, fallback_triggered: bool) -> String {
        let backend = if fallback_triggered {
            self.available_backends
                .fallback
                .unwrap_or(self.available_backends.primary)
        } else {
            self.available_backends.primary
        };
        backend.as_str().to_string()
    }

    /// Compute the ModelCacheIdentityV2 digest for this worker (PRD-06)
    ///
    /// This digest uniquely identifies the cache configuration used for inference,
    /// including kernel, quantization, tokenizer, tenant, and worker identity.
    fn compute_model_cache_identity_v2_digest(&self, backend: BackendKind) -> B3Hash {
        self.build_model_cache_identity_v2(backend).0
    }

    /// Build ModelCacheIdentityV2 and return (digest, canonical_bytes).
    ///
    /// The canonical bytes are needed for prefix KV cache key computation.
    /// The digest is stored in receipts for verification.
    fn build_model_cache_identity_v2(&self, backend: BackendKind) -> (B3Hash, Vec<u8>) {
        use adapteros_lora_kernel_api::attestation::BackendType;

        // Convert BackendKind to BackendType for identity computation
        let backend_type = match backend {
            BackendKind::CoreML => BackendType::CoreML,
            BackendKind::Metal => BackendType::Metal,
            BackendKind::Mlx | BackendKind::MlxBridge => BackendType::MLX,
            // Model Server uses MLX backend remotely
            BackendKind::ModelServer => BackendType::MLX,
            BackendKind::CPU | BackendKind::Auto => BackendType::Mock, // Fallback for non-accelerated
        };

        let identity = ModelCacheIdentityV2::for_backend_with_tokenizer(
            backend_type,
            self.manifest.base.tokenizer_hash,
            self.manifest.base.tokenizer_cfg_hash,
            self.tenant_namespace.clone(),
            self.worker_id,
        );

        // PRD-06: Validate identity is complete
        // Debug builds: panic to catch issues early
        // Release builds: warn but continue (backward compat)
        if let Err(e) = identity.validate_strict() {
            tracing::warn!(error = %e, "ModelCacheIdentityV2 validation failed - proceeding with potentially incomplete identity");
            #[cfg(debug_assertions)]
            panic!("ModelCacheIdentityV2 validation failed: {}", e);
        }

        (identity.digest(), identity.canonical_bytes())
    }

    fn kv_receipt_fields(&self) -> (u64, u64, u32, Option<String>, bool) {
        let (tenant_kv_bytes_used, tenant_kv_quota_bytes, kv_evictions, kv_quota_enforced) =
            if let Some(qm) = self.kv_quota_manager.as_ref() {
                let usage = qm.usage();
                (
                    usage.used_bytes,
                    usage.quota_bytes.unwrap_or(0),
                    qm.evictions(),
                    qm.is_quota_enforced(),
                )
            } else {
                let used_bytes = match self.kv_cache.lock() {
                    Ok(cache) => cache.usage().0,
                    Err(_) => 0,
                };
                (used_bytes, 0, 0, false)
            };

        (
            tenant_kv_quota_bytes,
            tenant_kv_bytes_used,
            kv_evictions,
            self.kv_residency_policy_id.clone(),
            kv_quota_enforced,
        )
    }

    /// Run inference with comprehensive safety mechanisms
    pub async fn infer(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        self.infer_with_stream(request, None).await
    }

    /// Run inference while emitting streaming token events.
    pub async fn infer_stream(
        &mut self,
        request: InferenceRequest,
        stream_tx: mpsc::Sender<WorkerStreamEvent>,
    ) -> Result<()> {
        match self
            .infer_with_stream(request, Some(stream_tx.clone()))
            .await
        {
            Ok(response) => {
                let _ = stream_tx
                    .send(WorkerStreamEvent::Complete(Box::new(response)))
                    .await;
                Ok(())
            }
            Err(e) => {
                let _ = stream_tx
                    .send(WorkerStreamEvent::Error(e.to_string()))
                    .await;
                Err(e)
            }
        }
    }

    async fn infer_with_stream(
        &mut self,
        request: InferenceRequest,
        stream_tx: Option<mpsc::Sender<WorkerStreamEvent>>,
    ) -> Result<InferenceResponse> {
        // Guardrail: Acquire resource permit (limits concurrency and checks quotas)
        let limiter = self.resource_limiter.clone();
        let _permit = limiter.acquire_request().await?;

        let start_time = Instant::now();

        // Compute queue wait time (time from request arrival to inference start)
        let queue_time_us = request
            .arrival_instant
            .map(|arrival| start_time.saturating_duration_since(arrival).as_micros() as u64)
            .unwrap_or(0);

        // Record health metrics
        self.health_monitor.record_request();

        // Run inference with timeout (simplified to avoid borrow checker issues)
        let result = self.infer_internal(request, stream_tx).await;

        // Compute generation time (actual inference duration)
        let generation_time = start_time.elapsed();
        let generation_time_us = generation_time.as_micros() as u64;

        // Convert times to seconds for Prometheus histograms
        let queue_time_secs = queue_time_us as f64 / 1_000_000.0;
        let generation_time_secs = generation_time.as_secs_f64();
        let worker_id_str = self.worker_id.to_string();
        let model_id = self.manifest.base.model_id.as_str();

        // Record Prometheus histograms for queue wait and generation time
        if let Some(metrics) = &self.critical_metrics {
            metrics.record_worker_inference_timing(
                &worker_id_str,
                model_id,
                queue_time_secs,
                generation_time_secs,
            );
        }

        // Log telemetry with queue/generation time breakdown
        if let Some(t) = &self.telemetry {
            let _ = t
                .log(
                    "inference",
                    InferenceEvent {
                        duration_ms: generation_time.as_millis() as u64,
                        success: result.is_ok(),
                        timeout_occurred: matches!(result, Err(AosError::Worker(ref msg)) if msg.contains("timeout")),
                        circuit_breaker_open: matches!(CircuitBreakerTrait::state(&self.circuit_breaker), CircuitState::Open { .. }),
                        memory_usage: self.health_monitor.get_memory_usage().unwrap_or(0),
                        queue_time_us,
                        generation_time_us,
                    },
                )
                .ok();
        }

        result
    }

    fn log_inference_cancelled(
        &self,
        request: &InferenceRequest,
        reason: &str,
        tokens_generated: usize,
    ) {
        if let Some(t) = &self.telemetry {
            let _ = t.log(
                "inference.cancelled",
                serde_json::json!({
                    "cpid": request.cpid,
                    "request_id": request.request_id.as_ref(),
                    "reason": reason,
                    "tokens_generated": tokens_generated,
                    "stack_id": request.stack_id,
                    "stack_version": request.stack_version,
                }),
            );
        }
    }

    /// Check if inference has been cancelled by client or system.
    ///
    /// Returns `Err(CancellationState)` when cancelled, carrying:
    /// - Trace ID for receipt generation
    /// - Token count at cancellation point
    /// - Cancellation source and reason
    ///
    /// The caller is responsible for generating a cancellation receipt using
    /// `StopReasonCode::Cancelled` before returning to the client.
    fn check_inference_cancelled(
        &self,
        request: &InferenceRequest,
        token: Option<&InferenceCancelToken>,
        tokens_generated: usize,
    ) -> std::result::Result<(), CancellationState> {
        let Some(token) = token else {
            return Ok(());
        };

        if !token.is_cancelled() {
            return Ok(());
        }

        let reason = token
            .reason()
            .unwrap_or_else(|| "client_cancelled".to_string());
        self.log_inference_cancelled(request, &reason, tokens_generated);

        // Parse the cancellation source from the reason string
        let source: CancelSource = reason.parse().unwrap_or(CancelSource::ClientDisconnect);

        // Return cancellation state for receipt generation
        Err(CancellationState::new(
            request.request_id.clone().unwrap_or_default(),
            tokens_generated,
            source,
            reason,
        ))
    }

    /// Check cancellation and convert to AosError for backward compatibility.
    ///
    /// This wrapper calls `check_inference_cancelled` and converts `CancellationState`
    /// to `AosError::Worker`. The cancellation state is preserved in the error for
    /// generating cancellation receipts at the call site where trace_sink is available.
    fn check_cancelled_or_error(
        &self,
        request: &InferenceRequest,
        token: Option<&InferenceCancelToken>,
        tokens_generated: usize,
    ) -> Result<()> {
        self.check_inference_cancelled(request, token, tokens_generated)
            .map_err(|state: CancellationState| {
                // Log cancellation with audit context
                info!(
                    trace_id = %state.trace_id,
                    tokens_generated = state.tokens_generated,
                    source = %state.source,
                    reason = %state.reason_message,
                    "Inference cancelled - generating audit record"
                );
                AosError::Worker(format!(
                    "Inference cancelled after {} tokens: {}",
                    state.tokens_generated, state.reason_message
                ))
            })
    }

    /// Generate a cancellation receipt for audit trail completeness.
    ///
    /// This should be called when inference is cancelled and a trace_sink is available.
    /// It creates and stores a cryptographic receipt capturing the partial output state.
    ///
    /// # Arguments
    /// * `trace_sink` - The SQL trace sink to store the receipt
    /// * `partial_tokens` - Tokens generated before cancellation
    /// * `cancellation_source` - Why the inference was cancelled
    /// * `cancelled_at_token` - Token index at cancellation
    /// * `tenant_id` - Tenant ID for multi-tenant isolation
    ///
    /// # Returns
    /// The cancellation receipt if successful, or logs a warning and returns None on failure.
    async fn generate_cancellation_receipt(
        &self,
        trace_sink: &mut SqlTraceSink,
        partial_tokens: Vec<u32>,
        cancellation_source: CancelSource,
        cancelled_at_token: u32,
        tenant_id: String,
    ) -> Option<adapteros_db::TraceCancellationReceipt> {
        let cancellation = TraceCancellation {
            partial_tokens,
            cancellation_source,
            cancelled_at_token,
            equipment_profile: self.equipment_profile.clone(),
            tenant_id: Some(tenant_id),
        };

        match trace_sink.finalize_cancelled(cancellation).await {
            Ok(receipt) => {
                info!(
                    trace_id = %receipt.trace_id,
                    partial_output_count = receipt.partial_output_count,
                    cancellation_source = %receipt.cancellation_source,
                    receipt_digest = %receipt.receipt_digest.to_hex(),
                    "Cancellation receipt generated for audit trail"
                );
                Some(receipt)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to generate cancellation receipt - audit trail incomplete"
                );
                None
            }
        }
    }

    /// Check cancellation and generate receipt if cancelled.
    ///
    /// This combines the cancellation check with receipt generation for audit trail
    /// completeness. If cancelled, generates a receipt before returning the error.
    ///
    /// # Arguments
    /// * `request` - The inference request
    /// * `token` - The cancellation token
    /// * `tokens_generated` - Number of tokens generated so far
    /// * `generated_tokens` - The tokens generated so far (for receipt)
    /// * `trace_sink` - Optional trace sink for receipt generation
    ///
    /// # Returns
    /// Ok(()) if not cancelled, or Err with the cancellation error after generating receipt.
    async fn check_cancelled_with_receipt(
        &self,
        request: &InferenceRequest,
        token: Option<&InferenceCancelToken>,
        tokens_generated: usize,
        generated_tokens: &[u32],
        trace_sink: &mut Option<SqlTraceSink>,
    ) -> Result<()> {
        match self.check_inference_cancelled(request, token, tokens_generated) {
            Ok(()) => Ok(()),
            Err(state) => {
                // Log cancellation with audit context
                info!(
                    trace_id = %state.trace_id,
                    tokens_generated = state.tokens_generated,
                    source = %state.source,
                    reason = %state.reason_message,
                    "Inference cancelled - generating cancellation receipt"
                );

                // Generate cancellation receipt if trace sink is available
                if let Some(ref mut sink) = trace_sink {
                    let _ = self
                        .generate_cancellation_receipt(
                            sink,
                            generated_tokens.to_vec(),
                            state.source,
                            state.tokens_generated as u32,
                            request.cpid.clone(),
                        )
                        .await;
                }

                Err(AosError::Worker(format!(
                    "Inference cancelled after {} tokens: {}",
                    state.tokens_generated, state.reason_message
                )))
            }
        }
    }

    /// Internal inference implementation with safety checks
    async fn infer_internal(
        &mut self,
        request: InferenceRequest,
        stream_tx: Option<mpsc::Sender<WorkerStreamEvent>>,
    ) -> Result<InferenceResponse> {
        let mut request = request;
        let cancel_token = request
            .request_id
            .as_deref()
            .map(|id| self.inference_cancellations.register(id));
        let _cancel_guard = request
            .request_id
            .clone()
            .map(|id| InferenceCancelGuard::new(self.inference_cancellations.clone(), id));
        self.check_cancelled_or_error(&request, cancel_token.as_deref(), 0)?;
        // Start profiler session
        let mut _profiler_session = self.profiler.start_inference();

        // Enforce tenant isolation: worker must only serve its configured tenant
        if request.cpid != self.tenant_namespace {
            return Err(AosError::IsolationViolation(format!(
                "Request tenant {} does not match worker tenant {}",
                request.cpid, self.tenant_namespace
            )));
        }

        if let Some(qm) = self.kv_quota_manager.as_ref() {
            qm.reset_evictions();
        }

        // Check memory - handle memory pressure if needed
        if let Err(_e) = self.memory_monitor.check_headroom() {
            {
                let lifecycle = self.lifecycle.lock().await;
                lifecycle.handle_memory_pressure(&self.profiler)?;
            }
            // Try again after eviction
            self.memory_monitor.check_headroom().map_err(|e| {
                AosError::MemoryPressure(format!("Insufficient headroom after eviction: {}", e))
            })?;
        }

        let explicit_base_only = matches!(
            request.effective_adapter_ids.as_ref(),
            Some(ids) if ids.is_empty()
        );
        let allow_empty_stack = explicit_base_only || request.effective_adapter_ids.is_none();

        let strict_mode_active =
            strict_mode_enabled(request.strict_mode, &request.determinism_mode);

        // Resolve backend lane for this request with reasoning-aware routing
        // UNIFIED INFERENCE ROUTER: reasoning_mode -> CoreML, else -> MLX streaming
        let requested_backend = request.backend_profile;
        let reasoning_mode = request.reasoning_mode;

        // Step 1: Apply reasoning-aware routing to get suggested backend
        let (reasoning_suggested, reasoning_triggered, routing_reason) = self
            .available_backends
            .resolve_for_reasoning(requested_backend, reasoning_mode);

        // Log reasoning-aware routing decision for observability
        if reasoning_triggered {
            info!(
                target: "inference.backend.routing",
                reasoning_mode = true,
                suggested = %reasoning_suggested.as_str(),
                reason = routing_reason,
                "Reasoning-aware routing: CoreML selected for deterministic reasoning"
            );
        } else {
            debug!(
                target: "inference.backend.routing",
                reasoning_mode,
                suggested = %reasoning_suggested.as_str(),
                reason = routing_reason,
                "Backend routing decision"
            );
        }

        // Step 2: Validate the suggested backend is available, with fallback
        let (resolved_backend, backend_lane, backend_overridden) = {
            if self.available_backends.contains(reasoning_suggested) {
                (
                    reasoning_suggested,
                    self.available_backends.lane_for(reasoning_suggested),
                    false,
                )
            } else {
                // Suggested backend unavailable, fall back
                let fallback = self
                    .available_backends
                    .fallback
                    .unwrap_or(self.available_backends.primary);
                (fallback, self.available_backends.lane_for(fallback), true)
            }
        };
        if backend_overridden {
            warn!(
                requested = ?requested_backend.map(|b| b.as_str().to_string()),
                reasoning_suggested = %reasoning_suggested.as_str(),
                selected = %resolved_backend.as_str(),
                reasoning_mode,
                "Requested/suggested backend not available on this worker; falling back"
            );
        }
        request.backend_profile = Some(resolved_backend);
        let backend_label = resolved_backend.as_str().to_string();
        let baseline_backend = resolve_baseline_backend(requested_backend, resolved_backend);
        let baseline_level = declared_determinism_level(baseline_backend);
        let kernel_version_id = adapteros_core::version::VERSION.to_string();

        if strict_mode_active && kernel_version_id.is_empty() {
            emit_observability_event(&determinism_violation_event(
                DeterminismViolationKind::Unknown,
                None,
                Some(self.manifest.base.model_hash.to_hex()),
                None,
                true,
                Some(request.cpid.clone()),
                None,
            ));
            return Err(AosError::DeterminismViolation(
                "Strict determinism mode requires kernel_version_id".to_string(),
            ));
        }

        if strict_mode_active && baseline_level == DeterminismLevel::None {
            emit_observability_event(&determinism_violation_event(
                DeterminismViolationKind::Unknown,
                None,
                Some(self.manifest.base.model_hash.to_hex()),
                None,
                true,
                Some(request.cpid.clone()),
                request.request_id.clone(),
            ));
            return Err(AosError::DeterminismViolation(format!(
                "Strict determinism mode requires a deterministic backend; requested {}",
                baseline_backend.as_str()
            )));
        }

        // Validate effective adapter gate (if provided)
        let allowed_indices = self.validate_effective_adapter_gate(&request)?;

        if matches!(request.seed_mode, Some(SeedMode::Strict)) && request.request_seed.is_none() {
            emit_observability_event(&determinism_violation_event(
                DeterminismViolationKind::Unknown,
                None,
                Some(self.manifest.base.model_hash.to_hex()),
                None,
                true,
                Some(request.cpid.clone()),
                None,
            ));
            return Err(AosError::DeterminismViolation(
                "Strict seed_mode requires request_seed".to_string(),
            ));
        }

        // Compute unavailable pinned adapters (CHAT-PIN-02)
        // These are pinned adapter IDs not present in the worker's loaded adapters
        let unavailable_pinned_adapters =
            request.pinned_adapter_ids.as_ref().and_then(|pinned_ids| {
                let loaded_adapter_ids: Vec<&str> = self
                    .manifest
                    .adapters
                    .iter()
                    .map(|a| a.id.as_str())
                    .collect();
                let unavailable: Vec<String> = pinned_ids
                    .iter()
                    .filter(|id| !loaded_adapter_ids.contains(&id.as_str()))
                    .cloned()
                    .collect();
                if unavailable.is_empty() {
                    None
                } else {
                    Some(unavailable)
                }
            });

        // Compute pinned_routing_fallback based on unavailability (PRD-6A)
        let pinned_routing_fallback =
            match (&request.pinned_adapter_ids, &unavailable_pinned_adapters) {
                (Some(pinned), Some(unavailable))
                    if !pinned.is_empty() && !unavailable.is_empty() =>
                {
                    if unavailable.len() >= pinned.len() {
                        Some("stack_only".to_string())
                    } else {
                        Some("partial".to_string())
                    }
                }
                _ => None,
            };

        let validator = RequestValidator::new(self.tokenizer.as_ref(), self.max_seq_len);
        let validated_prompt = validator.validate(&request.prompt)?;

        // Retrieve evidence if required
        let mut evidence = Vec::new();
        if request.require_evidence {
            // Compute query embedding first (before borrowing rag)
            let query_emb = self.compute_embedding(&request.prompt)?;

            if let Some(ref mut rag) = self.rag {
                let namespace = self.tenant_namespace.clone();
                let spans = rag
                    .retrieve(&namespace, &query_emb, self.manifest.policies.rag.topk)
                    .map_err(|e| AosError::Rag(format!("Evidence retrieval failed: {}", e)))?;

                evidence = spans
                    .iter()
                    .map(|s| EvidenceRef {
                        doc_id: s.doc_id.clone(),
                        rev: s.rev.clone(),
                        span_hash: s.span_hash,
                        score: s.score,
                    })
                    .collect();

                // Check evidence policy
                if let Err(_e) = self.policy.check_evidence(evidence.len()) {
                    // Insufficient evidence, returning refusal
                    let fallback_triggered = {
                        let kernels = self.kernels.lock().await;
                        kernels.fallback_triggered()
                    };
                    let backend_used = self.backend_used_for_response(fallback_triggered);
                    let backend_raw = self.backend_raw_for_response(fallback_triggered);
                    let backend_version = adapteros_core::version::VERSION.to_string();
                    let (coreml_runtime, fallback_backend) =
                        self.runtime_metadata_for_response(fallback_triggered);

                    return Ok(InferenceResponse {
                        text: None,
                        status: "insufficient_evidence".to_string(),
                        trace: ResponseTrace {
                            cpid: request.cpid.clone(),
                            plan_id: self.generate_plan_id(&request.cpid),
                            evidence: evidence.clone(),
                            router_summary: RouterSummary {
                                adapters_used: vec![],
                                avg_activations: vec![],
                            },
                            token_count: 0,
                            router_decisions: None,
                            router_decision_chain: None,
                            fusion_intervals: None,
                            model_type: None,
                        },
                        run_receipt: None,
                        token_usage: None,
                        refusal: Some(RefusalResponse::insufficient_evidence(
                            self.manifest.policies.evidence.min_spans,
                            evidence.len(),
                        )),
                        patch_proposal: None,
                        stack_id: request.stack_id.clone(),
                        stack_version: request.stack_version,
                        backend_used: Some(backend_used),
                        backend_raw: Some(backend_raw),
                        backend_version: Some(backend_version),
                        fallback_triggered,
                        coreml_compute_preference: coreml_runtime
                            .as_ref()
                            .and_then(|r| r.compute_preference.clone()),
                        coreml_compute_units: coreml_runtime
                            .as_ref()
                            .and_then(|r| r.compute_units.clone()),
                        coreml_gpu_used: coreml_runtime.as_ref().and_then(|r| r.gpu_used),
                        coreml_package_hash: self.coreml_package_hash.clone(),
                        coreml_expected_package_hash: self
                            .coreml_verification
                            .as_ref()
                            .and_then(|v| v.expected.clone()),
                        coreml_hash_mismatch: self.coreml_verification.as_ref().map(|v| v.mismatch),
                        fallback_backend,
                        determinism_mode_applied: Some(request.determinism_mode.clone()),
                        unavailable_pinned_adapters: unavailable_pinned_adapters.clone(),
                        pinned_routing_fallback: pinned_routing_fallback.clone(),
                        placement_trace: None,
                        stop_reason_code: None,
                        stop_reason_token_index: None,
                        stop_policy_digest_b3: None,
                        error_details: None,
                    });
                }
            }
        }

        self.check_cancelled_or_error(&request, cancel_token.as_deref(), 0)?;

        // Apply request-provided sampling parameters (PRD-02: replay support)
        // This enables deterministic replay when the same parameters are provided
        let routing_mode = request
            .routing_determinism_mode
            .unwrap_or(RoutingDeterminismMode::Deterministic);
        let seed_mode = request.seed_mode.unwrap_or(SeedMode::BestEffort);

        if strict_mode_active && routing_mode == RoutingDeterminismMode::Adaptive {
            emit_observability_event(&determinism_violation_event(
                DeterminismViolationKind::Unknown,
                None,
                Some(self.manifest.base.model_hash.to_hex()),
                None,
                true,
                Some(request.cpid.clone()),
                request.request_id.clone(),
            ));
            return Err(AosError::DeterminismViolation(
                "Strict determinism mode requires deterministic routing".to_string(),
            ));
        }

        let determinism_ctx: Option<DeterminismContext> = request.request_seed.map(|seed_bytes| {
            DeterminismContext::new_with_router_seed(
                seed_bytes,
                request.router_seed.clone(),
                None,
                seed_mode,
                routing_mode,
                DeterminismSource::DerivedFromRequest,
            )
        });

        let fusion_interval = request
            .fusion_interval
            .unwrap_or(FusionInterval::PerRequest);

        if let Some(seed_bytes) = request.request_seed {
            self.generator.set_seed_bytes(seed_bytes)?;
            // Avoid overriding master request seed with low-entropy seed
            self.generator.apply_request_params(
                request.temperature,
                request.top_k,
                request.top_p,
                None,
            )?;
        }
        if request.request_seed.is_none() {
            self.generator.apply_request_params(
                request.temperature,
                request.top_k,
                request.top_p,
                request.seed,
            )?;
        }

        // Generate tokens using autoregressive loop
        let prompt_tokens = validated_prompt.tokens;

        // ============================================================================
        // MOE PRE-WARMING & FREE TOKENS (Protocol v3)
        // ============================================================================
        #[cfg(feature = "mlx-bridge")]
        let mut free_tokens: Vec<crate::moe_prefix_cache::FreeToken> = Vec::new();
        #[cfg(not(feature = "mlx-bridge"))]
        let free_tokens: Vec<()> = Vec::new();
        #[allow(unused_mut)]
        let mut free_token_ids: Vec<u32> = Vec::new();
        #[allow(unused_mut)]
        let mut free_token_text = String::new();

        #[allow(unused_mut)]
        let mut prompt_tokens_with_free = prompt_tokens.clone();
        #[allow(unused_mut)]
        let mut max_tokens_remaining = request.max_tokens;
        let is_moe = self.kernels.lock().await.is_moe();
        self.router.set_model_is_moe(is_moe);

        #[cfg(feature = "mlx-bridge")]
        if is_moe {
            let effective_temperature = request.temperature.unwrap_or(0.7);

            // Attempt to get free tokens from cache
            if let Some(tokens) = self.moe_prefix_cache.get_free_tokens_for_tokens(
                &prompt_tokens,
                effective_temperature,
                None, // We don't have a single adapter ID here easily, passing None for now
                5,    // Limit to 5 free tokens max
            ) {
                if !tokens.is_empty() {
                    debug!(count = tokens.len(), "Delivering free tokens from cache");

                    // Collect token IDs for pre-warming the *next* tokens
                    let free_ids: Vec<u32> = tokens.iter().map(|t| t.token_id).collect();
                    prompt_tokens_with_free.extend_from_slice(&free_ids);
                    max_tokens_remaining = max_tokens_remaining.saturating_sub(tokens.len());

                    free_tokens = tokens;
                }
            }

            // Pre-warm experts based on the EXTENDED prompt (original + free tokens)
            // This ensures we warm up experts for the first *actually computed* token
            if let Some(predicted_experts) = self
                .moe_prefix_cache
                .get_experts_for_tokens(&prompt_tokens_with_free)
            {
                debug!(
                    experts = predicted_experts.len(),
                    "MoE prefix cache hit: pre-warming experts"
                );

                // Pre-warm predicted experts on the backend
                let kernels = self.kernels.lock().await;
                match kernels.prewarm_experts(predicted_experts) {
                    Ok(count) => {
                        debug!(count = count, "Successfully pre-warmed experts");
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to pre-warm experts; continuing without pre-warm");
                    }
                }
            } else {
                debug!("MoE prefix cache miss");
            }
        }

        // Populate legacy variables used by downstream code
        #[cfg(feature = "mlx-bridge")]
        if !free_tokens.is_empty() {
            free_token_ids = free_tokens.iter().map(|t| t.token_id).collect();
            free_token_text = free_tokens.iter().map(|t| t.text.as_str()).collect();
        }

        if let Some(ref tx) = stream_tx {
            if !free_token_text.is_empty() {
                let event = WorkerStreamEvent::Token(StreamToken {
                    text: free_token_text.clone(),
                    token_id: None,
                });
                if tx.send(event).await.is_err() {
                    return Err(AosError::Worker("Stream cancelled".to_string()));
                }
            }
        }

        // If free tokens satisfied the entire request, return early!
        if !free_tokens.is_empty() && max_tokens_remaining == 0 {
            self.check_cancelled_or_error(&request, cancel_token.as_deref(), free_tokens.len())?;

            let fallback_triggered = {
                let kernels = self.kernels.lock().await;
                kernels.fallback_triggered()
            };
            let backend_used = self.backend_used_for_response(fallback_triggered);
            let backend_raw = self.backend_raw_for_response(fallback_triggered);
            let logical_prompt_tokens: u32 = prompt_tokens.len().try_into().unwrap_or(u32::MAX);
            let logical_output_tokens: u32 = free_tokens.len().try_into().unwrap_or(u32::MAX);
            let prefix_cached_token_count: u32 = 0;
            let billed_input_tokens =
                logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
            let billed_output_tokens = logical_output_tokens;
            let token_usage = TokenUsage {
                prompt_tokens: logical_prompt_tokens,
                completion_tokens: logical_output_tokens,
                billed_input_tokens,
                billed_output_tokens,
            };

            // Build trace for free token only response
            let trace = self.build_trace(
                &request.cpid,
                &Vec::new(), // evidence
                free_tokens.len(),
                None,
                None,
                adapteros_core::FusionInterval::PerRequest,
                &[],   // active_ids
                false, // base_only
            );

            return Ok(InferenceResponse {
                text: Some(free_token_text),
                status: "ok".to_string(),
                trace,
                run_receipt: None,
                token_usage: Some(token_usage),
                refusal: None,
                patch_proposal: None,
                stack_id: request.stack_id.clone(),
                stack_version: request.stack_version,
                backend_used: Some(backend_used),
                backend_raw: Some(backend_raw),
                backend_version: Some(adapteros_core::version::VERSION.to_string()),
                fallback_triggered,
                coreml_compute_preference: None,
                coreml_compute_units: None,
                coreml_gpu_used: None,
                coreml_package_hash: None,
                coreml_expected_package_hash: None,
                coreml_hash_mismatch: None,
                fallback_backend: None,
                determinism_mode_applied: Some(request.determinism_mode.clone()),
                unavailable_pinned_adapters: None,
                pinned_routing_fallback: None,
                placement_trace: None,
                stop_reason_code: Some(
                    adapteros_api_types::inference::StopReasonCode::CompletionConfident,
                ),
                stop_reason_token_index: Some(free_tokens.len() as u32),
                stop_policy_digest_b3: None,
                error_details: None,
            });
        }

        // Snapshot current stack and pin adapters for this request
        let pinner = RequestPinner::new(self.hotswap.table().clone());
        let pinned_request = if allow_empty_stack {
            pinner.pin_allow_empty()
        } else {
            pinner.pin()
        }
        .map_err(|e| AosError::Worker(format!("Failed to pin adapters: {}", e)))?;
        let stack_handle = pinned_request.stack().clone();
        let current_generation = pinned_request.generation();
        let base_only_request = explicit_base_only
            || (request.effective_adapter_ids.is_none() && stack_handle.active.is_empty());

        // Ensure KV cache coherence with current generation
        // This will reset cache if generation changed since last inference
        {
            let mut kv_cache = match self.kv_cache.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    error!(
                        error = %e,
                        generation = current_generation,
                        "KV cache lock poisoned"
                    );
                    return Err(AosError::Internal("KV cache lock poisoned".to_string()));
                }
            };
            if let Ok(reset) = kv_cache.ensure_cache_coherence(current_generation) {
                if reset {
                    tracing::info!(
                        old_generation = kv_cache.generation(),
                        new_generation = current_generation,
                        "KV cache reset due to adapter stack generation change"
                    );
                }
            }
        }

        // Build deterministic active adapter view for routing (no per-token attach/detach)
        let stack_hash = pinned_request.stack_hash();
        let mut context_bytes =
            Vec::with_capacity(self.tenant_namespace.len() + 32 + (prompt_tokens.len() * 4) + 4);
        context_bytes.extend_from_slice(self.tenant_namespace.as_bytes());
        context_bytes.extend_from_slice(stack_hash.as_bytes());
        context_bytes.extend_from_slice(self.manifest.base.tokenizer_hash.as_bytes());
        context_bytes.extend_from_slice(&(prompt_tokens.len() as u32).to_le_bytes());
        for t in &prompt_tokens {
            context_bytes.extend_from_slice(&t.to_le_bytes());
        }
        let context_digest = B3Hash::hash(&context_bytes).to_bytes();

        // PRD-01: Compute prefix KV cache lookup using PRD-compliant key
        // Key = BLAKE3(context_digest || tokens || tokenizer_hash || model_identity)
        let context_digest_hash = B3Hash::from_bytes(context_digest);
        let tokenizer_manifest_hash = compute_tokenizer_manifest_hash(
            &self.manifest.base.tokenizer_hash,
            &self.manifest.base.tokenizer_cfg_hash,
        );
        let (model_cache_identity_v2_digest, model_identity_bytes) =
            self.build_model_cache_identity_v2(resolved_backend);

        // Perform cache prefix lookup (deterministic, receipt-ready)
        let cache_lookup_result = cache_prefix_lookup::cache_prefix_lookup(
            &self.prefix_kv_cache,
            &prompt_tokens,
            &context_digest_hash,
            &tokenizer_manifest_hash,
            &model_cache_identity_v2_digest,
            &model_identity_bytes,
            &cache_prefix_lookup::CacheLookupConfig::default(),
        );

        // Extract receipt-bound values from lookup result
        let prefix_cache_hit = cache_lookup_result.cache_hit;
        let prefix_cached_token_count = cache_lookup_result.cached_token_count;
        let prefix_kv_bytes = cache_lookup_result.cached_kv_bytes;
        let prefix_kv_key = cache_lookup_result.cache_id.unwrap_or(context_digest_hash);

        if prefix_cache_hit {
            tracing::debug!(
                key = %prefix_kv_key.to_hex()[..16],
                cached_tokens = prefix_cached_token_count,
                kv_bytes = prefix_kv_bytes,
                exact_match = cache_lookup_result.is_exact_match,
                "Prefix KV cache hit (metrics only - KV reuse pending kernel integration)"
            );
        }

        let trace_id = Uuid::now_v7().to_string();

        let mut trace_sink = if let Some(trace_db) = self.trace_db.as_mut() {
            let start = TraceStart {
                trace_id: trace_id.clone(),
                tenant_id: request.cpid.clone(),
                request_id: None,
                context_digest,
                stack_id: request.stack_id.clone(),
                model_id: Some(self.manifest.base.model_id.clone()),
                policy_id: request.policy_id.clone(),
            };

            trace_db.create_sink(start, self.trace_flush_every).await
        } else {
            None
        };

        if strict_mode_active && trace_sink.is_none() && !self.trace_sink_missing_warned {
            warn!(
                tenant_id = %request.cpid,
                worker_id = %self.worker_id,
                "Strict determinism mode requested but trace sink unavailable; continuing without trace persistence"
            );
            emit_observability_event(&determinism_violation_event(
                DeterminismViolationKind::Unknown,
                None,
                Some(self.manifest.base.model_hash.to_hex()),
                None,
                true,
                Some(request.cpid.clone()),
                None,
            ));
            self.trace_sink_missing_warned = true;
        }

        // Phase 2: Create ReceiptGenerator for crypto receipt validation (Patent 3535886.0002)
        // Runs parallel to SqlTraceSink for parity validation before full migration
        let mut receipt_generator = if trace_sink.is_some() {
            // Compute adapter config hash from manifest
            let adapter_configs: Vec<(String, B3Hash, u32, f32)> = self
                .manifest
                .adapters
                .iter()
                .map(|a| (a.id.clone(), a.hash, a.rank, a.alpha))
                .collect();
            let adapter_config_hash = compute_adapter_config_hash(&adapter_configs);

            let mut gen = ReceiptGenerator::new(self.manifest.base.model_hash, adapter_config_hash);

            // Set equipment profile from worker initialization
            if let Some(ref profile) = self.equipment_profile {
                gen = gen.with_equipment_profile(profile.clone());
            }

            // Bind input tokens
            gen.bind_input_tokens(&prompt_tokens);

            // Set metadata
            gen.set_tenant_id(&request.cpid);
            gen.set_trace_id(&trace_id);

            Some(gen)
        } else {
            None
        };

        let mut active_entries: Vec<(usize, _)> = self
            .manifest
            .adapters
            .iter()
            .enumerate()
            .filter_map(|(idx, adapter)| {
                if stack_handle.active.contains_key(&adapter.id) {
                    Some((idx, adapter))
                } else {
                    None
                }
            })
            .collect();

        if active_entries.is_empty() && !base_only_request {
            return Err(AosError::Worker(
                "No active adapters available for routing".to_string(),
            ));
        }

        // Keep manifest order for deterministic index mapping
        active_entries.sort_by_key(|(idx, _)| *idx);

        let active_ids: Vec<String> = active_entries
            .iter()
            .map(|(_, adapter)| adapter.id.clone())
            .collect();
        let active_hashed: Vec<u16> = active_ids.iter().map(|id| adapter_id_to_u16(id)).collect();
        let active_manifest_indices: Vec<usize> =
            active_entries.iter().map(|(idx, _)| *idx).collect();
        let strength_overrides = request.adapter_strength_overrides.as_ref();
        let active_strengths: Vec<f32> = active_entries
            .iter()
            .map(|(_, adapter)| {
                let base = adapter.lora_strength.unwrap_or(1.0);
                let mult = strength_overrides
                    .and_then(|m| m.get(&adapter.id))
                    .copied()
                    .unwrap_or(1.0)
                    .clamp(0.0, 2.0);
                base * mult
            })
            .collect();

        // Map manifest index -> active position
        let mut manifest_to_active: HashMap<usize, usize> = HashMap::new();
        for (pos, (idx, _)) in active_entries.iter().enumerate() {
            manifest_to_active.insert(*idx, pos);
        }

        // Inform router of the active stack for deterministic filtering/telemetry
        self.router.set_active_stack(
            request.stack_id.clone(),
            Some(active_ids.clone()),
            Some(stack_hash),
        );

        // Configure router determinism mode from request (PRD-06: determinism configuration)
        // Parse routing_determinism_mode (deterministic|adaptive) and determinism_mode (strict/besteffort/relaxed)
        let adaptive_routing = routing_mode == RoutingDeterminismMode::Adaptive;
        self.router.set_routing_determinism_mode(adaptive_routing);

        let router_config = if adaptive_routing {
            // Adaptive mode: allow non-deterministic tie-breaking and disable hashing
            RouterDeterminismConfig {
                ieee754_deterministic: false,
                enable_decision_hashing: false,
            }
        } else {
            match request.determinism_mode.as_str() {
                "relaxed" => RouterDeterminismConfig {
                    ieee754_deterministic: false,
                    enable_decision_hashing: false,
                },
                "besteffort" => RouterDeterminismConfig {
                    ieee754_deterministic: true,
                    enable_decision_hashing: false,
                },
                _ => RouterDeterminismConfig::default(), // "strict" or unknown defaults to strict
            }
        };
        self.router.set_determinism_config(router_config);

        // Configure backend strictness (disable fallback when strict_mode=true)
        {
            let mut kernels = self.kernels.lock().await;
            kernels.set_strict_mode(request.strict_mode);
            kernels.reset_fallback();
            kernels.set_active_lane(backend_lane);
        }

        if strict_mode_active {
            let report = {
                let kernels = self.kernels.lock().await;
                kernels.attest_determinism()?
            };
            if is_determinism_downgrade(baseline_level, report.determinism_level) {
                emit_observability_event(&determinism_violation_event(
                    DeterminismViolationKind::Unknown,
                    None,
                    Some(self.manifest.base.model_hash.to_hex()),
                    None,
                    true,
                    Some(request.cpid.clone()),
                    request.request_id.clone(),
                ));
                return Err(AosError::DeterminismViolation(format!(
                    "Strict determinism mode forbids downgrade from {} ({}) to {} ({})",
                    baseline_backend.as_str(),
                    baseline_level,
                    resolved_backend.as_str(),
                    report.determinism_level
                )));
            }
        }

        // Build priors once from active adapter set
        let allowed_active_indices: Option<HashSet<usize>> =
            allowed_indices.as_ref().map(|allowed| {
                allowed
                    .iter()
                    .filter_map(|idx| manifest_to_active.get(idx).copied())
                    .collect::<HashSet<_>>()
            });

        if let Some(ref allowed) = allowed_active_indices {
            if allowed.is_empty() && !base_only_request {
                return Err(AosError::Worker(
                    "Effective adapter set has no overlap with active stack".to_string(),
                ));
            }
        }

        let policy_mask_digest_b3: Option<[u8; 32]> =
            allowed_active_indices.as_ref().map(|allowed| {
                let mut ordered: Vec<u16> = allowed.iter().map(|idx| *idx as u16).collect();
                ordered.sort_unstable();
                let mut buf = Vec::with_capacity(4 + ordered.len() * 2);
                buf.extend_from_slice(&(ordered.len() as u32).to_le_bytes());
                for idx in ordered {
                    buf.extend_from_slice(&idx.to_le_bytes());
                }
                B3Hash::hash(&buf).to_bytes()
            });

        // Build placement state per-request (env + optional override)
        let mut placement_state = if let Some((base_cfg, lanes)) = &self.placement_template {
            let mut effective_cfg = base_cfg.clone();
            if let Some(ref placement_override) = request.placement {
                effective_cfg.mode = placement_override
                    .mode
                    .parse()
                    .unwrap_or(adapteros_config::PlacementMode::Balanced);
                effective_cfg.weights = placement_override.weights;
            }
            if effective_cfg.mode == adapteros_config::PlacementMode::Off {
                None
            } else {
                Some(PlacementState::new(
                    PlacementEngine::new(effective_cfg.clone()),
                    TelemetryCollector::new(effective_cfg.sample_ms),
                    lanes.clone(),
                ))
            }
        } else {
            None
        };

        let adapter_info: Vec<AdapterInfo> = active_entries
            .iter()
            .map(|(_, adapter)| AdapterInfo {
                id: adapter.id.clone(),
                framework: None,    // Manifest adapters don't have framework info
                languages: vec![0], // Default language
                tier: format!("{:?}", adapter.tier).to_lowercase(),
                base_model: None, // Base model info not available in this context
                recommended_for_moe: adapter.recommended_for_moe,
                ..Default::default()
            })
            .collect();

        let mut priors = vec![1.0f32; adapter_info.len()];
        if base_only_request {
            priors.iter_mut().for_each(|p| *p = 0.0);
        } else if let Some(ref allowed) = allowed_active_indices {
            for (idx, prior) in priors.iter_mut().enumerate() {
                if !allowed.contains(&idx) {
                    *prior = 0.0;
                }
            }
        }

        let mut id_to_active: HashMap<&str, usize> = HashMap::new();
        for (pos, id) in active_ids.iter().enumerate() {
            id_to_active.insert(id.as_str(), pos);
        }
        if !base_only_request {
            if let Some(ref pinned_ids) = request.pinned_adapter_ids {
                for pinned in pinned_ids {
                    if let Some(pos) = id_to_active.get(pinned.as_str()) {
                        if let Some(prior) = priors.get_mut(*pos) {
                            *prior += PINNED_BOOST;
                        }
                    }
                }
            }
        }

        // Build deterministic policy mask from routing policy + effective set.
        if request.routing_policy.is_none() {
            return Err(AosError::PolicyViolation(
                "Routing policy missing for inference request".to_string(),
            ));
        }
        let policy_mask_digest_seed = request
            .routing_policy
            .as_ref()
            .and_then(|policy| serde_json::to_vec(policy).ok())
            .map(|bytes| B3Hash::hash(&bytes));
        let policy_mask = PolicyMask::build(
            &active_ids,
            request
                .routing_policy
                .as_ref()
                .and_then(|p| p.allowed_adapter_ids.as_deref()),
            request
                .routing_policy
                .as_ref()
                .and_then(|p| p.denied_adapter_ids.as_deref()),
            allowed_active_indices.as_ref(),
            None,
            policy_mask_digest_seed,
        );

        let mut generated_tokens = free_token_ids.clone();
        let free_token_offset = generated_tokens.len();
        let mut router_decisions_collected = Vec::new();
        let mut router_decision_chain = Vec::new();
        let mut previous_chain_hash: Option<String> = None;
        let mut placement_trace: Vec<PlacementTraceEntry> = Vec::new();
        let mut run_receipt: Option<RunReceipt> = None;

        // Initialize stop controller (PRD: Hard Deterministic Stop Controller)
        let stop_sequences_tokens = match request.stop_policy.as_ref() {
            Some(policy) => {
                let mut sequences = Vec::new();
                for sequence in &policy.stop_sequences {
                    if sequence.is_empty() {
                        continue;
                    }
                    let tokens = self.tokenizer.encode(sequence)?;
                    if tokens.is_empty() {
                        continue;
                    }
                    sequences.push(tokens);
                }
                sequences
            }
            None => Vec::new(),
        };
        let mut stop_controller = StopController::from_policy_or_default_with_stop_sequences(
            request.stop_policy.clone(),
            request.max_tokens as u32,
            stop_sequences_tokens,
        );
        stop_controller.preload_tokens(&generated_tokens);
        let stop_policy_digest = *stop_controller.policy_digest();
        let mut stop_reason_code = None;
        let mut stop_reason_token_index = None;

        // ============================================================================
        // TEXT GENERATION BACKEND DETECTION
        // ============================================================================
        // Some backends (e.g., MLXSubprocessBridge) only support bulk text generation,
        // not token-by-token inference with logits. Detect and handle them via the
        // FusedKernels::supports_streaming_text_generation() method.

        let (backend_supports_text_generation, device_name) = {
            let kernels = self.kernels.lock().await;
            (
                kernels.supports_streaming_text_generation(),
                kernels.device_name().to_string(),
            )
        };

        let prompt_chars = request.prompt.chars().count();
        let prompt_digest_b3 = B3Hash::hash(request.prompt.as_bytes()).to_hex();
        let mut abstain_queued = false;
        self.router.set_abstain_context(AbstainContext {
            request_id: Some(trace_id.clone()),
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            prompt_digest_b3: Some(prompt_digest_b3.clone()),
            prompt_chars: Some(prompt_chars),
            prompt: Some(request.prompt.clone()),
            tenant_id: Some(request.cpid.clone()),
        });

        if backend_supports_text_generation {
            info!(
                device = %device_name,
                "Text-generation backend detected, using generate_text_complete() path"
            );

            self.check_cancelled_or_error(
                &request,
                cancel_token.as_deref(),
                generated_tokens.len(),
            )?;

            // For text-generation backends, we bypass the router-driven token loop
            // and use bulk generation instead. The backend doesn't support run_step()
            // with logits output, only full text generation.

            // Call generate_text_complete() directly via FusedKernels trait
            let prompt_for_backend = if free_token_text.is_empty() {
                request.prompt.clone()
            } else {
                format!("{}{}", request.prompt, free_token_text)
            };

            let mut generation_result: adapteros_lora_kernel_api::TextGenerationResult =
                if max_tokens_remaining == 0 {
                    adapteros_lora_kernel_api::TextGenerationResult {
                        text: free_token_text.clone(),
                        tokens_generated: 0,
                        finish_reason: "max_tokens".to_string(),
                        usage_stats: None,
                        timing_stats: None,
                        moe_info: None,
                        expert_routing: None,
                        free_tokens_delivered: free_token_offset,
                        routing_hash: None,
                    }
                } else {
                    let temperature = request.temperature.unwrap_or(0.7);
                    let top_p = request.top_p.unwrap_or(0.9);
                    let mut stream_cancelled = false;

                    let mut result = if let Some(stream_tx) = stream_tx.clone() {
                        let stream_tx = stream_tx;
                        let stream_result = {
                            let kernels = self.kernels.lock().await;
                            kernels.generate_text_stream(
                                &prompt_for_backend,
                                max_tokens_remaining,
                                temperature,
                                top_p,
                                &mut |token| {
                                    let event = WorkerStreamEvent::Token(StreamToken {
                                        text: token.text.clone(),
                                        token_id: token.token_id.map(|id| id as u32),
                                    });
                                    if stream_tx.blocking_send(event).is_err() {
                                        stream_cancelled = true;
                                        return false;
                                    }
                                    true
                                },
                            )
                        };

                        match stream_result {
                            Ok(result) => result,
                            Err(e) => {
                                warn!(error = %e, "Streaming text generation failed; falling back to complete");
                                let kernels = self.kernels.lock().await;
                                kernels.generate_text_complete(
                                    &prompt_for_backend,
                                    max_tokens_remaining,
                                    temperature,
                                    top_p,
                                )?
                            }
                        }
                    } else {
                        let kernels = self.kernels.lock().await;
                        kernels.generate_text_complete(
                            &prompt_for_backend,
                            max_tokens_remaining,
                            temperature,
                            top_p,
                        )?
                    };

                    if stream_cancelled {
                        return Err(AosError::Worker("Stream cancelled".to_string()));
                    }

                    result.free_tokens_delivered = free_token_offset;
                    if !free_token_text.is_empty() {
                        result.text = format!("{}{}", free_token_text, result.text);
                    }
                    if let Some(stats) = result.usage_stats.as_mut() {
                        stats.completion_tokens = stats
                            .completion_tokens
                            .saturating_add(result.free_tokens_delivered);
                        stats.total_tokens = stats
                            .total_tokens
                            .saturating_add(result.free_tokens_delivered);
                    }
                    result
                };

            let output_token_ids = self.tokenizer.encode(&generation_result.text)?;
            let logical_prompt_tokens: u32 = prompt_tokens.len().try_into().unwrap_or(u32::MAX);
            let logical_output_tokens: u32 = output_token_ids.len().try_into().unwrap_or(u32::MAX);
            // PRD-01: Use prefix_cached_token_count from PrefixKvCache lookup
            let billed_input_tokens =
                logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
            let billed_output_tokens = logical_output_tokens;
            let token_usage = TokenUsage {
                prompt_tokens: logical_prompt_tokens,
                completion_tokens: logical_output_tokens,
                billed_input_tokens,
                billed_output_tokens,
            };
            generation_result.tokens_generated = output_token_ids.len();
            if max_tokens_remaining == 0 {
                stop_reason_code = Some(adapteros_api_types::inference::StopReasonCode::BudgetMax);
                stop_reason_token_index = Some(generation_result.tokens_generated as u32);
            }

            self.check_cancelled_or_error(
                &request,
                cancel_token.as_deref(),
                generation_result.tokens_generated as usize,
            )?;

            // Update MoE prefix cache with newly observed routing data
            #[cfg(feature = "mlx-bridge")]
            if is_moe {
                if let Some(ref routing) = generation_result.expert_routing {
                    debug!(
                        token_count = routing.len(),
                        "Updating MoE prefix cache with expert routing data"
                    );
                    let num_layers = self.kernels.lock().await.num_experts();
                    self.moe_prefix_cache.upsert_routing(
                        &prompt_tokens,
                        routing.clone(),
                        &self.tenant_namespace,
                        num_layers,
                        None,
                    );
                }
            }

            // Build simplified response for text-generation mode
            let fallback_triggered = {
                let kernels = self.kernels.lock().await;
                kernels.fallback_triggered()
            };
            let backend_used = self.backend_used_for_response(fallback_triggered);
            let backend_raw = self.backend_raw_for_response(fallback_triggered);
            let backend_version = adapteros_core::version::VERSION.to_string();
            let (coreml_runtime, fallback_backend) =
                self.runtime_metadata_for_response(fallback_triggered);

            let moe_info = if let Some(info) = generation_result.moe_info.clone() {
                Some(info)
            } else {
                self.current_moe_info(is_moe).await
            };

            // Build simplified trace (no router decisions for text-gen mode)
            let trace = self.build_trace(
                &request.cpid,
                &evidence,
                generation_result.tokens_generated,
                None, // No router decisions for text-gen
                None, // No router decision chain
                fusion_interval,
                &active_ids,
                base_only_request,
            );

            // ============================================================================
            // DETERMINISM ENFORCEMENT (Rectify: Enforce Deterministic Routing)
            // ============================================================================
            if strict_mode_active {
                // Verify that a deterministic routing hash was produced
                if let Some(hash) = generation_result.routing_hash {
                    debug!(
                        routing_hash = %hash,
                        "Deterministic routing hash verified"
                    );
                    // Note: Strict enforcement against a reference hash requires passing
                    // expected_routing_hash in the request, which is a future protocol extension.
                    // For now, the existence of the hash proves the chain was computed deterministically.
                } else if moe_info.is_some() {
                    // MoE model but no routing hash -> violation
                    return Err(AosError::DeterminismViolation(
                        "Strict mode requires deterministic routing hash for MoE models"
                            .to_string(),
                    ));
                }
            }

            // Phase 3: Finalize ReceiptGenerator FIRST to get crypto receipt digest for dual-write
            let crypto_receipt_digest_b3 = if let Some(gen) = receipt_generator.take() {
                let mut gen = gen;
                if let Some(stop_code) = stop_reason_code {
                    gen.set_stop_reason(&stop_code.to_string());
                }
                match gen.finalize(&output_token_ids) {
                    Ok(crypto_receipt) => Some(crypto_receipt.receipt_digest),
                    Err(e) => {
                        warn!(
                            error = %e,
                            trace_id = %trace_id,
                            "ReceiptGenerator finalization failed"
                        );
                        None
                    }
                }
            } else {
                None
            };

            let (
                tenant_kv_quota_bytes,
                tenant_kv_bytes_used,
                kv_evictions,
                kv_residency_policy_id,
                kv_quota_enforced,
            ) = self.kv_receipt_fields();

            let tokenizer_hash_b3 = Some(self.manifest.base.tokenizer_hash);
            let tokenizer_version = None;
            let tokenizer_normalization = None;
            let model_build_hash_b3 = None;
            let adapter_build_hash_b3 = None;
            let decode_algo = Some(if request.temperature.unwrap_or(1.0) <= 0.0 {
                "greedy".to_string()
            } else {
                "sampling".to_string()
            });
            let temperature_q15 = request.temperature.map(q15_from_unit);
            let top_p_q15 = request.top_p.map(q15_from_unit);
            let top_k = request.top_k.map(|v| v as u32);
            let seed_digest_b3 = request
                .request_seed
                .map(|seed| B3Hash::hash(&seed))
                .or_else(|| request.seed.map(|seed| B3Hash::hash(&seed.to_le_bytes())));
            let sampling_backend = Some(backend_used.clone());
            let thread_count = None;
            let reduction_strategy = None;
            let stop_eos_q15 = None;
            let stop_window_digest_b3 = None;
            let cache_scope = Some("global".to_string());
            let cached_prefix_digest_b3 = compute_cached_prefix_digest(
                &prompt_tokens,
                &self.manifest.base.tokenizer_hash,
                prefix_cached_token_count,
            );
            let cached_prefix_len = Some(prefix_cached_token_count);
            let cache_key_b3 = Some(prefix_kv_key);
            let (retrieval_merkle_root_b3, retrieval_order_digest_b3) =
                compute_retrieval_digests(&evidence, &request.cpid);
            let tool_call_inputs_digest_b3 = None;
            let tool_call_outputs_digest_b3 = None;
            let disclosure_level = Some("full".to_string());
            let receipt_signing_kid = None;
            let receipt_signed_at = None;

            if let Some(sink) = trace_sink.as_mut() {
                // model_cache_identity_v2_digest already computed earlier for cache lookup
                match sink
                    .finalize(TraceFinalization {
                        output_tokens: &output_token_ids,
                        logical_prompt_tokens,
                        prefix_cached_token_count,
                        billed_input_tokens,
                        logical_output_tokens,
                        billed_output_tokens,
                        stop_reason_code: stop_reason_code.map(|c| c.to_string()),
                        stop_reason_token_index,
                        stop_policy_digest_b3: Some(stop_policy_digest),
                        tenant_kv_quota_bytes,
                        tenant_kv_bytes_used,
                        kv_evictions,
                        kv_residency_policy_id: kv_residency_policy_id.clone(),
                        kv_quota_enforced,
                        // PRD-01: Wired from PrefixKvCache lookup
                        prefix_kv_key_b3: Some(prefix_kv_key),
                        prefix_cache_hit,
                        prefix_kv_bytes,
                        model_cache_identity_v2_digest_b3: Some(model_cache_identity_v2_digest),
                        attestation: None,
                        // Patent 3535886.0002: Equipment profile from worker initialization
                        equipment_profile: self.equipment_profile.clone(),
                        // Phase 3: Crypto receipt dual-write for parity validation
                        crypto_receipt_digest_b3,
                        receipt_parity_verified: None, // Computed post-hoc by comparing stored values
                        tenant_id: Some(request.cpid.clone()),
                        // P0-1: Cache attestation for billing fraud prevention
                        cache_attestation: None,
                        worker_public_key: None,
                        copy_bytes: None,
                        tokenizer_hash_b3,
                        tokenizer_version: tokenizer_version.clone(),
                        tokenizer_normalization: tokenizer_normalization.clone(),
                        model_build_hash_b3,
                        adapter_build_hash_b3,
                        decode_algo: decode_algo.clone(),
                        temperature_q15,
                        top_p_q15,
                        top_k,
                        seed_digest_b3,
                        sampling_backend: sampling_backend.clone(),
                        thread_count,
                        reduction_strategy: reduction_strategy.clone(),
                        stop_eos_q15,
                        stop_window_digest_b3,
                        cache_scope: cache_scope.clone(),
                        cached_prefix_digest_b3,
                        cached_prefix_len,
                        cache_key_b3,
                        retrieval_merkle_root_b3,
                        retrieval_order_digest_b3,
                        tool_call_inputs_digest_b3,
                        tool_call_outputs_digest_b3,
                        disclosure_level: disclosure_level.clone(),
                        receipt_signing_kid: receipt_signing_kid.clone(),
                        receipt_signed_at: receipt_signed_at.clone(),
                    })
                    .await
                {
                    Ok(receipt) => {
                        // Phase 3: Validate parity between crypto and legacy receipt digests
                        if let Some(crypto_digest) = crypto_receipt_digest_b3 {
                            if crypto_digest != receipt.receipt_digest {
                                warn!(
                                    crypto = %crypto_digest.to_hex(),
                                    legacy = %receipt.receipt_digest.to_hex(),
                                    trace_id = %trace_id,
                                    "Receipt digest mismatch between ReceiptGenerator and SqlTraceSink"
                                );
                            } else {
                                debug!(
                                    trace_id = %trace_id,
                                    "ReceiptGenerator parity check passed"
                                );
                            }
                        }

                        run_receipt = Some(RunReceipt {
                            trace_id: receipt.trace_id.clone(),
                            run_head_hash: receipt.run_head_hash,
                            output_digest: receipt.output_digest,
                            receipt_digest: receipt.receipt_digest,
                            signature: receipt
                                .signature
                                .as_ref()
                                .map(|s| base64::engine::general_purpose::STANDARD.encode(s)),
                            attestation: receipt
                                .attestation
                                .as_ref()
                                .map(|s| base64::engine::general_purpose::STANDARD.encode(s)),
                            logical_prompt_tokens,
                            prefix_cached_token_count,
                            billed_input_tokens,
                            logical_output_tokens,
                            billed_output_tokens,
                            stop_reason_code,
                            stop_reason_token_index,
                            stop_policy_digest_b3: Some(stop_policy_digest),
                            tenant_kv_quota_bytes,
                            tenant_kv_bytes_used,
                            kv_evictions,
                            kv_residency_policy_id: kv_residency_policy_id.clone(),
                            kv_quota_enforced,
                            // PRD-01: Wired from PrefixKvCache lookup
                            prefix_kv_key_b3: Some(prefix_kv_key),
                            prefix_cache_hit,
                            prefix_kv_bytes,
                            model_cache_identity_v2_digest_b3: receipt
                                .model_cache_identity_v2_digest_b3,
                            // V6 cross-run lineage (not tracked yet)
                            previous_receipt_digest: None,
                            session_sequence: 0,
                            tokenizer_hash_b3: tokenizer_hash_b3.map(|h| h),
                            tokenizer_version: tokenizer_version.clone(),
                            tokenizer_normalization: tokenizer_normalization.clone(),
                            model_build_hash_b3: model_build_hash_b3.map(|h| h),
                            adapter_build_hash_b3: adapter_build_hash_b3.map(|h| h),
                            decode_algo: decode_algo.clone(),
                            temperature_q15,
                            top_p_q15,
                            top_k,
                            seed_digest_b3: seed_digest_b3.map(|h| h),
                            sampling_backend: sampling_backend.clone(),
                            thread_count,
                            reduction_strategy: reduction_strategy.clone(),
                            stop_eos_q15,
                            stop_window_digest_b3,
                            cache_scope: cache_scope.clone(),
                            cached_prefix_digest_b3: cached_prefix_digest_b3.map(|h| h),
                            cached_prefix_len,
                            cache_key_b3: cache_key_b3.map(|h| h),
                            retrieval_merkle_root_b3: retrieval_merkle_root_b3.map(|h| h),
                            retrieval_order_digest_b3: retrieval_order_digest_b3.map(|h| h),
                            tool_call_inputs_digest_b3,
                            tool_call_outputs_digest_b3,
                            disclosure_level: disclosure_level.clone(),
                            receipt_signing_kid: receipt_signing_kid.clone(),
                            receipt_signed_at: receipt_signed_at.clone(),
                        });
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            info!(
                tokens = generation_result.tokens_generated,
                finish_reason = %generation_result.finish_reason,
                "Text generation completed"
            );

            // Clean up per-request abstain context; text-gen backends don't emit abstain events.
            self.router.clear_abstain_context();

            return Ok(InferenceResponse {
                text: Some(generation_result.text),
                status: "ok".to_string(),
                trace,
                run_receipt,
                token_usage: Some(token_usage),
                refusal: None,
                patch_proposal: None,
                stack_id: request.stack_id.clone(),
                stack_version: request.stack_version,
                backend_used: Some(backend_used),
                backend_raw: Some(backend_raw),
                backend_version: Some(backend_version),
                fallback_triggered,
                coreml_compute_preference: coreml_runtime
                    .as_ref()
                    .and_then(|r| r.compute_preference.clone()),
                coreml_compute_units: coreml_runtime
                    .as_ref()
                    .and_then(|r| r.compute_units.clone()),
                coreml_gpu_used: coreml_runtime.as_ref().and_then(|r| r.gpu_used),
                coreml_package_hash: self.coreml_package_hash.clone(),
                coreml_expected_package_hash: self
                    .coreml_verification
                    .as_ref()
                    .and_then(|v| v.expected.clone()),
                coreml_hash_mismatch: self.coreml_verification.as_ref().map(|v| v.mismatch),
                fallback_backend,
                determinism_mode_applied: Some(request.determinism_mode.clone()),
                unavailable_pinned_adapters,
                pinned_routing_fallback,
                placement_trace: None, // No placement for text-gen mode
                stop_reason_code,
                stop_reason_token_index: stop_reason_token_index
                    .or(Some(generation_result.tokens_generated as u32)),
                stop_policy_digest_b3: Some(stop_policy_digest.to_hex()),
                error_details: None,
            });
        }

        // ============================================================================
        // STANDARD TOKEN-BY-TOKEN GENERATION LOOP (for FusedKernels backends)
        // ============================================================================

        // Autoregressive generation loop
        if max_tokens_remaining == 0 {
            stop_reason_code = Some(adapteros_api_types::inference::StopReasonCode::BudgetMax);
            stop_reason_token_index = Some(generated_tokens.len() as u32);
        }

        self.check_cancelled_or_error(&request, cancel_token.as_deref(), generated_tokens.len())?;

        let reasoning_mode = request.reasoning_mode;
        let mut reasoning_buffer = String::new();
        let mut pending_reasoning_decision: Option<adapteros_lora_router::Decision> = None;
        let mut pending_hotswap: Option<u16> = None;
        let mut reasoning_swap_guard = ReasoningSwapGuard::new(MAX_REASONING_SWAPS);

        // Review trigger detector for human-in-the-loop pause/resume
        let mut review_detector = review_trigger::ReviewTriggerDetector::new(
            review_trigger::ReviewTriggerConfig::default(),
        );

        for step in 0..max_tokens_remaining {
            let step_with_free = free_token_offset + step;
            self.check_cancelled_or_error(
                &request,
                cancel_token.as_deref(),
                generated_tokens.len(),
            )?;
            // Prepare input for this step
            let input_ids_slice = if step == 0 {
                &prompt_tokens_with_free[..]
            } else {
                let last_token = generated_tokens.last().ok_or_else(|| {
                    AosError::Internal("Generated tokens cannot be empty".to_string())
                })?;
                std::slice::from_ref(last_token)
            };

            if let Some(adapter_id) = pending_hotswap.take() {
                let mut kernels = self.kernels.lock().await;
                if let Err(e) = kernels.switch_adapter(adapter_id) {
                    warn!(adapter_id, error = %e, "Hot-swap switch_adapter failed");
                }
            }

            // Run router to get active adapters
            // Extract features from the current prompt context for adaptive routing
            let mut decision_from_reasoning = pending_reasoning_decision.is_some();
            let decision = if let Some(pending) = pending_reasoning_decision.take() {
                pending
            } else {
                decision_from_reasoning = false;
                let features = if step == 0 {
                    // For the first step, use the prompt plus any free tokens for feature extraction
                    let mut context = request.prompt.clone();
                    if !free_token_text.is_empty() {
                        context.push_str(&free_token_text);
                    }
                    CodeFeatures::from_context(&context).to_vector()
                } else {
                    // For subsequent steps, use the current token context
                    // Decode recent tokens to get meaningful context for routing
                    let context_tokens =
                        &generated_tokens[generated_tokens.len().saturating_sub(10)..];
                    let context_text = self
                        .tokenizer
                        .decode(context_tokens)
                        .unwrap_or_else(|_| "".to_string());
                    CodeFeatures::from_context(&context_text).to_vector()
                };
                // Build priors with PINNED_BOOST for pinned adapters (CHAT-PIN-02)
                self.router.route_with_adapter_info_with_ctx(
                    &features,
                    &priors,
                    &adapter_info,
                    &policy_mask,
                    determinism_ctx.as_ref(),
                )?
            };

            let decision = self.apply_routing_policy_to_decision(
                decision,
                request.routing_policy.as_ref(),
                base_only_request,
            )?;

            // Capture abstain events once per request to feed the active-learning queue.
            if !abstain_queued {
                let abstain_events = self.router.take_abstain_events();
                if !abstain_events.is_empty() {
                    for event in abstain_events {
                        if let Err(e) =
                            active_learning::enqueue_abstain_sample(&event, Some(&request.prompt))
                        {
                            warn!(
                                error = %e,
                                "Failed to enqueue abstain sample for active learning"
                            );
                        }
                    }
                    abstain_queued = true;
                }
            }

            // Collect router decision for control plane transmission
            let input_token_id = if step == 0 {
                prompt_tokens_with_free.first().copied()
            } else {
                generated_tokens.last().copied()
            };
            router_decisions_collected.push(adapteros_api_types::inference::RouterDecision {
                step: step_with_free,
                input_token_id,
                candidate_adapters: decision
                    .candidates
                    .iter()
                    .map(|c| adapteros_api_types::inference::RouterCandidate {
                        adapter_idx: c.adapter_idx,
                        raw_score: c.raw_score,
                        gate_q15: c.gate_q15,
                    })
                    .collect(),
                entropy: decision.entropy,
                tau: self.router.tau(),
                entropy_floor: self.router.eps(),
                stack_hash: self.router.stack_hash(),
                allowed_mask: Some(policy_mask.allowed.clone()),
                interval_id: Some(fusion_interval.interval_id_for_step(step_with_free)),
                policy_mask_digest_b3: decision.policy_mask_digest_b3,
                policy_overrides_applied: decision.policy_overrides_applied.as_ref().map(|flags| {
                    adapteros_api_types::inference::PolicyOverrideFlags {
                        allow_list: flags.allow_list,
                        deny_list: flags.deny_list,
                        trust_state: flags.trust_state,
                    }
                }),
                model_type: RouterModelType::Dense,
                backend_type: None, // Populated at inference finalization (PRD-DET-001: G6)
            });

            // Build chained router decision entry (per-token)
            let adapter_ids_for_decision: Vec<String> = decision
                .indices
                .iter()
                .filter_map(|idx| {
                    self.manifest
                        .adapters
                        .get(*idx as usize)
                        .map(|a| a.id.clone())
                })
                .collect();
            let adapter_ids_for_trace = adapter_ids_for_decision.clone();

            let decision_hash_payload =
                decision.decision_hash.as_ref().map(|h| RouterDecisionHash {
                    input_hash: h.input_hash.clone(),
                    output_hash: h.output_hash.clone(),
                    reasoning_hash: h.reasoning_hash.clone(),
                    combined_hash: h.combined_hash.clone(),
                    tau: h.tau,
                    eps: h.eps,
                    k: h.k,
                });

            let indices_joined = decision
                .indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let gates_joined = decision
                .gates_q15
                .iter()
                .map(|g| g.to_string())
                .collect::<Vec<_>>()
                .join(",");

            let entry_material = format!(
                "{}|{}|{}|{}|{}|{}",
                step_with_free,
                input_token_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(String::new),
                indices_joined,
                gates_joined,
                decision_hash_payload
                    .as_ref()
                    .map(|h| h.combined_hash.as_str())
                    .unwrap_or(""),
                previous_chain_hash.as_deref().unwrap_or("")
            );

            let entry_hash = B3Hash::hash(entry_material.as_bytes()).to_hex();

            router_decision_chain.push(RouterDecisionChainEntry {
                step: step_with_free,
                input_token_id,
                adapter_indices: decision.indices.iter().copied().collect(),
                adapter_ids: adapter_ids_for_decision,
                gates_q15: decision.gates_q15.iter().copied().collect(),
                entropy: decision.entropy,
                decision_hash: decision_hash_payload,
                previous_hash: previous_chain_hash.clone(),
                entry_hash: entry_hash.clone(),
                policy_mask_digest_b3: decision.policy_mask_digest_b3,
                policy_overrides_applied: decision.policy_overrides_applied.as_ref().map(|flags| {
                    adapteros_api_types::inference::PolicyOverrideFlags {
                        allow_list: flags.allow_list,
                        deny_list: flags.deny_list,
                        trust_state: flags.trust_state,
                    }
                }),
            });

            previous_chain_hash = Some(entry_hash);

            // Record routing decision in profiler
            self.profiler.record_routing_decision(&decision.indices);
            {
                let lifecycle = self.lifecycle.lock().await;
                lifecycle.record_router_decision(&decision.indices).await?;

                // Enforce effective_adapter_ids gate: disallow adapters outside allowed set
                if let Some(ref allowed) = allowed_active_indices {
                    for &adapter_idx in &decision.indices {
                        if !allowed.contains(&(adapter_idx as usize)) {
                            let manifest_idx = active_manifest_indices
                                .get(adapter_idx as usize)
                                .copied()
                                .unwrap_or_default();
                            let err = AosError::AdapterNotInEffectiveSet {
                                adapter_id: self
                                    .manifest
                                    .adapters
                                    .get(manifest_idx)
                                    .map(|a| a.id.clone())
                                    .unwrap_or_else(|| format!("adapter_{}", adapter_idx)),
                                effective_set: request
                                    .effective_adapter_ids
                                    .clone()
                                    .unwrap_or_default(),
                            };
                            return Err(err);
                        }
                    }
                }

                // Validate all selected adapters are in a ready state (warm, hot, or resident)
                for &adapter_idx in &decision.indices {
                    if let Some(state) = lifecycle.get_state(adapter_idx) {
                        if !state.is_available() {
                            let adapter_id = self
                                .manifest
                                .adapters
                                .get(adapter_idx as usize)
                                .map(|a| a.id.clone())
                                .unwrap_or_else(|| format!("adapter_{}", adapter_idx));
                            let err = AosError::AdapterNotLoaded {
                                adapter_id,
                                current_state: state.to_string(),
                            };
                            return Err(err);
                        }
                    }
                }
            }

            if let Some(sink) = trace_sink.as_mut() {
                let kernel_version_for_trace = if decision_from_reasoning {
                    format!("{}|thought_swap", kernel_version_id)
                } else {
                    kernel_version_id.clone()
                };
                let token_input = TraceTokenInput {
                    token_index: step_with_free as u32,
                    adapter_ids: adapter_ids_for_trace.clone(),
                    gates_q15: decision.gates_q15.iter().copied().collect(),
                    policy_mask_digest_b3,
                    allowed_mask: Some(policy_mask.allowed.clone()),
                    policy_overrides_applied: decision.policy_overrides_applied.as_ref().map(
                        |flags| adapteros_api_types::inference::PolicyOverrideFlags {
                            allow_list: flags.allow_list,
                            deny_list: flags.deny_list,
                            trust_state: flags.trust_state,
                        },
                    ),
                    backend_id: Some(backend_label.clone()),
                    kernel_version_id: Some(kernel_version_for_trace.clone()),
                };
                sink.record_token(token_input).await?;

                // Record to ReceiptGenerator for parity validation
                if let Some(ref mut gen) = receipt_generator {
                    gen.record_routing_step_full(
                        step_with_free as u32,
                        input_token_id,
                        decision.indices.iter().copied().collect(),
                        adapter_ids_for_trace.clone(),
                        decision.gates_q15.iter().copied().collect(),
                        decision.entropy,
                        policy_mask_digest_b3.map(B3Hash::from_bytes),
                        Some(backend_label.clone()),
                        Some(kernel_version_for_trace),
                        Some(policy_mask.allowed.clone()),
                    );
                }
            }

            // Convert Decision to RouterRing
            let router_ring = decision_to_router_ring_with_active_ids_and_strengths(
                &decision,
                &active_hashed,
                Some(&active_strengths),
                step_with_free,
            )?;

            // Execute kernels through Metal and measure latency per adapter
            let mut io_buffers = IoBuffers {
                input_ids: input_ids_slice.to_vec(),
                output_logits: vec![0.0; self.manifest.base.vocab_size as usize],
                position: step_with_free,
                attention_entropy: None,
                activations: None,
            };

            if let Some(ref mut placement) = placement_state {
                if !request.strict_mode {
                    if let Some(decision) = placement.decide() {
                        {
                            let mut kernels = self.kernels.lock().await;
                            kernels.set_active_lane(decision.lane);
                        }
                        placement_trace.push(PlacementTraceEntry {
                            step,
                            lane: decision.lane_name.clone(),
                            score: decision.score,
                            temperature_c: decision.temperature_c,
                            utilization: decision.utilization,
                        });
                        if let Some(t) = &self.telemetry {
                            let _ = t.log(
                                "placement.step",
                                serde_json::json!({
                                    "step": step,
                                    "lane": decision.lane_name,
                                    "score": decision.score,
                                    "utilization": decision.utilization,
                                    "temperature_c": decision.temperature_c,
                                }),
                            );
                        }
                    }
                }
            }

            let kernel_start = Instant::now();
            {
                let mut kernels = self.kernels.lock().await;
                kernels.run_step(&router_ring, &mut io_buffers)?;
            }
            let kernel_duration = kernel_start.elapsed();
            // Check cancellation with receipt generation for audit trail completeness
            self.check_cancelled_with_receipt(
                &request,
                cancel_token.as_deref(),
                generated_tokens.len(),
                &generated_tokens,
                &mut trace_sink,
            )
            .await?;

            // Record latency for each active adapter (simplified: divide equally)
            if !decision.indices.is_empty() {
                let per_adapter_latency = kernel_duration / decision.indices.len() as u32;
                for &adapter_id in &decision.indices {
                    self.profiler
                        .record_step_latency(adapter_id, per_adapter_latency);
                }
            }

            // Re-seed generator for step-level determinism (enables replay)
            self.generator.reseed_for_step(step_with_free)?;

            // Sample next token
            let next_token = self.generator.next_token(&io_buffers.output_logits)?;

            // Check stopping criteria using StopController (PRD: Hard Deterministic Stop Controller)
            if let Some(decision) = stop_controller.check_stop(
                next_token,
                self.tokenizer.eos_token_id(),
                &io_buffers.output_logits,
            ) {
                stop_reason_code = Some(decision.reason);
                stop_reason_token_index = Some(decision.token_index);
                if decision.trim_tokens > 0 {
                    let trim = decision.trim_tokens.min(generated_tokens.len());
                    for _ in 0..trim {
                        generated_tokens.pop();
                    }
                }
                debug!(
                    step,
                    reason = %decision.reason,
                    token_index = decision.token_index,
                    "Stop controller triggered"
                );
                // For LENGTH stop (EOS token), don't include the EOS in output.
                // For STOP_SEQUENCE, trim previously emitted tokens that form the sequence.
                // For other reasons, we've already decided to stop before appending.
                break;
            }

            generated_tokens.push(next_token);

            if let Some(ref tx) = stream_tx {
                match self.tokenizer.decode(&[next_token]) {
                    Ok(token_text) => {
                        let event = WorkerStreamEvent::Token(StreamToken {
                            text: token_text,
                            token_id: Some(next_token),
                        });
                        if tx.send(event).await.is_err() {
                            return Err(AosError::Worker("Stream cancelled".to_string()));
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to decode streaming token");
                    }
                }
            }

            // Review trigger detection - pause for human review if triggered
            if let Some(ref registry) = self.pause_registry {
                if let Ok(token_text) = self.tokenizer.decode(&[next_token]) {
                    if let Some(trigger) = review_detector.on_token(&token_text) {
                        info!(
                            kind = ?trigger.kind,
                            token_index = trigger.token_index,
                            confidence = trigger.confidence,
                            "Review trigger detected, pausing for human review"
                        );

                        let (pause_token, ctx) =
                            review_detector.create_pause_token(&trigger, &request.cpid);
                        let pause_id = pause_token.pause_id.clone();
                        let resume_rx = registry.register(pause_token)?;

                        // Emit Paused event via stream to notify server
                        if let Some(ref tx) = stream_tx {
                            let text_so_far = self.tokenizer.decode(&generated_tokens).ok();
                            let paused_event = WorkerStreamEvent::Paused {
                                pause_id: pause_id.clone(),
                                inference_id: request.cpid.clone(),
                                trigger_kind: format!("{:?}", trigger.kind),
                                context: ctx.question.clone(),
                                text_so_far,
                                token_count: generated_tokens.len(),
                            };
                            if tx.send(paused_event).await.is_err() {
                                warn!("Failed to send Paused event to stream");
                            }
                        }

                        // Block until human submits review via UDS
                        info!(pause_id = %pause_id, "Waiting for human review...");
                        match resume_rx.await {
                            Ok(_review) => {
                                info!(pause_id = %pause_id, "Review submitted, resuming inference");
                            }
                            Err(_) => {
                                return Err(AosError::Worker(
                                    "Review channel closed before response".to_string(),
                                ));
                            }
                        }
                    }
                }
            }

            if reasoning_mode {
                if let Ok(decoded) = self.tokenizer.decode(&[next_token]) {
                    reasoning_buffer.push_str(&decoded);
                }

                let has_end_tag = reasoning_buffer.contains("</thinking>");
                let has_newline = reasoning_buffer.ends_with('\n');
                if (has_end_tag || has_newline) && !reasoning_buffer.trim().is_empty() {
                    let rationale = if let (Some(start), Some(end)) = (
                        reasoning_buffer.find("<thinking>"),
                        reasoning_buffer.find("</thinking>"),
                    ) {
                        let slice_start = start + "<thinking>".len();
                        reasoning_buffer[slice_start..end].trim().to_string()
                    } else {
                        reasoning_buffer.trim().to_string()
                    };
                    reasoning_buffer.clear();

                    let mut swap_decision = self.router.route_on_reasoning(
                        &rationale,
                        &priors,
                        &adapter_info,
                        &policy_mask,
                        determinism_ctx.as_ref(),
                    )?;

                    swap_decision = self.apply_routing_policy_to_decision(
                        swap_decision,
                        request.routing_policy.as_ref(),
                        base_only_request,
                    )?;

                    if let Some(&adapter_id) = swap_decision
                        .indices
                        .first()
                        .and_then(|idx| active_hashed.get(*idx as usize))
                    {
                        pending_hotswap = Some(adapter_id);
                    }

                    pending_reasoning_decision = Some(swap_decision);
                    if let Err(e) = reasoning_swap_guard.record_swap() {
                        warn!(
                            swaps = reasoning_swap_guard.count(),
                            "Terminating request due to excessive thought swaps"
                        );
                        return Err(e);
                    }
                }
            }
        }

        // Ensure stop reason is set if loop completed without explicit stop condition.
        // This handles the case where the loop exits by exhausting max_tokens_remaining
        // iterations without the StopController triggering BUDGET_MAX (off-by-one in check).
        // PRD: Every generation MUST have a stop_reason_code for verifiable receipts.
        if stop_reason_code.is_none() {
            stop_reason_code = Some(adapteros_api_types::inference::StopReasonCode::BudgetMax);
            stop_reason_token_index = Some(generated_tokens.len() as u32);
            debug!(
                tokens_generated = generated_tokens.len(),
                "Stop reason defaulted to BUDGET_MAX (loop exhausted)"
            );
        }

        // Evaluate lifecycle transitions after inference
        {
            let lifecycle = self.lifecycle.lock().await;
            lifecycle.evaluate_transitions(&self.profiler)?;
        }

        // Log profiling snapshot (sampled at 5%)
        self.profiler.maybe_log_snapshot()?;

        // Decode to text
        let generated_text = self.tokenizer.decode(&generated_tokens)?;

        let include_attestation = trace_sink.is_some();
        let (backend_used, backend_raw, fallback_triggered, determinism_attestation) = {
            let kernels = self.kernels.lock().await;
            let fallback_triggered = kernels.fallback_triggered();
            let attestation = if include_attestation {
                Some(encode_determinism_attestation(
                    &kernels.attest_determinism()?,
                )?)
            } else {
                None
            };
            let backend_used = self.backend_used_for_response(fallback_triggered);
            let backend_raw = self.backend_raw_for_response(fallback_triggered);
            (backend_used, backend_raw, fallback_triggered, attestation)
        };
        let backend_version = adapteros_core::version::VERSION.to_string();
        let (coreml_runtime, fallback_backend) =
            self.runtime_metadata_for_response(fallback_triggered);

        if strict_mode_active && fallback_triggered {
            let fallback_kind = self.available_backends.fallback;
            let fallback_level = fallback_kind
                .map(declared_determinism_level)
                .unwrap_or(DeterminismLevel::None);
            if is_determinism_downgrade(baseline_level, fallback_level) {
                emit_observability_event(&determinism_violation_event(
                    DeterminismViolationKind::Unknown,
                    None,
                    Some(self.manifest.base.model_hash.to_hex()),
                    None,
                    true,
                    Some(request.cpid.clone()),
                    request.request_id.clone(),
                ));
                return Err(AosError::DeterminismViolation(format!(
                    "Strict determinism mode forbids fallback from {} ({}) to {} ({})",
                    baseline_backend.as_str(),
                    baseline_level,
                    fallback_kind.map(|b| b.as_str()).unwrap_or("unknown"),
                    fallback_level
                )));
            }
        }

        let logical_prompt_tokens: u32 = prompt_tokens.len().try_into().unwrap_or(u32::MAX);
        let logical_output_tokens: u32 = generated_tokens.len().try_into().unwrap_or(u32::MAX);
        // PRD-01: Use prefix_cached_token_count from PrefixKvCache lookup (computed earlier in generate())
        let billed_input_tokens = logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
        let billed_output_tokens = logical_output_tokens;
        let token_usage = TokenUsage {
            prompt_tokens: logical_prompt_tokens,
            completion_tokens: logical_output_tokens,
            billed_input_tokens,
            billed_output_tokens,
        };

        // PRD-06: model_cache_identity_v2_digest already computed earlier for cache lookup

        // Phase 3: Finalize ReceiptGenerator FIRST to get crypto receipt digest for dual-write
        let crypto_receipt_digest_b3 = if let Some(gen) = receipt_generator.take() {
            let mut gen = gen;
            if let Some(stop_code) = stop_reason_code {
                gen.set_stop_reason(&stop_code.to_string());
            }
            match gen.finalize(&generated_tokens) {
                Ok(crypto_receipt) => Some(crypto_receipt.receipt_digest),
                Err(e) => {
                    warn!(
                        error = %e,
                        trace_id = %trace_id,
                        "ReceiptGenerator finalization failed"
                    );
                    None
                }
            }
        } else {
            None
        };

        let (
            tenant_kv_quota_bytes,
            tenant_kv_bytes_used,
            kv_evictions,
            kv_residency_policy_id,
            kv_quota_enforced,
        ) = self.kv_receipt_fields();

        let tokenizer_hash_b3 = Some(self.manifest.base.tokenizer_hash);
        let tokenizer_version = None;
        let tokenizer_normalization = None;
        let model_build_hash_b3 = None;
        let adapter_build_hash_b3 = None;
        let decode_algo = Some(if request.temperature.unwrap_or(1.0) <= 0.0 {
            "greedy".to_string()
        } else {
            "sampling".to_string()
        });
        let temperature_q15 = request.temperature.map(q15_from_unit);
        let top_p_q15 = request.top_p.map(q15_from_unit);
        let top_k = request.top_k.map(|v| v as u32);
        let seed_digest_b3 = request
            .request_seed
            .map(|seed| B3Hash::hash(&seed))
            .or_else(|| request.seed.map(|seed| B3Hash::hash(&seed.to_le_bytes())));
        let sampling_backend = Some(backend_used.clone());
        let thread_count = None;
        let reduction_strategy = None;
        let stop_eos_q15 = None;
        let stop_window_digest_b3 = None;
        let cache_scope = Some("global".to_string());
        let cached_prefix_digest_b3 = compute_cached_prefix_digest(
            &prompt_tokens,
            &self.manifest.base.tokenizer_hash,
            prefix_cached_token_count,
        );
        let cached_prefix_len = Some(prefix_cached_token_count);
        let cache_key_b3 = Some(prefix_kv_key);
        let (retrieval_merkle_root_b3, retrieval_order_digest_b3) =
            compute_retrieval_digests(&evidence, &request.cpid);
        let tool_call_inputs_digest_b3 = None;
        let tool_call_outputs_digest_b3 = None;
        let disclosure_level = Some("full".to_string());
        let receipt_signing_kid = None;
        let receipt_signed_at = None;

        if let Some(sink) = trace_sink.as_mut() {
            match sink
                .finalize(TraceFinalization {
                    output_tokens: &generated_tokens,
                    logical_prompt_tokens,
                    prefix_cached_token_count,
                    billed_input_tokens,
                    logical_output_tokens,
                    billed_output_tokens,
                    stop_reason_code: stop_reason_code.map(|c| c.to_string()),
                    stop_reason_token_index,
                    stop_policy_digest_b3: Some(stop_policy_digest),
                    tenant_kv_quota_bytes,
                    tenant_kv_bytes_used,
                    kv_evictions,
                    kv_residency_policy_id: kv_residency_policy_id.clone(),
                    kv_quota_enforced,
                    // PRD-01: Wired from PrefixKvCache lookup
                    prefix_kv_key_b3: Some(prefix_kv_key),
                    prefix_cache_hit,
                    prefix_kv_bytes,
                    // PRD-06: Model cache identity v2 digest
                    model_cache_identity_v2_digest_b3: Some(model_cache_identity_v2_digest),
                    attestation: determinism_attestation.clone(),
                    // Patent 3535886.0002: Equipment profile from worker initialization
                    equipment_profile: self.equipment_profile.clone(),
                    // Phase 3: Crypto receipt dual-write for parity validation
                    crypto_receipt_digest_b3,
                    receipt_parity_verified: None, // Computed post-hoc by comparing stored values
                    tenant_id: Some(request.cpid.clone()),
                    // P0-1: Cache attestation for billing fraud prevention
                    cache_attestation: None,
                    worker_public_key: None,
                    copy_bytes: None,
                    tokenizer_hash_b3,
                    tokenizer_version: tokenizer_version.clone(),
                    tokenizer_normalization: tokenizer_normalization.clone(),
                    model_build_hash_b3,
                    adapter_build_hash_b3,
                    decode_algo: decode_algo.clone(),
                    temperature_q15,
                    top_p_q15,
                    top_k,
                    seed_digest_b3,
                    sampling_backend: sampling_backend.clone(),
                    thread_count,
                    reduction_strategy: reduction_strategy.clone(),
                    stop_eos_q15,
                    stop_window_digest_b3,
                    cache_scope: cache_scope.clone(),
                    cached_prefix_digest_b3,
                    cached_prefix_len,
                    cache_key_b3,
                    retrieval_merkle_root_b3,
                    retrieval_order_digest_b3,
                    tool_call_inputs_digest_b3,
                    tool_call_outputs_digest_b3,
                    disclosure_level: disclosure_level.clone(),
                    receipt_signing_kid: receipt_signing_kid.clone(),
                    receipt_signed_at: receipt_signed_at.clone(),
                })
                .await
            {
                Ok(receipt) => {
                    // Phase 3: Validate parity between crypto and legacy receipt digests
                    if let Some(crypto_digest) = crypto_receipt_digest_b3 {
                        if crypto_digest != receipt.receipt_digest {
                            warn!(
                                crypto = %crypto_digest.to_hex(),
                                legacy = %receipt.receipt_digest.to_hex(),
                                trace_id = %trace_id,
                                "Receipt digest mismatch between ReceiptGenerator and SqlTraceSink"
                            );
                        } else {
                            debug!(
                                trace_id = %trace_id,
                                "ReceiptGenerator parity check passed"
                            );
                        }
                    }

                    run_receipt = Some(RunReceipt {
                        trace_id: receipt.trace_id.clone(),
                        run_head_hash: receipt.run_head_hash,
                        output_digest: receipt.output_digest,
                        receipt_digest: receipt.receipt_digest,
                        signature: receipt
                            .signature
                            .as_ref()
                            .map(|s| base64::engine::general_purpose::STANDARD.encode(s)),
                        attestation: receipt
                            .attestation
                            .as_ref()
                            .map(|s| base64::engine::general_purpose::STANDARD.encode(s)),
                        logical_prompt_tokens,
                        prefix_cached_token_count,
                        billed_input_tokens,
                        logical_output_tokens,
                        billed_output_tokens,
                        stop_reason_code,
                        stop_reason_token_index,
                        stop_policy_digest_b3: Some(stop_policy_digest),
                        tenant_kv_quota_bytes,
                        tenant_kv_bytes_used,
                        kv_evictions,
                        kv_residency_policy_id: kv_residency_policy_id.clone(),
                        kv_quota_enforced,
                        // PRD-01: Wired from PrefixKvCache lookup
                        prefix_kv_key_b3: Some(prefix_kv_key),
                        prefix_cache_hit,
                        prefix_kv_bytes,
                        // PRD-06: Model cache identity v2 digest (from receipt)
                        model_cache_identity_v2_digest_b3: receipt
                            .model_cache_identity_v2_digest_b3,
                        // V6 cross-run lineage (not tracked yet)
                        previous_receipt_digest: None,
                        session_sequence: 0,
                        tokenizer_hash_b3: tokenizer_hash_b3.map(|h| h),
                        tokenizer_version: tokenizer_version.clone(),
                        tokenizer_normalization: tokenizer_normalization.clone(),
                        model_build_hash_b3: model_build_hash_b3.map(|h| h),
                        adapter_build_hash_b3: adapter_build_hash_b3.map(|h| h),
                        decode_algo: decode_algo.clone(),
                        temperature_q15,
                        top_p_q15,
                        top_k,
                        seed_digest_b3: seed_digest_b3.map(|h| h),
                        sampling_backend: sampling_backend.clone(),
                        thread_count,
                        reduction_strategy: reduction_strategy.clone(),
                        stop_eos_q15,
                        stop_window_digest_b3,
                        cache_scope: cache_scope.clone(),
                        cached_prefix_digest_b3: cached_prefix_digest_b3.map(|h| h),
                        cached_prefix_len,
                        cache_key_b3: cache_key_b3.map(|h| h),
                        retrieval_merkle_root_b3: retrieval_merkle_root_b3.map(|h| h),
                        retrieval_order_digest_b3: retrieval_order_digest_b3.map(|h| h),
                        tool_call_inputs_digest_b3,
                        tool_call_outputs_digest_b3,
                        disclosure_level: disclosure_level.clone(),
                        receipt_signing_kid: receipt_signing_kid.clone(),
                        receipt_signed_at: receipt_signed_at.clone(),
                    });
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if let Some(profile) = request.backend_profile {
            let expected = profile.as_str();
            let backend_used_normalized = backend_used.to_lowercase();
            // Backend profile matching must account for normalized_id() mapping:
            // - mlx, mlxbridge → "native"
            // - coreml, metal → "accelerated"
            // - cpu → "cpu"
            let backend_matches = if expected == "cpu" {
                backend_used_normalized == expected || backend_used_normalized.contains("mock")
            } else if expected == "mlx" || expected == "mlxbridge" {
                // MLX variants normalize to "native"
                backend_used_normalized == "native"
                    || backend_used_normalized == expected
                    || backend_used_normalized.starts_with(expected)
            } else if expected == "coreml" || expected == "metal" {
                // CoreML and Metal normalize to "accelerated"
                backend_used_normalized == "accelerated"
                    || backend_used_normalized == expected
                    || backend_used_normalized.starts_with(expected)
            } else {
                // Allow match if backend name equals or starts with expected profile
                backend_used_normalized == expected || backend_used_normalized.starts_with(expected)
            };
            if !backend_matches {
                emit_observability_event(&determinism_violation_event(
                    DeterminismViolationKind::Unknown,
                    None,
                    Some(self.manifest.base.model_hash.to_hex()),
                    None,
                    true,
                    Some(request.cpid.clone()),
                    None,
                ));
                return Err(AosError::DeterminismViolation(format!(
                    "Backend profile mismatch: requested {}, got {}",
                    expected, backend_used
                )));
            }
        }

        let router_decision_chain_opt = if router_decision_chain.is_empty() {
            None
        } else {
            Some(router_decision_chain)
        };

        enforce_strict_router_chain(
            strict_mode_active,
            base_only_request,
            router_decision_chain_opt.as_deref().unwrap_or(&[]),
        )?;

        // Clear per-request abstain context before returning response.
        self.router.clear_abstain_context();

        Ok(InferenceResponse {
            text: Some(generated_text),
            status: "ok".to_string(),
            trace: self.build_trace(
                &request.cpid,
                &evidence,
                generated_tokens.len(),
                Some(router_decisions_collected),
                router_decision_chain_opt,
                fusion_interval,
                &active_ids,
                base_only_request,
            ),
            run_receipt,
            token_usage: Some(token_usage),
            refusal: None,
            patch_proposal: None,
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            backend_used: Some(backend_used),
            backend_raw: Some(backend_raw),
            backend_version: Some(backend_version),
            fallback_triggered,
            coreml_compute_preference: coreml_runtime
                .as_ref()
                .and_then(|r| r.compute_preference.clone()),
            coreml_compute_units: coreml_runtime
                .as_ref()
                .and_then(|r| r.compute_units.clone()),
            coreml_gpu_used: coreml_runtime.as_ref().and_then(|r| r.gpu_used),
            coreml_package_hash: self.coreml_package_hash.clone(),
            coreml_expected_package_hash: self
                .coreml_verification
                .as_ref()
                .and_then(|v| v.expected.clone()),
            coreml_hash_mismatch: self.coreml_verification.as_ref().map(|v| v.mismatch),
            fallback_backend,
            determinism_mode_applied: Some(request.determinism_mode.clone()),
            unavailable_pinned_adapters,
            pinned_routing_fallback,
            placement_trace: if placement_trace.is_empty() {
                None
            } else {
                Some(placement_trace)
            },
            stop_reason_code,
            stop_reason_token_index,
            stop_policy_digest_b3: Some(stop_policy_digest.to_hex()),
            error_details: None,
        })
    }

    async fn current_moe_info(&self, is_moe: bool) -> Option<adapteros_lora_kernel_api::MoEInfo> {
        if !is_moe {
            return None;
        }

        let kernels = self.kernels.lock().await;
        Some(adapteros_lora_kernel_api::MoEInfo {
            is_moe: true,
            num_experts: kernels.num_experts(),
            experts_per_token: kernels.experts_per_token(),
        })
    }

    // validate_effective_adapter_gate is now in adapter_operations.rs

    // Refactored methods are now in separate modules:
    // - patch_generation.rs: propose_patch, retrieve_evidence, build_trace, etc.
    // - adapter_operations.rs: execute_adapter_command, verify_gpu_integrity
    // - worker_utilities.rs: compute_embedding, generate_plan_id, etc.
    // - training_management.rs: training job management methods

    /// Get GPU memory report from the underlying kernel backend.
    ///
    /// Returns memory pool statistics, adapter allocations, and VRAM usage.
    /// Used by capacity handlers to expose real GPU metrics instead of hardcoded values.
    pub async fn memory_report(&self) -> Option<adapteros_lora_kernel_api::GpuMemoryReportData> {
        let kernels = self.kernels.lock().await;
        kernels.memory_report()
    }

    /// Flush trace buffers and persist state
    pub async fn flush(&self) -> Result<()> {
        if let Some(_trace_db) = &self.trace_db {
            // Note: trace_db flushes automatically or via its own task,
            // but we might want an explicit flush here if supported.
            // For now, trace persistence is async background.
        }

        // Persist MoE cache snapshot
        #[cfg(feature = "mlx-bridge")]
        {
            let adapter_paths = resolve_worker_adapter_paths();
            let adapters_path = adapter_paths.repo_root.join(&self.tenant_namespace);
            // Ensure directory exists
            if let Err(e) = std::fs::create_dir_all(&adapters_path) {
                warn!(error = %e, path = %adapters_path.display(), "Failed to create tenant directory for cache");
            }
            let moe_cache_path = adapters_path.join("moe_cache.json");
            if let Err(e) = self.moe_prefix_cache.save_snapshot(&moe_cache_path) {
                warn!(path = %moe_cache_path.display(), error = %e, "Failed to save MoE cache snapshot");
            } else {
                debug!(path = %moe_cache_path.display(), "Saved MoE cache snapshot");
            }
        }

        Ok(())
    }

    /// Gracefully shutdown background tasks and emit final telemetry.
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.shutdown_tx.send(());
        self.health_monitor.request_shutdown();

        if let Err(e) = self.flush().await {
            warn!(error = %e, "Failed to flush worker state during shutdown");
        }

        let task_timeout = Duration::from_secs(5);
        if let Some(handle) = self.persistence_handle.take() {
            Self::join_task_with_timeout("persistence", handle, task_timeout).await;
        }
        if let Some(handle) = self.retirement_handle.take() {
            Self::join_task_with_timeout("retirement", handle, task_timeout).await;
        }

        if let Some(telemetry) = self.telemetry.as_ref() {
            let active_training_jobs = self.active_training_jobs.read().len();
            if let Err(e) = telemetry.log(
                "worker.shutdown",
                serde_json::json!({
                    "tenant_id": self.tenant_namespace,
                    "worker_id": self.worker_id,
                    "active_training_jobs": active_training_jobs,
                }),
            ) {
                warn!(error = %e, "Failed to emit worker shutdown telemetry");
            }
        }

        Ok(())
    }

    async fn join_task_with_timeout(
        name: &str,
        mut handle: tokio::task::JoinHandle<()>,
        timeout: Duration,
    ) {
        tokio::select! {
            res = &mut handle => {
                if let Err(e) = res {
                    warn!(task = name, error = %e, "Shutdown task failed");
                }
            }
            _ = tokio::time::sleep(timeout) => {
                warn!(
                    task = name,
                    timeout_ms = timeout.as_millis() as u64,
                    "Shutdown task timed out; aborting"
                );
                handle.abort();
                if let Err(e) = handle.await {
                    warn!(task = name, error = %e, "Shutdown task abort failed");
                }
            }
        }
    }
}

impl<K: FusedKernels + StrictnessControl + Send + Sync + 'static> Drop for Worker<K> {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());

        if let Some(handle) = self.persistence_handle.take() {
            handle.abort();
        }

        if let Some(handle) = self.retirement_handle.take() {
            handle.abort();
        }
    }
}

/// Inference event for telemetry
#[derive(Debug, Clone, Serialize)]
pub struct InferenceEvent {
    pub duration_ms: u64,
    pub success: bool,
    pub timeout_occurred: bool,
    pub circuit_breaker_open: bool,
    pub memory_usage: u64,
    /// Time spent waiting in queue before inference starts (microseconds)
    #[serde(default)]
    pub queue_time_us: u64,
    /// Time spent in actual token generation (microseconds)
    #[serde(default)]
    pub generation_time_us: u64,
}

/// Initialize determinism guards for the worker
pub fn init_determinism_guards() -> Result<()> {
    // Initialize strict mode from environment variables
    // strict_mode::init_strict_mode();  // Temporarily disabled due to dependency issues

    // Initialize runtime guards
    // let guard_config = runtime_guards::GuardConfig {
    //     enabled: true,
    //     strict_mode: strict_mode::is_strict_mode(),
    //     max_violations: if strict_mode::is_strict_mode() { 1 } else { 10 },
    //     log_violations: true,
    // };

    // runtime_guards::init_guards(guard_config);

    info!("Determinism guards initialization temporarily disabled due to dependency issues");

    Ok(())
}

/// Check if determinism guards are enabled
pub fn determinism_guards_enabled() -> bool {
    // runtime_guards::guards_enabled()  // Temporarily disabled due to dependency issues
    false
}

/// Get current violation count
pub fn determinism_violation_count() -> u64 {
    // runtime_guards::violation_count()  // Temporarily disabled due to dependency issues
    0
}

#[cfg(test)]
mod reasoning_swap_tests;

#[cfg(test)]
mod reasoning_loop_trace_tests;

#[cfg(test)]
mod strict_mode_guard_tests {
    use super::{enforce_strict_router_chain, strict_mode_enabled};
    use adapteros_api_types::inference::RouterDecisionChainEntry;

    #[test]
    fn detects_strict_mode() {
        assert!(strict_mode_enabled(true, ""));
        assert!(strict_mode_enabled(false, "strict"));
        assert!(!strict_mode_enabled(false, "relaxed"));
    }

    #[test]
    fn strict_router_chain_requires_q15_gates() {
        let entry = RouterDecisionChainEntry {
            step: 0,
            input_token_id: Some(1),
            adapter_indices: vec![0, 1],
            adapter_ids: vec!["a".into(), "b".into()],
            gates_q15: vec![123, 456],
            entropy: 0.0,
            decision_hash: None,
            previous_hash: None,
            entry_hash: "h".into(),
            policy_mask_digest_b3: None,
            policy_overrides_applied: None,
        };

        // Happy path
        enforce_strict_router_chain(true, false, &[entry.clone()]).unwrap();

        // Missing gates should fail
        let mut missing = entry.clone();
        missing.gates_q15.clear();
        assert!(enforce_strict_router_chain(true, false, &[missing]).is_err());

        // Mismatched gate count should fail
        let mut mismatched = entry;
        mismatched.gates_q15 = vec![123];
        assert!(enforce_strict_router_chain(true, false, &[mismatched]).is_err());
    }
}

#[cfg(test)]
mod backend_normalization_tests {
    use super::normalize_backend_id;

    /// Regression test: verify backend identifier normalization returns canonical values.
    ///
    /// This ensures consistent telemetry aggregation and UI display across different
    /// backend implementations and naming variations.
    #[test]
    fn test_backend_id_normalization() {
        // MLX variants -> native
        assert_eq!(normalize_backend_id("mlx"), "native");
        assert_eq!(normalize_backend_id("mlx-bridge"), "native");
        assert_eq!(normalize_backend_id("mlx_bridge"), "native");
        assert_eq!(normalize_backend_id("mlxbridge"), "native");
        assert_eq!(normalize_backend_id("mlx-ffi"), "native");
        assert_eq!(normalize_backend_id("mlxffi"), "native");
        assert_eq!(normalize_backend_id("subprocess"), "native");

        // Hardware-accelerated backends -> accelerated
        assert_eq!(normalize_backend_id("coreml"), "accelerated");
        assert_eq!(normalize_backend_id("CoreML"), "accelerated");
        assert_eq!(normalize_backend_id("COREML"), "accelerated");
        assert_eq!(normalize_backend_id("metal"), "accelerated");
        assert_eq!(normalize_backend_id("Metal"), "accelerated");
        assert_eq!(normalize_backend_id("ane"), "accelerated");

        // CPU -> cpu
        assert_eq!(normalize_backend_id("cpu"), "cpu");
        assert_eq!(normalize_backend_id("CPU"), "cpu");
        assert_eq!(normalize_backend_id("cpu-only"), "cpu");
        assert_eq!(normalize_backend_id("cpu_only"), "cpu");
        assert_eq!(normalize_backend_id("cpuonly"), "cpu");

        // Auto/default -> native (since MLX is the default inference backend)
        assert_eq!(normalize_backend_id("auto"), "native");
        assert_eq!(normalize_backend_id("Auto"), "native");
        assert_eq!(normalize_backend_id("autodev"), "native");
        assert_eq!(normalize_backend_id("default"), "native");

        // Unknown backends
        assert_eq!(normalize_backend_id("unknown-backend"), "unknown");
        assert_eq!(normalize_backend_id("cuda"), "unknown");
        assert_eq!(normalize_backend_id("rocm"), "unknown");
        assert_eq!(normalize_backend_id(""), "unknown");
        assert_eq!(normalize_backend_id("   "), "unknown");
    }

    #[test]
    fn test_backend_id_normalization_handles_whitespace() {
        assert_eq!(normalize_backend_id("  mlx  "), "native");
        assert_eq!(normalize_backend_id("\tcoreml\n"), "accelerated");
        assert_eq!(normalize_backend_id(" cpu "), "cpu");
    }

    #[test]
    fn test_backend_id_normalization_handles_mixed_separators() {
        assert_eq!(normalize_backend_id("mlx_bridge"), "native");
        assert_eq!(normalize_backend_id("mlx-bridge"), "native");
        assert_eq!(normalize_backend_id("cpu_only"), "cpu");
        assert_eq!(normalize_backend_id("cpu-only"), "cpu");
    }
}
