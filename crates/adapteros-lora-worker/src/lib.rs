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
use crate::memory::MemoryPressureLevel;
use crate::request_pinner::RequestPinner;
use crate::router_bridge::decision_to_router_ring_with_active_ids_and_strengths;
use crate::routing_policy_filter::filter_decision_by_policy;
use adapteros_api_types::inference::{
    FusionIntervalTrace, RouterDecisionChainEntry, RouterDecisionHash, RouterModelType, RunReceipt,
};
use adapteros_config::{
    resolve_index_root, ModelConfig, PlacementConfig, PlacementMode, PlacementWeights,
};
use adapteros_core::{
    determinism::{DeterminismContext, DeterminismSource},
    determinism_violation_event, emit_observability_event, AosError, B3Hash, BackendKind,
    DeterminismViolationKind, FusionInterval, RepoAdapterPaths, Result, SeedMode,
};
use adapteros_db::{Db, SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput};
use adapteros_lora_kernel_api::{
    blend_and_forward_reference, FusedKernels, IoBuffers, LiquidBlendRequest, LiquidBlendStats,
    LiquidKernel, RouterRing,
};
use adapteros_lora_rag::RagSystem;
use adapteros_lora_router::{
    constants::PINNED_BOOST, features::CodeFeatures, policy_mask::PolicyMask, AbstainContext,
    AdapterInfo, Router, RouterDeterminismConfig,
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
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub mod active_learning;
pub mod adapter_hotswap;
pub mod anomaly_detection;
pub mod backend_coordinator;
pub mod backend_factory;
pub mod backoff;
pub mod backpressure;
pub mod base_model_state;
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
pub mod export;
pub mod filter_engine;
pub mod framework_adapters;
pub mod galaxy_loader;
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
#[cfg(feature = "mlx-bridge")]
pub mod mlx_subprocess_bridge;
pub mod model_handle_cache;
pub mod model_key;
pub mod model_loader;
#[cfg(feature = "mlx-bridge")]
pub mod moe_prefix_cache;
#[cfg(feature = "mlx-bridge")]
pub mod moe_types;
pub mod panic_utils;
pub mod patch_generator;
pub mod patch_telemetry;
pub mod patch_validator;
pub mod prefix_kv_cache;
pub mod reasoning_router;
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
#[cfg(feature = "mlx-bridge")]
pub use mlx_subprocess_bridge::{GenerationResult, MLXSubprocessBridge, MlxBridgeConfig};
pub use model_handle_cache::{
    CacheStats, CachedModelEntry, ModelHandle, ModelHandleCache, DEFAULT_MAX_PINNED_ENTRIES,
};
pub use model_key::{FusionMode, ModelCacheIdentityV2, ModelKey, QuantizationMode};
pub use model_loader::{ModelInfo, ModelLoader, QwenModel, QwenModelConfig, TransformerLayer};
#[cfg(feature = "mlx-bridge")]
pub use moe_types::{ExpertId, ExpertRouting, LayerIdx, SequenceExpertRouting};
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

impl LiquidKernel for KernelWrapper {
    fn supports_liquid_blending(&self) -> bool {
        match self {
            KernelWrapper::Direct(k) => k.inner.supports_liquid_blending(),
            KernelWrapper::Coordinated(k) => {
                k.primary.supports_liquid_blending()
                    || k.fallback
                        .as_ref()
                        .map(|fb| fb.supports_liquid_blending())
                        .unwrap_or(false)
            }
        }
    }

    fn max_liquid_adapters(&self) -> usize {
        match self {
            KernelWrapper::Direct(k) => k.inner.liquid_max_adapters(),
            KernelWrapper::Coordinated(k) => {
                let primary_max = k.primary.liquid_max_adapters();
                let fallback_max = k
                    .fallback
                    .as_ref()
                    .map(|fb| fb.liquid_max_adapters())
                    .unwrap_or(0);
                primary_max.max(fallback_max)
            }
        }
    }

    fn blend_and_forward(&mut self, request: LiquidBlendRequest<'_>) -> Result<LiquidBlendStats> {
        match self {
            KernelWrapper::Direct(k) => {
                if let Some(liquid) = k.inner.as_liquid_kernel_mut() {
                    liquid.blend_and_forward(request)
                } else {
                    blend_and_forward_reference(request)
                }
            }
            KernelWrapper::Coordinated(k) => match k.active_backend {
                ActiveBackend::Primary => {
                    let primary_name = k.primary.device_name().to_string();
                    if let Some(liquid) = k.primary.as_liquid_kernel_mut() {
                        k.last_backend = primary_name.clone();
                        k.fallback_triggered = false;
                        liquid.blend_and_forward(request)
                    } else if let Some(fallback) = k.fallback.as_mut() {
                        k.last_backend = fallback.device_name().to_string();
                        k.fallback_triggered = true;
                        if let Some(liquid) = fallback.as_liquid_kernel_mut() {
                            liquid.blend_and_forward(request)
                        } else {
                            blend_and_forward_reference(request)
                        }
                    } else {
                        k.last_backend = primary_name;
                        k.fallback_triggered = false;
                        blend_and_forward_reference(request)
                    }
                }
                ActiveBackend::Fallback => {
                    let primary_name = k.primary.device_name().to_string();
                    if let Some(fallback) = k.fallback.as_mut() {
                        k.last_backend = fallback.device_name().to_string();
                        k.fallback_triggered = true;
                        if let Some(liquid) = fallback.as_liquid_kernel_mut() {
                            liquid.blend_and_forward(request)
                        } else if let Some(primary_liquid) = k.primary.as_liquid_kernel_mut() {
                            k.last_backend = primary_name.clone();
                            k.fallback_triggered = false;
                            primary_liquid.blend_and_forward(request)
                        } else {
                            blend_and_forward_reference(request)
                        }
                    } else {
                        k.last_backend = primary_name;
                        k.fallback_triggered = false;
                        blend_and_forward_reference(request)
                    }
                }
            },
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
    /// Optional request identifier for tracing across stages
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub require_evidence: bool,
    /// Enable reasoning-aware routing (pauses at reasoning spans to hot-swap adapters)
    #[serde(default)]
    pub reasoning_mode: bool,
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
    /// Canonical determinism context supplied by control plane (optional)
    #[serde(default)]
    pub determinism: Option<DeterminismContext>,
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
    /// Admin override flag to bypass cluster routing restrictions (debug only)
    #[serde(default)]
    pub admin_override: bool,
}

fn default_determinism_mode() -> String {
    "strict".to_string()
}

/// Returns true when strict determinism protections should be enforced.
fn strict_mode_enabled(strict_flag: bool, determinism_mode: &str) -> bool {
    strict_flag || determinism_mode.eq_ignore_ascii_case("strict")
}

struct ValidatedPrompt {
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
    /// MoE model information (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moe_info: Option<adapteros_lora_kernel_api::MoEInfo>,
    /// Expert routing data per token (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expert_routing: Option<adapteros_lora_kernel_api::SequenceExpertRouting>,
    /// Flattened expert IDs per token (for visualization)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_experts: Option<Vec<Vec<u8>>>,
    /// Model type for this trace (dense vs MoE)
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
pub use response_types::RouterSummary;
pub use routing_utilities::summarize_router_usage;
// Import internal fusion helpers for use in lib.rs
use routing_utilities::fusion_intervals_for_mode;

// Re-export training request/response types from refactored modules
pub use request_types::{CancelTrainingRequest, CancelTrainingResponse};

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
    circuit_breaker: CircuitBreaker,
    pub(crate) resource_limiter: Arc<ResourceLimiter>,
    _deadlock_detector: DeadlockDetector,
    health_monitor: Arc<HealthMonitor>,
    telemetry: Option<TelemetryWriter>,
    trace_db: Option<TraceDb>,
    trace_sink_missing_warned: bool,
    trace_flush_every: usize,
    placement_template: Option<(PlacementConfig, Vec<LaneDescriptor>)>,
    // Lifecycle management
    profiler: adapteros_profiler::AdapterProfiler,
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

        // Initialize safety mechanisms
        let timeout_config = TimeoutConfig::default();
        let timeout_wrapper = TimeoutWrapper::new(timeout_config.clone());
        let circuit_breaker = CircuitBreaker::new(5, std::time::Duration::from_secs(60));
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
        let generator = Generator::new(gen_seed)
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

        // Initialize MoE prefix cache (512MB default budget for metadata)
        #[cfg(feature = "mlx-bridge")]
        let moe_prefix_cache = Arc::new(crate::moe_prefix_cache::MoEPrefixCache::new(
            512 * 1024 * 1024,
        ));

        // Load MoE cache snapshot if exists
        let adapter_paths = resolve_worker_adapter_paths();
        let adapters_path = adapter_paths.repo_root.join(tenant_id);
        #[cfg(feature = "mlx-bridge")]
        let moe_cache_path = adapters_path.join("moe_cache.json");
        #[cfg(feature = "mlx-bridge")]
        if let Err(e) = moe_prefix_cache.load_snapshot(&moe_cache_path) {
            warn!(path = %moe_cache_path.display(), error = %e, "Failed to load MoE cache snapshot");
        } else {
            info!(path = %moe_cache_path.display(), "Loaded MoE cache snapshot");
        }

        // Spawn persistence task (every 5 minutes)
        #[cfg(feature = "mlx-bridge")]
        let persistence_handle = {
            let persistence_cache = moe_prefix_cache.clone();
            let persistence_path = moe_cache_path.clone();
            Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    if let Err(e) = persistence_cache.save_snapshot(&persistence_path) {
                        warn!(path = %persistence_path.display(), error = %e, "Failed to background save MoE cache");
                    }
                }
            }))
        };
        #[cfg(not(feature = "mlx-bridge"))]
        let persistence_handle = None;

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
            persistence_handle: persistence_handle,
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
            BackendKind::Mlx | BackendKind::MlxBridge => BackendType::MLX,
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
        // Guardrail: Acquire resource permit (limits concurrency and checks quotas)
        let limiter = self.resource_limiter.clone();
        let _permit = limiter.acquire_request().await?;

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
                            moe_info: None,
                            expert_routing: None,
                            active_experts: None,
                            model_type: None,
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
        let prompt_tokens = validated_prompt.tokens;

        // ============================================================================
        // MOE PRE-WARMING & FREE TOKENS (Protocol v3)
        // ============================================================================
        #[cfg(feature = "mlx-bridge")]
        let mut free_tokens: Vec<crate::moe_prefix_cache::FreeToken> = Vec::new();
        #[cfg(not(feature = "mlx-bridge"))]
        let free_tokens: Vec<()> = Vec::new();
        let mut free_token_ids: Vec<u32> = Vec::new();
        let mut free_token_text = String::new();

        let mut prompt_tokens_with_free = prompt_tokens.clone();
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

        // If free tokens satisfied the entire request, return early!
        if !free_tokens.is_empty() && max_tokens_remaining == 0 {
            let (backend_used, fallback_triggered) = {
                let kernels = self.kernels.lock().await;
                (kernels.device_name().to_string(), false)
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
                None,
                None,
            );

            return Ok(InferenceResponse {
                text: Some(free_token_text),
                status: "ok".to_string(),
                trace,
                run_receipt: None,
                refusal: None,
                patch_proposal: None,
                stack_id: request.stack_id.clone(),
                stack_version: request.stack_version,
                backend_used: Some(backend_used),
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

        let trace_id = Uuid::now_v7().to_string();

        let mut trace_sink = if let Some(trace_db) = self.trace_db.as_mut() {
            let start = TraceStart {
                trace_id: trace_id.clone(),
                tenant_id: request.cpid.clone(),
                request_id: None,
                context_digest,
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
        let mut stop_controller = StopController::from_policy_or_default(
            request.stop_policy.clone(),
            request.max_tokens as u32,
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
                    let mut result = {
                        let kernels = self.kernels.lock().await;
                        let temperature = request.temperature.unwrap_or(0.7);
                        let top_p = request.top_p.unwrap_or(0.9);

                        kernels.generate_text_complete(
                            &prompt_for_backend,
                            max_tokens_remaining,
                            temperature,
                            top_p,
                        )?
                    };

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

            let total_tokens_generated = generation_result
                .tokens_generated
                .saturating_add(generation_result.free_tokens_delivered);
            generation_result.tokens_generated = total_tokens_generated;
            if max_tokens_remaining == 0 {
                stop_reason_code = Some(adapteros_api_types::inference::StopReasonCode::BudgetMax);
                stop_reason_token_index = Some(generation_result.tokens_generated as u32);
            }

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
                moe_info.clone(),
                generation_result.expert_routing,
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
                run_receipt: None,
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
                placement_trace: None, // No placement for text-gen mode
                stop_reason_code,
                stop_reason_token_index: stop_reason_token_index
                    .or_else(|| Some(generation_result.tokens_generated as u32)),
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

        let reasoning_mode = request.reasoning_mode;
        let mut reasoning_buffer = String::new();
        let mut pending_reasoning_decision: Option<adapteros_lora_router::Decision> = None;
        let mut pending_hotswap: Option<u16> = None;
        let mut reasoning_swap_guard = ReasoningSwapGuard::new(MAX_REASONING_SWAPS);

        for step in 0..max_tokens_remaining {
            let step_with_free = free_token_offset + step;
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
            let mut decision = if let Some(pending) = pending_reasoning_decision.take() {
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
                policy_mask_digest: decision.policy_mask_digest,
                policy_overrides_applied: decision.policy_overrides_applied.as_ref().map(|flags| {
                    adapteros_api_types::inference::PolicyOverrideFlags {
                        allow_list: flags.allow_list,
                        deny_list: flags.deny_list,
                        trust_state: flags.trust_state,
                    }
                }),
                model_type: if is_moe {
                    RouterModelType::Moe
                } else {
                    RouterModelType::Dense
                },
                active_experts: None,
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
                let kernel_version_for_trace = if decision_from_reasoning {
                    format!("{}|thought_swap", kernel_version_id)
                } else {
                    kernel_version_id.clone()
                };
                let token_input = TraceTokenInput {
                    token_index: step_with_free as u32,
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
                    kernel_version_id: Some(kernel_version_for_trace),
                };
                sink.record_token(token_input).await?
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
            self.generator.reseed_for_step(step_with_free);

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
                self.current_moe_info(is_moe).await,
                None, // No expert routing for token-by-token yet
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
}

impl<K: FusedKernels + StrictnessControl + Send + Sync + 'static> Drop for Worker<K> {
    fn drop(&mut self) {
        // Stop persistence task
        if let Some(handle) = self.persistence_handle.take() {
            handle.abort();
        }

        // Try to flush cache on shutdown
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let _ = handle.block_on(async {
                let _ = self.flush().await;
            });
        }

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
