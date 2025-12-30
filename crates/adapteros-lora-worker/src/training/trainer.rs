//! Micro-LoRA training loop with forward/backward pass
//!
//! Implements LoRA training with low rank adaptation matrices.
//! This is a Rust-native implementation that avoids Python dependencies
//! and integrates with GPU backends (CoreML, MLX, Metal) for deterministic training.

use super::checkpoint::{CheckpointManager, TrainingCheckpoint};
use super::coreml_pipeline::{
    prepare_coreml_dataset, BatchPlan, CoreMLInputSpec, PreparedDataset, PreparedExample,
};
pub use super::dataset::TrainingExample;
use adapteros_core::{derive_seed, AosError, Result};
use adapteros_db::{Db, TrainingMetricRow};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_router::ROUTER_GATE_Q15_MAX;
use adapteros_telemetry::TelemetryWriter;
use adapteros_types::training::TrainingBackendPolicy;
use chrono::Utc;
use parking_lot::RwLock;
use rand::Rng;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

mod types;
pub use types::{
    DatasetSubsample, DeterminismConfig, DevicePolicyConfig, EpochMetrics, LoRAWeights,
    MoELoRAStrategy, MoETrainingConfig, TrainingBackend, TrainingConfig,
    TrainingPerformanceMetrics, TrainingResult,
};

/// Micro-LoRA trainer with multi-backend GPU support.
/// Base model weights are intentionally not loaded here; only LoRA matrices are
/// ever mutated or registered with optimizers.
pub struct MicroLoRATrainer {
    pub config: TrainingConfig,
    /// GPU kernels for accelerated training
    kernels: Option<crate::backend_factory::KernelBox>,
    /// Selected backend for this training session
    selected_backend: Option<TrainingBackend>,
    /// Selected device description (Metal/MLX/ANE)
    backend_device: Option<String>,
    /// Rationale for backend selection/fallback (for audit/job records)
    backend_reason: Option<String>,
    /// Telemetry writer for training events
    telemetry: TelemetryWriter,
    /// Training seed for deterministic RNG
    training_seed: u64,
    /// Performance metrics for GPU utilization tracking
    performance_metrics: Arc<RwLock<TrainingPerformanceMetrics>>,
    /// Cumulative token counter for the current run
    total_tokens_processed: u64,
    /// Cumulative example counter for the current run
    total_examples_processed: u64,
    /// Optional checkpoint manager for saving/resuming training
    checkpoint_manager: Option<CheckpointManager>,
    /// Cancellation token - set to true to request training stop
    cancel_token: Option<Arc<AtomicBool>>,
    /// Job ID for this training run (used for metrics persistence and cancellation)
    job_id: Option<String>,
    /// Optional database connection for metrics persistence
    db: Option<Db>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackendAvailability {
    coreml: bool,
    mlx: bool,
    metal: bool,
}

impl BackendAvailability {
    fn any_gpu(&self) -> bool {
        self.coreml || self.mlx || self.metal
    }
}

impl MicroLoRATrainer {
    /// Derive a deterministic seed from training config context.
    ///
    /// This creates a reproducible seed from config parameters (rank, hidden_dim,
    /// epochs, batch_size, dataset_version_id) for use when no explicit seed is provided.
    fn derive_seed_from_context(config: &TrainingConfig) -> u64 {
        // Build a deterministic label from config context
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"lora_trainer_v2");
        hasher.update(&(config.rank as u64).to_le_bytes());
        hasher.update(&(config.hidden_dim as u64).to_le_bytes());
        hasher.update(&(config.epochs as u64).to_le_bytes());
        hasher.update(&(config.batch_size as u64).to_le_bytes());
        hasher.update(&config.alpha.to_le_bytes());
        hasher.update(&config.learning_rate.to_le_bytes());

        // Include dataset_version_id if available for job-specific determinism
        if let Some(ref det) = config.determinism {
            if let Some(ref version_id) = det.dataset_version_id {
                hasher.update(version_id.as_bytes());
            }
        }

        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }

    /// Create a new trainer with configuration
    pub fn new(mut config: TrainingConfig) -> Result<Self> {
        // Derive deterministic training seed
        //
        // Issue D-3 Fix: Don't filter out seed=0 (it's a valid seed value).
        // When determinism.seed is explicitly set (including 0), use it directly.
        // When not set, derive from config context for reproducibility.
        let training_seed = if let Some(ref det) = config.determinism {
            if let Some(explicit_seed) = det.seed {
                // Use explicit seed directly (including 0 - it's a valid seed!)
                explicit_seed
            } else {
                // Derive seed from determinism context when available
                Self::derive_seed_from_context(&config)
            }
        } else {
            // No determinism config - derive from training config context
            Self::derive_seed_from_context(&config)
        };

        // Align hidden_dim/rank with CoreML placement when provided
        if let Some(placement) = config.coreml_placement.as_ref() {
            if placement.bindings.is_empty() {
                return Err(AosError::Training(
                    "CoreML placement provided but contains no bindings".to_string(),
                ));
            }
            let first = &placement.bindings[0];
            let placement_hidden = first.shape.output_dim as usize;
            if placement_hidden == 0 {
                return Err(AosError::Training(
                    "CoreML placement binding has zero output_dim".to_string(),
                ));
            }
            if config.hidden_dim != placement_hidden {
                info!(
                    hidden_dim = config.hidden_dim,
                    placement_hidden, "Adjusting hidden_dim to CoreML placement output_dim"
                );
                config.hidden_dim = placement_hidden;
            }
            // Ensure binding ranks align with config.rank
            for binding in placement.bindings.iter() {
                if binding.rank as usize != config.rank {
                    return Err(AosError::Training(format!(
                        "CoreML placement rank {} does not match training rank {} for binding {}",
                        binding.rank, config.rank, binding.binding_id
                    )));
                }
            }
        }

        // Initialize telemetry
        let telemetry = TelemetryWriter::new("training", 1000, 1024 * 1024)?;

        info!(
            "Created MicroLoRA trainer with seed: {}, GPU optional: {}",
            training_seed, !config.require_gpu
        );

        let config_for_metrics = config.clone();

        Ok(Self {
            config,
            kernels: None,
            selected_backend: None,
            backend_device: None,
            backend_reason: None,
            telemetry,
            training_seed,
            performance_metrics: Arc::new(RwLock::new(TrainingPerformanceMetrics {
                total_gpu_time_ms: 0,
                total_cpu_time_ms: 0,
                gpu_operations: 0,
                cpu_operations: 0,
                avg_gpu_utilization: 0.0,
                peak_gpu_memory_mb: 0.0,
                total_batches: 0,
                throughput_examples_per_sec: 0.0,
                total_tokens_processed: 0,
                total_examples_processed: 0,
                coreml_forward_mean_us: None,
                coreml_forward_p95_us: None,
                coreml_forward_total_us: 0,
                coreml_forward_samples: 0,
                coreml_forward_latency_samples: VecDeque::new(),
                effective_batch_size: config_for_metrics.batch_size,
                max_tokens_per_batch: config_for_metrics.max_tokens_per_batch.unwrap_or_else(
                    || config_for_metrics.batch_size * config_for_metrics.hidden_dim * 2,
                ),
                sequences_truncated: 0,
                sequences_dropped: 0,
                device_tier: None,
                input_shape: None,
            })),
            total_tokens_processed: 0,
            total_examples_processed: 0,
            checkpoint_manager: None,
            cancel_token: None,
            job_id: None,
            db: None,
        })
    }

    /// Detect backend availability using runtime capability detection
    fn detect_backend_availability() -> BackendAvailability {
        #[cfg(any(test, debug_assertions))]
        if let Some(forced) = Self::forced_backend_override() {
            return forced;
        }

        let caps = crate::backend_factory::detect_capabilities();
        BackendAvailability {
            coreml: caps.has_coreml && caps.has_ane,
            mlx: caps.has_mlx,
            metal: caps.has_metal,
        }
    }

    /// Allow tests/dev builds to force backend availability via env
    #[cfg(any(test, debug_assertions))]
    fn forced_backend_override() -> Option<BackendAvailability> {
        if let Ok(value) = std::env::var("AOS_FORCE_GPU_BACKEND") {
            let val = value.to_ascii_lowercase();
            let forced = match val.as_str() {
                "coreml" | "ane" => BackendAvailability {
                    coreml: true,
                    mlx: false,
                    metal: false,
                },
                "mlx" => BackendAvailability {
                    coreml: false,
                    mlx: true,
                    metal: false,
                },
                "metal" => BackendAvailability {
                    coreml: false,
                    mlx: false,
                    metal: true,
                },
                "all" => BackendAvailability {
                    coreml: true,
                    mlx: true,
                    metal: true,
                },
                "none" | "cpu" => BackendAvailability {
                    coreml: false,
                    mlx: false,
                    metal: false,
                },
                _ => return None,
            };

            tracing::info!(
                forced = %val,
                "Forcing backend availability via AOS_FORCE_GPU_BACKEND (test/dev only)"
            );
            return Some(forced);
        }

        None
    }

    /// Detect available GPU backends and select optimal one
    #[allow(dead_code)]
    fn detect_available_backends() -> Vec<(TrainingBackend, String)> {
        let availability = Self::detect_backend_availability();
        let mut backends = Vec::new();

        if availability.coreml {
            backends.push((
                TrainingBackend::CoreML,
                "CoreML with ANE available".to_string(),
            ));
        }

        if availability.mlx {
            backends.push((TrainingBackend::Mlx, "MLX backend available".to_string()));
        }

        if availability.metal {
            backends.push((TrainingBackend::Metal, "Metal GPU available".to_string()));
        }

        backends.push((TrainingBackend::Cpu, "CPU-only training".to_string()));

        backends
    }

    /// Get a description of available backends
    pub fn describe_available_backends() -> String {
        let availability = Self::detect_backend_availability();
        let mut desc = String::from("Available training backends:\n");

        desc.push_str(&format!(
            "  - CoreML (ANE): {}\n",
            if availability.coreml {
                "available"
            } else {
                "unavailable (missing ANE or feature)"
            }
        ));
        desc.push_str(&format!(
            "  - MLX: {}\n",
            if availability.mlx {
                "available"
            } else {
                "unavailable (feature/runtime)"
            }
        ));
        desc.push_str(&format!(
            "  - Metal: {}\n",
            if availability.metal {
                "available"
            } else {
                "unavailable (no macOS Metal device)"
            }
        ));
        desc.push_str("  - CPU: always available\n");

        desc
    }

