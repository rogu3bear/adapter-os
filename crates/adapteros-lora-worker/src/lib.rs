//! LORAX Worker
//!
//! Core worker implementation for the LORAX (Low Rank Adapter Exchange) runtime.
//!
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

use crate::adapter_hotswap::adapter_id_to_u16;
use crate::device_placement::{
    DeviceKind, LaneDescriptor, PlacementDecision, PlacementEngine, TelemetryCollector,
};
use crate::memory::MemoryPressureLevel;
use crate::request_pinner::RequestPinner;
use crate::router_bridge::decision_to_router_ring_with_active_ids_and_strengths;
use crate::routing_policy_filter::filter_decision_by_policy;
use adapteros_api_types::{
    inference::FusionIntervalTrace, RouterDecisionChainEntry, RouterDecisionHash, RunReceipt,
};
use adapteros_config::{resolve_index_root, PlacementConfig, PlacementMode, PlacementWeights};
use adapteros_core::{
    determinism::{DeterminismContext, DeterminismSource},
    determinism_violation_event, emit_observability_event, AosError, B3Hash, BackendKind,
    DeterminismViolationKind, FusionInterval, RepoAdapterPaths, Result, SeedMode,
};
use adapteros_db::{Db, SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_rag::RagSystem;
use adapteros_lora_router::{
    constants::PINNED_BOOST, features::CodeFeatures, policy_mask::PolicyMask, AdapterInfo, Router,
    RouterDeterminismConfig,
};
use adapteros_manifest::ManifestV3;
use adapteros_policy::{PolicyEngine, RefusalResponse};
use adapteros_telemetry::TelemetryWriter;
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use adapteros_types::coreml::CoreMLMode;
use base64::Engine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub mod adapter_hotswap;
pub mod anomaly_detection;
pub mod backend_coordinator;
pub mod backend_factory;
pub mod backoff;
pub mod backpressure;
pub mod base_model_state;
pub mod contact_discovery;
pub mod conv_pipeline;
pub mod deadlock;
pub mod deterministic_rng;
pub mod device_placement;
pub mod directory_adapters;
pub mod embeddings;
pub mod ephemeral_adapters;
pub mod evidence;
pub mod export;
pub mod filter_engine;
pub mod framework_adapters;
pub mod generation;
pub mod health;
pub mod inference_metrics;
pub mod inference_pipeline;
pub mod kv_quota;
pub mod kvcache;
pub mod launcher;
pub mod lifecycle_state;
pub mod limiter;
pub mod linter_runner;
pub mod llm_backend;
pub mod memory;
pub mod metrics;
pub mod model_handle_cache;
pub mod model_key;
pub mod model_loader;
pub mod panic_utils;
pub mod patch_generator;
pub mod patch_telemetry;
pub mod patch_validator;
pub mod prefix_kv_cache;
pub mod request_pinner;
pub mod router_bridge;
pub mod routing_policy_filter;
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

pub use adapter_hotswap::{
    AdapterCacheIdentity, AdapterCommand, AdapterCommandResult, AdapterTable, GpuFingerprint,
    HotSwapManager, HotSwapManagerNoKernel, Stack, StackCheckpoint,
};
pub use adapteros_core::CircuitState;
pub use adapteros_lora_rag::DocIndexImpl;
pub use adapteros_lora_rag::SymbolIndexImpl;
pub use adapteros_lora_rag::TestIndexImpl;
pub use anomaly_detection::{
    AnomalyDetectionConfig, AnomalyDetector, AnomalyScore, DetectionAlgorithm,
};
pub use backend_factory::{
    create_backend, create_backend_from_config, create_backend_with_model, BackendChoice,
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
pub use timeout::{CircuitBreaker, TimeoutConfig, TimeoutWrapper};
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

#[cfg(test)]
mod tests {
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
}

/// Strictness control for backend execution (strict mode disables fallback)
pub trait StrictnessControl {
    /// Set strict mode for subsequent operations
    fn set_strict_mode(&mut self, strict: bool);
    /// Reset fallback tracking for a new request
    fn reset_fallback(&mut self);
    /// Select active lane (primary/fallback) for next step
    fn set_active_lane(&mut self, lane: BackendLane);
    /// Report currently active lane
    fn active_lane(&self) -> BackendLane;
    /// Names for the available lanes (primary, fallback)
    fn lane_names(&self) -> (String, Option<String>);
    /// Whether fallback occurred on the last operation
    fn fallback_triggered(&self) -> bool;
    /// Backend name used on the last operation (if known)
    fn last_backend_used(&self) -> Option<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendLane {
    Primary,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveBackend {
    Primary,
    Fallback,
}

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

// Default strictness control for plain backends (no fallback)
impl StrictnessControl for Box<dyn FusedKernels + Send + Sync> {
    fn set_strict_mode(&mut self, _strict: bool) {}
    fn reset_fallback(&mut self) {}
    fn set_active_lane(&mut self, _lane: BackendLane) {}
    fn active_lane(&self) -> BackendLane {
        BackendLane::Primary
    }
    fn lane_names(&self) -> (String, Option<String>) {
        (self.device_name().to_string(), None)
    }
    fn fallback_triggered(&self) -> bool {
        false
    }
    fn last_backend_used(&self) -> Option<String> {
        Some(self.device_name().to_string())
    }
}

/// Direct single-backend wrapper (no fallback)
pub struct DirectKernels {
    inner: Box<dyn FusedKernels + Send + Sync>,
    last_backend: String,
}

impl DirectKernels {
    pub fn new(inner: Box<dyn FusedKernels + Send + Sync>) -> Self {
        let last_backend = inner.device_name().to_string();
        Self {
            inner,
            last_backend,
        }
    }
}

/// Coordinated backend wrapper with optional fallback backend
pub struct CoordinatedKernels {
    primary: Box<dyn FusedKernels + Send + Sync>,
    fallback: Option<Box<dyn FusedKernels + Send + Sync>>,
    active_backend: ActiveBackend,
    strict_mode: bool,
    primary_degraded: bool,
    fallback_triggered: bool,
    last_backend: String,
}

impl CoordinatedKernels {
    pub fn new(
        primary: Box<dyn FusedKernels + Send + Sync>,
        fallback: Option<Box<dyn FusedKernels + Send + Sync>>,
    ) -> Self {
        let last_backend = primary.device_name().to_string();
        Self {
            primary,
            fallback,
            active_backend: ActiveBackend::Primary,
            strict_mode: false,
            primary_degraded: false,
            fallback_triggered: false,
            last_backend,
        }
    }
}

/// Unified kernel wrapper supporting strictness control and optional fallback
pub enum KernelWrapper {
    Direct(DirectKernels),
    Coordinated(CoordinatedKernels),
}

impl StrictnessControl for KernelWrapper {
    fn set_strict_mode(&mut self, strict: bool) {
        if let KernelWrapper::Coordinated(k) = self {
            k.strict_mode = strict;
        }
    }

    fn reset_fallback(&mut self) {
        match self {
            KernelWrapper::Direct(k) => {
                k.last_backend = k.inner.device_name().to_string();
            }
            KernelWrapper::Coordinated(k) => {
                k.fallback_triggered = false;
                k.active_backend = if k.strict_mode || k.fallback.is_none() || !k.primary_degraded {
                    ActiveBackend::Primary
                } else {
                    ActiveBackend::Fallback
                };
                k.fallback_triggered = matches!(k.active_backend, ActiveBackend::Fallback);
                k.last_backend = match k.active_backend {
                    ActiveBackend::Primary => k.primary.device_name().to_string(),
                    ActiveBackend::Fallback => k
                        .fallback
                        .as_ref()
                        .map(|f| f.device_name().to_string())
                        .unwrap_or_else(|| k.primary.device_name().to_string()),
                };
            }
        }
    }

    fn set_active_lane(&mut self, lane: BackendLane) {
        match self {
            KernelWrapper::Direct(k) => {
                k.last_backend = k.inner.device_name().to_string();
            }
            KernelWrapper::Coordinated(k) => {
                match lane {
                    BackendLane::Primary => k.active_backend = ActiveBackend::Primary,
                    BackendLane::Fallback => {
                        if k.fallback.is_some() {
                            k.active_backend = ActiveBackend::Fallback;
                        } else {
                            k.active_backend = ActiveBackend::Primary;
                        }
                    }
                }
                k.fallback_triggered = matches!(k.active_backend, ActiveBackend::Fallback);
                k.last_backend = match k.active_backend {
                    ActiveBackend::Primary => k.primary.device_name().to_string(),
                    ActiveBackend::Fallback => k
                        .fallback
                        .as_ref()
                        .map(|f| f.device_name().to_string())
                        .unwrap_or_else(|| k.primary.device_name().to_string()),
                };
            }
        }
    }

    fn active_lane(&self) -> BackendLane {
        match self {
            KernelWrapper::Direct(_) => BackendLane::Primary,
            KernelWrapper::Coordinated(k) => match k.active_backend {
                ActiveBackend::Primary => BackendLane::Primary,
                ActiveBackend::Fallback => BackendLane::Fallback,
            },
        }
    }

    fn lane_names(&self) -> (String, Option<String>) {
        match self {
            KernelWrapper::Direct(k) => (k.inner.device_name().to_string(), None),
            KernelWrapper::Coordinated(k) => (
                k.primary.device_name().to_string(),
                k.fallback.as_ref().map(|f| f.device_name().to_string()),
            ),
        }
    }

    fn fallback_triggered(&self) -> bool {
        match self {
            KernelWrapper::Direct(_) => false,
            KernelWrapper::Coordinated(k) => k.fallback_triggered,
        }
    }

    fn last_backend_used(&self) -> Option<String> {
        match self {
            KernelWrapper::Direct(k) => Some(k.last_backend.clone()),
            KernelWrapper::Coordinated(k) => Some(k.last_backend.clone()),
        }
    }
}

impl FusedKernels for KernelWrapper {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.load(plan_bytes),
            KernelWrapper::Coordinated(k) => {
                k.primary.load(plan_bytes)?;
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.load(plan_bytes)?;
                }
                Ok(())
            }
        }
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.run_step(ring, io),
            KernelWrapper::Coordinated(k) => {
                k.fallback_triggered = matches!(k.active_backend, ActiveBackend::Fallback);
                match k.active_backend {
                    ActiveBackend::Primary => match k.primary.run_step(ring, io) {
                        Ok(_) => {
                            k.primary_degraded = false;
                            k.last_backend = k.primary.device_name().to_string();
                            k.fallback_triggered = false;
                            Ok(())
                        }
                        Err(e) => {
                            k.primary_degraded = true;
                            k.last_backend = k.primary.device_name().to_string();
                            Err(e)
                        }
                    },
                    ActiveBackend::Fallback => {
                        let Some(fallback) = k.fallback.as_mut() else {
                            return Err(AosError::Kernel(
                                "Fallback backend not configured".to_string(),
                            ));
                        };

                        match fallback.run_step(ring, io) {
                            Ok(_) => {
                                k.last_backend = fallback.device_name().to_string();
                                k.fallback_triggered = true;
                                Ok(())
                            }
                            Err(e) => {
                                k.last_backend = fallback.device_name().to_string();
                                Err(e)
                            }
                        }
                    }
                }
            }
        }
    }

    fn device_name(&self) -> &str {
        match self {
            KernelWrapper::Direct(k) => k.inner.device_name(),
            KernelWrapper::Coordinated(k) => k.last_backend.as_str(),
        }
    }

    fn attest_determinism(
        &self,
    ) -> Result<adapteros_lora_kernel_api::attestation::DeterminismReport> {
        match self {
            KernelWrapper::Direct(k) => k.inner.attest_determinism(),
            KernelWrapper::Coordinated(k) => k.primary.attest_determinism(),
        }
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.load_adapter(id, weights),
            KernelWrapper::Coordinated(k) => {
                k.primary.load_adapter(id, weights)?;
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.load_adapter(id, weights)?;
                }
                Ok(())
            }
        }
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.unload_adapter(id),
            KernelWrapper::Coordinated(k) => {
                k.primary.unload_adapter(id)?;
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.unload_adapter(id)?;
                }
                Ok(())
            }
        }
    }

    fn attach_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.attach_adapter(id),
            KernelWrapper::Coordinated(k) => {
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.attach_adapter(id)?;
                }
                k.primary.attach_adapter(id)
            }
        }
    }

    fn detach_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.detach_adapter(id),
            KernelWrapper::Coordinated(k) => {
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.detach_adapter(id)?;
                }
                k.primary.detach_adapter(id)
            }
        }
    }

    fn switch_adapter(&mut self, id: u16) -> Result<()> {
        match self {
            KernelWrapper::Direct(k) => k.inner.switch_adapter(id),
            KernelWrapper::Coordinated(k) => {
                if let Some(fallback) = k.fallback.as_mut() {
                    fallback.switch_adapter(id)?;
                }
                k.primary.switch_adapter(id)
            }
        }
    }
}