    /// Validate GPU requirements and provide actionable error messages
    fn validate_gpu_requirements(&self, availability: &BackendAvailability) -> Result<()> {
        if !self.config.require_gpu {
            return Ok(());
        }

        if !availability.any_gpu() {
            let available_desc = Self::describe_available_backends();
            error!(
                "GPU acceleration required but no GPU backends available\n{}",
                available_desc
            );
            return Err(AosError::Config(format!(
                "GPU acceleration required but no suitable GPU backend available. {}",
                available_desc
            )));
        }

        if self.config.preferred_backend == Some(TrainingBackend::Cpu) {
            return Err(AosError::Config(
                "GPU acceleration required but preferred backend is CPU".to_string(),
            ));
        }

        Ok(())
    }

    /// Determine if a backend is available given detected capabilities
    fn backend_is_available(backend: TrainingBackend, availability: &BackendAvailability) -> bool {
        match backend {
            TrainingBackend::CoreML => availability.coreml,
            TrainingBackend::Mlx => availability.mlx,
            TrainingBackend::Metal => availability.metal,
            TrainingBackend::Cpu => true,
        }
    }

    /// Append a backend selection/fallback rationale (deduplicates separator).
    fn append_backend_reason<S: Into<String>>(&mut self, note: S) {
        let note = note.into();
        if let Some(existing) = &mut self.backend_reason {
            if !existing.is_empty() {
                existing.push_str("; ");
            }
            existing.push_str(&note);
        } else {
            self.backend_reason = Some(note);
        }
    }

    /// Resolve preferred device order into concrete backends with de-duplication.
    fn resolve_device_order(&self) -> Vec<TrainingBackend> {
        let mut order: Vec<TrainingBackend> = Vec::new();
        for label in self.config.device_policy_order() {
            if let Some(b) = Self::parse_backend_label(&label) {
                if !order.contains(&b) {
                    order.push(b);
                }
            }
        }
        if order.is_empty() {
            order = vec![
                TrainingBackend::CoreML,
                TrainingBackend::Mlx,
                TrainingBackend::Metal,
                TrainingBackend::Cpu,
            ];
        }
        order
    }

    fn parse_backend_label(label: &str) -> Option<TrainingBackend> {
        match label.to_ascii_lowercase().as_str() {
            "coreml" | "ane" => Some(TrainingBackend::CoreML),
            "mlx" => Some(TrainingBackend::Mlx),
            "metal" | "gpu" => Some(TrainingBackend::Metal),
            "cpu" => Some(TrainingBackend::Cpu),
            _ => None,
        }
    }

    /// Build the candidate backend list according to policy and availability
    fn build_backend_candidates(
        &mut self,
        availability: &BackendAvailability,
    ) -> Result<Vec<TrainingBackend>> {
        let mut candidates: Vec<TrainingBackend> = Vec::new();

        if let Some(policy) = self.config.backend_policy {
            match policy {
                TrainingBackendPolicy::CoremlOnly => {
                    if Self::backend_is_available(TrainingBackend::CoreML, availability) {
                        candidates.push(TrainingBackend::CoreML);
                        return Ok(candidates);
                    }
                    self.append_backend_reason("coreml_only_unavailable");
                    return Err(AosError::Config(
                        "CoreML backend requested (coreml_only) but unavailable".to_string(),
                    ));
                }
                TrainingBackendPolicy::CoremlElseFallback => {
                    if Self::backend_is_available(TrainingBackend::CoreML, availability) {
                        candidates.push(TrainingBackend::CoreML);
                        return Ok(candidates);
                    }
                    self.append_backend_reason("coreml_policy_fallback");
                    if let Some(fallback_backend) = self.config.coreml_fallback_backend {
                        if Self::backend_is_available(fallback_backend, availability) {
                            candidates.push(fallback_backend);
                            return Ok(candidates);
                        }
                    }
                }
                TrainingBackendPolicy::Auto => {}
            }
        }

        // Handle preferred backend first (honor user intent).
        if let Some(preferred) = self.config.preferred_backend {
            if Self::backend_is_available(preferred, availability) {
                candidates.push(preferred);
            } else if preferred == TrainingBackend::CoreML {
                self.append_backend_reason("coreml_unavailable");
                if let Some(fallback_backend) = self.config.coreml_fallback_backend {
                    if self.config.require_gpu && fallback_backend == TrainingBackend::Cpu {
                        warn!(
                            "CoreML requested with CPU fallback while require_gpu=true; skipping CPU fallback"
                        );
                    } else if Self::backend_is_available(fallback_backend, availability) {
                        self.append_backend_reason(format!(
                            "coreml_unavailable_fallback_to_{}",
                            fallback_backend.tag()
                        ));
                        candidates.push(fallback_backend);
                    }
                }
            } else {
                warn!(
                    "Preferred backend {} unavailable, applying policy fallback",
                    preferred.name()
                );
            }
        }

        // Policy-driven ordering (default: CoreML → MLX → Metal → CPU)
        for backend in self.resolve_device_order() {
            if backend == TrainingBackend::Cpu && self.config.require_gpu {
                continue;
            }
            if Self::backend_is_available(backend, availability) && !candidates.contains(&backend) {
                candidates.push(backend);
            } else if !Self::backend_is_available(backend, availability) {
                self.append_backend_reason(format!("{}_unavailable", backend.tag()));
            }
        }

        // CPU only if GPU optional and policy allows.
        let cpu_allowed = self
            .config
            .device_policy
            .as_ref()
            .map(|p| p.allow_cpu_fallback)
            .unwrap_or(true);
        if !self.config.require_gpu && cpu_allowed && !candidates.contains(&TrainingBackend::Cpu) {
            candidates.push(TrainingBackend::Cpu);
        }

        if self.config.require_gpu && candidates.is_empty() {
            let available_desc = Self::describe_available_backends();
            return Err(AosError::Config(format!(
                "GPU required but no backends available. {}",
                available_desc
            )));
        }

        if candidates.is_empty() {
            candidates.push(TrainingBackend::Cpu);
        }

        Ok(candidates)
    }

    fn compute_batch_plan(&mut self, backend: TrainingBackend) -> BatchPlan {
        let mut effective = std::cmp::max(1, self.config.batch_size);
        let cap = match backend {
            TrainingBackend::CoreML => {
                if self.config.hidden_dim > 2048 {
                    2
                } else if self.config.hidden_dim > 1024 {
                    4
                } else {
                    8
                }
            }
            TrainingBackend::Mlx | TrainingBackend::Metal => {
                if self.config.hidden_dim > 4096 {
                    2
                } else if self.config.hidden_dim > 2048 {
                    4
                } else {
                    8
                }
            }
            TrainingBackend::Cpu => 1,
        };

        if effective > cap {
            self.append_backend_reason(format!("batch_capped_to_{}", cap));
            effective = cap;
        }

        let context_window = self.config.effective_context_window();
        let default_tokens = context_window
            .saturating_mul(2)
            .saturating_mul(effective)
            .max(context_window);
        let max_tokens = self.config.max_tokens_per_batch.unwrap_or(default_tokens);

        BatchPlan {
            effective_batch_size: effective,
            max_tokens_per_batch: std::cmp::max(1, max_tokens),
            sequences_truncated: 0,
            sequences_dropped: 0,
        }
    }

    fn backend_device_tier(backend: TrainingBackend) -> &'static str {
        match backend {
            TrainingBackend::CoreML => "ane",
            TrainingBackend::Mlx | TrainingBackend::Metal => "gpu",
            TrainingBackend::Cpu => "cpu",
        }
    }

    fn prepare_dataset_for_training(
        &mut self,
        examples: &[TrainingExample],
    ) -> Result<PreparedDataset> {
        let backend = self.selected_backend.unwrap_or(TrainingBackend::Cpu);
        let plan = self.compute_batch_plan(backend);
        let spec = CoreMLInputSpec {
            hidden_dim: self.config.hidden_dim,
            vocab_size: self.config.vocab_size,
            context_window: self.config.effective_context_window(),
        };
        let spec_for_logs = spec.clone();

        let mut prepared = prepare_coreml_dataset(
            examples,
            spec,
            plan.effective_batch_size,
            Some(plan.max_tokens_per_batch),
        )?;
        // Preserve planner metadata (currently zeroed in pipeline).
        prepared.batch_plan.sequences_truncated = plan.sequences_truncated;
        prepared.batch_plan.sequences_dropped = plan.sequences_dropped;

        self.record_preparation_metrics(backend, &plan, &prepared);

        self.telemetry
            .log(
                "training.dataset_prepared",
                serde_json::json!({
                    "examples": prepared.summary.total_examples,
                    "tokens": prepared.summary.total_tokens,
                    "min_seq_len": prepared.summary.min_seq_len,
                    "max_seq_len": prepared.summary.max_seq_len,
                    "avg_seq_len": prepared.summary.avg_seq_len,
                    "batch_size": prepared.batch_plan.effective_batch_size,
                    "max_tokens_per_batch": prepared.batch_plan.max_tokens_per_batch,
                    "length_histogram_bucket": prepared.summary.length_histogram.bucket_size,
                    "length_histogram": prepared.summary.length_histogram.buckets,
                    "device": backend.name(),
                    "device_tier": Self::backend_device_tier(backend),
                    "context_window": spec_for_logs.context_window,
                    "hidden_dim": spec_for_logs.hidden_dim
                }),
            )
            .ok();

        Ok(prepared)
    }

    fn record_preparation_metrics(
        &self,
        backend: TrainingBackend,
        plan: &BatchPlan,
        dataset: &PreparedDataset,
    ) {
        let observed_batch = dataset
            .batches
            .iter()
            .map(|b| b.examples.len())
            .max()
            .unwrap_or(plan.effective_batch_size);
        let mut metrics = self.performance_metrics.write();
        metrics.effective_batch_size = plan.effective_batch_size;
        metrics.max_tokens_per_batch = plan.max_tokens_per_batch;
        metrics.sequences_truncated =
            plan.sequences_truncated as u64 + dataset.batch_plan.sequences_truncated as u64;
        metrics.sequences_dropped =
            plan.sequences_dropped as u64 + dataset.batch_plan.sequences_dropped as u64;
        metrics.device_tier = Some(Self::backend_device_tier(backend).to_string());
        metrics.input_shape = Some((observed_batch, self.config.effective_context_window()));
    }

    /// Initialize GPU kernels for training with automatic backend selection
    ///
    /// Selection policy (ADR-aligned):
    /// 1. Preferred backend if available
    /// 2. CoreML (ANE) → MLX → Metal
    /// 3. CPU only when GPU is optional
    pub fn init_kernels(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let availability = Self::detect_backend_availability();

        // Validate GPU requirements first
        self.validate_gpu_requirements(&availability)?;

        let candidates = self.build_backend_candidates(&availability)?;
        let mut errors: Vec<String> = Vec::new();

        for backend in candidates {
            // Return early for CPU-only training (no kernel initialization needed)
            if backend == TrainingBackend::Cpu {
                self.append_backend_reason("selected_cpu_optional_gpu_or_no_candidate");
                self.selected_backend = Some(TrainingBackend::Cpu);
                self.backend_device = self.resolve_backend_device(TrainingBackend::Cpu);
                self.kernels = None;
                info!("Training will run on CPU (GPU not selected or unavailable)");
                self.telemetry
                    .log(
                        "training.backend_selected",
                        serde_json::json!({
                            "backend": backend.name(),
                            "reason": "cpu-fallback-or-preference",
                            "plan_size": plan_bytes.len(),
                            "seed": self.training_seed,
                            "require_gpu": self.config.require_gpu,
                            "backend_device": self.backend_device,
                            "job_id": self.job_id,
                        }),
                    )
                    .ok();
                return Ok(());
            }

            let reason = if self.config.preferred_backend == Some(backend) {
                "user-specified backend"
            } else {
                "policy-selected backend"
            };

            info!(
                "Initializing {} kernels for training: {}",
                backend.name(),
                reason
            );

            self.backend_device = self.resolve_backend_device(backend);

            self.telemetry
                .log(
                    "training.backend_selected",
                    serde_json::json!({
                        "backend": backend.name(),
                        "reason": reason,
                        "plan_size": plan_bytes.len(),
                        "seed": self.training_seed,
                        "require_gpu": self.config.require_gpu,
                        "backend_device": self.backend_device,
                        "job_id": self.job_id,
                    }),
                )
                .ok();

            match self.init_gpu_backend(backend, plan_bytes) {
                Ok(()) => {
                    self.selected_backend = Some(backend);
                    self.backend_device = self
                        .backend_device
                        .clone()
                        .or_else(|| self.resolve_backend_device(backend));
                    let selection_reason = if self.config.preferred_backend == Some(backend) {
                        "preferred"
                    } else if self.config.preferred_backend == Some(TrainingBackend::CoreML) {
                        "coreml_fallback"
                    } else {
                        "policy"
                    };
                    self.append_backend_reason(format!(
                        "selected_{}({})",
                        backend.tag(),
                        selection_reason
                    ));
                    if backend == TrainingBackend::CoreML {
                        self.telemetry
                            .log(
                                "training.coreml_started",
                                serde_json::json!({
                                    "backend": backend.name(),
                                    "backend_device": self.backend_device,
                                    "plan_size": plan_bytes.len(),
                                    "job_id": self.job_id,
                                }),
                            )
                            .ok();
                    } else if self.config.preferred_backend == Some(TrainingBackend::CoreML) {
                        self.telemetry
                            .log(
                                "training.coreml_fallback",
                                serde_json::json!({
                                    "fallback_backend": backend.tag(),
                                    "backend_device": self.backend_device,
                                    "job_id": self.job_id,
                                    "reason": self.backend_reason,
                                }),
                            )
                            .ok();
                    }
                    info!(
                        "Successfully initialized {} backend for training",
                        backend.name()
                    );
                    return Ok(());
                }
                Err(e) => {
                    errors.push(format!("{}: {}", backend.name(), e));
                    if backend == TrainingBackend::CoreML {
                        self.telemetry
                            .log(
                                "training.coreml_fallback",
                                serde_json::json!({
                                    "fallback_backend": "init_failure",
                                    "reason": e.to_string(),
                                    "job_id": self.job_id,
                                }),
                            )
                            .ok();
                    }
                    if self.config.require_gpu {
                        warn!(
                            "Failed to initialize required GPU backend {}: {}, trying next candidate if available",
                            backend.name(),
                            e
                        );
                        continue;
                    } else {
                        warn!(
                            "Failed to initialize {} backend: {}, attempting fallback",
                            backend.name(),
                            e
                        );
                        self.telemetry
                            .log(
                                "training.gpu_fallback",
                                serde_json::json!({
                                    "original_backend": backend.name(),
                                    "reason": e.to_string(),
                                    "using_cpu": false,
                                    "job_id": self.job_id,
                                }),
                            )
                            .ok();
                        continue;
                    }
                }
            }
        }

        if self.config.require_gpu {
            return Err(AosError::Config(format!(
                "Failed to initialize GPU backend(s): {}",
                errors.join("; ")
            )));
        }

        // Optional GPU: final fallback to CPU
        self.selected_backend = Some(TrainingBackend::Cpu);
        self.backend_device = self.resolve_backend_device(TrainingBackend::Cpu);
        self.kernels = None;
        let error_summary = if errors.is_empty() {
            "none".to_string()
        } else {
            errors.join("; ")
        };
        self.append_backend_reason(format!("cpu_fallback_after_gpu_errors: {}", error_summary));
        self.telemetry
            .log(
                "training.gpu_fallback",
                serde_json::json!({
                    "original_backend": "all GPU candidates",
                    "reason": errors.join("; "),
                    "using_cpu": true,
                    "job_id": self.job_id,
                }),
            )
            .ok();
        if self.config.preferred_backend == Some(TrainingBackend::CoreML) {
            self.telemetry
                .log(
                    "training.coreml_fallback",
                    serde_json::json!({
                        "fallback_backend": "cpu",
                        "reason": self.backend_reason,
                        "job_id": self.job_id,
                    }),
                )
                .ok();
        }
        info!("GPU optional and all candidates failed, using CPU training");
        Ok(())
    }

    /// Initialize a specific GPU backend
    fn init_gpu_backend(&mut self, backend: TrainingBackend, plan_bytes: &[u8]) -> Result<()> {
        use crate::backend_factory::{create_backend, BackendChoice};

        let backend_choice = match backend {
            #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
            TrainingBackend::CoreML => {
                info!("Creating CoreML backend with Neural Engine acceleration");
                BackendChoice::CoreML
            }
            #[cfg(all(target_os = "macos", not(feature = "coreml-backend")))]
            TrainingBackend::CoreML => {
                return Err(AosError::Config(
                    "CoreML backend requested but 'coreml-backend' feature not enabled".to_string(),
                ));
            }
            #[cfg(not(target_os = "macos"))]
            TrainingBackend::CoreML => {
                return Err(AosError::Config(
                    "CoreML backend requires macOS".to_string(),
                ));
            }

            #[cfg(target_os = "macos")]
            TrainingBackend::Metal => {
                info!("Creating Metal GPU backend");
                BackendChoice::Metal
            }
            #[cfg(not(target_os = "macos"))]
            TrainingBackend::Metal => {
                return Err(AosError::Config("Metal backend requires macOS".to_string()));
            }

            #[cfg(feature = "multi-backend")]
            TrainingBackend::Mlx => {
                info!("Creating MLX backend for training (requires AOS_MODEL_PATH)");
                BackendChoice::Mlx
            }
            #[cfg(not(feature = "multi-backend"))]
            TrainingBackend::Mlx => {
                return Err(AosError::Config(
                    "MLX backend requires 'multi-backend' feature".to_string(),
                ));
            }

            TrainingBackend::Cpu => {
                return Err(AosError::Internal(
                    "CPU backend should not be initialized via GPU path".to_string(),
                ));
            }
        };

        // Create and initialize backend
        let mut kernel = create_backend(backend_choice).map_err(|e| {
            error!("Failed to create {} backend: {}", backend.name(), e);
            e
        })?;

        // Load plan
        kernel.load(plan_bytes).map_err(|e| {
            error!(
                "Failed to load plan on {} backend (size={}): {}",
                backend.name(),
                plan_bytes.len(),
                e
            );
            e
        })?;

        self.kernels = Some(kernel);

        // Log kernel initialization success
        self.telemetry
            .log(
                "training.kernels_initialized",
                serde_json::json!({
                    "backend": backend.name(),
                    "plan_size": plan_bytes.len(),
                    "seed": self.training_seed,
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                }),
            )
            .ok();

        Ok(())
    }

    /// Resolve a human-readable device name for telemetry/metrics
    fn resolve_backend_device(&self, backend: TrainingBackend) -> Option<String> {
        match backend {
            TrainingBackend::Metal => {
                let caps = crate::backend_factory::detect_capabilities();
                caps.metal_device_name
            }
            TrainingBackend::CoreML => Some("Apple Neural Engine".to_string()),
            TrainingBackend::Mlx => {
                #[cfg(feature = "multi-backend")]
                {
                    adapteros_lora_mlx_ffi::mlx_get_backend_capabilities()
                        .ok()
                        .and_then(|c| {
                            let name = c.device_name_str().to_string();
                            if name.is_empty() {
                                None
                            } else {
                                Some(name)
                            }
                        })
                }
                #[cfg(not(feature = "multi-backend"))]
                {
                    None
                }
            }
            TrainingBackend::Cpu => Some("CPU".to_string()),
        }
    }

    /// Get information about the selected training backend
    pub fn backend_info(&self) -> Option<&'static str> {
        self.selected_backend.map(|b| b.name())
    }

    /// Get the reason/rationale for backend selection or fallback
    pub fn backend_reason(&self) -> Option<&str> {
        self.backend_reason.as_deref()
    }

    /// Check if training will use GPU acceleration
    pub fn using_gpu(&self) -> bool {
        matches!(
            self.selected_backend,
            Some(TrainingBackend::CoreML | TrainingBackend::Metal | TrainingBackend::Mlx)
        )
    }

    /// Get the training seed used for deterministic RNG
    ///
    /// Returns the 64-bit seed derived from HKDF during trainer construction.
    /// Two trainers with identical configuration will have the same seed,
    /// ensuring deterministic training results.
    pub fn training_seed(&self) -> u64 {
        self.training_seed
    }

    /// Resolve the target epoch count, honoring deterministic harness overrides.
    pub fn target_epochs(&self) -> usize {
        self.config
            .determinism
            .as_ref()
            .and_then(|d| d.max_steps)
            .filter(|steps| *steps > 0)
            .unwrap_or(self.config.epochs)
    }

    /// Set the cancellation token for this training run
    ///
    /// The token should be an `Arc<AtomicBool>` shared with the worker that can
    /// be set to `true` to request cancellation. The training loop checks this
    /// token at epoch boundaries and stops gracefully when set.
    pub fn set_cancel_token(&mut self, token: Arc<AtomicBool>) {
        self.cancel_token = Some(token);
    }

    /// Set the job ID for this training run
    ///
    /// The job ID is used for metrics persistence and logging.
    pub fn set_job_id(&mut self, job_id: String) {
        self.job_id = Some(job_id);
    }

    /// Set the database connection for metrics persistence
    ///
    /// When set, the trainer will persist metrics (loss, tokens/sec, etc.)
    /// to the `repository_training_metrics` table after each epoch.
    pub fn set_db(&mut self, db: Db) {
        self.db = Some(db);
    }

    /// Check if cancellation has been requested
    ///
    /// Returns `true` if the cancellation token is set and has been triggered.
    fn is_cancelled(&self) -> bool {
        self.cancel_token
            .as_ref()
            .map(|t| t.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// Persist training metrics to database
    ///
    /// Writes key metrics (loss, tokens_per_sec, etc.) to the repository_training_metrics table.
    /// If no DB is configured or job_id is not set, this is a no-op.
    async fn persist_epoch_metrics(
        &self,
        epoch: u32,
        step: u64,
        loss: f32,
        examples_count: u64,
        tokens_count: u64,
        epoch_duration_us: u64,
    ) {
        let (job_id, db) = match (&self.job_id, &self.db) {
            (Some(jid), Some(db)) => (jid.clone(), db.clone()),
            _ => return, // No DB or job_id, skip persistence
        };

        let timestamp = Utc::now().to_rfc3339();
        let tokens_per_sec = if epoch_duration_us > 0 {
            (tokens_count as f64) / (epoch_duration_us as f64 / 1_000_000.0)
        } else {
            0.0
        };
        let examples_per_sec = if epoch_duration_us > 0 {
            (examples_count as f64) / (epoch_duration_us as f64 / 1_000_000.0)
        } else {
            0.0
        };

        let metrics = vec![
            TrainingMetricRow {
                id: Uuid::now_v7().to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "loss".to_string(),
                metric_value: loss as f64,
                metric_timestamp: Some(timestamp.clone()),
            },
            TrainingMetricRow {
                id: Uuid::now_v7().to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "tokens_per_sec".to_string(),
                metric_value: tokens_per_sec,
                metric_timestamp: Some(timestamp),
            },
            TrainingMetricRow {
                id: Uuid::now_v7().to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "examples_per_sec".to_string(),
                metric_value: examples_per_sec,
                metric_timestamp: Some(Utc::now().to_rfc3339()),
            },
            TrainingMetricRow {
                id: Uuid::now_v7().to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "tokens_processed".to_string(),
                metric_value: tokens_count as f64,
                metric_timestamp: Some(Utc::now().to_rfc3339()),
            },
        ];

        if let Err(e) = db.insert_training_metrics_batch(&metrics).await {
            warn!(
                job_id = %job_id,
                epoch = epoch,
                error = %e,
                "Failed to persist training metrics (non-fatal)"
            );
        } else {
            debug!(
                job_id = %job_id,
                epoch = epoch,
                loss = loss,
                tokens_per_sec = tokens_per_sec,
                "Training metrics persisted"
            );
        }
    }

    /// Enable checkpoint saving for resumable training
    ///
    /// This configures the trainer to save checkpoints periodically during training.
    /// Checkpoints allow training to be resumed from interruptions.
    ///
    /// # Arguments
    /// * `checkpoint_dir` - Directory to store checkpoint files
    /// * `adapter_id` - Adapter ID for naming checkpoint files
    /// * `max_checkpoints` - Maximum number of checkpoints to retain (older ones are deleted)
    pub fn enable_checkpointing<P: AsRef<std::path::Path>>(
        &mut self,
        checkpoint_dir: P,
        adapter_id: &str,
        max_checkpoints: usize,
    ) {
        let interval = self.config.checkpoint_interval.unwrap_or(5);
        self.checkpoint_manager = Some(CheckpointManager::new(
            checkpoint_dir,
            interval,
            max_checkpoints,
            adapter_id.to_string(),
        ));
        info!(
            adapter_id = %adapter_id,
            interval = interval,
            max_checkpoints = max_checkpoints,
            "Checkpoint saving enabled"
        );
    }

    /// Resume training from a checkpoint
    ///
    /// Loads the latest checkpoint and returns the starting epoch and weights.
    /// Returns None if no checkpoint exists.
    pub async fn try_resume_from_checkpoint(&self) -> Option<(u32, LoRAWeights, f32)> {
        let manager = self.checkpoint_manager.as_ref()?;

        if !manager.has_checkpoint().await {
            info!("No checkpoint found, starting fresh training");
            return None;
        }

        match manager.load_latest().await {
            Ok(checkpoint) => {
                info!(
                    epoch = checkpoint.epoch,
                    loss = checkpoint.loss,
                    "Resuming training from checkpoint"
                );
                Some((checkpoint.epoch, checkpoint.weights, checkpoint.best_loss))
            }
            Err(e) => {
                warn!(error = %e, "Failed to load checkpoint, starting fresh training");
                None
            }
        }
    }

    /// Train LoRA adapter on examples with GPU acceleration (if available)
    ///
    /// This method provides backward compatibility with automatic progress callback.
    /// For more control, use `train_with_callback` instead.
    pub async fn train(&mut self, examples: &[TrainingExample]) -> Result<TrainingResult> {
        // Backward-compatible behavior: no external progress callback
        self.train_with_callback(examples, |_| {}).await
    }

    /// Train with automatic checkpoint resume
    ///
    /// If a checkpoint exists, resumes from the saved state. Otherwise starts fresh.
    /// This method automatically enables checkpointing if configured.
    pub async fn train_with_resume<C>(
        &mut self,
        examples: &[TrainingExample],
        on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        // Try to resume from checkpoint
        let resume_state = self.try_resume_from_checkpoint().await;

        let prepared_dataset = self.prepare_dataset_for_training(examples)?;

        if let Some((start_epoch, weights, _best_loss)) = resume_state {
            info!(
                start_epoch = start_epoch,
                "Resuming training from checkpoint"
            );
            self.run_training(
                prepared_dataset,
                start_epoch as usize,
                Some(weights),
                on_epoch,
            )
            .await
        } else {
            self.run_training(prepared_dataset, 0, None, on_epoch).await
        }
    }

    /// Train starting from a specific epoch with optional initial weights
    #[cfg(any())]
    #[allow(dead_code)]
    async fn train_from_epoch<C>(
        &mut self,
        examples: &[TrainingExample],
        start_epoch: usize,
        initial_weights: Option<LoRAWeights>,
        on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        if self.selected_backend.is_none() {
            self.selected_backend = Some(TrainingBackend::Cpu);
        }
        if self.backend_device.is_none() {
            if let Some(backend) = self.selected_backend {
                self.backend_device = self.resolve_backend_device(backend);
            }
        }

        let backend_name = self.backend_info().unwrap_or("CPU");

        info!(
            "Resuming LoRA training from epoch {}: rank={}, epochs={}, examples={}, backend={}, seed={}",
            start_epoch,
            self.config.rank,
            self.config.epochs,
            examples.len(),
            backend_name,
            self.training_seed
        );

        self.telemetry.log(
            "training.resumed",
            serde_json::json!({
                "start_epoch": start_epoch,
                "total_epochs": self.config.epochs,
                "examples": examples.len(),
                "backend": backend_name,
            }),
        )?;

        let start = Instant::now();
        let adapter_id = Self::generate_adapter_id();

        // Use provided weights or initialize fresh
        let mut weights =
            initial_weights.unwrap_or_else(|| self.initialize_weights_deterministic().unwrap());

        // Training loop starting from resume point with cancellation support
        let mut final_loss = 0.0;
        let mut completed_epochs: u32 = start_epoch as u32;
        let mut examples_processed: u64 = (start_epoch as u64) * examples.len() as u64;
        let tokens_per_epoch = self.tokens_per_epoch(examples);
        let mut tokens_processed: u64 = (start_epoch as u64) * tokens_per_epoch;
        self.total_tokens_processed = tokens_processed;
        self.total_examples_processed = examples_processed;
        let mut was_cancelled = false;

        for epoch in start_epoch..self.config.epochs {
            // Check for cancellation at start of each epoch
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch,
                    "Cancellation requested, stopping resumed training"
                );
                self.telemetry
                    .log(
                        "training.cancelled",
                        serde_json::json!({
                            "job_id": job_id_str,
                            "adapter_id": adapter_id,
                            "stopped_at_epoch": epoch,
                            "final_loss": final_loss,
                            "examples_processed": examples_processed
                        }),
                    )
                    .ok();
                was_cancelled = true;
                break;
            }

            debug!("Epoch {}/{}", epoch + 1, self.config.epochs);

            let epoch_start = Instant::now();
            let epoch_loss = self.train_epoch_deterministic(&mut weights, examples, epoch)?;
            let epoch_duration_us = epoch_start.elapsed().as_micros() as u64;
            final_loss = epoch_loss;
            completed_epochs = (epoch + 1) as u32;
            examples_processed += examples.len() as u64;
            tokens_processed += tokens_per_epoch;
            self.total_tokens_processed = tokens_processed;
            self.total_examples_processed = examples_processed;

            info!("Epoch {} loss: {:.4}", epoch + 1, epoch_loss);

            // Persist metrics to database
            self.persist_epoch_metrics(
                completed_epochs,
                examples_processed,
                epoch_loss,
                examples.len() as u64,
                tokens_per_epoch,
                epoch_duration_us,
            )
            .await;

            self.telemetry.log(
                "training.epoch_completed",
                serde_json::json!({
                    "epoch": epoch + 1,
                    "loss": epoch_loss,
                    "adapter_id": adapter_id,
                    "tokens_in_epoch": tokens_per_epoch,
                    "tokens_per_sec": if epoch_duration_us > 0 {
                        (tokens_per_epoch as f32) / (epoch_duration_us as f32 / 1_000_000.0)
                    } else {
                        0.0
                    },
                    "examples_per_sec": if epoch_duration_us > 0 {
                        (examples.len() as f32) / (epoch_duration_us as f32 / 1_000_000.0)
                    } else {
                        0.0
                    },
                    "total_tokens_processed": self.total_tokens_processed,
                    "total_examples_processed": self.total_examples_processed,
                }),
            )?;

            let epoch_metrics = EpochMetrics {
                epoch: (epoch + 1) as u32,
                loss: epoch_loss,
                duration_us: epoch_duration_us,
                examples_in_epoch: examples.len() as u64,
                tokens_in_epoch: tokens_per_epoch,
                tokens_per_sec: if epoch_duration_us > 0 {
                    (tokens_per_epoch as f32) / (epoch_duration_us as f32 / 1_000_000.0)
                } else {
                    0.0
                },
                examples_per_sec: if epoch_duration_us > 0 {
                    (examples.len() as f32) / (epoch_duration_us as f32 / 1_000_000.0)
                } else {
                    0.0
                },
                total_tokens_processed: self.total_tokens_processed,
                total_examples_processed: self.total_examples_processed,
            };
            on_epoch(epoch_metrics);

            // Save checkpoint if configured
            if let Some(ref manager) = self.checkpoint_manager {
                let epoch_u32 = (epoch + 1) as u32;
                if manager.should_save(epoch_u32) {
                    let checkpoint = TrainingCheckpoint::new(
                        epoch_u32,
                        0,
                        epoch_loss,
                        self.config.learning_rate,
                        self.config.clone(),
                        weights.clone(),
                    );
                    if let Err(e) = manager.save_checkpoint(&checkpoint).await {
                        warn!(
                            epoch = epoch + 1,
                            error = %e,
                            "Failed to save checkpoint (non-fatal)"
                        );
                    } else {
                        info!(epoch = epoch + 1, loss = epoch_loss, "Checkpoint saved");
                    }
                }
            }

            // Check for cancellation after epoch completion
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch + 1,
                    "Cancellation confirmed after epoch completion"
                );
                was_cancelled = true;
                break;
            }

            if epoch_loss < 0.01 {
                info!("Early stopping: loss below threshold");
                break;
            }
        }

        let training_time_us = start.elapsed().as_micros() as u64;
        let examples_per_second = if training_time_us > 0 {
            (examples_processed as f32) / (training_time_us as f32 / 1_000_000.0)
        } else {
            0.0
        };
        let tokens_per_second = if training_time_us > 0 {
            (tokens_processed as f32) / (training_time_us as f32 / 1_000_000.0)
        } else {
            0.0
        };
        {
            let mut perf = self.performance_metrics.write();
            perf.throughput_examples_per_sec = examples_per_second;
            perf.total_tokens_processed = tokens_processed;
            perf.total_examples_processed = examples_processed;
        }
        let backend_name = self.backend_info().unwrap_or("CPU").to_string();

        if matches!(self.selected_backend, Some(TrainingBackend::CoreML)) {
            let perf_snapshot = self.performance_metrics.read().clone();
            self.telemetry
                .log(
                    "training.coreml_completed",
                    serde_json::json!({
                        "job_id": self.job_id,
                        "backend_device": self.backend_device,
                        "mean_forward_us": perf_snapshot.coreml_forward_mean_us,
                        "p95_forward_us": perf_snapshot.coreml_forward_p95_us,
                        "samples": perf_snapshot.coreml_forward_samples,
                    }),
                )
                .ok();
        }

        Ok(TrainingResult {
            adapter_id,
            final_loss,
            training_time_us,
            weights,
            cancelled: was_cancelled,
            stopped_at_epoch: Some(completed_epochs),
            examples_processed: Some(examples_processed),
            tokens_processed: Some(tokens_processed),
            tokens_per_sec: tokens_per_second,
            examples_per_sec: examples_per_second,
            backend: Some(backend_name),
            backend_device: self.backend_device.clone(),
            using_gpu: self.using_gpu(),
            effective_batch_size: Some(self.config.batch_size),
            loss_curve: Vec::new(),
            determinism_seed: self.config.determinism.as_ref().and_then(|d| d.seed),
            determinism_backend: self
                .config
                .determinism
                .as_ref()
                .and_then(|d| d.backend.clone())
                .or_else(|| self.selected_backend.map(|b| b.tag().to_string())),
            determinism_device: self
                .config
                .determinism
                .as_ref()
                .and_then(|d| d.device.clone())
                .or_else(|| self.backend_device.clone()),
            dataset_version_id: self
                .config
                .determinism
                .as_ref()
                .and_then(|d| d.dataset_version_id.clone()),
        })
    }

    /// Legacy training loop with per-epoch callback (pre-CoreML pipeline)
    ///
    /// The callback is invoked after each epoch with (epoch_index starting at 1, epoch_loss).
    /// This method automatically selects the best available GPU backend if kernels have been
    /// initialized, otherwise falls back to CPU training.
    ///
    /// # Arguments
    /// * `examples` - Training examples with input/target pairs
    /// * `on_epoch` - Callback invoked after each epoch with (epoch_number, epoch_loss)
    #[allow(dead_code)]
    pub async fn train_with_callback_legacy<C>(
        &mut self,
        examples: &[TrainingExample],
        on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        self.train_with_callback(examples, on_epoch).await
    }

    /// Initialize LoRA weight matrices with deterministic seeding
    fn initialize_weights_deterministic(&self) -> Result<LoRAWeights> {
        use rand::{Rng, SeedableRng};
        use rand_chacha::ChaCha20Rng;

        // Create deterministic RNG from training seed
        let mut rng = ChaCha20Rng::seed_from_u64(self.training_seed);

        // Initialize lora_a with small random values
        let lora_a = (0..self.config.rank)
            .map(|_| {
                (0..self.config.hidden_dim)
                    .map(|_| rng.gen_range(-0.01..0.01))
                    .collect()
            })
            .collect();

        // Initialize lora_b with zeros (standard practice)
        let lora_b = (0..self.config.hidden_dim)
            .map(|_| vec![0.0; self.config.rank])
            .collect();

        debug!(
            "Initialized LoRA weights deterministically with seed: {}",
            self.training_seed
        );

        Ok(LoRAWeights {
            lora_a,
            lora_b,
            moe_config: self.config.moe_config.clone(),
            precomputed_delta: None,
        })
    }

    /// Train with a per-epoch progress callback and GPU acceleration (CoreML-aware)
    ///
    /// The callback is invoked after each epoch with (epoch_index starting at 1, epoch_loss).
    /// This method prepares CoreML-friendly batches (scaling/padding) and uses
    /// device-aware batching limits before entering the training loop.
    pub async fn train_with_callback<C>(
        &mut self,
        examples: &[TrainingExample],
        on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        if self.selected_backend.is_none() {
            self.selected_backend = Some(TrainingBackend::Cpu);
        }
        if self.backend_device.is_none() {
            if let Some(backend) = self.selected_backend {
                self.backend_device = self.resolve_backend_device(backend);
            }
        }

        let backend_name = self.backend_info().unwrap_or("CPU");
        let using_gpu = self.using_gpu();
        let target_epochs = self.target_epochs();

        let prepared_dataset = self.prepare_dataset_for_training(examples)?;
        let total_examples = prepared_dataset.summary.total_examples;

        info!(
            "Starting LoRA training: rank={}, epochs={}, examples={}, backend={}, seed={}, batch_size={}, max_tokens_per_batch={}",
            self.config.rank,
            target_epochs,
            total_examples,
            backend_name,
            self.training_seed,
            prepared_dataset.batch_plan.effective_batch_size,
            prepared_dataset.batch_plan.max_tokens_per_batch,
        );

        // Log training start with GPU information
        self.telemetry.log(
            "training.started",
            serde_json::json!({
                "rank": self.config.rank,
                "epochs": target_epochs,
                "examples": total_examples,
                "seed": self.training_seed,
                "backend": backend_name,
                "using_gpu": using_gpu,
                "has_kernels": self.kernels.is_some(),
                "config": {
                    "batch_size": self.config.batch_size,
                    "learning_rate": self.config.learning_rate,
                    "alpha": self.config.alpha,
                    "hidden_dim": self.config.hidden_dim,
                    "max_tokens_per_batch": prepared_dataset.batch_plan.max_tokens_per_batch
                }
            }),
        )?;

        self.run_training(prepared_dataset, 0, None, on_epoch).await
    }

    async fn run_training<C>(
        &mut self,
        dataset: PreparedDataset,
        start_epoch: usize,
        initial_weights: Option<LoRAWeights>,
        mut on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        let start = Instant::now();
        let adapter_id = Self::generate_adapter_id();

        // Use provided weights or initialize fresh
        let mut weights =
            initial_weights.unwrap_or_else(|| self.initialize_weights_deterministic().unwrap());

        // Training loop with telemetry and cancellation support
        let mut final_loss = 0.0;
        let mut completed_epochs: u32 = start_epoch as u32;
        let mut examples_processed: u64 =
            (start_epoch as u64) * dataset.summary.total_examples as u64;
        let tokens_per_epoch = dataset.summary.total_tokens;
        let mut tokens_processed: u64 = (start_epoch as u64) * tokens_per_epoch;
        self.total_tokens_processed = tokens_processed;
        self.total_examples_processed = examples_processed;
        let mut was_cancelled = false;
        let target_epochs = self.target_epochs();
        let mut loss_curve = Vec::with_capacity(target_epochs.saturating_sub(start_epoch));

        for epoch in start_epoch..target_epochs {
            // Check for cancellation at start of each epoch
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch,
                    "Cancellation requested, stopping training"
                );
                self.telemetry
                    .log(
                        "training.cancelled",
                        serde_json::json!({
                            "job_id": job_id_str,
                            "adapter_id": adapter_id,
                            "stopped_at_epoch": epoch,
                            "final_loss": final_loss,
                            "examples_processed": examples_processed
                        }),
                    )
                    .ok();
                was_cancelled = true;
                break;
            }

            debug!("Epoch {}/{}", epoch + 1, self.config.epochs);

            // Emit epoch_started telemetry event
            tracing::event!(
                tracing::Level::INFO,
                name = "epoch_started",
                job_id = %self.job_id.as_deref().unwrap_or("unknown"),
                epoch = epoch + 1,
                total_epochs = target_epochs,
                tokens_in_epoch = tokens_per_epoch,
                examples_in_epoch = dataset.summary.total_examples,
                "Training epoch started"
            );

            let epoch_start = Instant::now();
            let epoch_loss = self.train_epoch_deterministic(&mut weights, &dataset, epoch)?;
            let epoch_duration_us = epoch_start.elapsed().as_micros() as u64;
            final_loss = epoch_loss;
            completed_epochs = (epoch + 1) as u32;
            loss_curve.push(epoch_loss);
            examples_processed += dataset.summary.total_examples as u64;
            tokens_processed += tokens_per_epoch;
            self.total_tokens_processed = tokens_processed;
            self.total_examples_processed = examples_processed;

            info!("Epoch {} loss: {:.4}", epoch + 1, epoch_loss);

            // Persist metrics to database
            self.persist_epoch_metrics(
                completed_epochs,
                examples_processed,
                epoch_loss,
                dataset.summary.total_examples as u64,
                tokens_per_epoch,
                epoch_duration_us,
            )
            .await;

            self.telemetry.log(
                "training.epoch_completed",
                serde_json::json!({
                    "epoch": epoch + 1,
                    "loss": epoch_loss,
                    "adapter_id": adapter_id,
                    "tokens_in_epoch": tokens_per_epoch,
                    "tokens_per_sec": if epoch_duration_us > 0 {
                        (tokens_per_epoch as f32) / (epoch_duration_us as f32 / 1_000_000.0)
                    } else {
                        0.0
                    },
                    "examples_per_sec": if epoch_duration_us > 0 {
                        (dataset.summary.total_examples as f32) / (epoch_duration_us as f32 / 1_000_000.0)
                    } else {
                        0.0
                    },
                    "total_tokens_processed": self.total_tokens_processed,
                    "total_examples_processed": self.total_examples_processed,
                }),
            )?;

            let epoch_metrics = EpochMetrics {
                epoch: (epoch + 1) as u32,
                loss: epoch_loss,
                duration_us: epoch_duration_us,
                examples_in_epoch: dataset.summary.total_examples as u64,
                tokens_in_epoch: tokens_per_epoch,
                tokens_per_sec: if epoch_duration_us > 0 {
                    (tokens_per_epoch as f32) / (epoch_duration_us as f32 / 1_000_000.0)
                } else {
                    0.0
                },
                examples_per_sec: if epoch_duration_us > 0 {
                    (dataset.summary.total_examples as f32)
                        / (epoch_duration_us as f32 / 1_000_000.0)
                } else {
                    0.0
                },
                total_tokens_processed: self.total_tokens_processed,
                total_examples_processed: self.total_examples_processed,
            };
            on_epoch(epoch_metrics);

            // Save checkpoint if configured
            if let Some(ref manager) = self.checkpoint_manager {
                let epoch_u32 = (epoch + 1) as u32;
                if manager.should_save(epoch_u32) {
                    let checkpoint = TrainingCheckpoint::new(
                        epoch_u32,
                        0,
                        epoch_loss,
                        self.config.learning_rate,
                        self.config.clone(),
                        weights.clone(),
                    );
                    if let Err(e) = manager.save_checkpoint(&checkpoint).await {
                        warn!(
                            epoch = epoch + 1,
                            error = %e,
                            "Failed to save checkpoint (non-fatal)"
                        );
                    } else {
                        info!(epoch = epoch + 1, loss = epoch_loss, "Checkpoint saved");
                    }
                }
            }

            // Check for cancellation after epoch completion
            if self.is_cancelled() {
                let job_id_str = self.job_id.as_deref().unwrap_or("unknown");
                info!(
                    job_id = %job_id_str,
                    epoch = epoch + 1,
                    "Cancellation confirmed after epoch completion"
                );
                was_cancelled = true;
                break;
            }

            if epoch_loss < 0.01 {
                info!("Early stopping: loss below threshold");
                break;
            }
        }

        let training_time_us = start.elapsed().as_micros() as u64;
        let examples_per_second = if training_time_us > 0 {
            (examples_processed as f32) / (training_time_us as f32 / 1_000_000.0)
        } else {
            0.0
        };
        let tokens_per_second = if training_time_us > 0 {
            (tokens_processed as f32) / (training_time_us as f32 / 1_000_000.0)
        } else {
            0.0
        };
        {
            let mut perf = self.performance_metrics.write();
            perf.throughput_examples_per_sec = examples_per_second;
            perf.total_tokens_processed = tokens_processed;
            perf.total_examples_processed = examples_processed;
        }
        let backend_name = self.backend_info().unwrap_or("CPU").to_string();

        Ok(TrainingResult {
            adapter_id,
            final_loss,
            training_time_us,
            weights,
            cancelled: was_cancelled,
            stopped_at_epoch: Some(completed_epochs),
            examples_processed: Some(examples_processed),
            tokens_processed: Some(tokens_processed),
            tokens_per_sec: tokens_per_second,
            examples_per_sec: examples_per_second,
            backend: Some(backend_name),
            backend_device: self.backend_device.clone(),
            using_gpu: self.using_gpu(),
            effective_batch_size: Some(dataset.batch_plan.effective_batch_size),
            loss_curve,
            determinism_seed: self.config.determinism.as_ref().and_then(|d| d.seed),
            determinism_backend: self
                .config
                .determinism
                .as_ref()
                .and_then(|d| d.backend.clone())
                .or_else(|| self.selected_backend.map(|b| b.tag().to_string())),
            determinism_device: self
                .config
                .determinism
                .as_ref()
                .and_then(|d| d.device.clone())
                .or_else(|| self.backend_device.clone()),
            dataset_version_id: self
                .config
                .determinism
                .as_ref()
                .and_then(|d| d.dataset_version_id.clone()),
        })
    }

    /// Train one epoch with deterministic execution
    ///
    /// Checks for cancellation every 10 batches to ensure bounded cancellation time.
    fn train_epoch_deterministic(
        &mut self,
        weights: &mut LoRAWeights,
        dataset: &PreparedDataset,
        epoch: usize,
    ) -> Result<f32> {
        use rand::SeedableRng;
        use rand_chacha::ChaCha20Rng;

        // Create epoch-specific RNG seed
        let epoch_seed_bytes = derive_seed(
            &adapteros_core::B3Hash::hash(&self.training_seed.to_le_bytes()),
            &format!("epoch_{}", epoch),
        );
        let epoch_seed = u64::from_le_bytes([
            epoch_seed_bytes[0],
            epoch_seed_bytes[1],
            epoch_seed_bytes[2],
            epoch_seed_bytes[3],
            epoch_seed_bytes[4],
            epoch_seed_bytes[5],
            epoch_seed_bytes[6],
            epoch_seed_bytes[7],
        ]);
        let mut rng = ChaCha20Rng::seed_from_u64(epoch_seed);

        let mut total_loss = 0.0;
        let mut num_batches = 0;

        // Check cancel every N batches for bounded cancellation time
        const CANCEL_CHECK_INTERVAL: usize = 10;

        // Process examples in batches with deterministic ordering
        for batch in dataset.batches.iter() {
            // Check for cancellation every N batches
            if num_batches > 0 && num_batches % CANCEL_CHECK_INTERVAL == 0 && self.is_cancelled() {
                debug!(
                    epoch = epoch,
                    batch = num_batches,
                    "Cancellation detected mid-epoch, stopping batch loop"
                );
                // Return partial loss (average of completed batches)
                return Ok(if num_batches > 0 {
                    total_loss / num_batches as f32
                } else {
                    0.0
                });
            }

            let loss =
                self.train_batch_deterministic(weights, batch.examples.as_slice(), &mut rng)?;
            total_loss += loss;
            num_batches += 1;
        }

        Ok(total_loss / num_batches as f32)
    }

    /// Train one batch with deterministic RNG (GPU-accelerated if kernels available)
    fn train_batch_deterministic(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
        rng: &mut impl Rng,
    ) -> Result<f32> {
        // Check if GPU kernels are available
        if self.kernels.is_some() {
            // GPU-accelerated training path with fallback-on-failure when GPU optional
            match self.train_batch_gpu(weights, batch, rng) {
                Ok(loss) => Ok(loss),
                Err(e) => {
                    if self.config.require_gpu {
                        return Err(e);
                    }

                    warn!(
                        "GPU batch failed ({}), falling back to CPU for remaining training",
                        e
                    );
                    self.append_backend_reason(format!("gpu_batch_failed_fallback_cpu: {}", e));
                    self.telemetry
                        .log(
                            "training.gpu_fallback",
                            serde_json::json!({
                                "original_backend": self.backend_info().unwrap_or("unknown"),
                                "reason": e.to_string(),
                                "using_cpu": true,
                                "phase": "mid-training"
                            }),
                        )
                        .ok();
                    self.kernels = None;
                    self.selected_backend = Some(TrainingBackend::Cpu);
                    self.backend_device = self.resolve_backend_device(TrainingBackend::Cpu);
                    self.train_batch_cpu(weights, batch, rng)
                }
            }
        } else {
            // CPU-only training path (fallback)
            self.train_batch_cpu(weights, batch, rng)
        }
    }

    /// Train one batch on GPU (using FusedKernels)
    fn train_batch_gpu(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
        rng: &mut impl Rng,
    ) -> Result<f32> {
        use adapteros_lora_kernel_api::{IoBuffers, RouterRing};

        let batch_start = Instant::now();
        let mut batch_loss = 0.0;
        let vocab_size = self.config.vocab_size;

        let mut gpu_time_us = 0u64;
        let batch_tokens = self.tokens_in_batch(batch);

        for example in batch {
            // Prepare router ring for GPU kernel (using all available adapters)
            let mut ring = RouterRing::new(1); // K=1 for training (single adapter)
            ring.set(&[0], &[ROUTER_GATE_Q15_MAX]); // Max Q15 gate value for training

            // Prepare IO buffers for GPU inference
            let mut io = IoBuffers::new(vocab_size);
            io.input_ids = example.padded_input.clone();
            io.position = 0;

            // Measure GPU forward pass time
            let gpu_start = Instant::now();

            // GPU forward pass through kernels
            if let Some(ref mut kernels) = self.kernels {
                kernels.run_step(&ring, &mut io)?;
            }

            let forward_us = gpu_start.elapsed().as_micros() as u64;
            gpu_time_us += forward_us;
            if matches!(self.selected_backend, Some(TrainingBackend::CoreML)) {
                self.record_coreml_forward_latency(forward_us);
            }

            // Extract hidden state from GPU output
            let hidden: Vec<f32> = io.output_logits[..self.config.hidden_dim].to_vec();
            let output = io.output_logits.clone();

            // Compute loss
            let loss = self.compute_loss(&output, &example.target_tokens);
            batch_loss += loss;

            // Backward pass and update weights (CPU-based gradient descent)
            // TODO: Move gradient computation to GPU kernels for full GPU training
            self.backward_and_update_deterministic(
                weights,
                &hidden,
                &output,
                &example.target_tokens,
                loss,
                rng,
            )?;
        }

        // Update performance metrics
        let batch_time_us = batch_start.elapsed().as_micros() as u64;
        let cpu_time_us = batch_time_us.saturating_sub(gpu_time_us);

        let gpu_utilization = if batch_time_us > 0 {
            (gpu_time_us as f32 / batch_time_us as f32) * 100.0
        } else {
            0.0
        };

        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_gpu_time_ms += gpu_time_us / 1000;
            metrics.total_cpu_time_ms += cpu_time_us / 1000;
            metrics.gpu_operations += batch.len() as u64;
            metrics.total_examples_processed += batch.len() as u64;
            metrics.total_tokens_processed += batch_tokens;
            metrics.total_batches += 1;

            // Running average of GPU utilization
            let total_time = metrics.total_gpu_time_ms + metrics.total_cpu_time_ms;
            if total_time > 0 {
                metrics.avg_gpu_utilization =
                    (metrics.total_gpu_time_ms as f32 / total_time as f32) * 100.0;
            }
        }

        debug!(
            "GPU batch: {}us GPU, {}us CPU, {:.1}% GPU utilization",
            gpu_time_us, cpu_time_us, gpu_utilization
        );

        Ok(batch_loss / batch.len() as f32)
    }

    /// Train one batch on CPU (fallback when GPU unavailable)
    fn train_batch_cpu(
        &self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
        rng: &mut impl Rng,
    ) -> Result<f32> {
        let batch_start = Instant::now();
        let mut batch_loss = 0.0;
        let batch_tokens = self.tokens_in_batch(batch);

        for example in batch {
            // CPU forward pass
            let (output, hidden) = self.forward(weights, example)?;

            // Compute loss (simplified cross-entropy)
            let loss = self.compute_loss(&output, &example.target_tokens);
            batch_loss += loss;

            // Backward pass and update weights with deterministic RNG
            self.backward_and_update_deterministic(
                weights,
                &hidden,
                &output,
                &example.target_tokens,
                loss,
                rng,
            )?;
        }

        // Update CPU metrics
        let cpu_time_ms = batch_start.elapsed().as_millis() as u64;
        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_cpu_time_ms += cpu_time_ms;
            metrics.cpu_operations += batch.len() as u64;
            metrics.total_examples_processed += batch.len() as u64;
            metrics.total_tokens_processed += batch_tokens;
            metrics.total_batches += 1;
        }

        Ok(batch_loss / batch.len() as f32)
    }

    /// Get current GPU utilization percentage
    pub fn get_gpu_utilization(&self) -> f32 {
        self.performance_metrics.read().avg_gpu_utilization
    }

    /// Get performance metrics
    pub fn get_performance_metrics(&self) -> TrainingPerformanceMetrics {
        self.performance_metrics.read().clone()
    }

    /// Reset performance metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self.performance_metrics.write();
        *metrics = TrainingPerformanceMetrics {
            total_gpu_time_ms: 0,
            total_cpu_time_ms: 0,
            gpu_operations: 0,
            cpu_operations: 0,
            avg_gpu_utilization: 0.0,
            peak_gpu_memory_mb: 0.0,
            total_batches: 0,
            throughput_examples_per_sec: 0.0,
            total_tokens_processed: 0,
            total_examples_processed: 0,
            coreml_forward_mean_us: None,
            coreml_forward_p95_us: None,
            coreml_forward_total_us: 0,
            coreml_forward_samples: 0,
            coreml_forward_latency_samples: VecDeque::new(),
            effective_batch_size: self.config.batch_size,
            max_tokens_per_batch: self
                .config
                .max_tokens_per_batch
                .unwrap_or_else(|| self.config.batch_size * self.config.hidden_dim * 2),
            sequences_truncated: 0,
            sequences_dropped: 0,
            device_tier: None,
            input_shape: None,
        };
    }

    /// Tokens processed in a full epoch (input + target)
    #[allow(dead_code)]
    fn tokens_per_epoch(&self, examples: &[TrainingExample]) -> u64 {
        examples
            .iter()
            .map(|ex| (ex.input.len() + ex.target.len()) as u64)
            .sum()
    }

    /// Tokens processed within a single batch
    fn tokens_in_batch(&self, batch: &[PreparedExample]) -> u64 {
        batch
            .iter()
            .map(|ex| (ex.input_len + ex.target_len) as u64)
            .sum()
    }

    /// Record a CoreML forward latency sample (bounded buffer, mean + p95).
    fn record_coreml_forward_latency(&self, latency_us: u64) {
        const MAX_SAMPLES: usize = 512;
        let mut metrics = self.performance_metrics.write();
        metrics.coreml_forward_samples += 1;
        metrics.coreml_forward_total_us =
            metrics.coreml_forward_total_us.saturating_add(latency_us);
        metrics.coreml_forward_mean_us =
            Some(metrics.coreml_forward_total_us as f64 / metrics.coreml_forward_samples as f64);
        metrics.coreml_forward_latency_samples.push_back(latency_us);
        if metrics.coreml_forward_latency_samples.len() > MAX_SAMPLES {
            metrics.coreml_forward_latency_samples.pop_front();
        }
        let mut sorted: Vec<u64> = metrics
            .coreml_forward_latency_samples
            .iter()
            .copied()
            .collect();
        sorted.sort_unstable();
        if !sorted.is_empty() {
            let idx = ((sorted.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
            metrics.coreml_forward_p95_us = Some(sorted[idx]);
        }
    }

    /// Forward pass with LoRA injection
    fn forward(
        &self,
        weights: &LoRAWeights,
        example: &PreparedExample,
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        // Simplified forward pass
        // In production, this would integrate with the actual model

        // Use pre-scaled, padded hidden state from the CoreML pipeline.
        let mut hidden = example.scaled_input.clone();
        hidden.truncate(self.config.hidden_dim);
        if hidden.len() < self.config.hidden_dim {
            hidden.resize(self.config.hidden_dim, 0.0);
        }

        // Apply LoRA: output = hidden + hidden * LoRA_B * LoRA_A
        let lora_output = self.apply_lora(&hidden, weights);

        // Combine base hidden with LoRA adjustment
        let output: Vec<f32> = hidden
            .iter()
            .zip(lora_output.iter())
            .map(|(h, l)| h + l * self.config.alpha / self.config.rank as f32)
            .collect();

        Ok((output, hidden))
    }

    /// Apply LoRA transformation
    #[allow(clippy::needless_range_loop)]
    fn apply_lora(&self, hidden: &[f32], weights: &LoRAWeights) -> Vec<f32> {
        // Compute: hidden * LoRA_A^T * LoRA_B^T

        // First: hidden * LoRA_A^T = intermediate (size: rank)
        let mut intermediate = vec![0.0; self.config.rank];
        for r in 0..self.config.rank {
            for (h_idx, &h_val) in hidden.iter().enumerate() {
                if h_idx < weights.lora_a[r].len() {
                    intermediate[r] += h_val * weights.lora_a[r][h_idx];
                }
            }
        }

        // Second: intermediate * LoRA_B^T = output (size: hidden_dim)
        let mut output = vec![0.0; self.config.hidden_dim];
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for (r, &inter_val) in intermediate.iter().enumerate() {
                    if r < weights.lora_b[h_idx].len() {
                        output[h_idx] += inter_val * weights.lora_b[h_idx][r];
                    }
                }
            }
        }

        output
    }

    /// Apply routing-weighted LoRA transformation for MoE models.
    ///
    /// For routing-weighted shared LoRA strategy:
    /// `lora_out = sum(routing_weight[e]) * apply_lora(hidden)`
    ///
    /// This uses shared LoRA weights scaled by the sum of routing weights
    /// for active experts.
    #[allow(clippy::needless_range_loop)]
    #[allow(dead_code)]
    fn apply_lora_moe(
        &self,
        hidden: &[f32],
        weights: &LoRAWeights,
        routing_weights: &[f32],
    ) -> Vec<f32> {
        // Compute base LoRA output
        let lora_output = self.apply_lora(hidden, weights);

        // Sum of routing weights for active experts (Q15 normalized)
        let routing_scale: f32 = routing_weights.iter().sum();

        // Scale LoRA output by routing weights
        lora_output.into_iter().map(|v| v * routing_scale).collect()
    }

    /// MoE-aware forward pass with routing weights.
    ///
    /// Uses routing-weighted shared LoRA: same weights scaled by expert routing.
    #[allow(dead_code)]
    fn forward_moe(
        &self,
        weights: &LoRAWeights,
        example: &PreparedExample,
        routing_weights: &[f32],
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        // Use pre-scaled, padded hidden state from the CoreML pipeline.
        let mut hidden = example.scaled_input.clone();
        hidden.truncate(self.config.hidden_dim);
        if hidden.len() < self.config.hidden_dim {
            hidden.resize(self.config.hidden_dim, 0.0);
        }

        // Apply routing-weighted LoRA for MoE
        let lora_output = self.apply_lora_moe(&hidden, weights, routing_weights);

        // Combine base hidden with routing-weighted LoRA adjustment
        let output: Vec<f32> = hidden
            .iter()
            .zip(lora_output.iter())
            .map(|(h, l)| h + l * self.config.alpha / self.config.rank as f32)
            .collect();

        Ok((output, hidden))
    }

    /// Compute loss (simplified cross-entropy)
    fn compute_loss(&self, output: &[f32], target: &[u32]) -> f32 {
        let mut loss = 0.0;
        let n = output.len().min(target.len());
        let vocab_scale = (self.config.vocab_size.saturating_sub(1).max(1)) as f32;

        for i in 0..n {
            // Use same scaling as forward pass
            let target_val = ((target[i] as f32) / vocab_scale) * 2.0 - 1.0;
            let diff = output[i] - target_val;
            loss += diff * diff; // MSE for simplicity
        }

        // Avoid returning 0.0 which could cause issues
        let avg_loss = loss / n as f32;
        if avg_loss.is_nan() || avg_loss.is_infinite() {
            0.1 // Fallback to small non-zero value
        } else {
            avg_loss
        }
    }

    /// Backward pass and weight update with deterministic RNG
    fn backward_and_update_deterministic(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        output: &[f32],
        target: &[u32],
        _loss: f32,
        rng: &mut impl Rng,
    ) -> Result<()> {
        // Adapter-only invariant: optimizer must only see LoRA matrices, never
        // base model parameters (those remain frozen outside this trainer).
        debug_assert_eq!(
            weights.lora_a.len(),
            self.config.rank,
            "adapter-only training: LoRA A rows must equal rank"
        );
        debug_assert_eq!(
            weights.lora_b.len(),
            self.config.hidden_dim,
            "adapter-only training: LoRA B rows must equal hidden_dim"
        );
        debug_assert!(
            weights
                .lora_a
                .iter()
                .all(|row| row.len() == self.config.hidden_dim),
            "adapter-only training: LoRA A row width must equal hidden_dim"
        );
        debug_assert!(
            weights
                .lora_b
                .iter()
                .all(|row| row.len() == self.config.rank),
            "adapter-only training: LoRA B row width must equal rank"
        );

        // Simplified gradient descent with deterministic noise
        // In production, use proper backpropagation

        let n = output.len().min(target.len());
        let learning_rate = self.config.learning_rate;
        let vocab_scale = (self.config.vocab_size.saturating_sub(1).max(1)) as f32;

        // Compute gradient (simplified)
        let mut grad_output = vec![0.0; output.len()];
        for i in 0..n {
            // Use same scaling as forward pass
            let target_val = ((target[i] as f32) / vocab_scale) * 2.0 - 1.0;
            grad_output[i] = 2.0 * (output[i] - target_val) / n as f32;
        }

        // Add deterministic noise for regularization
        let noise_scale = 0.001;
        for grad in &mut grad_output {
            *grad += rng.gen_range(-noise_scale..noise_scale);
        }

        // Gradient clipping to prevent explosion
        const MAX_GRAD_NORM: f32 = 1.0;
        let grad_norm: f32 = grad_output.iter().map(|g| g * g).sum::<f32>().sqrt();
        if grad_norm > MAX_GRAD_NORM {
            let scale = MAX_GRAD_NORM / grad_norm;
            for grad in &mut grad_output {
                *grad *= scale;
            }
            debug!(
                "Clipped gradient norm from {:.4} to {:.4}",
                grad_norm, MAX_GRAD_NORM
            );
        }

        // NaN prevention: zero out any non-finite gradients
        for grad in &mut grad_output {
            if !grad.is_finite() {
                *grad = 0.0;
            }
        }

        // Update LoRA_A: gradient is dL/dA = hidden^T * grad_output (simplified)
        const MAX_UPDATE: f32 = 0.1;
        for r in 0..self.config.rank {
            for h_idx in 0..self.config.hidden_dim.min(hidden.len()) {
                if h_idx < weights.lora_a[r].len() {
                    let grad = grad_output[h_idx] * hidden[h_idx];
                    let update = (learning_rate * grad).clamp(-MAX_UPDATE, MAX_UPDATE);
                    weights.lora_a[r][h_idx] -= update;
                }
            }
        }

        // Update LoRA_B: gradient is dL/dB = intermediate^T * grad_output (simplified)
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for r in 0..self.config.rank {
                    if r < weights.lora_b[h_idx].len() {
                        let grad = grad_output[h_idx] * hidden[h_idx];
                        let update = (learning_rate * grad).clamp(-MAX_UPDATE, MAX_UPDATE);
                        weights.lora_b[h_idx][r] -= update;
                    }
                }
            }
        }

        Ok(())
    }

    /// MoE-aware backward pass with routing-weighted gradients.
    ///
    /// Gradients are scaled by the routing weights for active experts.
    /// For routing-weighted shared LoRA:
    /// `grad_scale = sum(routing_weight[e]) for e in active_experts`
    #[allow(dead_code, clippy::too_many_arguments)]
    fn backward_and_update_moe(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        output: &[f32],
        target: &[u32],
        routing_weights: &[f32],
        _loss: f32,
        rng: &mut impl Rng,
    ) -> Result<()> {
        debug_assert_eq!(
            weights.lora_a.len(),
            self.config.rank,
            "MoE training: LoRA A rows must equal rank"
        );
        debug_assert_eq!(
            weights.lora_b.len(),
            self.config.hidden_dim,
            "MoE training: LoRA B rows must equal hidden_dim"
        );

        // Compute routing scale (sum of active expert weights)
        let routing_scale: f32 = routing_weights.iter().sum();

        let n = output.len().min(target.len());
        let learning_rate = self.config.learning_rate;
        let vocab_scale = (self.config.vocab_size.saturating_sub(1).max(1)) as f32;

        // Compute gradient with routing weight scaling
        let mut grad_output = vec![0.0; output.len()];
        for i in 0..n {
            let target_val = ((target[i] as f32) / vocab_scale) * 2.0 - 1.0;
            // Scale gradient by routing weight
            grad_output[i] = 2.0 * (output[i] - target_val) / n as f32 * routing_scale;
        }

        // Add deterministic noise for regularization
        let noise_scale = 0.001;
        for grad in &mut grad_output {
            *grad += rng.gen_range(-noise_scale..noise_scale);
        }

        // Gradient clipping
        const MAX_GRAD_NORM: f32 = 1.0;
        let grad_norm: f32 = grad_output.iter().map(|g| g * g).sum::<f32>().sqrt();
        if grad_norm > MAX_GRAD_NORM {
            let scale = MAX_GRAD_NORM / grad_norm;
            for grad in &mut grad_output {
                *grad *= scale;
            }
            debug!(
                "MoE: Clipped gradient norm from {:.4} to {:.4}",
                grad_norm, MAX_GRAD_NORM
            );
        }

        // NaN prevention
        for grad in &mut grad_output {
            if !grad.is_finite() {
                *grad = 0.0;
            }
        }

        // Update LoRA_A with routing-scaled gradients
        const MAX_UPDATE: f32 = 0.1;
        for r in 0..self.config.rank {
            for h_idx in 0..self.config.hidden_dim.min(hidden.len()) {
                if h_idx < weights.lora_a[r].len() {
                    let grad = grad_output[h_idx] * hidden[h_idx];
                    let update = (learning_rate * grad).clamp(-MAX_UPDATE, MAX_UPDATE);
                    weights.lora_a[r][h_idx] -= update;
                }
            }
        }

        // Update LoRA_B with routing-scaled gradients
        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for r in 0..self.config.rank {
                    if r < weights.lora_b[h_idx].len() {
                        let grad = grad_output[h_idx] * hidden[h_idx];
                        let update = (learning_rate * grad).clamp(-MAX_UPDATE, MAX_UPDATE);
                        weights.lora_b[h_idx][r] -= update;
                    }
                }
            }
        }

        Ok(())
    }

    /// Train one batch for MoE models with simulated routing.
    ///
    /// For training, we simulate routing by distributing examples across experts
    /// using deterministic routing based on the example index and MoE config.
    #[allow(dead_code)]
    fn train_batch_moe(
        &self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
        rng: &mut impl Rng,
    ) -> Result<f32> {
        let moe_config = self.config.moe_config.as_ref().ok_or_else(|| {
            AosError::Training("MoE batch training requires moe_config".to_string())
        })?;

        let batch_start = Instant::now();
        let mut batch_loss = 0.0;
        let batch_tokens = self.tokens_in_batch(batch);
        let num_experts_per_token = moe_config.num_experts_per_token;

        for (idx, example) in batch.iter().enumerate() {
            // Simulate routing weights (uniform distribution for training)
            // In production, these come from the actual router
            let routing_weights: Vec<f32> = (0..num_experts_per_token)
                .map(|e| {
                    // Deterministic routing weight based on example and expert index
                    let seed = (idx * 1000 + e) as f32;
                    let weight = (seed.sin().abs() + 0.5) / num_experts_per_token as f32;
                    weight.min(1.0)
                })
                .collect();

            // Normalize to sum to ~1.0
            let sum: f32 = routing_weights.iter().sum();
            let normalized_weights: Vec<f32> = if sum > 0.0 {
                routing_weights.iter().map(|w| w / sum).collect()
            } else {
                vec![1.0 / num_experts_per_token as f32; num_experts_per_token]
            };

            // MoE forward pass
            let (output, hidden) = self.forward_moe(weights, example, &normalized_weights)?;

            // Compute loss
            let loss = self.compute_loss(&output, &example.target_tokens);
            batch_loss += loss;

            // MoE backward pass with routing weights
            self.backward_and_update_moe(
                weights,
                &hidden,
                &output,
                &example.target_tokens,
                &normalized_weights,
                loss,
                rng,
            )?;
        }

        // Update CPU metrics
        let cpu_time_ms = batch_start.elapsed().as_millis() as u64;
        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_cpu_time_ms += cpu_time_ms;
            metrics.cpu_operations += batch.len() as u64;
            metrics.total_examples_processed += batch.len() as u64;
            metrics.total_tokens_processed += batch_tokens;
            metrics.total_batches += 1;
        }

        Ok(batch_loss / batch.len() as f32)
    }

    /// Generate unique adapter ID
    fn generate_adapter_id() -> String {
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("microlora_{}", timestamp)
    }
}

#[cfg(test)]
mod tests;