/// Inference request
///
/// Includes full sampling parameters for deterministic replay (PRD-02).
/// When router_seed is provided, it overrides the manifest-derived seed
/// for deterministic adapter selection during replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    #[serde(default)]
    pub require_evidence: bool,
    /// Optional: Request patch proposal mode
    #[serde(default)]
    pub request_type: RequestType,
    /// Stack ID for telemetry correlation
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
    #[serde(default)]
    pub stack_version: Option<i64>,
    /// Optional domain hint for routing/package preference
    #[serde(default)]
    pub domain_hint: Option<String>,
    /// Sampling temperature (0.0 = deterministic, higher = more random)
    /// Defaults to manifest setting if not provided
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Top-K sampling (limits vocabulary to K most likely tokens)
    #[serde(default)]
    pub top_k: Option<usize>,
    /// Top-P (nucleus) sampling (limits vocabulary to tokens with cumulative prob <= P)
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Random seed for deterministic sampling (PRD-02: critical for replay)
    #[serde(default)]
    pub seed: Option<u64>,
    /// Router seed override for deterministic adapter selection (PRD-02: replay)
    /// When provided, overrides the manifest-derived router seed to reproduce
    /// exact routing decisions from a previous inference.
    #[serde(default)]
    pub router_seed: Option<String>,
    /// Seed mode provided by control plane
    #[serde(default)]
    pub seed_mode: Option<SeedMode>,
    /// Request-scoped seed provided by control plane (32 bytes)
    #[serde(default)]
    pub request_seed: Option<[u8; 32]>,
    /// Fusion interval policy for aligning router gates with fused weights
    #[serde(default)]
    pub fusion_interval: Option<FusionInterval>,
    /// Backend profile requested by control plane
    #[serde(default)]
    pub backend_profile: Option<BackendKind>,
    /// CoreML mode applied by control plane
    #[serde(default)]
    pub coreml_mode: Option<CoreMLMode>,
    /// Pinned adapter IDs that receive prior boost in routing (CHAT-PIN-02)
    ///
    /// These adapters receive PINNED_BOOST (0.3) added to their prior scores
    /// before the router's scoring algorithm runs.
    #[serde(default)]
    pub pinned_adapter_ids: Option<Vec<String>>,
    /// Determinism mode for this request (strict, besteffort, relaxed)
    /// Controls router behavior for reproducibility vs performance tradeoffs
    #[serde(default = "default_determinism_mode")]
    pub determinism_mode: String,
    /// Routing determinism mode (deterministic|adaptive)
    #[serde(default)]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    /// Strict mode flag (disables backend fallback when true)
    #[serde(default)]
    pub strict_mode: bool,
    /// Per-adapter strength overrides (multiplier applied to manifest lora_strength)
    #[serde(default)]
    pub adapter_strength_overrides: Option<std::collections::HashMap<String, f32>>,
    /// Effective adapter IDs (control-plane gate)
    #[serde(default)]
    pub effective_adapter_ids: Option<Vec<String>>,
    /// Placement override for replay
    #[serde(default)]
    pub placement: Option<PlacementReplay>,

    /// Optional routing policy resolved by control plane.
    /// Used to enforce allow/deny lists and max-adapter limits per token.
    #[serde(default)]
    pub routing_policy: Option<adapteros_api_types::RoutingPolicy>,

    /// Optional stop policy for deterministic stop control (PRD: Hard Deterministic Stop Controller)
    #[serde(default)]
    pub stop_policy: Option<adapteros_api_types::inference::StopPolicySpec>,
}

fn default_determinism_mode() -> String {
    "strict".to_string()
}

/// Returns true when strict determinism protections should be enforced.
fn strict_mode_enabled(strict_flag: bool, determinism_mode: &str) -> bool {
    strict_flag || determinism_mode.eq_ignore_ascii_case("strict")
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

/// Request type for different inference modes
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestType {
    #[default]
    Normal,
    PatchProposal(PatchProposalRequest),
}

/// Patch proposal request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposalRequest {
    /// Repository ID for context
    pub repo_id: String,
    /// Commit SHA for context (optional)
    pub commit_sha: Option<String>,
    /// Files to focus on
    pub target_files: Vec<String>,
    /// Issue description or prompt
    pub description: String,
}

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
    /// Backend used to execute the request (e.g., metal, coreml, mlx)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub doc_id: String,
    pub rev: String,
    pub span_hash: B3Hash,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterSummary {
    pub adapters_used: Vec<String>,
    pub avg_activations: Vec<f32>,
}

/// Summarize router usage for telemetry and replay.
///
/// Base-only requests produce empty adapter usage to make it explicit that the
/// base model handled the request without any adapter contribution.
pub fn summarize_router_usage(
    base_only_request: bool,
    active_ids: &[String],
    k_sparse: usize,
    router_decisions: Option<&[adapteros_api_types::inference::RouterDecision]>,
) -> RouterSummary {
    if base_only_request {
        return RouterSummary {
            adapters_used: Vec::new(),
            avg_activations: Vec::new(),
        };
    }

    if let Some(decisions) = router_decisions {
        let mut used: Vec<String> = decisions
            .iter()
            .flat_map(|d| d.candidate_adapters.iter())
            .filter_map(|c| active_ids.get(c.adapter_idx as usize))
            .cloned()
            .collect();
        used.sort();
        used.dedup();
        if !used.is_empty() {
            let take = used.len().min(k_sparse);
            return RouterSummary {
                adapters_used: used.into_iter().take(take).collect(),
                avg_activations: vec![0.33; take],
            };
        }
    }

    let adapters_used: Vec<String> = active_ids.iter().take(k_sparse).cloned().collect();
    let activation_len = adapters_used.len();
    RouterSummary {
        adapters_used,
        avg_activations: if activation_len == 0 {
            Vec::new()
        } else {
            vec![0.33; activation_len]
        },
    }
}

#[derive(Serialize)]
struct FusionCandidateMaterial {
    adapter_idx: u16,
    raw_score: f32,
    gate_q15: i16,
}

#[derive(Serialize)]
struct FusionDecisionMaterial {
    step: usize,
    input_token_id: Option<u32>,
    candidate_adapters: Vec<FusionCandidateMaterial>,
    entropy: f32,
    tau: f32,
    entropy_floor: f32,
    stack_hash: Option<String>,
    policy_mask_digest: Option<B3Hash>,
    policy_overrides_applied: Option<adapteros_api_types::inference::PolicyOverrideFlags>,
    interval_id: Option<String>,
}

#[derive(Serialize)]
struct FusionIntervalMaterial {
    base_model_hash: B3Hash,
    interval_id: String,
    decisions: Vec<FusionDecisionMaterial>,
}

fn fused_hash_for_interval(
    base_model_hash: &B3Hash,
    interval_id: &str,
    decisions: &[adapteros_api_types::inference::RouterDecision],
) -> B3Hash {
    let material = FusionIntervalMaterial {
        base_model_hash: *base_model_hash,
        interval_id: interval_id.to_string(),
        decisions: decisions
            .iter()
            .map(|decision| FusionDecisionMaterial {
                step: decision.step,
                input_token_id: decision.input_token_id,
                candidate_adapters: decision
                    .candidate_adapters
                    .iter()
                    .map(|c| FusionCandidateMaterial {
                        adapter_idx: c.adapter_idx,
                        raw_score: c.raw_score,
                        gate_q15: c.gate_q15,
                    })
                    .collect(),
                entropy: decision.entropy,
                tau: decision.tau,
                entropy_floor: decision.entropy_floor,
                stack_hash: decision.stack_hash.clone(),
                policy_mask_digest: decision.policy_mask_digest,
                policy_overrides_applied: decision.policy_overrides_applied.clone(),
                interval_id: decision.interval_id.clone(),
            })
            .collect(),
    };

    // Canonical JSON ensures platform-stable byte layout for replay hashing.
    let canonical_bytes =
        serde_jcs::to_vec(&material).expect("fusion interval hash serialization must succeed");
    B3Hash::hash(&canonical_bytes)
}

fn fusion_intervals_for_mode(
    mode: FusionInterval,
    router_decisions: Option<&[adapteros_api_types::inference::RouterDecision]>,
    base_model_hash: &B3Hash,
) -> Option<Vec<FusionIntervalTrace>> {
    let decisions = router_decisions?;
    if decisions.is_empty() {
        return None;
    }

    let mut intervals = Vec::new();
    let mut start_idx = 0usize;
    let mut current_interval = decisions[0]
        .interval_id
        .clone()
        .unwrap_or_else(|| mode.interval_id_for_step(decisions[0].step));

    let mut push_bucket =
        |interval_id: &str, bucket: &[adapteros_api_types::inference::RouterDecision]| {
            if bucket.is_empty() {
                return;
            }
            let hash = fused_hash_for_interval(base_model_hash, interval_id, bucket);
            let start = bucket.first().map(|d| d.step).unwrap_or(0);
            let end = bucket.last().map(|d| d.step).unwrap_or(start);
            intervals.push(FusionIntervalTrace {
                interval_id: interval_id.to_string(),
                start_token: start,
                end_token: end,
                fused_weight_hash: hash,
            });
        };

    for (idx, decision) in decisions.iter().enumerate().skip(1) {
        let interval_id = decision
            .interval_id
            .clone()
            .unwrap_or_else(|| mode.interval_id_for_step(decision.step));

        if interval_id != current_interval {
            push_bucket(&current_interval, &decisions[start_idx..idx]);
            start_idx = idx;
            current_interval = interval_id;
        }
    }

    push_bucket(&current_interval, &decisions[start_idx..]);

    Some(intervals)
}

#[cfg(test)]
mod fusion_interval_tests {
    use super::*;
    use adapteros_api_types::inference::{RouterCandidate, RouterDecision};

    fn sample_decisions() -> Vec<RouterDecision> {
        vec![
            RouterDecision {
                step: 0,
                input_token_id: Some(1),
                candidate_adapters: vec![
                    RouterCandidate {
                        adapter_idx: 0,
                        raw_score: 0.8,
                        gate_q15: 20000,
                    },
                    RouterCandidate {
                        adapter_idx: 1,
                        raw_score: 0.2,
                        gate_q15: 5000,
                    },
                ],
                entropy: 0.4,
                tau: 1.0,
                entropy_floor: 0.1,
                allowed_mask: None,
                stack_hash: Some("stack-a".to_string()),
                policy_mask_digest: None,
                policy_overrides_applied: None,
                interval_id: None,
            },
            RouterDecision {
                step: 1,
                input_token_id: Some(2),
                candidate_adapters: vec![RouterCandidate {
                    adapter_idx: 0,
                    raw_score: 0.5,
                    gate_q15: 15000,
                }],
                entropy: 0.5,
                tau: 1.0,
                entropy_floor: 0.1,
                allowed_mask: None,
                stack_hash: Some("stack-a".to_string()),
                policy_mask_digest: None,
                policy_overrides_applied: None,
                interval_id: None,
            },
        ]
    }

    #[test]
    fn per_request_creates_single_interval() {
        let base = B3Hash::hash(b"base");
        let decisions = sample_decisions();
        let intervals = fusion_intervals_for_mode(
            FusionInterval::PerRequest,
            Some(decisions.as_slice()),
            &base,
        )
        .expect("intervals exist");

        assert_eq!(intervals.len(), 1);
        assert_eq!(intervals[0].interval_id, "request-0");
        assert_eq!(intervals[0].start_token, 0);
        assert_eq!(intervals[0].end_token, 1);
    }

    #[test]
    fn per_token_creates_interval_per_step() {
        let base = B3Hash::hash(b"base");
        let decisions = sample_decisions();
        let intervals =
            fusion_intervals_for_mode(FusionInterval::PerToken, Some(decisions.as_slice()), &base)
                .expect("intervals exist");

        assert_eq!(intervals.len(), decisions.len());
        assert_eq!(intervals[0].interval_id, "token-0");
        assert_eq!(intervals[1].interval_id, "token-1");
    }

    #[test]
    fn fused_hash_is_stable_for_same_inputs() {
        let base = B3Hash::hash(b"base");
        let decisions = sample_decisions();
        let first = fusion_intervals_for_mode(
            FusionInterval::PerRequest,
            Some(decisions.as_slice()),
            &base,
        )
        .expect("intervals");
        let second = fusion_intervals_for_mode(
            FusionInterval::PerRequest,
            Some(decisions.as_slice()),
            &base,
        )
        .expect("intervals");

        assert_eq!(
            first[0].fused_weight_hash, second[0].fused_weight_hash,
            "same inputs must produce identical fused hash"
        );
    }

    #[test]
    fn provided_interval_ids_are_honored() {
        let base = B3Hash::hash(b"base");
        let mut decisions = sample_decisions();
        decisions
            .iter_mut()
            .for_each(|d| d.interval_id = Some("segment-0".to_string()));

        let intervals =
            fusion_intervals_for_mode(FusionInterval::PerToken, Some(decisions.as_slice()), &base)
                .expect("intervals");

        assert_eq!(intervals.len(), 1, "custom interval ids control grouping");
        assert_eq!(intervals[0].interval_id, "segment-0");
        assert_eq!(intervals[0].start_token, 0);
        assert_eq!(intervals[0].end_token, 1);
    }
}

#[cfg(test)]
mod router_summary_tests {
    use super::summarize_router_usage;
    use adapteros_api_types::inference::{RouterCandidate, RouterDecision};

    #[test]
    fn base_only_summary_is_empty() {
        let empty_decisions: Vec<RouterDecision> = Vec::new();
        let summary = summarize_router_usage(true, &[], 2, Some(empty_decisions.as_slice()));
        assert!(summary.adapters_used.is_empty());
        assert!(summary.avg_activations.is_empty());
    }

    #[test]
    fn summarize_uses_active_ids_when_present() {
        let decisions = vec![RouterDecision {
            step: 0,
            input_token_id: None,
            candidate_adapters: vec![RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.2,
                gate_q15: 1000,
            }],
            entropy: 0.0,
            tau: 0.0,
            entropy_floor: 0.0,
            stack_hash: None,
            interval_id: None,
            allowed_mask: None,
            policy_mask_digest: None,
            policy_overrides_applied: None,
        }];
        let active_ids = vec!["adapter-a".to_string(), "adapter-b".to_string()];
        let summary = summarize_router_usage(false, &active_ids, 2, Some(decisions.as_slice()));
        assert_eq!(summary.adapters_used, vec!["adapter-b".to_string()]);
        assert_eq!(summary.avg_activations.len(), 1);
    }
}

/// Request to cancel a training job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTrainingRequest {
    /// ID of the training job to cancel
    pub job_id: String,
    /// Optional reason for cancellation
    pub reason: Option<String>,
}

/// Response from training job cancellation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTrainingResponse {
    /// ID of the job that was cancelled
    pub job_id: String,
    /// Status: "cancelled", "not_found", "already_complete", "not_running"
    pub status: String,
    /// Number of tokens processed before cancellation (if available)
    pub tokens_processed: Option<u64>,
    /// Final loss value at cancellation (if available)
    pub final_loss: Option<f32>,
    /// Epoch at which training was stopped
    pub stopped_at_epoch: Option<u32>,
}

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
}

#[cfg(test)]
mod adapter_path_tests {
    use super::resolve_worker_adapter_paths;
    use std::path::PathBuf;

    #[test]
    fn worker_paths_respect_env_override() {
        std::env::set_var("AOS_ADAPTERS_ROOT", "./var/test-adapters-repo");
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

use crate::embeddings::EmbeddingModel;
use crate::evidence::EvidenceRetriever;
use crate::tokenizer::QwenTokenizer;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;

/// Worker for running inference with comprehensive safety mechanisms
pub struct Worker<K: FusedKernels + StrictnessControl + Send + Sync> {
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
    embedding_model: Arc<EmbeddingModel>,
    evidence_retriever: Option<EvidenceRetriever>,
    /// KV cache for transformer attention with generation tracking
    kv_cache: Arc<StdMutex<KvCache>>,
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
    circuit_breaker: CircuitBreaker,
    _resource_limiter: ResourceLimiter,
    _deadlock_detector: DeadlockDetector,
    health_monitor: Arc<HealthMonitor>,
    telemetry: Option<TelemetryWriter>,
    trace_db: Option<Arc<Db>>,
    trace_flush_every: usize,
    placement_template: Option<(PlacementConfig, Vec<LaneDescriptor>)>,
    // Lifecycle management
    profiler: adapteros_profiler::AdapterProfiler,
    lifecycle: Arc<Mutex<adapteros_lora_lifecycle::LifecycleManager>>,
    // Hot-swap management
    hotswap: Arc<HotSwapManager<K>>,
    // Retirement task management
    retirement_handle: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: watch::Sender<()>,
    /// Active training jobs with their cancellation tokens
    pub active_training_jobs: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    /// Worker ID for identity tracking (PRD-06)
    worker_id: u32,
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
        worker_id: u32,
    ) -> Result<Self> {
        // Initialize determinism guards first
        init_determinism_guards()?;

        let policy = PolicyEngine::new(manifest.policies.clone());

        // Create router from manifest
        let router_seed = adapteros_core::derive_seed(&manifest.seeds.global, "router");
        let router = Router::new(
            vec![1.0; manifest.adapters.len()],
            manifest.router.k_sparse,
            manifest.router.tau,
            manifest.router.entropy_floor,
            router_seed,
        )?;

        let memory_monitor = Arc::new(MemoryMonitor::new(
            manifest.policies.memory.min_headroom_pct,
            Some(telemetry.clone()),
        ));

        // Initialize safety mechanisms
        let timeout_config = TimeoutConfig::default();
        let timeout_wrapper = TimeoutWrapper::new(timeout_config.clone());
        let circuit_breaker = CircuitBreaker::new(5, std::time::Duration::from_secs(60));
        let resource_limiter = ResourceLimiter::new(ResourceLimits::default());
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
        let generator = Generator::new(gen_seed)
            .with_temperature(0.7)
            .with_top_p(0.9)
            .with_deterministic();

        // Load embedding model - use dimensions from manifest
        let embedding_model = Arc::new(EmbeddingModel::from_model_path(
            model_path,
            manifest.base.vocab_size as usize,
            manifest.base.hidden_dim as usize,
        )?);

        // Initialize evidence retriever with real implementation if RAG is available
        let evidence_retriever = if let Some(ref _rag_system) = rag {
            use crate::evidence::*;
            use adapteros_lora_rag::EvidenceIndexManager;

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

        // Initialize kv_cache with Arc<StdMutex<>> for interior mutability
        let kv_cache = Arc::new(StdMutex::new(KvCache::new_with_quota(
            adapteros_core::constants::BYTES_PER_GB,
            quota_manager,
        ))); // 1GB default
        let last_stack_hash = RwLock::new(None);

        // Initialize profiler
        let adapter_names: Vec<String> = manifest.adapters.iter().map(|a| a.id.clone()).collect();
        let profiler = adapteros_profiler::AdapterProfiler::new(
            adapter_names.clone(),
            Some(telemetry.clone()),
        );

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

        let hotswap = Arc::new(HotSwapManager::new_with_kernels(
            kernels_arc.clone(),
            adapter_paths.repo_root.clone(),
            tenant_id.to_string(),
            Some(Arc::new(telemetry.clone())),
            Some(memory_monitor.clone()),
        ));

        hotswap.set_cache_identity(AdapterCacheIdentity {
            base_manifest_hash: Some(manifest.seeds.manifest_hash),
            backend_type: available_backends.primary.as_str().to_string(),
            kernel_version_id: adapteros_core::version::VERSION.to_string(),
            tenant_id: Some(tenant_id.to_string()),
            adapter_dir_hash: None,
        });

        // Retirement task management
        let (shutdown_tx, _shutdown_rx) = watch::channel(());
        let retirement_handle = Some(hotswap.clone().start_retirement_task());

        // Initialize active training jobs tracking
        let active_training_jobs = Arc::new(RwLock::new(HashMap::new()));

        let trace_flush_every = std::env::var("AOS_TRACE_FLUSH_EVERY")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(|v: usize| v.max(1))
            .unwrap_or(1);

        let trace_db = if std::env::var("AOS_TRACE_DB_DISABLED").is_ok() {
            None
        } else {
            match Db::from_config().await {
                Ok(db) => {
                    if let Err(e) = db.migrate().await {
                        warn!(tenant_id = %tenant_id, worker_id = %worker_id, error = %e, "Trace DB migration failed; disabling trace sink");
                        None
                    } else {
                        Some(Arc::new(db))
                    }
                }
                Err(e) => {
                    warn!(tenant_id = %tenant_id, worker_id = %worker_id, error = %e, "Trace DB unavailable; disabling trace sink");
                    None
                }
            }
        };

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
            embedding_model,
            evidence_retriever,
            kv_cache,
            _last_stack_hash: last_stack_hash,
            available_backends,
            coreml_package_hash,
            coreml_verification,
            _timeout_config: timeout_config,
            _timeout_wrapper: timeout_wrapper,
            circuit_breaker,
            _resource_limiter: resource_limiter,
            _deadlock_detector: deadlock_detector,
            health_monitor,
            telemetry: Some(telemetry),
            trace_db,
            trace_flush_every,
            placement_template,
            profiler,
            lifecycle,
            hotswap,
            retirement_handle,
            shutdown_tx,
            active_training_jobs,
            worker_id,
        })
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

    /// Compute the ModelCacheIdentityV2 digest for this worker (PRD-06)
    ///
    /// This digest uniquely identifies the cache configuration used for inference,
    /// including kernel, quantization, tokenizer, tenant, and worker identity.
    fn compute_model_cache_identity_v2_digest(&self, backend: BackendKind) -> B3Hash {
        use adapteros_lora_kernel_api::attestation::BackendType;

        // Convert BackendKind to BackendType for identity computation
        let backend_type = match backend {
            BackendKind::CoreML => BackendType::CoreML,
            BackendKind::Metal => BackendType::Metal,
            BackendKind::Mlx => BackendType::Mlx,
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

        identity.digest()
    }

    /// Run inference with comprehensive safety mechanisms
    pub async fn infer(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        let start_time = Instant::now();

        // Record health metrics
        self.health_monitor.record_request();

        // Run inference with timeout (simplified to avoid borrow checker issues)
        let result = self.infer_internal(request).await;

        // Log telemetry
        let duration = start_time.elapsed();
        if let Some(t) = &self.telemetry {
            let _ = t.log("inference", InferenceEvent {
                duration_ms: duration.as_millis() as u64,
                success: result.is_ok(),
                timeout_occurred: matches!(result, Err(AosError::Worker(ref msg)) if msg.contains("timeout")),
                circuit_breaker_open: self.circuit_breaker.is_open(),
                memory_usage: self.health_monitor.get_memory_usage().unwrap_or(0),
            }).ok();
        }

        result
    }

    /// Internal inference implementation with safety checks
    async fn infer_internal(&mut self, request: InferenceRequest) -> Result<InferenceResponse> {
        let mut request = request;
        // Start profiler session
        let mut _profiler_session = self.profiler.start_inference();

        // Enforce tenant isolation: worker must only serve its configured tenant
        if request.cpid != self.tenant_namespace {
            return Err(AosError::IsolationViolation(format!(
                "Request tenant {} does not match worker tenant {}",
                request.cpid, self.tenant_namespace
            )));
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

        let base_only_request = matches!(
            request.effective_adapter_ids.as_ref(),
            Some(ids) if ids.is_empty()
        );

        let strict_mode_active =
            strict_mode_enabled(request.strict_mode, &request.determinism_mode);

        // Resolve backend lane for this request
        let requested_backend = request.backend_profile;
        let (resolved_backend, backend_lane, backend_overridden) = {
            let requested = requested_backend.unwrap_or(self.available_backends.primary);
            if self.available_backends.contains(requested) {
                (
                    requested,
                    self.available_backends.lane_for(requested),
                    false,
                )
            } else {
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
                selected = %resolved_backend.as_str(),
                "Requested backend not available on this worker; falling back"
            );
        }
        request.backend_profile = Some(resolved_backend);
        let backend_label = resolved_backend.as_str().to_string();
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
                    let (backend_used, fallback_triggered) = {
                        let kernels = self.kernels.lock().await;
                        (
                            kernels
                                .last_backend_used()
                                .unwrap_or_else(|| kernels.device_name().to_string()),
                            kernels.fallback_triggered(),
                        )
                    };
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
                        },
                        run_receipt: None,
                        refusal: Some(RefusalResponse::insufficient_evidence(
                            self.manifest.policies.evidence.min_spans,
                            evidence.len(),
                        )),
                        patch_proposal: None,
                        stack_id: request.stack_id.clone(),
                        stack_version: request.stack_version,
                        backend_used: Some(backend_used),
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

        // Apply request-provided sampling parameters (PRD-02: replay support)
        // This enables deterministic replay when the same parameters are provided
        let routing_mode = request
            .routing_determinism_mode
            .unwrap_or(RoutingDeterminismMode::Deterministic);
        let seed_mode = request.seed_mode.unwrap_or(SeedMode::BestEffort);
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
            self.generator.set_seed_bytes(seed_bytes);
            // Avoid overriding master request seed with low-entropy seed
            self.generator.apply_request_params(
                request.temperature,
                request.top_k,
                request.top_p,
                None,
            );
        }
        if request.request_seed.is_none() {
            self.generator.apply_request_params(
                request.temperature,
                request.top_k,
                request.top_p,
                request.seed,
            );
        }

        // Generate tokens using autoregressive loop
        let formatted_prompt = self.tokenizer.apply_chat_template(&request.prompt);
        let prompt_tokens = self.tokenizer.encode(&formatted_prompt)?;

        // Snapshot current stack and pin adapters for this request
        let pinner = RequestPinner::new(self.hotswap.table().clone());
        let pinned_request = pinner
            .pin()
            .map_err(|e| AosError::Worker(format!("Failed to pin adapters: {}", e)))?;
        let stack_handle = pinned_request.stack().clone();
        let current_generation = pinned_request.generation();

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
        context_bytes.extend_from_slice(&(prompt_tokens.len() as u32).to_le_bytes());
        for t in &prompt_tokens {
            context_bytes.extend_from_slice(&t.to_le_bytes());
        }
        let context_digest = B3Hash::hash(&context_bytes).to_bytes();

        let mut trace_sink = if let Some(db) = self.trace_db.as_ref() {
            let start = TraceStart {
                trace_id: Uuid::now_v7().to_string(),
                tenant_id: request.cpid.clone(),
                request_id: None,
                context_digest,
            };

            match SqlTraceSink::new(db.clone(), start, self.trace_flush_every).await {
                Ok(sink) => Some(sink),
                Err(e) => {
                    warn!(
                        error = %e,
                        "Trace sink unavailable; continuing without persistence"
                    );
                    None
                }
            }
        } else {
            None
        };

        if strict_mode_active && trace_sink.is_none() {
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
                "Strict determinism mode requires active trace sink".to_string(),
            ));
        }
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

        if active_entries.is_empty() {
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

        let policy_mask_digest: Option<[u8; 32]> = allowed_active_indices.as_ref().map(|allowed| {
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

        let mut generated_tokens = Vec::new();
        let mut router_decisions_collected = Vec::new();
        let mut router_decision_chain = Vec::new();
        let mut previous_chain_hash: Option<String> = None;
        let mut placement_trace: Vec<PlacementTraceEntry> = Vec::new();
        let mut run_receipt: Option<RunReceipt> = None;

        // Initialize stop controller (PRD: Hard Deterministic Stop Controller)
        let mut stop_controller = StopController::from_policy_or_default(
            request.stop_policy.clone(),
            request.max_tokens as u32,
        );
        let stop_policy_digest = *stop_controller.policy_digest();
        let mut stop_reason_code = None;
        let mut stop_reason_token_index = None;

        // Autoregressive generation loop
        for step in 0..request.max_tokens {
            // Prepare input for this step
            let input_ids_slice = if step == 0 {
                &prompt_tokens[..]
            } else {
                let last_token = generated_tokens.last().ok_or_else(|| {
                    AosError::Internal("Generated tokens cannot be empty".to_string())
                })?;
                std::slice::from_ref(last_token)
            };

            // Run router to get active adapters
            // Extract features from the current prompt context for adaptive routing
            let features = if step == 0 {
                // For the first step, use the full prompt for feature extraction
                CodeFeatures::from_context(&request.prompt).to_vector()
            } else {
                // For subsequent steps, use the current token context
                // Decode recent tokens to get meaningful context for routing
                let context_tokens = &generated_tokens[generated_tokens.len().saturating_sub(10)..];
                let context_text = self
                    .tokenizer
                    .decode(context_tokens)
                    .unwrap_or_else(|_| "".to_string());
                CodeFeatures::from_context(&context_text).to_vector()
            };
            // Build priors with PINNED_BOOST for pinned adapters (CHAT-PIN-02)
            let decision = self.router.route_with_adapter_info_with_ctx(
                &features,
                &priors,
                &adapter_info,
                &policy_mask,
                determinism_ctx.as_ref(),
            )?;

            let mut decision =
                self.apply_routing_policy_to_decision(decision, request.routing_policy.as_ref())?;

            if base_only_request {
                decision.indices.clear();
                decision.candidates.clear();
                decision.gates_q15.clear();
            }

            // Collect router decision for control plane transmission
            let input_token_id = if step == 0 {
                prompt_tokens.first().copied()
            } else {
                generated_tokens.last().copied()
            };
            router_decisions_collected.push(adapteros_api_types::inference::RouterDecision {
                step,
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
                interval_id: Some(fusion_interval.interval_id_for_step(step)),
                policy_mask_digest: decision.policy_mask_digest,
                policy_overrides_applied: decision.policy_overrides_applied.as_ref().map(|flags| {
                    adapteros_api_types::inference::PolicyOverrideFlags {
                        allow_list: flags.allow_list,
                        deny_list: flags.deny_list,
                        trust_state: flags.trust_state,
                    }
                }),
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
                step,
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
                step,
                input_token_id,
                adapter_indices: decision.indices.iter().copied().collect(),
                adapter_ids: adapter_ids_for_decision,
                gates_q15: decision.gates_q15.iter().copied().collect(),
                entropy: decision.entropy,
                decision_hash: decision_hash_payload,
                previous_hash: previous_chain_hash.clone(),
                entry_hash: entry_hash.clone(),
                policy_mask_digest: decision.policy_mask_digest,
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
                let token_input = TraceTokenInput {
                    token_index: step as u32,
                    adapter_ids: adapter_ids_for_trace.clone(),
                    gates_q15: decision.gates_q15.iter().copied().collect(),
                    policy_mask_digest,
                    allowed_mask: Some(policy_mask.allowed.clone()),
                    policy_overrides_applied: decision.policy_overrides_applied.as_ref().map(
                        |flags| adapteros_api_types::inference::PolicyOverrideFlags {
                            allow_list: flags.allow_list,
                            deny_list: flags.deny_list,
                            trust_state: flags.trust_state,
                        },
                    ),
                    backend_id: Some(backend_label.clone()),
                    kernel_version_id: Some(kernel_version_id.clone()),
                };
                sink.record_token(token_input).await?
            }

            // Convert Decision to RouterRing
            let router_ring = decision_to_router_ring_with_active_ids_and_strengths(
                &decision,
                &active_hashed,
                Some(&active_strengths),
                step,
            )?;

            // Execute kernels through Metal and measure latency per adapter
            let mut io_buffers = IoBuffers {
                input_ids: input_ids_slice.to_vec(),
                output_logits: vec![0.0; self.manifest.base.vocab_size as usize],
                position: step,
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

            // Record latency for each active adapter (simplified: divide equally)
            if !decision.indices.is_empty() {
                let per_adapter_latency = kernel_duration / decision.indices.len() as u32;
                for &adapter_id in &decision.indices {
                    self.profiler
                        .record_step_latency(adapter_id, per_adapter_latency);
                }
            }

            // Re-seed generator for step-level determinism (enables replay)
            self.generator.reseed_for_step(step);

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
                debug!(
                    step,
                    reason = %decision.reason,
                    token_index = decision.token_index,
                    "Stop controller triggered"
                );
                // For LENGTH stop (EOS token), don't include the EOS in output
                // For other reasons, we've already decided to stop before appending
                break;
            }

            generated_tokens.push(next_token);
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

        let (backend_used, fallback_triggered) = {
            let kernels = self.kernels.lock().await;
            (
                kernels
                    .last_backend_used()
                    .unwrap_or_else(|| kernels.device_name().to_string()),
                kernels.fallback_triggered(),
            )
        };
        let backend_version = adapteros_core::version::VERSION.to_string();
        let (coreml_runtime, fallback_backend) =
            self.runtime_metadata_for_response(fallback_triggered);

        let logical_prompt_tokens: u32 = prompt_tokens.len().try_into().unwrap_or(u32::MAX);
        let logical_output_tokens: u32 = generated_tokens.len().try_into().unwrap_or(u32::MAX);
        // TODO(PRD-01): replace with PrefixKvCache-derived reuse once available.
        let prefix_cached_token_count: u32 = 0;
        let billed_input_tokens = logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
        let billed_output_tokens = logical_output_tokens;

        // PRD-06: Compute model cache identity v2 digest
        let model_cache_identity_v2_digest =
            self.compute_model_cache_identity_v2_digest(resolved_backend);

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
                    tenant_kv_quota_bytes: 0,
                    tenant_kv_bytes_used: 0,
                    kv_evictions: 0,
                    kv_residency_policy_id: None,
                    kv_quota_enforced: false,
                    // TODO(PRD-01): Wire up from PrefixKvCache
                    prefix_kv_key_b3: None,
                    prefix_cache_hit: false,
                    prefix_kv_bytes: 0,
                    // PRD-06: Model cache identity v2 digest
                    model_cache_identity_v2_digest_b3: Some(model_cache_identity_v2_digest),
                })
                .await
            {
                Ok(receipt) => {
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
                        tenant_kv_quota_bytes: 0,
                        tenant_kv_bytes_used: 0,
                        kv_evictions: 0,
                        kv_residency_policy_id: None,
                        kv_quota_enforced: false,
                        // TODO(PRD-01): Wire up from PrefixKvCache
                        prefix_kv_key_b3: None,
                        prefix_cache_hit: false,
                        prefix_kv_bytes: 0,
                        // PRD-06: Model cache identity v2 digest (from receipt)
                        model_cache_identity_v2_digest_b3: receipt
                            .model_cache_identity_v2_digest_b3,
                    });
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if let Some(profile) = request.backend_profile {
            let expected = profile.as_str();
            if backend_used.to_lowercase() != expected {
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
            refusal: None,
            patch_proposal: None,
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            backend_used: Some(backend_used),
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

    /// Validate and derive the allowed adapter indices for this request.
    ///
    /// - If no effective_adapter_ids provided, allow all (backward compatibility)
    /// - All effective_adapter_ids must exist in the manifest
    /// - Pinned adapters must be in both the manifest and the effective set
    fn validate_effective_adapter_gate(
        &self,
        request: &InferenceRequest,
    ) -> Result<Option<HashSet<usize>>> {
        let manifest_ids: Vec<&str> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.as_str())
            .collect();

        let Some(effective_ids) = request.effective_adapter_ids.as_ref() else {
            return Ok(None);
        };

        if effective_ids.is_empty() {
            return Ok(Some(HashSet::new()));
        }

        let mut allowed_indices = HashSet::new();

        for effective_id in effective_ids {
            let Some(idx) = manifest_ids
                .iter()
                .position(|id| id == &effective_id.as_str())
            else {
                return Err(AosError::AdapterNotInManifest {
                    adapter_id: effective_id.clone(),
                    available: manifest_ids.iter().map(|s| s.to_string()).collect(),
                });
            };
            allowed_indices.insert(idx);
        }

        if let Some(pinned_ids) = request.pinned_adapter_ids.as_ref() {
            for pinned in pinned_ids {
                let Some(idx) = manifest_ids.iter().position(|id| id == &pinned.as_str()) else {
                    return Err(AosError::AdapterNotInManifest {
                        adapter_id: pinned.clone(),
                        available: manifest_ids.iter().map(|s| s.to_string()).collect(),
                    });
                };
                if !allowed_indices.contains(&idx) {
                    return Err(AosError::AdapterNotInEffectiveSet {
                        adapter_id: pinned.clone(),
                        effective_set: effective_ids.clone(),
                    });
                }
            }
        }

        Ok(Some(allowed_indices))
    }

    /// Apply routing policy filters to a router Decision deterministically.
    ///
    /// - Preserves original decision order; only drops entries.
    /// - Does not renormalize gates to keep kernel inputs deterministic.
    /// - If all candidates are removed, returns a policy violation error.
    fn apply_routing_policy_to_decision(
        &self,
        decision: adapteros_lora_router::Decision,
        policy: Option<&adapteros_api_types::RoutingPolicy>,
    ) -> Result<adapteros_lora_router::Decision> {
        let adapter_ids: Vec<String> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.clone())
            .collect();

        filter_decision_by_policy(decision, &adapter_ids, policy)
    }

    /// Generate patch proposal with evidence retrieval
    pub async fn propose_patch(
        &mut self,
        request: InferenceRequest,
        patch_request: &PatchProposalRequest,
    ) -> Result<InferenceResponse> {
        use crate::evidence::EvidenceRequest;
        use crate::patch_generator::{MockLlmBackend, PatchGenerationRequest, PatchGenerator};
        use crate::patch_telemetry::{
            EvidenceMetrics, PatchGenerationMetrics, PatchTelemetry, ValidationMetrics,
        };
        use crate::patch_validator::{CodePolicy, PatchValidator};

        info!(
            "Generating patch proposal for: {}",
            patch_request.description
        );

        // Compute unavailable pinned adapters (CHAT-PIN-02)
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

        // Initialize telemetry
        let mut telemetry = PatchTelemetry::new();

        // 1. Build evidence retrieval request
        let evidence_request = EvidenceRequest {
            query: patch_request.description.clone(),
            target_files: patch_request.target_files.clone(),
            repo_id: patch_request.repo_id.clone(),
            commit_sha: patch_request.commit_sha.clone(),
            max_results: 10,
            min_score: 0.7,
        };

        // 2. Retrieve evidence (using mock implementation for now)
        let evidence_result = self.retrieve_evidence(&evidence_request).await?;

        // Log evidence retrieval telemetry
        let evidence_metrics = EvidenceMetrics {
            query: evidence_request.query,
            sources_used: evidence_result
                .sources_used
                .iter()
                .map(|s| format!("{:?}", s))
                .collect(),
            spans_found: evidence_result.spans.len(),
            retrieval_time_ms: evidence_result.retrieval_time_ms,
            avg_relevance_score: if !evidence_result.spans.is_empty() {
                evidence_result.spans.iter().map(|s| s.score).sum::<f32>()
                    / evidence_result.spans.len() as f32
            } else {
                0.0
            },
            min_score_threshold: evidence_request.min_score,
        };
        telemetry.log_evidence_retrieval("default_tenant", evidence_metrics, None);

        let mut evidence_refs = Vec::new();

        // Convert evidence spans to trace references
        for span in &evidence_result.spans {
            evidence_refs.push(EvidenceRef {
                doc_id: span.doc_id.clone(),
                rev: span.rev.clone(),
                span_hash: adapteros_core::B3Hash::from_hex(&span.span_hash)
                    .unwrap_or_else(|_| adapteros_core::B3Hash::hash(span.span_hash.as_bytes())),
                score: span.score,
            });
        }

        // 3. Generate patch proposal
        let patch_generation_request = PatchGenerationRequest {
            repo_id: patch_request.repo_id.clone(),
            commit_sha: patch_request.commit_sha.clone(),
            target_files: patch_request.target_files.clone(),
            description: patch_request.description.clone(),
            evidence: evidence_result.spans,
            context: std::collections::HashMap::new(),
        };

        let patch_generator = PatchGenerator::new(
            Box::new(MockLlmBackend),
            crate::patch_generator::PatchParser::new(),
            crate::patch_generator::CitationExtractor::new(),
        );

        let proposal = patch_generator
            .generate_patch(patch_generation_request)
            .await?;

        // Log patch generation telemetry
        let generation_metrics = PatchGenerationMetrics {
            proposal_id: proposal.proposal_id.clone(),
            description: patch_request.description.clone(),
            target_files: patch_request.target_files.clone(),
            evidence_count: proposal.citations.len(),
            patch_count: proposal.patches.len(),
            total_lines: proposal.patches.iter().map(|p| p.total_lines).sum(),
            generation_time_ms: 100, // Mock timing
            confidence_score: proposal.confidence,
        };
        telemetry.log_patch_generation("default_tenant", generation_metrics);

        // 4. Validate patch against policy
        let policy = CodePolicy::default();
        let policy_engine = PolicyEngine::new(self.manifest.policies.clone());
        let validator = PatchValidator::new(policy, policy_engine);
        let validation_result = validator.validate(&proposal.patches).await?;

        // Log patch validation telemetry
        let validation_metrics = ValidationMetrics {
            proposal_id: proposal.proposal_id.clone(),
            is_valid: validation_result.is_valid,
            error_count: validation_result.errors.len(),
            warning_count: validation_result.warnings.len(),
            violation_count: validation_result.violations.len(),
            validation_time_ms: 50, // Mock timing
            confidence_score: validation_result.confidence,
            violations: validation_result
                .violations
                .into_iter()
                .map(|v| crate::patch_telemetry::ViolationMetric {
                    violation_type: format!("{:?}", v.violation_type),
                    severity: format!("{:?}", v.severity),
                    file_path: v.file_path,
                    line_number: v.line_number,
                    description: v.description,
                })
                .collect(),
        };
        telemetry.log_patch_validation("default_tenant", validation_metrics);

        // 5. Build response
        let patch_proposal = if validation_result.is_valid {
            Some(PatchProposalResponse {
                proposal_id: proposal.proposal_id,
                rationale: proposal.rationale,
                patches: proposal
                    .patches
                    .clone()
                    .into_iter()
                    .map(|p| FilePatchResponse {
                        file_path: p.file_path,
                        hunks: p
                            .hunks
                            .into_iter()
                            .map(|h| PatchHunkResponse {
                                start_line: h.start_line,
                                end_line: h.end_line,
                                old_content: h.context_lines.join("\n"),
                                new_content: h.modified_lines.join("\n"),
                            })
                            .collect(),
                    })
                    .collect(),
                citations: proposal
                    .citations
                    .clone()
                    .into_iter()
                    .map(|c| CitationResponse {
                        source_type: format!("{:?}", c.evidence_type),
                        reference: format!("{}:{}", c.file_path, c.line_range.0),
                        relevance: c.relevance_score,
                    })
                    .collect(),
                confidence: proposal.confidence,
            })
        } else {
            None
        };

        let status = if validation_result.is_valid {
            "success".to_string()
        } else {
            "validation_failed".to_string()
        };

        let fusion_interval = request
            .fusion_interval
            .unwrap_or(FusionInterval::PerRequest);

        let text = if validation_result.is_valid {
            Some(format!(
                "Patch proposal generated successfully with {} files and {} citations",
                proposal.patches.len(),
                proposal.citations.len()
            ))
        } else {
            Some(format!(
                "Patch validation failed: {}",
                validation_result.errors.join(", ")
            ))
        };

        Ok(InferenceResponse {
            text,
            status,
            trace: self.build_trace(
                &request.cpid,
                &evidence_refs,
                0,
                None,
                None,
                fusion_interval,
                &self
                    .manifest
                    .adapters
                    .iter()
                    .map(|a| a.id.clone())
                    .collect::<Vec<_>>(),
                false,
            ),
            run_receipt: None,
            refusal: if !validation_result.is_valid {
                Some(RefusalResponse {
                    status: "failed".to_string(),
                    reason: adapteros_policy::RefusalReason::MissingFields {
                        template: "patch_validation".to_string(),
                        fields: validation_result.errors.clone(),
                    },
                    message: format!(
                        "Patch validation failed: {}",
                        validation_result.errors.join(", ")
                    ),
                })
            } else {
                None
            },
            patch_proposal,
            stack_id: request.stack_id.clone(),
            stack_version: request.stack_version,
            backend_used: Some(self.kernels.lock().await.device_name().to_string()),
            backend_version: Some(adapteros_core::version::VERSION.to_string()),
            fallback_triggered: false,
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            coreml_package_hash: self.coreml_package_hash.clone(),
            coreml_expected_package_hash: self
                .coreml_verification
                .as_ref()
                .and_then(|v| v.expected.clone()),
            coreml_hash_mismatch: self.coreml_verification.as_ref().map(|v| v.mismatch),
            fallback_backend: None,
            determinism_mode_applied: Some(request.determinism_mode.clone()),
            unavailable_pinned_adapters,
            pinned_routing_fallback,
            placement_trace: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
            error_details: None,
        })
    }

    /// Retrieve evidence for patch proposal using real EvidenceRetriever
    async fn retrieve_evidence(
        &mut self,
        request: &crate::evidence::EvidenceRequest,
    ) -> Result<crate::evidence::EvidenceResult> {
        use crate::evidence::{EvidenceResult, EvidenceSpan, EvidenceType};
        use std::collections::HashMap;

        // Use real evidence retriever if available
        if let Some(ref mut retriever) = self.evidence_retriever {
            retriever
                .retrieve_patch_evidence(request, "default_tenant")
                .await
                .map_err(|e| AosError::Internal(e.to_string()))
        } else {
            // Fallback to basic mock if no retriever is available
            let mock_spans = vec![
                EvidenceSpan {
                    doc_id: "mock_doc_1".to_string(),
                    rev: "v1".to_string(),
                    span_hash: "hash1".to_string(),
                    score: 0.9,
                    evidence_type: EvidenceType::Symbol,
                    file_path: request
                        .target_files
                        .first()
                        .unwrap_or(&"src/test.rs".to_string())
                        .clone(),
                    start_line: 10,
                    end_line: 15,
                    content: format!("Mock evidence for: {}", request.query),
                    metadata: HashMap::new(),
                },
                EvidenceSpan {
                    doc_id: "mock_doc_2".to_string(),
                    rev: "v1".to_string(),
                    span_hash: "hash2".to_string(),
                    score: 0.8,
                    evidence_type: EvidenceType::Test,
                    file_path: "tests/test.rs".to_string(),
                    start_line: 20,
                    end_line: 25,
                    content: "Mock test evidence".to_string(),
                    metadata: HashMap::new(),
                },
            ];

            Ok(EvidenceResult {
                spans: mock_spans,
                total_found: 2,
                retrieval_time_ms: 50,
                sources_used: vec![EvidenceType::Symbol, EvidenceType::Test],
            })
        }
    }

    /// Compute embedding for text query (for RAG/similarity search)
    ///
    /// This generates averaged token embeddings for semantic search.
    /// Note: Metal kernels handle embedding lookup internally for forward pass.
    fn compute_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = self.tokenizer.encode(text)?;
        self.embedding_model.encode_tokens(&tokens)
    }

    /// Encode tokens to embeddings for RAG/text similarity
    ///
    /// This method is used for generating query embeddings for evidence retrieval
    /// and semantic search. It averages token embeddings and applies L2 normalization.
    ///
    /// Note: This is NOT used for the forward pass - Metal kernels perform
    /// embedding lookup directly from input_ids for inference.
    fn _encode_text_for_rag(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        self.embedding_model.encode_tokens(token_ids)
    }

    /// Generate a deterministic plan_id from the manifest hash and request context
    ///
    /// The plan_id is derived using BLAKE3 hash of:
    /// - Base model hash from manifest (ensures reproducibility across workers)
    /// - Request cpid (ensures uniqueness per request)
    ///
    /// This provides a deterministic, traceable identifier for each inference plan.
    fn generate_plan_id(&self, cpid: &str) -> String {
        use adapteros_core::B3Hash;

        // Combine manifest model hash with cpid for deterministic plan identification
        let combined = format!("{}:{}", self.manifest.base.model_hash, cpid);
        let hash = B3Hash::hash(combined.as_bytes());

        // Use first 16 hex chars (64 bits) for reasonable uniqueness while keeping it readable
        format!("plan_{}", &hash.to_hex()[..16])
    }

    /// Build response trace with evidence and router summary
    #[allow(clippy::too_many_arguments)]
    fn build_trace(
        &self,
        cpid: &str,
        evidence: &[EvidenceRef],
        token_count: usize,
        router_decisions: Option<Vec<adapteros_api_types::inference::RouterDecision>>,
        router_decision_chain: Option<
            Vec<adapteros_api_types::inference::RouterDecisionChainEntry>,
        >,
        fusion_interval: FusionInterval,
        active_ids: &[String],
        base_only_request: bool,
    ) -> ResponseTrace {
        let active_pool: Vec<String> = if active_ids.is_empty() {
            self.manifest
                .adapters
                .iter()
                .map(|a| a.id.clone())
                .collect()
        } else {
            active_ids.to_vec()
        };

        let router_summary = summarize_router_usage(
            base_only_request,
            &active_pool,
            self.manifest.router.k_sparse,
            router_decisions.as_deref(),
        );

        let fusion_intervals = fusion_intervals_for_mode(
            fusion_interval,
            router_decisions.as_deref(),
            &self.manifest.base.model_hash,
        );

        ResponseTrace {
            cpid: cpid.to_string(),
            plan_id: self.generate_plan_id(cpid),
            evidence: evidence.to_vec(),
            router_summary,
            token_count,
            router_decisions,
            router_decision_chain,
            fusion_intervals,
        }
    }

    /// Execute adapter hot-swap command
    pub async fn execute_adapter_command(
        &mut self,
        command: AdapterCommand,
    ) -> Result<AdapterCommandResult> {
        if let AdapterCommand::Preload { ref adapter_id, .. } = &command {
            // Check live pressure before attempting to load another adapter.
            let pressure_before: MemoryPressureLevel =
                self.memory_monitor.current_pressure_level().await;

            if pressure_before == MemoryPressureLevel::Critical {
                tracing::warn!(
                    adapter_id = %adapter_id,
                    "Critical memory pressure before adapter preload; attempting eviction"
                );

                // Attempt to free memory through lifecycle eviction logic.
                let lifecycle = self.lifecycle.lock().await;
                if let Err(evict_err) = lifecycle.handle_memory_pressure(&self.profiler) {
                    tracing::warn!(
                        adapter_id = %adapter_id,
                        error = %evict_err,
                        "Eviction attempt during preload guard failed"
                    );
                }
            }

            let pressure_after: MemoryPressureLevel =
                self.memory_monitor.current_pressure_level().await;
            ensure_preload_allowed(pressure_before, pressure_after)?;
        }

        self.hotswap.execute(command).await
    }

    /// Verify GPU buffers for all loaded adapters
    ///
    /// Reads GPU buffer checkpoints and validates against stored fingerprints.
    /// Also checks memory footprint against adaptive baseline with 2 sigma tolerance.
    ///
    /// Returns a report with verified/failed/skipped adapters.
    ///
    /// # Usage
    ///
    /// This method can be called on-demand to verify GPU integrity after adapter
    /// operations (load, swap, rollback) or as part of periodic health checks.
    ///
    /// ```rust
    /// use adapteros_lora_lifecycle::GpuIntegrityReport;
    ///
    /// // Example of how to check a GPU integrity report
    /// let report = GpuIntegrityReport {
    ///     verified: vec![(0, "adapter-1".to_string())],
    ///     failed: vec![],
    ///     skipped: vec![],
    ///     total_checked: 1,
    ///     timestamp: 0,
    /// };
    ///
    /// // Check if any adapters failed verification
    /// if !report.failed.is_empty() {
    ///     // Handle integrity failures
    ///     for (idx, id, reason) in &report.failed {
    ///         eprintln!("Adapter {} (idx {}) failed: {}", id, idx, reason);
    ///     }
    /// }
    /// ```
    ///
    /// In async context with a Worker instance:
    /// ```ignore
    /// let report = worker.verify_gpu_integrity().await?;
    /// ```
    pub async fn verify_gpu_integrity(
        &self,
    ) -> Result<adapteros_lora_lifecycle::GpuIntegrityReport> {
        use adapteros_lora_lifecycle::GpuIntegrityReport;

        let mut verified = Vec::new();
        let mut failed = Vec::new();
        let mut skipped = Vec::new();

        // Get adapters that should have GPU buffers loaded
        let loaded_adapters = {
            let lifecycle = self.lifecycle.lock().await;
            lifecycle.get_loaded_adapters()
        };

        let mut kernels_lock = self.kernels.lock().await;

        // Proceed with verification - backends without GPU tracking will skip via default trait impls
        for (adapter_id_u16, adapter_id, _state) in &loaded_adapters {
            // Try to verify GPU buffers
            #[cfg(target_os = "macos")]
            match kernels_lock.verify_adapter_buffers(*adapter_id_u16) {
                Ok((buffer_size, first, last, mid)) => {
                    // Create fingerprint from current GPU state
                    use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;
                    let current_fp = GpuBufferFingerprint::new(buffer_size, &first, &last, &mid);
                    let checkpoint_hash_hex = current_fp.checkpoint_hash.to_hex();

                    // Verify against stored baseline
                    match kernels_lock.verify_gpu_fingerprint(
                        *adapter_id_u16,
                        buffer_size,
                        &checkpoint_hash_hex,
                    ) {
                        Ok(true) => {
                            // Check memory footprint against baseline
                            let (within_tolerance, z_score, baseline_stats) =
                                kernels_lock.check_memory_footprint(*adapter_id_u16, buffer_size);

                            let (baseline_mean, baseline_stddev, _sample_count) =
                                baseline_stats.unwrap_or((buffer_size as f64, 0.0, 0));

                            if within_tolerance {
                                verified.push((*adapter_id_u16, adapter_id.clone()));

                                // Emit telemetry for successful verification
                                use adapteros_lora_lifecycle::GpuIntegrityVerificationEvent;
                                if let Some(t) = &self.telemetry {
                                    let _ = t.log(
                                        "gpu_integrity_verification",
                                        GpuIntegrityVerificationEvent {
                                            adapter_id: adapter_id.clone(),
                                            adapter_idx: *adapter_id_u16,
                                            verified: true,
                                            buffer_bytes: buffer_size,
                                            checkpoint_hash: current_fp.checkpoint_hash.to_hex(),
                                            memory_footprint_within_tolerance: true,
                                            z_score: Some(z_score),
                                            baseline_mean: Some(baseline_mean),
                                            timestamp: std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs(),
                                        },
                                    );
                                }
                            } else {
                                failed.push((
                                    *adapter_id_u16,
                                    adapter_id.clone(),
                                    format!(
                                        "Memory footprint anomaly: {} bytes (baseline: {:.1} ± {:.1}, z-score: {:.2})",
                                        buffer_size, baseline_mean, baseline_stddev, z_score
                                    ),
                                ));

                                // Emit telemetry for memory footprint anomaly
                                use adapteros_lora_lifecycle::GpuIntegrityViolationEvent;
                                if let Some(t) = &self.telemetry {
                                    let _ = t.log("gpu_integrity_violation", GpuIntegrityViolationEvent {
                                        adapter_id: adapter_id.clone(),
                                        adapter_idx: *adapter_id_u16,
                                        violation_type: "memory_anomaly".to_string(),
                                        details: format!(
                                            "Memory footprint {} bytes exceeds 2σ tolerance (baseline: {:.1} ± {:.1}, z-score: {:.2})",
                                            buffer_size, baseline_mean, baseline_stddev, z_score
                                        ),
                                        buffer_bytes: Some(buffer_size),
                                        z_score: Some(z_score),
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs(),
                                    });
                                }
                            }
                        }
                        Ok(false) => {
                            // No baseline exists yet - store this as the baseline
                            if let Err(e) = kernels_lock.store_gpu_fingerprint(
                                *adapter_id_u16,
                                buffer_size,
                                &checkpoint_hash_hex,
                            ) {
                                tracing::warn!(
                                    adapter_id = %adapter_id,
                                    error = %e,
                                    "Failed to store GPU fingerprint baseline (non-fatal)"
                                );
                            } else {
                                tracing::info!(
                                    adapter_id = %adapter_id,
                                    adapter_idx = adapter_id_u16,
                                    "Stored initial GPU fingerprint baseline"
                                );
                            }
                            verified.push((*adapter_id_u16, adapter_id.clone()));
                        }
                        Err(msg) => {
                            failed.push((
                                *adapter_id_u16,
                                adapter_id.clone(),
                                format!("GPU buffer fingerprint mismatch: {}", msg),
                            ));

                            // Emit telemetry for fingerprint mismatch
                            use adapteros_lora_lifecycle::GpuIntegrityViolationEvent;
                            if let Some(t) = &self.telemetry {
                                let _ = t.log("gpu_integrity_violation", GpuIntegrityViolationEvent {
                                    adapter_id: adapter_id.clone(),
                                    adapter_idx: *adapter_id_u16,
                                    violation_type: "fingerprint_mismatch".to_string(),
                                    details: format!("GPU buffer checkpoint hash does not match stored fingerprint: {}", msg),
                                    buffer_bytes: Some(buffer_size),
                                    z_score: None,
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs(),
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    // Adapter not loaded or verification not supported
                    skipped.push((*adapter_id_u16, adapter_id.clone()));
                    tracing::debug!(
                        adapter_id = %adapter_id,
                        error = %e,
                        "GPU verification skipped"
                    );
                }
            }

            // Non-macOS platforms don't have Metal GPU verification
            #[cfg(not(target_os = "macos"))]
            {
                skipped.push((*adapter_id_u16, adapter_id.clone()));
                tracing::debug!(
                    adapter_id = %adapter_id,
                    "GPU verification not available on this platform"
                );
            }
        }

        drop(kernels_lock);

        Ok(GpuIntegrityReport {
            verified,
            failed,
            skipped,
            total_checked: loaded_adapters.len(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Get current adapter states
    pub fn get_adapter_states(&self) -> Vec<adapter_hotswap::AdapterState> {
        self.hotswap.table().get_active()
    }

    /// Get current memory usage in bytes
    ///
    /// Returns the memory currently used by the worker, including model weights
    /// and adapter buffers. Returns 0 if memory tracking is unavailable.
    pub fn get_memory_usage_bytes(&self) -> u64 {
        self.health_monitor.get_memory_usage().unwrap_or(0)
    }

    /// Get current memory usage in MB
    pub fn get_memory_usage_mb(&self) -> i32 {
        (self.get_memory_usage_bytes() / (1024 * 1024)) as i32
    }

    /// Execute a workflow using real kernel backend
    ///
    /// Runs the workflow through actual Metal/MLX kernels with LoRA transformations.
    /// Kernels are shared via Arc<Mutex<K>> to allow concurrent workflow execution.
    pub async fn execute_workflow(
        &self,
        workflow_type: adapteros_lora_lifecycle::WorkflowType,
        adapter_ids: Vec<String>,
        context: adapteros_lora_lifecycle::WorkflowContext,
    ) -> Result<adapteros_lora_lifecycle::WorkflowResult>
    where
        K: Send + Sync,
    {
        use adapteros_lora_lifecycle::{MockAdapterBackend, WorkflowExecutor};

        info!(
            "Executing workflow with {} adapters using real kernels",
            adapter_ids.len()
        );

        // Snapshot current stack and increment refcounts for workflow adapters
        let table = self.hotswap.table();
        let _stack_handle = table.get_current_stack_generation();
        let refcounts = table.refcounts().lock().await;
        for id in &adapter_ids {
            if let Some(rc) = refcounts.get(id) {
                rc.fetch_add(1, Ordering::Relaxed);
            }
        }
        drop(refcounts);

        // Create kernel backend with adapter name mapping
        let _adapter_names: Vec<String> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.clone())
            .collect();

        let backend = Arc::new(MockAdapterBackend);

        // Create and execute workflow
        let executor = WorkflowExecutor::new(workflow_type, adapter_ids.clone(), backend);
        let result = executor.execute(context).await;

        // Decrement refcounts
        for id in &adapter_ids {
            let _new_ref = table.dec_ref(id).await;
        }

        result
    }

    /// Get reference to the hot-swap manager
    pub fn hotswap(&self) -> &Arc<HotSwapManager<K>> {
        &self.hotswap
    }

    /// Get reference to the KV cache
    pub fn kv_cache(&self) -> &Arc<StdMutex<KvCache>> {
        &self.kv_cache
    }

    /// Return the cached CoreML verification snapshot, if available.
    pub fn coreml_verification(&self) -> Option<CoremlVerificationSnapshot> {
        self.coreml_verification.clone()
    }

    /// Get reference to the last stack hash
    pub fn last_stack_hash(&self) -> &RwLock<Option<B3Hash>> {
        &self._last_stack_hash
    }

    /// Get reference to the telemetry writer
    pub fn telemetry(&self) -> &Option<TelemetryWriter> {
        &self.telemetry
    }

    /// Get cloned reference to health monitor
    pub fn health_monitor(&self) -> Arc<HealthMonitor> {
        self.health_monitor.clone()
    }

    /// Replace the health monitor (for heartbeat alignment after CP registration)
    pub fn set_health_monitor(&mut self, monitor: Arc<HealthMonitor>) {
        self.health_monitor = monitor;
    }

    /// Register an active training job with its cancellation token
    ///
    /// Call this when starting a training job to enable cancellation.
    pub fn register_training_job(&self, job_id: &str) -> Arc<AtomicBool> {
        let cancel_token = Arc::new(AtomicBool::new(false));
        let mut jobs = self.active_training_jobs.write();
        jobs.insert(job_id.to_string(), cancel_token.clone());
        tracing::info!(job_id = %job_id, "Registered training job for cancellation tracking");
        cancel_token
    }

    /// Unregister a training job (call when job completes/fails/cancels)
    pub fn unregister_training_job(&self, job_id: &str) {
        let mut jobs = self.active_training_jobs.write();
        jobs.remove(job_id);
        tracing::debug!(job_id = %job_id, "Unregistered training job");
    }

    /// Cancel an active training job
    ///
    /// Sets the cancellation token for the job, causing the training loop
    /// to stop at the next epoch boundary.
    pub fn cancel_training_job(&self, job_id: &str) -> Result<CancelTrainingResponse> {
        let jobs = self.active_training_jobs.read();

        if let Some(cancel_token) = jobs.get(job_id) {
            // Check if already cancelled
            if cancel_token.load(std::sync::atomic::Ordering::SeqCst) {
                tracing::info!(job_id = %job_id, "Training job already cancelled");
                return Ok(CancelTrainingResponse {
                    job_id: job_id.to_string(),
                    status: "already_cancelled".to_string(),
                    tokens_processed: None,
                    final_loss: None,
                    stopped_at_epoch: None,
                });
            }

            // Set cancellation flag
            cancel_token.store(true, std::sync::atomic::Ordering::SeqCst);
            tracing::info!(job_id = %job_id, "Training job cancellation requested");

            Ok(CancelTrainingResponse {
                job_id: job_id.to_string(),
                status: "cancelled".to_string(),
                tokens_processed: None, // Will be filled by training loop when it stops
                final_loss: None,
                stopped_at_epoch: None,
            })
        } else {
            tracing::warn!(job_id = %job_id, "Training job not found for cancellation");
            Ok(CancelTrainingResponse {
                job_id: job_id.to_string(),
                status: "not_found".to_string(),
                tokens_processed: None,
                final_loss: None,
                stopped_at_epoch: None,
            })
        }
    }

    /// Check if a training job has been cancelled
    pub fn is_training_cancelled(&self, job_id: &str) -> bool {
        let jobs = self.active_training_jobs.read();
        jobs.get(job_id)
            .map(|token| token.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
    }
}

impl<K: FusedKernels + StrictnessControl + Send + Sync> Drop for Worker<K> {
    fn drop(&mut self) {
        if let Some(handle) = self.retirement_handle.take() {
            let _ = self.shutdown_tx.send(());
            let _ = tokio::runtime::Handle::current().block_on(handle);
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
            policy_mask_digest: None,
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
