//! Micro-LoRA training loop with forward/backward pass
//!
//! Implements LoRA training with low rank adaptation matrices.
//! This is a Rust-native implementation that avoids Python dependencies
//! and integrates with GPU backends (CoreML, MLX, Metal) for deterministic training.

use super::checkpoint::{CheckpointManager, TrainingCheckpoint};
use super::coreml_pipeline::{
    prepare_coreml_dataset, BatchPlan, CoreMLInputSpec, PreparedDataset, PreparedExample,
};
use super::dataset::example_hash_for_tokens;
use super::learning_rate_schedule::{LRScheduleType, LRScheduler, LRSchedulerConfig};
use super::loss::{self, LOSS_IGNORE_INDEX};
use super::perplexity::compute_perplexity;
use super::preprocessing::{preprocess_examples, PreprocessResult};
use adapteros_core::{derive_seed, AosError, Result};
use adapteros_db::{Db, TrainingMetricRow};
use adapteros_id::{IdPrefix, TypedId};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_router::ROUTER_GATE_Q15_MAX;
use adapteros_telemetry::TelemetryWriter;
pub use adapteros_types::training::TrainingExampleV1 as TrainingExample;
use adapteros_types::training::{
    validate_training_examples, PreprocessedExampleV1, TrainingBackendPolicy,
    TrainingDataContractConfig, TrainingExampleBatchSummary, PREPROCESSED_EXAMPLE_SCHEMA_VERSION,
    PREPROCESSED_FEATURE_BACKEND_COREML,
};
use chrono::Utc;
use parking_lot::RwLock;
use rand::Rng;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use std::path::Path;

// MLX training FFI imports for GPU-accelerated backward pass
#[cfg(feature = "multi-backend")]
use adapteros_lora_mlx_ffi::training::{
    mlx_lora_backward_ce_gpu, mlx_lora_backward_gpu, MlxOptimizer,
};

mod types;
pub use types::{
    DatasetSubsample, DeterminismConfig, DevicePolicyConfig, EpochMetrics, LoRAWeights,
    MoELoRAStrategy, MoETrainingConfig, ModuleOptimizerState, ModuleWeights,
    MultiModuleOptimizerState, OptimizerConfig, OptimizerType, PreprocessCompression,
    PreprocessOutputFeature, PreprocessingConfig, TrainingBackend, TrainingConfig,
    TrainingPerformanceMetrics, TrainingResult,
};

/// Micro-LoRA trainer with multi-backend GPU support.
///
/// IMPORTANT: GPU training requires a base model to be loaded. The trainer extracts
/// real hidden states from the base model and computes cross-entropy loss on
/// vocabulary logits for proper LoRA training. When `use_gpu_backward=false`,
/// the CPU proxy path skips base model loading and uses scaled-token MSE loss.
///
/// Only LoRA matrices are ever mutated or registered with optimizers.
pub struct MicroLoRATrainer {
    pub config: TrainingConfig,
    /// Whether to use cross-entropy loss instead of legacy MSE.
    use_cross_entropy_loss: bool,
    /// Whether to use chunked CE computation for large vocabularies.
    /// When true, processes vocabulary in chunks to reduce peak memory usage.
    use_chunked_ce: bool,
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
    /// Prepared validation examples for per-epoch evaluation
    validation_examples: Vec<PreparedExample>,
    /// Optional preprocessed example cache keyed by example hash
    preprocessed_examples: Option<HashMap<[u8; 32], PreprocessedExampleV1>>,
    /// BLAKE3 hash of deterministic train/validation split
    split_hash_b3: Option<String>,
    /// Token counts for train/validation splits
    train_token_count: u64,
    validation_token_count: u64,
    /// Example counts for train/validation splits
    train_example_count: u64,
    validation_example_count: u64,
    /// Optional checkpoint manager for saving/resuming training
    checkpoint_manager: Option<CheckpointManager>,
    /// Force resume even when config mismatches checkpoint.
    force_resume: bool,
    /// Cancellation token - set to true to request training stop
    cancel_token: Option<Arc<AtomicBool>>,
    /// Job ID for this training run (used for metrics persistence and cancellation)
    job_id: Option<String>,
    /// Correlation ID for tracing across pipeline stages
    correlation_id: Option<String>,
    /// Optional database connection for metrics persistence
    db: Option<Db>,
    /// Base model for extracting real hidden states during training.
    /// REQUIRED: Training without a base model will fail.
    #[cfg(feature = "multi-backend")]
    base_model: Option<Arc<adapteros_lora_mlx_ffi::MLXFFIModel>>,
    /// Hidden state layer key to extract from the base model (e.g., "layer_31_output").
    hidden_state_key: String,
    /// Number of transformer layers in the base model (for multi-module training).
    n_layers: usize,
    /// Current global training step (across all epochs)
    global_step: usize,
    /// Persistent optimizer state (Adam/SGD moments)
    #[cfg(feature = "multi-backend")]
    optimizer: Option<MlxOptimizer>,
    /// Per-module optimizer state for multi-module training (step counts, CPU-side state)
    multi_module_optimizer: MultiModuleOptimizerState,
    /// Per-module GPU optimizers for multi-layer training (keyed by module_key)
    /// Uses BTreeMap for deterministic iteration order across gradient updates.
    #[cfg(feature = "multi-backend")]
    module_optimizers: BTreeMap<String, MlxOptimizer>,
    /// Learning rate scheduler for warmup and decay
    lr_scheduler: Option<LRScheduler>,
    /// Accumulated gradients for gradient accumulation (keyed by module_key)
    /// Each entry is (grad_a_sum, grad_b_sum, accumulation_count)
    /// Uses BTreeMap for deterministic iteration order during gradient application.
    accumulated_gradients: BTreeMap<String, (Vec<f32>, Vec<f32>, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendAvailability {
    coreml: bool,
    mlx: bool,
    metal: bool,
    coreml_reason: Option<String>,
}

struct SplitExamplesResult {
    train: Vec<TrainingExample>,
    validation: Vec<TrainingExample>,
    split_hash_b3: String,
}

/// Maximum vocabulary size for standard cross-entropy loss computation.
/// MLX uses lazy evaluation with unified memory, so large vocabularies are handled
/// efficiently without materializing the full logits tensor in memory at once.
/// Set to usize::MAX to effectively disable the MSE fallback since cross-entropy
/// is the correct loss function for language model training.
const DEFAULT_CE_MAX_VOCAB: usize = usize::MAX;

/// Vocabulary size threshold for chunked cross-entropy computation.
/// Above this threshold, CE is computed in memory-efficient chunks.
const CHUNKED_CE_VOCAB_THRESHOLD: usize = 100_000;

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

        // Initialize telemetry under var/ to avoid writing runtime artifacts into the repo.
        let telemetry_dir = adapteros_core::resolve_var_dir().join("telemetry/training");
        let telemetry = TelemetryWriter::new(telemetry_dir, 1000, 1024 * 1024)?;

        info!(
            "Created MicroLoRA trainer with seed: {}, GPU optional: {}",
            training_seed, !config.require_gpu
        );
        #[cfg(feature = "multi-backend")]
        {
            let mlx_ver = adapteros_lora_mlx_ffi::mlx_version();
            info!("MLX version: {}", mlx_ver);
            // Capture MLX version for training reproducibility if not already set
            if config.mlx_version.is_none() {
                config.mlx_version = Some(mlx_ver.to_string());
            }
        }

        let config_for_metrics = config.clone();

        let requires_base_model = config.requires_base_model();
        let base_model_path = config.base_model_path.clone();
        if requires_base_model && base_model_path.is_none() {
            return Err(AosError::Config(
                "base_model_path is required when use_gpu_backward=true, validation_split>0, \
                 or multi_module_training=true. Set via TrainingConfig::with_base_model() or \
                 --base-model CLI flag."
                    .to_string(),
            ));
        }
        if !requires_base_model && base_model_path.is_none() {
            warn!(
                "base_model_path is not set; tokenizer validation should be enforced before training"
            );
        }

        let mut trainer = Self {
            config,
            use_cross_entropy_loss: true,
            use_chunked_ce: false, // Will be set during train() based on vocab size
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
            validation_examples: Vec::new(),
            preprocessed_examples: None,
            split_hash_b3: None,
            train_token_count: 0,
            validation_token_count: 0,
            train_example_count: 0,
            validation_example_count: 0,
            checkpoint_manager: None,
            force_resume: false,
            cancel_token: None,
            job_id: None,
            correlation_id: None,
            db: None,
            #[cfg(feature = "multi-backend")]
            base_model: None,
            hidden_state_key: String::new(),
            n_layers: 0,
            global_step: 0,
            #[cfg(feature = "multi-backend")]
            optimizer: None,
            multi_module_optimizer: MultiModuleOptimizerState::new(),
            #[cfg(feature = "multi-backend")]
            module_optimizers: BTreeMap::new(),
            lr_scheduler: None, // Initialized when training starts
            accumulated_gradients: BTreeMap::new(),
        };

        // Load base model when required (GPU backward, validation, or multi-module training).
        #[cfg(feature = "multi-backend")]
        if requires_base_model {
            let base_model_path = base_model_path.as_ref().ok_or_else(|| {
                AosError::Config(
                    "base_model_path is required for this training configuration".to_string(),
                )
            })?;
            trainer.load_base_model(base_model_path)?;
        } else {
            info!(
                "Skipping base model load (use_gpu_backward=false, validation_split=0, multi_module_training=false)"
            );
        }

        Ok(trainer)
    }

    /// Create a trainer for unit tests without requiring a base model.
    ///
    /// This is ONLY for testing backend selection, config validation, and other
    /// unit test scenarios that don't perform actual training. Attempting to
    /// call `train()` on a trainer created this way will fail.
    ///
    /// For integration tests that need actual training, use `new()` with a valid
    /// base model path.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_test(config: TrainingConfig) -> Result<Self> {
        let training_seed = if let Some(ref det) = config.determinism {
            if let Some(explicit_seed) = det.seed {
                explicit_seed
            } else {
                Self::derive_seed_from_context(&config)
            }
        } else {
            Self::derive_seed_from_context(&config)
        };

        let telemetry_dir = adapteros_core::resolve_var_dir().join("telemetry/training");
        let telemetry = TelemetryWriter::new(telemetry_dir, 1000, 1024 * 1024)?;
        let config_for_metrics = config.clone();

        Ok(Self {
            config,
            use_cross_entropy_loss: true,
            use_chunked_ce: false, // Will be set during train() based on vocab size
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
            validation_examples: Vec::new(),
            preprocessed_examples: None,
            split_hash_b3: None,
            train_token_count: 0,
            validation_token_count: 0,
            train_example_count: 0,
            validation_example_count: 0,
            checkpoint_manager: None,
            force_resume: false,
            cancel_token: None,
            job_id: None,
            correlation_id: None,
            db: None,
            #[cfg(feature = "multi-backend")]
            base_model: None,
            hidden_state_key: String::new(),
            n_layers: 0,
            global_step: 0,
            #[cfg(feature = "multi-backend")]
            optimizer: None,
            multi_module_optimizer: MultiModuleOptimizerState::new(),
            #[cfg(feature = "multi-backend")]
            module_optimizers: BTreeMap::new(),
            lr_scheduler: None, // Initialized when training starts
            accumulated_gradients: BTreeMap::new(),
        })
    }

    /// Detect backend availability using runtime capability detection
    fn detect_backend_availability() -> BackendAvailability {
        #[cfg(any(test, debug_assertions))]
        if let Some(forced) = Self::forced_backend_override() {
            return forced;
        }

        let caps = crate::backend_factory::detect_capabilities();
        let coreml_reason = if caps.has_coreml && caps.has_ane {
            None
        } else {
            crate::backend_factory::coreml_unavailable_reason(&caps)
        };
        BackendAvailability {
            coreml: caps.has_coreml && caps.has_ane,
            mlx: caps.has_mlx,
            metal: caps.has_metal,
            coreml_reason,
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
                    coreml_reason: None,
                },
                "mlx" => BackendAvailability {
                    coreml: false,
                    mlx: true,
                    metal: false,
                    coreml_reason: None,
                },
                "metal" => BackendAvailability {
                    coreml: false,
                    mlx: false,
                    metal: true,
                    coreml_reason: None,
                },
                "all" => BackendAvailability {
                    coreml: true,
                    mlx: true,
                    metal: true,
                    coreml_reason: None,
                },
                "none" | "cpu" => BackendAvailability {
                    coreml: false,
                    mlx: false,
                    metal: false,
                    coreml_reason: None,
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

    fn append_coreml_unavailable_reason(&mut self, availability: &BackendAvailability) {
        if availability.coreml {
            return;
        }
        if self
            .backend_reason
            .as_ref()
            .map(|existing| existing.contains("coreml_unavailable"))
            .unwrap_or(false)
        {
            return;
        }
        let note = availability
            .coreml_reason
            .as_ref()
            .map(|reason| format!("coreml_unavailable({})", reason))
            .unwrap_or_else(|| "coreml_unavailable".to_string());
        self.append_backend_reason(note);
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
                    self.append_coreml_unavailable_reason(availability);
                    warn!(
                        reason = availability.coreml_reason.as_deref().unwrap_or("unknown"),
                        "CoreML requested (coreml_only) but unavailable"
                    );
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
                    self.append_coreml_unavailable_reason(availability);
                    warn!(
                        reason = availability.coreml_reason.as_deref().unwrap_or("unknown"),
                        "CoreML requested (coreml_else_fallback) but unavailable"
                    );
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
                self.append_coreml_unavailable_reason(availability);
                warn!(
                    reason = availability.coreml_reason.as_deref().unwrap_or("unknown"),
                    "CoreML requested (preferred) but unavailable"
                );
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
                if backend == TrainingBackend::CoreML {
                    self.append_coreml_unavailable_reason(availability);
                } else {
                    self.append_backend_reason(format!("{}_unavailable", backend.tag()));
                }
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

    fn log_training_contract(&self, summary: &TrainingExampleBatchSummary) {
        info!(
            contract_version = %summary.contract_version,
            pad_token_id = summary.pad_token_id,
            ignore_index = summary.ignore_index,
            total_examples = summary.total_examples,
            total_tokens = summary.total_tokens,
            "Training example contract validated"
        );
        self.telemetry
            .log(
                "training.contract",
                serde_json::json!({
                    "contract_version": summary.contract_version,
                    "pad_token_id": summary.pad_token_id,
                    "ignore_index": summary.ignore_index,
                    "total_examples": summary.total_examples,
                    "total_tokens": summary.total_tokens,
                }),
            )
            .ok();
    }

    fn training_contract_config(&self) -> TrainingDataContractConfig {
        TrainingDataContractConfig {
            contract_version: self.config.training_contract_version.clone(),
            pad_token_id: self.config.pad_token_id,
            ignore_index: self.config.ignore_index,
        }
    }

    fn split_examples_for_validation(&self, examples: &[TrainingExample]) -> SplitExamplesResult {
        let (train_examples, validation_examples, summary) =
            super::dataset::split_examples_for_validation(
                examples,
                self.config.validation_split,
                self.training_seed,
            );

        info!(
            train = summary.train_count,
            validation = summary.validation_count,
            split_ratio = summary.split_ratio,
            split_hash = %summary.split_hash_b3,
            "Training/validation split"
        );

        SplitExamplesResult {
            train: train_examples,
            validation: validation_examples,
            split_hash_b3: summary.split_hash_b3,
        }
    }

    fn prepare_validation_examples(
        &self,
        examples: &[TrainingExample],
        batch_plan: &BatchPlan,
    ) -> Result<Vec<PreparedExample>> {
        let spec = CoreMLInputSpec {
            hidden_dim: self.config.hidden_dim,
            vocab_size: self.config.vocab_size,
            context_window: self.config.effective_context_window(),
        };

        let mut prepared = prepare_coreml_dataset(
            examples,
            spec,
            batch_plan.effective_batch_size,
            Some(batch_plan.max_tokens_per_batch),
        )?;

        if self.preprocessed_examples.is_some() {
            self.attach_preprocessed_from_map(&mut prepared.examples, examples)?;
        }

        Ok(prepared.examples)
    }

    fn maybe_preprocess_examples(
        &self,
        examples: &[TrainingExample],
    ) -> Result<Option<PreprocessResult>> {
        let Some(cfg) = self.config.preprocessing.as_ref() else {
            return Ok(None);
        };
        if !cfg.enabled {
            return Ok(None);
        }

        let base_model_path = self.config.base_model_path.as_ref().ok_or_else(|| {
            AosError::Config("preprocessing enabled but base_model_path is missing".to_string())
        })?;
        let seed = cfg.seed.unwrap_or(self.training_seed);
        let contract = self.training_contract_config();
        let result = preprocess_examples(
            examples,
            &contract,
            cfg,
            self.config.hidden_dim,
            self.config.vocab_size,
            base_model_path,
            None,
            None,
            None,
            seed,
        )?;

        Ok(Some(result))
    }

    pub fn set_preprocessed_examples(
        &mut self,
        examples: Vec<PreprocessedExampleV1>,
    ) -> Result<()> {
        let mut map = HashMap::with_capacity(examples.len());
        for (idx, example) in examples.into_iter().enumerate() {
            if example.schema_version != PREPROCESSED_EXAMPLE_SCHEMA_VERSION {
                return Err(AosError::Training(format!(
                    "Preprocessed schema version mismatch at {}: {}",
                    idx, example.schema_version
                )));
            }
            if example.backend != PREPROCESSED_FEATURE_BACKEND_COREML {
                return Err(AosError::Training(format!(
                    "Preprocessed backend mismatch at {}: {}",
                    idx, example.backend
                )));
            }
            let hash = example_hash_for_tokens(
                self.training_seed,
                &example.input_tokens,
                &example.target_tokens,
                &example.attention_mask,
            );
            if map.insert(hash, example).is_some() {
                return Err(AosError::Training(format!(
                    "Duplicate preprocessed example hash at {}",
                    idx
                )));
            }
        }
        self.preprocessed_examples = Some(map);
        Ok(())
    }

    fn attach_preprocessed_from_map(
        &self,
        prepared: &mut [PreparedExample],
        examples: &[TrainingExample],
    ) -> Result<()> {
        let Some(map) = self.preprocessed_examples.as_ref() else {
            return Ok(());
        };
        if prepared.len() != examples.len() {
            return Err(AosError::Training(format!(
                "Preprocessed mapping length mismatch: {} != {}",
                prepared.len(),
                examples.len()
            )));
        }
        for (idx, (prepared_example, example)) in
            prepared.iter_mut().zip(examples.iter()).enumerate()
        {
            let hash = example_hash_for_tokens(
                self.training_seed,
                &example.input_tokens,
                &example.target_tokens,
                &example.attention_mask,
            );
            let preprocessed = map.get(&hash).ok_or_else(|| {
                AosError::Training(format!("Missing preprocessed example for index {}", idx))
            })?;
            if preprocessed.input_tokens != example.input_tokens
                || preprocessed.target_tokens != example.target_tokens
            {
                return Err(AosError::Training(format!(
                    "Preprocessed token mismatch at {}",
                    idx
                )));
            }
            if preprocessed.features.len() != self.config.hidden_dim {
                return Err(AosError::Training(format!(
                    "Preprocessed hidden state size mismatch: {} != {}",
                    preprocessed.features.len(),
                    self.config.hidden_dim
                )));
            }
            prepared_example.preprocessed = Some(preprocessed.features.clone());
        }
        Ok(())
    }

    fn prepare_datasets_for_training(
        &mut self,
        examples: &[TrainingExample],
    ) -> Result<PreparedDataset> {
        let contract = self.training_contract_config();
        let summary = validate_training_examples(examples, self.config.vocab_size, &contract)
            .map_err(|e| {
                AosError::Training(format!(
                    "Training example contract validation failed: {}",
                    e
                ))
            })?;
        self.log_training_contract(&summary);

        let split = self.split_examples_for_validation(examples);
        let prepared_dataset = self.prepare_dataset_for_training(&split.train)?;

        if split.validation.is_empty() {
            self.validation_examples.clear();
        } else {
            self.validation_examples =
                self.prepare_validation_examples(&split.validation, &prepared_dataset.batch_plan)?;
        }

        self.split_hash_b3 = Some(split.split_hash_b3);
        self.train_example_count = prepared_dataset.summary.total_examples as u64;
        self.train_token_count = prepared_dataset.summary.total_tokens;
        self.validation_example_count = self.validation_examples.len() as u64;
        self.validation_token_count = self.total_tokens_in_prepared(&self.validation_examples);

        Ok(prepared_dataset)
    }

    fn prepare_datasets_for_training_with_split(
        &mut self,
        train_examples: &[TrainingExample],
        validation_examples: &[TrainingExample],
    ) -> Result<PreparedDataset> {
        let contract = self.training_contract_config();
        let train_summary =
            validate_training_examples(train_examples, self.config.vocab_size, &contract).map_err(
                |e| {
                    AosError::Training(format!(
                        "Training example contract validation failed: {}",
                        e
                    ))
                },
            )?;
        let combined_summary = if validation_examples.is_empty() {
            train_summary.clone()
        } else {
            let validation_summary =
                validate_training_examples(validation_examples, self.config.vocab_size, &contract)
                    .map_err(|e| {
                        AosError::Training(format!(
                            "Training example contract validation failed: {}",
                            e
                        ))
                    })?;
            if validation_summary.contract_version != train_summary.contract_version {
                return Err(AosError::Training(format!(
                    "Training example contract_version mismatch between splits: train={} validation={}",
                    train_summary.contract_version, validation_summary.contract_version
                )));
            }
            if validation_summary.pad_token_id != train_summary.pad_token_id {
                return Err(AosError::Training(format!(
                    "Training example pad_token_id mismatch between splits: train={:?} validation={:?}",
                    train_summary.pad_token_id, validation_summary.pad_token_id
                )));
            }
            if validation_summary.ignore_index != train_summary.ignore_index {
                return Err(AosError::Training(format!(
                    "Training example ignore_index mismatch between splits: train={:?} validation={:?}",
                    train_summary.ignore_index, validation_summary.ignore_index
                )));
            }
            TrainingExampleBatchSummary {
                contract_version: train_summary.contract_version.clone(),
                pad_token_id: train_summary.pad_token_id,
                ignore_index: train_summary.ignore_index,
                total_examples: train_summary.total_examples + validation_summary.total_examples,
                total_tokens: train_summary.total_tokens + validation_summary.total_tokens,
            }
        };
        self.log_training_contract(&combined_summary);

        let prepared_dataset = self.prepare_dataset_for_training(train_examples)?;

        if validation_examples.is_empty() {
            self.validation_examples.clear();
        } else {
            self.validation_examples = self
                .prepare_validation_examples(validation_examples, &prepared_dataset.batch_plan)?;
        }

        let total_examples = train_examples.len() + validation_examples.len();
        let split_ratio = if total_examples == 0 {
            0.0
        } else {
            validation_examples.len() as f32 / total_examples as f32
        };
        self.split_hash_b3 = Some(super::dataset::compute_split_hash_for_sets(
            train_examples,
            validation_examples,
            self.training_seed,
            split_ratio,
        ));
        self.train_example_count = prepared_dataset.summary.total_examples as u64;
        self.train_token_count = prepared_dataset.summary.total_tokens;
        self.validation_example_count = self.validation_examples.len() as u64;
        self.validation_token_count = self.total_tokens_in_prepared(&self.validation_examples);

        Ok(prepared_dataset)
    }

    fn total_tokens_in_prepared(&self, examples: &[PreparedExample]) -> u64 {
        examples
            .iter()
            .map(|example| (example.input_len + example.target_len) as u64)
            .sum()
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

        if self.preprocessed_examples.is_some() {
            self.attach_preprocessed_from_map(&mut prepared.examples, examples)?;
        } else if let Some(preprocessed) = self.maybe_preprocess_examples(examples)? {
            if preprocessed.examples.len() != prepared.examples.len() {
                return Err(AosError::Training(format!(
                    "Preprocessing output size mismatch: {} examples for {} inputs",
                    preprocessed.examples.len(),
                    prepared.examples.len()
                )));
            }
            for (example, preprocessed_example) in prepared
                .examples
                .iter_mut()
                .zip(preprocessed.examples.into_iter())
            {
                example.preprocessed = Some(preprocessed_example.features);
            }
            self.telemetry
                .log(
                    "training.preprocessing_completed",
                    serde_json::json!({
                        "backend": preprocessed.stats.backend,
                        "cache_hit": preprocessed.stats.cache_hit,
                        "cached_examples": preprocessed.stats.cached_examples,
                        "processed_examples": preprocessed.stats.processed_examples,
                        "elapsed_ms": preprocessed.stats.elapsed_ms,
                        "cache_dir": preprocessed.stats.cache_dir,
                        "preprocess_id": preprocessed.stats.preprocess_id,
                        "cache_key": preprocessed.stats.cache_key,
                    }),
                )
                .ok();
        }

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
                        let class = crate::backend_factory::classify_coreml_error(&e);
                        error!(
                            coreml_failure_stage = class.stage,
                            coreml_failure_reason = class.reason,
                            error = %e,
                            "CoreML training backend initialization failed"
                        );
                        let reason = crate::backend_factory::format_coreml_failure_reason(&e);
                        self.append_backend_reason(reason);
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
                    "CoreML backend requested but 'coreml-backend' feature not enabled (coreml-stub is test-only)"
                        .to_string(),
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

    /// Set the correlation ID for tracing across pipeline stages.
    pub fn set_correlation_id(&mut self, correlation_id: Option<String>) {
        self.correlation_id = correlation_id;
    }

    /// Set the database connection for metrics persistence
    ///
    /// When set, the trainer will persist metrics (loss, tokens/sec, etc.)
    /// to the `repository_training_metrics` table after each epoch.
    pub fn set_db(&mut self, db: Db) {
        self.db = Some(db);
    }

    /// Load a base model for extracting real hidden states during training.
    ///
    /// The forward pass runs actual model inference and extracts hidden states
    /// from the specified layer (or the last transformer layer by default).
    /// This produces correctly trained LoRA adapters with proper cross-entropy
    /// loss computation on vocabulary logits.
    ///
    /// # Arguments
    /// * `model_path` - Path to the model directory (containing safetensors files)
    ///
    /// # Errors
    /// Returns an error if the model cannot be loaded or if dimensions don't match
    /// the training configuration.
    #[cfg(feature = "multi-backend")]
    pub fn load_base_model(&mut self, model_path: &Path) -> Result<()> {
        use crate::backend_factory::canonicalize_model_path;
        use adapteros_lora_mlx_ffi::MLXFFIModel;

        let canonical_path = canonicalize_model_path(model_path)
            .map_err(|e| AosError::Training(format!("Model path rejected: {}", e)))?;
        info!(
            model_path = %canonical_path.display(),
            "Loading base model for hidden state extraction during training"
        );

        // Load the MLX model
        let model = MLXFFIModel::load(&canonical_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to load base model from '{}': {}",
                canonical_path.display(),
                e
            ))
        })?;

        // Get model config for validation and layer selection
        // Extract values before moving model into Arc
        let model_config = model.config();
        let num_hidden_layers = model_config.num_hidden_layers;
        let hidden_size = model_config.hidden_size;
        let vocab_size = model_config.vocab_size;

        // Validate hidden dimension matches training config
        if hidden_size != self.config.hidden_dim {
            return Err(AosError::Training(format!(
                "Base model hidden_size ({}) doesn't match training hidden_dim ({}). \
                 Update TrainingConfig.hidden_dim to match the model.",
                hidden_size, self.config.hidden_dim
            )));
        }

        // Warn if vocab size differs
        if vocab_size != self.config.vocab_size {
            warn!(
                model_vocab = vocab_size,
                config_vocab = self.config.vocab_size,
                "Base model vocab_size differs from training config vocab_size"
            );
        }

        // Determine which hidden state layer to extract
        let hidden_state_key = self.config.hidden_state_layer.clone().unwrap_or_else(|| {
            // Default to the last transformer layer's output
            let last_layer = num_hidden_layers.saturating_sub(1);
            format!("layer_{}_output", last_layer)
        });

        info!(
            num_layers = num_hidden_layers,
            hidden_size = hidden_size,
            vocab_size = vocab_size,
            hidden_state_key = %hidden_state_key,
            "Base model loaded successfully for training"
        );

        self.base_model = Some(Arc::new(model));
        self.hidden_state_key = hidden_state_key;
        self.n_layers = num_hidden_layers;

        Ok(())
    }

    /// Check if a base model is loaded for real hidden state extraction.
    #[cfg(feature = "multi-backend")]
    pub fn has_base_model(&self) -> bool {
        self.base_model.is_some()
    }

    /// Check if a base model is loaded (always false without multi-backend).
    #[cfg(not(feature = "multi-backend"))]
    pub fn has_base_model(&self) -> bool {
        false
    }

    /// Map a target module name to the appropriate hidden state layer key.
    ///
    /// Different target modules require hidden states from different points
    /// in the transformer architecture:
    /// - Attention modules (q_proj, k_proj, v_proj, o_proj): Use pre-attention hidden states
    /// - FFN modules (gate_proj, up_proj, down_proj): Use post-attention hidden states
    ///
    /// # Arguments
    /// * `target` - The target module name (e.g., "q_proj", "gate_proj")
    /// * `layer_idx` - The layer index to extract hidden states from
    ///
    /// # Returns
    /// The hidden state key string (e.g., "layer_31_pre_attn")
    pub fn layer_key_for_module(&self, target: &str, layer_idx: usize) -> String {
        match target {
            // Attention modules: extract before attention (input to attention)
            "q_proj" | "k_proj" | "v_proj" | "o_proj" => {
                format!("layer_{}_pre_attn", layer_idx)
            }
            // FFN modules: extract after attention (input to MLP)
            "gate_proj" | "up_proj" | "down_proj" => {
                format!("layer_{}_post_attn", layer_idx)
            }
            // Default: use layer output (after both attention and MLP)
            _ => format!("layer_{}_output", layer_idx),
        }
    }

    /// Get the default layer index for LoRA injection.
    ///
    /// Uses the configured hidden_state_layer if set, otherwise defaults
    /// to the last transformer layer (n_layers - 1).
    pub fn default_lora_layer_idx(&self) -> usize {
        self.config
            .hidden_state_layer
            .as_ref()
            .and_then(|key| {
                // Parse layer index from key like "layer_31_output"
                key.strip_prefix("layer_")
                    .and_then(|s| s.split('_').next())
                    .and_then(|n| n.parse().ok())
            })
            .unwrap_or_else(|| self.n_layers.saturating_sub(1))
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
                id: TypedId::new(IdPrefix::Evt).to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "loss".to_string(),
                metric_value: loss as f64,
                metric_timestamp: Some(timestamp.clone()),
            },
            TrainingMetricRow {
                id: TypedId::new(IdPrefix::Evt).to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "tokens_per_sec".to_string(),
                metric_value: tokens_per_sec,
                metric_timestamp: Some(timestamp),
            },
            TrainingMetricRow {
                id: TypedId::new(IdPrefix::Evt).to_string(),
                training_job_id: job_id.clone(),
                step: step as i64,
                epoch: Some(epoch as i64),
                metric_name: "examples_per_sec".to_string(),
                metric_value: examples_per_sec,
                metric_timestamp: Some(Utc::now().to_rfc3339()),
            },
            TrainingMetricRow {
                id: TypedId::new(IdPrefix::Evt).to_string(),
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

    /// Force resume behavior when config mismatches checkpoint.
    pub fn set_force_resume(&mut self, force_resume: bool) {
        self.force_resume = force_resume;
    }

    /// Check if a latest checkpoint exists.
    pub async fn has_checkpoint(&self) -> bool {
        if let Some(manager) = self.checkpoint_manager.as_ref() {
            manager.has_checkpoint().await
        } else {
            false
        }
    }

    /// Resume training from a checkpoint
    ///
    /// Loads the latest checkpoint and returns the checkpoint data.
    /// Returns None if no checkpoint exists.
    /// Returns an error if a checkpoint exists but the config is incompatible.
    pub async fn try_resume_from_checkpoint(&self) -> Result<Option<TrainingCheckpoint>> {
        let manager = match self.checkpoint_manager.as_ref() {
            Some(manager) => manager,
            None => return Ok(None),
        };

        if !manager.has_checkpoint().await {
            info!("No checkpoint found, starting fresh training");
            return Ok(None);
        }

        match manager.load_latest().await {
            Ok(checkpoint) => {
                let mismatches = self.validate_config_compatibility(&checkpoint.config);
                if !mismatches.is_empty() {
                    if self.force_resume {
                        error!(
                            mismatches = ?mismatches,
                            "Force resume enabled; continuing despite config mismatch"
                        );
                    } else {
                        return Err(AosError::Config(format!(
                            "Cannot resume: config changed since checkpoint. Mismatches: {}. \
Use --force-resume to override (may produce incorrect results).",
                            mismatches.join(", ")
                        )));
                    }
                }

                let warnings = self.detect_config_warnings(&checkpoint.config);
                for warning in warnings {
                    warn!("Config change on resume: {}", warning);
                }

                Ok(Some(checkpoint))
            }
            Err(e) => {
                warn!(error = %e, "Failed to load checkpoint, starting fresh training");
                Ok(None)
            }
        }
    }

    fn validate_config_compatibility(&self, checkpoint_config: &TrainingConfig) -> Vec<String> {
        const FLOAT_TOLERANCE: f32 = 1e-6;
        let mut mismatches = Vec::new();

        if self.config.optimizer_config.optimizer_type
            != checkpoint_config.optimizer_config.optimizer_type
        {
            mismatches.push(format!(
                "optimizer_type: checkpoint={:?}, current={:?}",
                checkpoint_config.optimizer_config.optimizer_type,
                self.config.optimizer_config.optimizer_type
            ));
        }

        if self.config.hidden_dim != checkpoint_config.hidden_dim {
            mismatches.push(format!(
                "hidden_dim: checkpoint={} current={}",
                checkpoint_config.hidden_dim, self.config.hidden_dim
            ));
        }

        if self.config.rank != checkpoint_config.rank {
            mismatches.push(format!(
                "rank: checkpoint={} current={}",
                checkpoint_config.rank, self.config.rank
            ));
        }

        if self.config.hidden_state_layer != checkpoint_config.hidden_state_layer {
            mismatches.push(format!(
                "hidden_state_layer: checkpoint={:?} current={:?}",
                checkpoint_config.hidden_state_layer, self.config.hidden_state_layer
            ));
        }

        if self.config.base_model_path != checkpoint_config.base_model_path {
            mismatches.push(format!(
                "base_model_path: checkpoint={:?} current={:?}",
                checkpoint_config.base_model_path, self.config.base_model_path
            ));
        }
        if self.config.preprocessing != checkpoint_config.preprocessing {
            mismatches.push(format!(
                "preprocessing: checkpoint={:?} current={:?}",
                checkpoint_config.preprocessing, self.config.preprocessing
            ));
        }

        if (self.config.learning_rate - checkpoint_config.learning_rate).abs() > FLOAT_TOLERANCE {
            mismatches.push(format!(
                "learning_rate: checkpoint={} current={}",
                checkpoint_config.learning_rate, self.config.learning_rate
            ));
        }

        if self.config.batch_size != checkpoint_config.batch_size {
            mismatches.push(format!(
                "batch_size: checkpoint={} current={}",
                checkpoint_config.batch_size, self.config.batch_size
            ));
        }

        if (self.config.validation_split - checkpoint_config.validation_split).abs()
            > FLOAT_TOLERANCE
        {
            mismatches.push(format!(
                "validation_split: checkpoint={:.4} current={:.4}",
                checkpoint_config.validation_split, self.config.validation_split
            ));
        }

        let current_opt = self.config.optimizer_config.optimizer_type;
        let checkpoint_opt = checkpoint_config.optimizer_config.optimizer_type;
        if current_opt == checkpoint_opt {
            match current_opt {
                OptimizerType::Sgd => {
                    if (self.config.optimizer_config.momentum
                        - checkpoint_config.optimizer_config.momentum)
                        .abs()
                        > FLOAT_TOLERANCE
                    {
                        mismatches.push(format!(
                            "momentum: checkpoint={} current={}",
                            checkpoint_config.optimizer_config.momentum,
                            self.config.optimizer_config.momentum
                        ));
                    }
                    if (self.config.optimizer_config.weight_decay
                        - checkpoint_config.optimizer_config.weight_decay)
                        .abs()
                        > FLOAT_TOLERANCE
                    {
                        mismatches.push(format!(
                            "weight_decay: checkpoint={} current={}",
                            checkpoint_config.optimizer_config.weight_decay,
                            self.config.optimizer_config.weight_decay
                        ));
                    }
                }
                OptimizerType::Adam | OptimizerType::AdamW => {
                    if (self.config.optimizer_config.beta1
                        - checkpoint_config.optimizer_config.beta1)
                        .abs()
                        > FLOAT_TOLERANCE
                    {
                        mismatches.push(format!(
                            "beta1: checkpoint={} current={}",
                            checkpoint_config.optimizer_config.beta1,
                            self.config.optimizer_config.beta1
                        ));
                    }
                    if (self.config.optimizer_config.beta2
                        - checkpoint_config.optimizer_config.beta2)
                        .abs()
                        > FLOAT_TOLERANCE
                    {
                        mismatches.push(format!(
                            "beta2: checkpoint={} current={}",
                            checkpoint_config.optimizer_config.beta2,
                            self.config.optimizer_config.beta2
                        ));
                    }
                    if (self.config.optimizer_config.epsilon
                        - checkpoint_config.optimizer_config.epsilon)
                        .abs()
                        > FLOAT_TOLERANCE
                    {
                        mismatches.push(format!(
                            "epsilon: checkpoint={} current={}",
                            checkpoint_config.optimizer_config.epsilon,
                            self.config.optimizer_config.epsilon
                        ));
                    }
                    if (self.config.optimizer_config.weight_decay
                        - checkpoint_config.optimizer_config.weight_decay)
                        .abs()
                        > FLOAT_TOLERANCE
                    {
                        mismatches.push(format!(
                            "weight_decay: checkpoint={} current={}",
                            checkpoint_config.optimizer_config.weight_decay,
                            self.config.optimizer_config.weight_decay
                        ));
                    }
                }
            }
        }

        mismatches
    }

    fn detect_config_warnings(&self, checkpoint_config: &TrainingConfig) -> Vec<String> {
        const FLOAT_TOLERANCE: f32 = 1e-6;
        let mut warnings = Vec::new();

        if self.config.epochs != checkpoint_config.epochs {
            warnings.push(format!(
                "epochs: checkpoint={} current={}",
                checkpoint_config.epochs, self.config.epochs
            ));
        }

        if self.config.checkpoint_interval != checkpoint_config.checkpoint_interval {
            warnings.push(format!(
                "checkpoint_interval: checkpoint={:?} current={:?}",
                checkpoint_config.checkpoint_interval, self.config.checkpoint_interval
            ));
        }

        if self.config.early_stopping != checkpoint_config.early_stopping {
            warnings.push(format!(
                "early_stopping: checkpoint={:?} current={:?}",
                checkpoint_config.early_stopping, self.config.early_stopping
            ));
        }

        if self.config.patience != checkpoint_config.patience {
            warnings.push(format!(
                "patience: checkpoint={:?} current={:?}",
                checkpoint_config.patience, self.config.patience
            ));
        }

        match (self.config.min_delta, checkpoint_config.min_delta) {
            (Some(current), Some(checkpoint)) => {
                if (current - checkpoint).abs() > FLOAT_TOLERANCE {
                    warnings.push(format!(
                        "min_delta: checkpoint={} current={}",
                        checkpoint, current
                    ));
                }
            }
            (current, checkpoint) if current != checkpoint => {
                warnings.push(format!(
                    "min_delta: checkpoint={:?} current={:?}",
                    checkpoint, current
                ));
            }
            _ => {}
        }

        if (self.config.validation_split - checkpoint_config.validation_split).abs()
            > FLOAT_TOLERANCE
        {
            warnings.push(format!(
                "validation_split: checkpoint={} current={}",
                checkpoint_config.validation_split, self.config.validation_split
            ));
        }

        warnings
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
        self.train_with_resume_state(examples, on_epoch, None).await
    }

    /// Train with optional preloaded resume state.
    pub async fn train_with_resume_state<C>(
        &mut self,
        examples: &[TrainingExample],
        on_epoch: C,
        resume_state: Option<TrainingCheckpoint>,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        let mut resume_state = resume_state;
        let loaded_internally = resume_state.is_none();
        if loaded_internally {
            // Try to resume from checkpoint
            resume_state = self.try_resume_from_checkpoint().await?;
        }

        let prepared_dataset = self.prepare_datasets_for_training(examples)?;

        if let Some(checkpoint) = resume_state {
            if loaded_internally {
                info!(
                    start_epoch = checkpoint.epoch,
                    config = %checkpoint.config.summary(),
                    "Resuming training from checkpoint"
                );
            }
            // Restore optimizer state if present (for multi-module training)
            if let Some(ref opt_state) = checkpoint.multi_module_optimizer_state {
                self.multi_module_optimizer = opt_state.clone();
                info!(
                    modules = opt_state.module_states.len(),
                    "Restored multi-module optimizer state from checkpoint"
                );
            } else if self.config.multi_module_training {
                warn!(
                    "Multi-module training enabled but checkpoint has no optimizer state. \
                     Adam momentum will start fresh."
                );
            }
            self.run_training(
                prepared_dataset,
                checkpoint.epoch as usize,
                Some(checkpoint.weights),
                on_epoch,
            )
            .await
        } else {
            self.run_training(prepared_dataset, 0, None, on_epoch).await
        }
    }

    /// Train with automatic checkpoint resume using pre-split datasets.
    pub async fn train_with_resume_split<C>(
        &mut self,
        train_examples: &[TrainingExample],
        validation_examples: &[TrainingExample],
        on_epoch: C,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        self.train_with_resume_split_state(train_examples, validation_examples, on_epoch, None)
            .await
    }

    /// Train with optional preloaded resume state using pre-split datasets.
    pub async fn train_with_resume_split_state<C>(
        &mut self,
        train_examples: &[TrainingExample],
        validation_examples: &[TrainingExample],
        on_epoch: C,
        resume_state: Option<TrainingCheckpoint>,
    ) -> Result<TrainingResult>
    where
        C: FnMut(EpochMetrics),
    {
        let mut resume_state = resume_state;
        let loaded_internally = resume_state.is_none();
        if loaded_internally {
            resume_state = self.try_resume_from_checkpoint().await?;
        }

        let prepared_dataset =
            self.prepare_datasets_for_training_with_split(train_examples, validation_examples)?;

        if let Some(checkpoint) = resume_state {
            if loaded_internally {
                info!(
                    start_epoch = checkpoint.epoch,
                    config = %checkpoint.config.summary(),
                    "Resuming training from checkpoint (pre-split)"
                );
            }
            // Restore optimizer state if present (for multi-module training)
            if let Some(ref opt_state) = checkpoint.multi_module_optimizer_state {
                self.multi_module_optimizer = opt_state.clone();
                info!(
                    modules = opt_state.module_states.len(),
                    "Restored multi-module optimizer state from checkpoint"
                );
            } else if self.config.multi_module_training {
                warn!(
                    "Multi-module training enabled but checkpoint has no optimizer state. \
                     Adam momentum will start fresh."
                );
            }
            self.run_training(
                prepared_dataset,
                checkpoint.epoch as usize,
                Some(checkpoint.weights),
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
        let (use_cross_entropy_loss, use_chunked, vocab_threshold, force_ce, force_legacy) =
            self.resolve_cross_entropy_loss();
        self.use_cross_entropy_loss = use_cross_entropy_loss;
        if force_ce {
            info!(
                vocab_size = self.config.vocab_size,
                vocab_threshold, "Cross-entropy loss forced via AOS_TRAIN_FORCE_CE"
            );
        } else if force_legacy {
            warn!(
                vocab_size = self.config.vocab_size,
                vocab_threshold, "Legacy MSE loss forced via AOS_TRAIN_LEGACY_LOSS"
            );
        }
        if use_chunked {
            info!(
                vocab_size = self.config.vocab_size,
                vocab_threshold, "Using chunked cross-entropy for large vocabulary"
            );
        }

        // Use provided weights or initialize fresh
        let mut weights = match initial_weights {
            Some(w) => w,
            None => self.init_weights_deterministic()?,
        };

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
            #[cfg(feature = "multi-backend")]
            let epoch_loss = if self.config.multi_module_training {
                self.train_epoch_multi_module(&mut weights, examples, epoch)?
            } else {
                self.train_epoch_deterministic(&mut weights, examples, epoch)?
            };
            #[cfg(not(feature = "multi-backend"))]
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
                validation_loss: None,
                validation_perplexity: None,
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

            // Save checkpoint if configured (includes optimizer state for multi-module)
            if let Some(ref manager) = self.checkpoint_manager {
                let epoch_u32 = (epoch + 1) as u32;
                if manager.should_save(epoch_u32) {
                    let checkpoint = if self.config.multi_module_training {
                        TrainingCheckpoint::new_with_optimizer_state(
                            epoch_u32,
                            0,
                            epoch_loss,
                            self.config.learning_rate,
                            self.config.clone(),
                            weights.clone(),
                            self.multi_module_optimizer.clone(),
                        )
                    } else {
                        TrainingCheckpoint::new(
                            epoch_u32,
                            0,
                            epoch_loss,
                            self.config.learning_rate,
                            self.config.clone(),
                            weights.clone(),
                        )
                    };
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
            validation_loss_curve: Vec::new(),
            train_perplexity_curve: Vec::new(),
            validation_perplexity_curve: Vec::new(),
            split_hash_b3: self.split_hash_b3.clone(),
            train_example_count: self.train_example_count,
            validation_example_count: self.validation_example_count,
            train_token_count: self.train_token_count,
            validation_token_count: self.validation_token_count,
            best_validation: None,
            final_validation_loss: None,
            mlx_version: self.config.mlx_version.clone(),
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
    fn init_weights_deterministic(&self) -> Result<LoRAWeights> {
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
            modules: BTreeMap::new(),
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
        if !self.config.use_gpu_backward {
            self.validate_cpu_proxy_training()?;
            info!(
                "CPU proxy training enabled (use_gpu_backward=false); using scaled token targets with MSE loss"
            );
            if self.kernels.is_some() {
                warn!(
                    "GPU kernels initialized but use_gpu_backward=false; CPU proxy training will ignore GPU kernels"
                );
            }
            self.append_backend_reason("cpu_proxy_training");
            self.selected_backend = Some(TrainingBackend::Cpu);
            self.backend_device = self.resolve_backend_device(TrainingBackend::Cpu);
        } else {
            if self.selected_backend.is_none() {
                self.selected_backend = Some(TrainingBackend::Cpu);
            }
            if self.backend_device.is_none() {
                if let Some(backend) = self.selected_backend {
                    self.backend_device = self.resolve_backend_device(backend);
                }
            }
        }

        let backend_name = self.backend_info().unwrap_or("CPU");
        let using_gpu = self.using_gpu();
        let target_epochs = self.target_epochs();
        let correlation_id = self
            .correlation_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let job_id = self.job_id.clone().unwrap_or_else(|| "unknown".to_string());

        let prepared_dataset = self.prepare_datasets_for_training(examples)?;
        let total_examples = prepared_dataset.summary.total_examples;

        // Compute reproducibility metadata
        let dataset_hash = super::dataset::compute_examples_hash(examples);
        let config_hash = self.config.canonical_hash();
        let base_model_id = self
            .config
            .base_model_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "none".to_string());

        info!(
            job_id = %job_id,
            correlation_id = %correlation_id,
            dataset_hash = %dataset_hash,
            config_hash = %config_hash,
            base_model_id = %base_model_id,
            "Starting LoRA training: rank={}, epochs={}, examples={}, backend={}, seed={}, batch_size={}, max_tokens_per_batch={}",
            self.config.rank,
            target_epochs,
            total_examples,
            backend_name,
            self.training_seed,
            prepared_dataset.batch_plan.effective_batch_size,
            prepared_dataset.batch_plan.max_tokens_per_batch,
        );

        // Log training start with GPU information and reproducibility metadata
        self.telemetry.log(
            "training.started",
            serde_json::json!({
                "correlation_id": correlation_id,
                "job_id": job_id,
                "rank": self.config.rank,
                "epochs": target_epochs,
                "examples": total_examples,
                "seed": self.training_seed,
                "backend": backend_name,
                "using_gpu": using_gpu,
                "has_kernels": self.kernels.is_some(),
                "dataset_hash_b3": dataset_hash,
                "config_hash_b3": config_hash,
                "base_model_id": base_model_id,
                "mlx_version": self.config.mlx_version,
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
        let mut weights = match initial_weights {
            Some(w) => w,
            None => self.init_weights_deterministic()?,
        };

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

        // Initialize learning rate scheduler
        let total_training_steps =
            (target_epochs - start_epoch) as u32 * dataset.summary.total_examples as u32;
        let warmup_steps = self.config.warmup_steps.unwrap_or(0);
        let lr_config = if warmup_steps > 0 {
            // Use cosine decay with warmup if warmup is configured
            LRSchedulerConfig::cosine(
                self.config.learning_rate,
                self.config.learning_rate * 0.1, // Decay to 10% of initial
                total_training_steps,
            )
            .with_warmup(warmup_steps)
        } else {
            // Default: constant LR
            LRSchedulerConfig::constant(self.config.learning_rate)
        };
        self.lr_scheduler = Some(LRScheduler::new(lr_config));
        let accumulation_steps = self.gradient_accumulation_steps();
        info!(
            total_steps = total_training_steps,
            warmup_steps = warmup_steps,
            initial_lr = self.config.learning_rate,
            gradient_accumulation_steps = accumulation_steps,
            effective_batch_size =
                accumulation_steps * dataset.summary.total_examples / total_training_steps as usize,
            "Learning rate scheduler initialized"
        );
        if accumulation_steps > 1 {
            info!(
                "Gradient accumulation enabled: {} steps (effective batch multiplier)",
                accumulation_steps
            );
        }

        let curve_capacity = target_epochs.saturating_sub(start_epoch);
        let mut loss_curve = Vec::with_capacity(curve_capacity);
        let mut train_perplexity_curve = Vec::with_capacity(curve_capacity);
        let validation_enabled =
            !self.validation_examples.is_empty() && self.use_cross_entropy_loss;
        if !self.use_cross_entropy_loss && !self.validation_examples.is_empty() {
            warn!("Validation loss disabled for legacy MSE training");
        }
        let mut validation_loss_curve = Vec::with_capacity(curve_capacity);
        let mut validation_perplexity_curve = Vec::with_capacity(curve_capacity);
        let mut best_validation: Option<(f32, u32)> = None;
        #[cfg(feature = "multi-backend")]
        let mut epochs_without_improvement: u32 = 0;
        #[cfg(feature = "multi-backend")]
        let early_stopping_enabled =
            validation_enabled && self.config.early_stopping.unwrap_or(false);
        #[cfg(feature = "multi-backend")]
        let patience = self.config.patience.unwrap_or(5);
        #[cfg(feature = "multi-backend")]
        let min_delta = self.config.min_delta.unwrap_or(0.001);
        let training_loss_spec = if self.use_cross_entropy_loss {
            loss::training_loss_spec(self.config.ignore_index)
        } else {
            loss::legacy_training_loss_spec(self.config.ignore_index)
        };
        info!(loss_spec = %training_loss_spec.summary(), "Training loss spec");
        if validation_enabled {
            let validation_loss_spec = loss::validation_loss_spec(self.config.ignore_index);
            let diffs = training_loss_spec.diffs(&validation_loss_spec);
            if !diffs.is_empty() {
                warn!(
                    mismatches = ?diffs,
                    "Training and validation loss specs are not comparable"
                );
            }
            info!(
                loss_spec = %validation_loss_spec.summary(),
                "Validation loss spec"
            );
        }

        #[cfg(feature = "multi-backend")]
        let validation_output_proj = if validation_enabled {
            let model = self.base_model.as_ref().ok_or_else(|| {
                AosError::Training(
                    "Base model required for validation loss computation".to_string(),
                )
            })?;
            Some(model.get_weight("lm_head.weight")?)
        } else {
            None
        };

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
            #[cfg(feature = "multi-backend")]
            let epoch_loss = if self.config.multi_module_training {
                self.train_epoch_multi_module(&mut weights, &dataset, epoch)?
            } else {
                self.train_epoch_deterministic(&mut weights, &dataset, epoch)?
            };
            #[cfg(not(feature = "multi-backend"))]
            let epoch_loss = self.train_epoch_deterministic(&mut weights, &dataset, epoch)?;
            let epoch_duration_us = epoch_start.elapsed().as_micros() as u64;
            final_loss = epoch_loss;
            completed_epochs = (epoch + 1) as u32;
            loss_curve.push(epoch_loss);
            train_perplexity_curve.push(compute_perplexity(epoch_loss));
            examples_processed += dataset.summary.total_examples as u64;
            tokens_processed += tokens_per_epoch;
            self.total_tokens_processed = tokens_processed;
            self.total_examples_processed = examples_processed;

            let mut validation_loss: Option<f32> = None;
            let mut validation_perplexity: Option<f32> = None;
            let mut should_stop_early = false;

            if validation_enabled {
                #[cfg(not(feature = "multi-backend"))]
                {
                    return Err(AosError::Training(
                        "Validation loss requires multi-backend (MLX) support".to_string(),
                    ));
                }

                #[cfg(feature = "multi-backend")]
                {
                    let mut total_validation_loss = 0.0;
                    let mut validation_warnings: HashSet<String> = HashSet::new();

                    let output_proj = validation_output_proj.as_ref().ok_or_else(|| {
                        AosError::Training(
                            "Validation output projection missing for loss computation".to_string(),
                        )
                    })?;
                    for example in &self.validation_examples {
                        let (_output, hidden) = self.forward(&weights, example)?;
                        let report = loss::compute_validation_loss_with_output_proj(
                            &self.config,
                            &weights,
                            &hidden,
                            &example.target_tokens,
                            output_proj,
                        )?;
                        total_validation_loss += report.loss;
                        loss::merge_loss_warnings(&mut validation_warnings, &report);
                    }

                    if !validation_warnings.is_empty() {
                        for warning in validation_warnings {
                            warn!("Validation loss warning: {}", warning);
                        }
                    }

                    let val_loss = total_validation_loss / self.validation_examples.len() as f32;
                    let val_perplexity = compute_perplexity(val_loss);
                    validation_loss_curve.push(val_loss);
                    validation_perplexity_curve.push(val_perplexity);
                    validation_loss = Some(val_loss);
                    validation_perplexity = Some(val_perplexity);

                    let previous_best = best_validation.map(|(loss, _)| loss);
                    if previous_best.is_none_or(|best| val_loss < best) {
                        best_validation = Some((val_loss, (epoch + 1) as u32));
                    }

                    if early_stopping_enabled {
                        if let Some(best_loss) = previous_best {
                            let improvement = best_loss - val_loss;
                            if improvement > min_delta {
                                epochs_without_improvement = 0;
                            } else {
                                epochs_without_improvement =
                                    epochs_without_improvement.saturating_add(1);
                                if epochs_without_improvement >= patience {
                                    should_stop_early = true;
                                    info!(
                                        epoch = epoch + 1,
                                        best_validation_loss = best_validation
                                            .map(|(loss, _)| loss)
                                            .unwrap_or(val_loss),
                                        patience,
                                        min_delta,
                                        "Early stopping triggered: validation loss plateaued"
                                    );
                                }
                            }
                        } else {
                            epochs_without_improvement = 0;
                        }
                    }
                }
            }

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
                validation_loss,
                validation_perplexity,
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

            // Save checkpoint if configured (includes optimizer state for multi-module)
            if let Some(ref manager) = self.checkpoint_manager {
                let epoch_u32 = (epoch + 1) as u32;
                if manager.should_save(epoch_u32) {
                    let checkpoint = if self.config.multi_module_training {
                        TrainingCheckpoint::new_with_optimizer_state(
                            epoch_u32,
                            0,
                            epoch_loss,
                            self.config.learning_rate,
                            self.config.clone(),
                            weights.clone(),
                            self.multi_module_optimizer.clone(),
                        )
                    } else {
                        TrainingCheckpoint::new(
                            epoch_u32,
                            0,
                            epoch_loss,
                            self.config.learning_rate,
                            self.config.clone(),
                            weights.clone(),
                        )
                    };
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

            if should_stop_early {
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

        let final_validation_loss = validation_loss_curve.last().copied();

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
            validation_loss_curve,
            train_perplexity_curve,
            validation_perplexity_curve,
            split_hash_b3: self.split_hash_b3.clone(),
            train_example_count: self.train_example_count,
            validation_example_count: self.validation_example_count,
            train_token_count: self.train_token_count,
            validation_token_count: self.validation_token_count,
            best_validation,
            final_validation_loss,
            mlx_version: self.config.mlx_version.clone(),
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
                self.train_batch_deterministic(weights, batch.examples.as_slice(), epoch_seed)?;
            total_loss += loss;
            num_batches += 1;
        }

        Ok(total_loss / num_batches as f32)
    }

    /// Train one epoch with multi-module and multi-layer support.
    ///
    /// When `multi_module_training` is enabled, this function trains separate LoRA weights
    /// for each (layer, module) combination. If `lora_layer_indices` is empty, falls back
    /// to single-layer training at the default layer index.
    ///
    /// Module keys follow the pattern:
    /// - Multi-layer: `layer_{idx}.{module}` (e.g., `layer_0.q_proj`, `layer_31.v_proj`)
    /// - Single-layer fallback: `{module}` (e.g., `q_proj`, `v_proj`)
    #[cfg(feature = "multi-backend")]
    fn train_epoch_multi_module(
        &mut self,
        weights: &mut LoRAWeights,
        dataset: &PreparedDataset,
        epoch: usize,
    ) -> Result<f32> {
        // Create epoch-specific RNG seed
        let epoch_seed_bytes = derive_seed(
            &adapteros_core::B3Hash::hash(&self.training_seed.to_le_bytes()),
            &format!("epoch_{}_multi", epoch),
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

        let targets = self.config.targets.clone();
        let rank = self.config.rank;
        let hidden_dim = self.config.hidden_dim;
        if !self.use_cross_entropy_loss {
            return Err(AosError::Training(
                "Legacy MSE loss does not support multi-module training".to_string(),
            ));
        }

        // Determine layer indices: use configured list or fallback to single layer
        let layer_indices: Vec<usize> = if self.config.lora_layer_indices.is_empty() {
            vec![self.default_lora_layer_idx()]
        } else {
            self.config.lora_layer_indices.clone()
        };

        // Determine if we're in multi-layer mode (affects module key naming)
        let is_multi_layer = !self.config.lora_layer_indices.is_empty();

        if targets.is_empty() {
            return Err(AosError::Training(
                "No target modules specified for multi-module training".to_string(),
            ));
        }

        // Validate layer indices against model
        if self.n_layers > 0 {
            for layer_idx in &layer_indices {
                if *layer_idx >= self.n_layers {
                    return Err(AosError::Training(format!(
                        "Invalid layer index {}: model has {} layers (valid range: 0-{})",
                        layer_idx,
                        self.n_layers,
                        self.n_layers.saturating_sub(1)
                    )));
                }
            }
        }

        // Memory usage warning for large multi-layer configurations
        let weight_count = targets.len() * layer_indices.len();
        let weight_memory_mb = (weight_count * rank * hidden_dim * 4 * 2) / (1024 * 1024);
        if weight_count > 20 {
            warn!(
                "Multi-layer training: {} weight sets (~{} MB). Consider reducing targets or layers if memory constrained.",
                weight_count,
                weight_memory_mb
            );
        }

        info!(
            "Multi-module training: {} targets x {} layers = {} weight sets",
            targets.len(),
            layer_indices.len(),
            weight_count
        );

        // Clone the Arc to allow &mut self access during backward pass
        let model = self.base_model.clone().ok_or_else(|| {
            AosError::Training("Base model required for multi-module training".to_string())
        })?;

        // Validate hidden state availability before starting training
        // Use first example to probe model outputs
        if let Some(first_batch) = dataset.batches.first() {
            if let Some(first_example) = first_batch.examples.first() {
                // Position 0 for training - process full sequence from the start
                let (_, probe_hidden_states) =
                    model.forward_with_hidden_states(&first_example.input_tokens, 0)?;

                for layer_idx in &layer_indices {
                    for target in &targets {
                        let layer_key = self.layer_key_for_module(target, *layer_idx);
                        if !probe_hidden_states.contains_key(&layer_key) {
                            // Sort keys for deterministic error message
                            let mut available_keys: Vec<_> = probe_hidden_states.keys().collect();
                            available_keys.sort();
                            return Err(AosError::Training(format!(
                                "Hidden state '{}' not available for layer {} module '{}'. \
                                 Model may not capture this layer. Available hidden states: {:?}",
                                layer_key, layer_idx, target, available_keys
                            )));
                        }
                    }
                }
                debug!(
                    "Hidden state availability validated for {} layer/module combinations",
                    layer_indices.len() * targets.len()
                );
            }
        }

        let mut total_loss = 0.0;
        let mut num_updates = 0;

        const CANCEL_CHECK_INTERVAL: usize = 10;

        for (batch_count, batch) in dataset.batches.iter().enumerate() {
            // Check for cancellation
            if batch_count > 0 && batch_count % CANCEL_CHECK_INTERVAL == 0 && self.is_cancelled() {
                debug!(
                    epoch = epoch,
                    batch = batch_count,
                    "Cancellation detected mid-epoch in multi-module training"
                );
                return Ok(if num_updates > 0 {
                    total_loss / num_updates as f32
                } else {
                    0.0
                });
            }

            for example in batch.examples.iter() {
                if example.target_tokens.is_empty() {
                    return Err(AosError::Training(
                        "Training example missing target tokens".to_string(),
                    ));
                }
                let mut target_tokens = example.target_tokens.as_slice();
                let mut target_token_buf = [0u32; 1];
                if target_tokens.len() > 1 {
                    target_token_buf[0] = *target_tokens.last().ok_or_else(|| {
                        AosError::Training("Training example missing target tokens".to_string())
                    })?;
                    target_tokens = &target_token_buf;
                }

                // Get all hidden states from forward pass
                // Position 0 for training - process full sequence from the start
                let (_logits, hidden_states) =
                    model.forward_with_hidden_states(&example.input_tokens, 0)?;

                // Train each (layer, module) combination
                for layer_idx in &layer_indices {
                    for target in &targets {
                        let layer_key = self.layer_key_for_module(target, *layer_idx);

                        let hidden = hidden_states.get(&layer_key).ok_or_else(|| {
                            AosError::Training(format!(
                                "Hidden state '{}' not found for layer {} module '{}'. Available: {:?}",
                                layer_key,
                                layer_idx,
                                target,
                                hidden_states.keys().collect::<Vec<_>>()
                            ))
                        })?;
                        let hidden_slice = if hidden.len() == hidden_dim {
                            hidden.as_slice()
                        } else if hidden.len() > hidden_dim {
                            &hidden[hidden.len().saturating_sub(hidden_dim)..]
                        } else {
                            return Err(AosError::Training(format!(
                                "Hidden state '{}' size {} is smaller than hidden_dim {}",
                                layer_key,
                                hidden.len(),
                                hidden_dim
                            )));
                        };

                        // Module key: include layer index if multi-layer mode
                        let module_key = if is_multi_layer {
                            format!("layer_{}.{}", layer_idx, target)
                        } else {
                            target.clone()
                        };

                        // Get or create module weights
                        let module_weights =
                            weights.get_or_create_module(&module_key, rank, hidden_dim);

                        // Backward pass for this (layer, module) combination
                        let loss = self.backward_and_update_module_gpu_ce(
                            module_weights,
                            hidden_slice,
                            target_tokens,
                            &model,
                            epoch_seed.wrapping_add(num_updates as u64),
                            &module_key,
                        )?;

                        total_loss += loss;
                        num_updates += 1;

                        // Step LR scheduler after each training step
                        self.step_lr_scheduler();
                    }
                }
            }
        }

        if num_updates == 0 {
            return Err(AosError::Training(
                "No updates performed in multi-module training epoch".to_string(),
            ));
        }

        Ok(total_loss / num_updates as f32)
    }

    /// Train one batch with deterministic RNG (GPU-accelerated if kernels available)
    fn train_batch_deterministic(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
        epoch_seed: u64,
    ) -> Result<f32> {
        if self.config.use_gpu_backward {
            if self.kernels.is_some() {
                self.train_batch_gpu(weights, batch, epoch_seed)
            } else {
                Err(AosError::Training(
                    "GPU backward requested but no kernels are initialized. \
                     Provide --plan or set use_gpu_backward=false for CPU proxy training."
                        .to_string(),
                ))
            }
        } else {
            self.train_batch_cpu_proxy(weights, batch)
        }
    }

    fn validate_cpu_proxy_training(&self) -> Result<()> {
        if self.config.require_gpu {
            return Err(AosError::Training(
                "CPU proxy training is incompatible with require_gpu=true".to_string(),
            ));
        }
        if self.config.validation_split > 0.0 {
            return Err(AosError::Training(
                "CPU proxy training requires validation_split=0.0".to_string(),
            ));
        }
        if self.config.multi_module_training {
            return Err(AosError::Training(
                "CPU proxy training does not support multi_module_training".to_string(),
            ));
        }
        Ok(())
    }

    fn train_batch_cpu_proxy(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
    ) -> Result<f32> {
        let batch_start = Instant::now();
        let mut batch_loss = 0.0;
        let mut examples_used = 0usize;
        let learning_rate = self.get_current_lr();
        let scale = self.config.alpha / self.config.rank as f32;
        let batch_tokens = self.tokens_in_batch(batch);

        for example in batch {
            let hidden = if let Some(ref preprocessed) = example.preprocessed {
                preprocessed.as_slice()
            } else {
                example.scaled_input.as_slice()
            };

            if hidden.len() != self.config.hidden_dim {
                return Err(AosError::Training(format!(
                    "CPU proxy hidden state size mismatch: {} != {}",
                    hidden.len(),
                    self.config.hidden_dim
                )));
            }

            let lora_output = self.apply_lora(hidden, weights);
            let mut output = Vec::with_capacity(self.config.hidden_dim);
            for i in 0..self.config.hidden_dim {
                output.push(hidden[i] + lora_output[i] * scale);
            }

            let (loss, grad_output, valid_tokens) =
                self.compute_proxy_loss_and_grad(&output, &example.target_tokens)?;
            if valid_tokens == 0 {
                warn!(
                    "CPU proxy training skipped example with fully masked targets (ignore_index)"
                );
                continue;
            }

            batch_loss += loss;
            examples_used += 1;
            self.apply_proxy_update(weights, hidden, &grad_output, learning_rate)?;
        }

        if examples_used == 0 {
            return Err(AosError::Training(
                "CPU proxy training found no valid targets in batch".to_string(),
            ));
        }

        let cpu_time_ms = batch_start.elapsed().as_millis() as u64;
        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_cpu_time_ms += cpu_time_ms;
            metrics.cpu_operations += batch.len() as u64;
            metrics.total_examples_processed += batch.len() as u64;
            metrics.total_tokens_processed += batch_tokens;
            metrics.total_batches += 1;
        }

        Ok(batch_loss / examples_used as f32)
    }

    fn compute_proxy_loss_and_grad(
        &self,
        output: &[f32],
        target_tokens: &[u32],
    ) -> Result<(f32, Vec<f32>, usize)> {
        let usable = output.len().min(target_tokens.len());
        if usable == 0 {
            return Err(AosError::Training(
                "CPU proxy training requires non-empty target tokens".to_string(),
            ));
        }

        let mut grad_output = vec![0.0; output.len()];
        let mut loss_sum = 0.0f32;
        let mut count = 0usize;

        for i in 0..usable {
            let token = target_tokens[i];
            if self.config.ignore_index >= 0 && token as i32 == self.config.ignore_index {
                continue;
            }
            let target_val = self.scale_proxy_token(token);
            let diff = output[i] - target_val;
            loss_sum += diff * diff;
            grad_output[i] = diff;
            count += 1;
        }

        if count == 0 {
            return Ok((0.0, grad_output, 0));
        }

        let loss = loss_sum / count as f32;
        if !loss.is_finite() {
            return Err(AosError::Training(
                "CPU proxy loss is non-finite".to_string(),
            ));
        }

        let scale = 2.0 / count as f32;
        for grad in &mut grad_output[..usable] {
            *grad *= scale;
        }

        Ok((loss, grad_output, count))
    }

    fn scale_proxy_token(&self, token: u32) -> f32 {
        let denom = (self.config.vocab_size.saturating_sub(1)).max(1) as f32;
        ((token as f32) / denom) * 2.0 - 1.0
    }

    fn apply_proxy_update(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        grad_output: &[f32],
        learning_rate: f32,
    ) -> Result<()> {
        let mut grad_output = grad_output.to_vec();
        let mut grad_norm = 0.0f32;
        for grad in &grad_output {
            grad_norm += grad * grad;
        }
        let grad_norm = grad_norm.sqrt();
        if grad_norm > 1.0 {
            let scale = 1.0 / grad_norm;
            for grad in &mut grad_output {
                *grad *= scale;
            }
        }

        for grad in &mut grad_output {
            if !grad.is_finite() {
                *grad = 0.0;
            }
        }

        const MAX_UPDATE: f32 = 0.1;
        let hidden_len = hidden.len().min(self.config.hidden_dim);

        for r in 0..self.config.rank {
            for h_idx in 0..hidden_len {
                if h_idx < weights.lora_a[r].len() {
                    let update = (learning_rate * grad_output[h_idx] * hidden[h_idx])
                        .clamp(-MAX_UPDATE, MAX_UPDATE);
                    weights.lora_a[r][h_idx] -= update;
                }
            }
        }

        for h_idx in 0..self.config.hidden_dim {
            if h_idx < weights.lora_b.len() {
                for r in 0..self.config.rank {
                    if r < weights.lora_b[h_idx].len() {
                        let update = (learning_rate * grad_output[h_idx] * hidden[h_idx])
                            .clamp(-MAX_UPDATE, MAX_UPDATE);
                        weights.lora_b[h_idx][r] -= update;
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(not(feature = "multi-backend"))]
    /// Train one batch on GPU (using FusedKernels)
    fn train_batch_gpu(
        &mut self,
        _weights: &mut LoRAWeights,
        _batch: &[PreparedExample],
        _epoch_seed: u64,
    ) -> Result<f32> {
        Err(AosError::Training(
            "GPU backward is required for correct training. Enable multi-backend (MLX) support."
                .to_string(),
        ))
    }

    #[cfg(feature = "multi-backend")]
    /// Train one batch on GPU (using FusedKernels)
    fn train_batch_gpu(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
        epoch_seed: u64,
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

            let (hidden, forward_us) = if let Some(ref preprocessed) = example.preprocessed {
                (preprocessed.clone(), 0u64)
            } else {
                // Prepare IO buffers for GPU inference
                let mut io = IoBuffers::new(vocab_size);
                // MLX backend expects unpadded sequences; CoreML/Metal kernels require fixed length.
                io.input_ids = if self.selected_backend == Some(TrainingBackend::Mlx) {
                    let mut effective_len = example.input_tokens.len();
                    if let Some(max_seq) = self.config.max_seq_length {
                        effective_len = effective_len.min(max_seq as usize);
                    }
                    if let Some(last_real) =
                        example.input_mask.iter().rposition(|value| *value != 0)
                    {
                        effective_len = effective_len.min(last_real.saturating_add(1));
                    }
                    if effective_len == 0 {
                        example.input_tokens.clone()
                    } else if effective_len < example.input_tokens.len() {
                        debug!(
                            original_len = example.input_tokens.len(),
                            trimmed_len = effective_len,
                            "Trimming MLX training input to effective length"
                        );
                        example.input_tokens[..effective_len].to_vec()
                    } else {
                        example.input_tokens.clone()
                    }
                } else {
                    example.padded_input.clone()
                };
                io.position = 0;

                // Measure GPU forward pass time
                let gpu_start = Instant::now();

                // GPU forward pass through kernels
                if let Some(ref mut kernels) = self.kernels {
                    kernels.run_step(&ring, &mut io)?;
                }

                let forward_us = gpu_start.elapsed().as_micros() as u64;
                if matches!(self.selected_backend, Some(TrainingBackend::CoreML)) {
                    self.record_coreml_forward_latency(forward_us);
                }

                // Extract hidden state from GPU output
                let hidden: Vec<f32> = io.output_logits[..self.config.hidden_dim].to_vec();
                (hidden, forward_us)
            };

            let hidden_slice = if hidden.len() == self.config.hidden_dim {
                hidden.as_slice()
            } else if hidden.len() > self.config.hidden_dim {
                &hidden[hidden.len().saturating_sub(self.config.hidden_dim)..]
            } else {
                return Err(AosError::Training(format!(
                    "Preprocessed hidden state size mismatch: {} < {}",
                    hidden.len(),
                    self.config.hidden_dim
                )));
            };

            gpu_time_us += forward_us;

            // Backward pass and update weights using cross-entropy loss
            // GPU backward is always used with base model (now required)
            #[cfg(feature = "multi-backend")]
            {
                let loss = if self.should_use_gpu_backward() {
                    // GPU-accelerated backward pass via MLX autograd with cross-entropy loss
                    let backward_start = Instant::now();

                    // Clone the Arc to allow &mut self access in backward pass
                    let model = self.base_model.clone().ok_or_else(|| {
                        AosError::Training(
                            "Base model required for GPU backward pass with cross-entropy loss"
                                .to_string(),
                        )
                    })?;

                    if example.target_tokens.is_empty() {
                        return Err(AosError::Training(
                            "Training example missing target tokens".to_string(),
                        ));
                    }
                    let mut target_tokens = example.target_tokens.as_slice();
                    let mut target_token_buf = [0u32; 1];
                    if target_tokens.len() > 1 {
                        target_token_buf[0] = *target_tokens.last().ok_or_else(|| {
                            AosError::Training("Training example missing target tokens".to_string())
                        })?;
                        target_tokens = &target_token_buf;
                    }

                    let gpu_loss = self.backward_and_update_gpu_ce(
                        weights,
                        hidden_slice,
                        target_tokens,
                        &model,
                        epoch_seed,
                    )?;

                    gpu_time_us += backward_start.elapsed().as_micros() as u64;

                    // Step LR scheduler after each training step
                    self.step_lr_scheduler();

                    gpu_loss
                } else {
                    return Err(AosError::Training(
                        "GPU backward is required for correct training. Enable MLX GPU backward."
                            .to_string(),
                    ));
                };

                batch_loss += loss;
            }
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
            .map(|ex| (ex.input_tokens.len() + ex.target_tokens.len()) as u64)
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

    /// Forward pass with LoRA injection using real base model hidden states.
    ///
    /// Runs actual model inference to extract hidden states at the configured layer,
    /// then applies LoRA transformation to those real hidden states for proper
    /// cross-entropy loss computation.
    #[cfg(feature = "multi-backend")]
    fn forward(
        &self,
        weights: &LoRAWeights,
        example: &PreparedExample,
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        let model = self.base_model.as_ref().ok_or_else(|| {
            AosError::Training(
                "Base model not loaded. Training requires a base model for proper \
                 hidden state extraction and cross-entropy loss computation."
                    .to_string(),
            )
        })?;

        // Run actual model forward pass with hidden state capture
        // Use input_tokens (actual token IDs) not padded_input (which was incorrectly padded to hidden_dim)
        // Position 0 for training - process full sequence from the start
        let (_logits, hidden_states) =
            model.forward_with_hidden_states(&example.input_tokens, 0)?;

        // Extract hidden state from the configured layer
        let hidden_raw = hidden_states.get(&self.hidden_state_key).ok_or_else(|| {
            // Sort keys for deterministic error message
            let mut available_keys: Vec<_> = hidden_states.keys().collect();
            available_keys.sort();
            AosError::Training(format!(
                "Hidden state layer '{}' not found in model output. Available layers: {:?}",
                self.hidden_state_key, available_keys
            ))
        })?;

        // Handle 3D hidden states: model returns [batch, seq_len, hidden_dim] flattened
        // For LoRA training, we use the last token's hidden state
        let hidden_dim = self.config.hidden_dim;
        let hidden = if hidden_raw.len() == hidden_dim {
            // Already the right size (single position)
            hidden_raw.clone()
        } else if hidden_raw.len() % hidden_dim == 0 {
            // 3D tensor flattened - extract last token's hidden state
            let num_positions = hidden_raw.len() / hidden_dim;
            let last_token_start = (num_positions - 1) * hidden_dim;
            hidden_raw[last_token_start..].to_vec()
        } else {
            return Err(AosError::Training(format!(
                "Hidden state dimension mismatch: model returned {} elements, which is not divisible by hidden_dim {}",
                hidden_raw.len(),
                hidden_dim
            )));
        };

        // Apply LoRA transformation to the real hidden states
        let lora_output = self.apply_lora(&hidden, weights);

        // Combine: output = hidden + scale * lora_output
        let output: Vec<f32> = hidden
            .iter()
            .zip(lora_output.iter())
            .map(|(h, l)| h + l * self.config.alpha / self.config.rank as f32)
            .collect();

        Ok((output, hidden))
    }

    /// DEPRECATED: Proxy forward pass using scaled token IDs.
    ///
    /// This method is deprecated and produces incorrect adapters. It is only
    /// kept for migration purposes and will be removed in a future version.
    /// Always use the base model forward pass instead.
    #[deprecated(
        since = "0.13.0",
        note = "Produces incorrect adapters. Use forward() with base model instead."
    )]
    #[allow(dead_code)]
    fn forward_proxy(
        &self,
        weights: &LoRAWeights,
        example: &PreparedExample,
    ) -> Result<(Vec<f32>, Vec<f32>)> {
        warn!(
            "forward_proxy is deprecated and produces incorrect adapters. \
             Configure base_model_path for proper training."
        );

        // Use pre-scaled, padded hidden state from the CoreML pipeline.
        // This is a proxy representation, not real model hidden states.
        let mut hidden = example.scaled_input.clone();
        hidden.truncate(self.config.hidden_dim);
        if hidden.len() < self.config.hidden_dim {
            hidden.resize(self.config.hidden_dim, 0.0);
        }

        // Apply LoRA: output = hidden + scale * (hidden @ A @ B)
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

    /// Compute loss (simplified cross-entropy).
    #[cfg(test)]
    fn compute_loss_ce(&self, output: &[f32], target: &[u32]) -> f32 {
        self.compute_loss(output, target)
    }

    /// Compute loss (simplified cross-entropy).
    #[cfg(test)]
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
    #[cfg(test)]
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
        let learning_rate = self.get_current_lr();
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

    /// GPU-accelerated backward pass using MLX autograd.
    ///
    /// Uses MLX's value_and_grad for efficient GPU-based gradient computation.
    /// Requires MLX backend and `use_gpu_backward` config flag.
    ///
    /// Note: GPU backward may not be bit-exact with CPU backward due to
    /// floating-point operation ordering differences in parallel reductions.
    #[cfg(feature = "multi-backend")]
    fn backward_and_update_gpu(
        &self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        target: &[u32],
        seed: u64,
    ) -> Result<f32> {
        use adapteros_lora_mlx_ffi::MLXFFITensor;

        let rank = self.config.rank;
        let hidden_dim = self.config.hidden_dim;
        let alpha = self.config.alpha;
        let learning_rate = self.get_current_lr();

        // Convert hidden state to MLX tensor [1, hidden_dim]
        let hidden_tensor = MLXFFITensor::from_data(hidden, vec![1, hidden_dim])?;

        // Convert targets to MLX tensor [1, seq_len]
        let target_f32: Vec<f32> = target.iter().map(|&t| t as f32).collect();
        let targets_tensor = MLXFFITensor::from_data(&target_f32, vec![1, target.len()])?;

        // Convert LoRA A weights to tensor [rank, hidden_dim]
        let lora_a_flat: Vec<f32> = weights.lora_a.iter().flatten().copied().collect();
        let lora_a_tensor = MLXFFITensor::from_data(&lora_a_flat, vec![rank, hidden_dim])?;

        // Convert LoRA B weights to tensor [hidden_dim, rank]
        let lora_b_flat: Vec<f32> = weights.lora_b.iter().flatten().copied().collect();
        let lora_b_tensor = MLXFFITensor::from_data(&lora_b_flat, vec![hidden_dim, rank])?;

        // Compute loss and gradients on GPU
        let result = mlx_lora_backward_gpu(
            &hidden_tensor,
            &targets_tensor,
            &lora_a_tensor,
            &lora_b_tensor,
            alpha,
            rank,
            seed,
        )?;

        let loss = result.loss;

        // Create optimizer based on config (defaults to Adam)
        let mut optimizer = self.create_optimizer(learning_rate)?;

        // Get mutable references to gradient tensors for clipping
        let mut grad_a = result.grad_a;
        let mut grad_b = result.grad_b;

        // Clip gradients
        use adapteros_lora_mlx_ffi::training::mlx_clip_grad_norm_gpu;
        let grad_norm =
            mlx_clip_grad_norm_gpu(&mut [grad_a.clone_tensor()?, grad_b.clone_tensor()?], 1.0);
        if grad_norm > 1.0 {
            debug!("GPU clipped gradient norm from {:.4} to 1.0", grad_norm);
        }

        // Apply optimizer step
        // Note: step() takes ownership through mut slice, so we need to reassign after
        let mut params = [lora_a_tensor, lora_b_tensor];
        let grads_array = [grad_a, grad_b];
        optimizer.step(&mut params, &grads_array)?;

        // Copy updated weights back to CPU
        let new_lora_a = params[0].to_float_vec()?;
        let new_lora_b = params[1].to_float_vec()?;

        // Update weights in-place
        for r in 0..rank {
            for h in 0..hidden_dim {
                weights.lora_a[r][h] = new_lora_a[r * hidden_dim + h];
            }
        }
        for h in 0..hidden_dim {
            for r in 0..rank {
                weights.lora_b[h][r] = new_lora_b[h * rank + r];
            }
        }

        Ok(loss)
    }

    /// GPU backward pass with cross-entropy loss for real language model training.
    ///
    /// This method uses the base model's output projection (lm_head) to compute
    /// proper cross-entropy loss against target token IDs, enabling real LLM fine-tuning.
    ///
    /// Requires:
    /// - Base model loaded with `lm_head.weight` available
    /// - MLX backend selected
    ///
    /// Supports gradient accumulation when `gradient_accumulation_steps > 1`.
    #[cfg(feature = "multi-backend")]
    fn backward_and_update_gpu_ce(
        &mut self,
        weights: &mut LoRAWeights,
        hidden: &[f32],
        target_tokens: &[u32],
        model: &adapteros_lora_mlx_ffi::MLXFFIModel,
        seed: u64,
    ) -> Result<f32> {
        use adapteros_lora_mlx_ffi::MLXFFITensor;

        let rank = self.config.rank;
        let hidden_dim = self.config.hidden_dim;
        let alpha = self.config.alpha;
        let learning_rate = self.get_current_lr();

        // Get output projection (lm_head) weights from base model
        let output_proj = model.get_weight("lm_head.weight")?;

        // Convert hidden state to MLX tensor [1, hidden_dim]
        let hidden_tensor = MLXFFITensor::from_data(hidden, vec![1, hidden_dim])?;

        // Convert targets to MLX tensor [1, seq_len] as i32 for indexing
        let targets_i32: Vec<i32> = target_tokens.iter().map(|&t| t as i32).collect();
        let targets_tensor = MLXFFITensor::from_ints(&targets_i32, vec![1, target_tokens.len()])?;

        // Convert LoRA A weights to tensor [rank, hidden_dim]
        let lora_a_flat: Vec<f32> = weights.lora_a.iter().flatten().copied().collect();
        let lora_a_tensor = MLXFFITensor::from_data(&lora_a_flat, vec![rank, hidden_dim])?;

        // Convert LoRA B weights to tensor [hidden_dim, rank]
        let lora_b_flat: Vec<f32> = weights.lora_b.iter().flatten().copied().collect();
        let lora_b_tensor = MLXFFITensor::from_data(&lora_b_flat, vec![hidden_dim, rank])?;

        // Compute loss and gradients on GPU using cross-entropy
        // ignore_index = 0 (typically padding token)
        let result = mlx_lora_backward_ce_gpu(
            &hidden_tensor,
            &output_proj,
            &targets_tensor,
            &lora_a_tensor,
            &lora_b_tensor,
            alpha,
            rank,
            LOSS_IGNORE_INDEX,
            seed,
        )?;

        let loss = result.loss;
        let accumulation_steps = self.gradient_accumulation_steps();

        // Get mutable references to gradient tensors for clipping
        let mut grad_a = result.grad_a;
        let mut grad_b = result.grad_b;

        // Clip gradients before accumulation
        use adapteros_lora_mlx_ffi::training::mlx_clip_grad_norm_gpu;
        let grad_norm =
            mlx_clip_grad_norm_gpu(&mut [grad_a.clone_tensor()?, grad_b.clone_tensor()?], 1.0);
        if grad_norm > 1.0 {
            debug!(
                "GPU (CE) clipped gradient norm from {:.4} to 1.0",
                grad_norm
            );
        }

        // Extract gradients to CPU for accumulation
        let grad_a_cpu = grad_a.to_float_vec()?;
        let grad_b_cpu = grad_b.to_float_vec()?;

        // Use "default" key for legacy single-module path
        let module_key = "default";
        let accum_entry = self
            .accumulated_gradients
            .entry(module_key.to_string())
            .or_insert_with(|| {
                let a_size = rank * hidden_dim;
                let b_size = hidden_dim * rank;
                (vec![0.0; a_size], vec![0.0; b_size], 0)
            });

        // Accumulate gradients (scale by 1/N for averaging)
        let scale = 1.0 / accumulation_steps as f32;
        for (i, &g) in grad_a_cpu.iter().enumerate() {
            accum_entry.0[i] += g * scale;
        }
        for (i, &g) in grad_b_cpu.iter().enumerate() {
            accum_entry.1[i] += g * scale;
        }
        accum_entry.2 += 1;

        // Only apply optimizer step when accumulation is complete
        if accum_entry.2 >= accumulation_steps {
            // Get optimizer state (CPU-side, checkpointable)
            let opt_state = self
                .multi_module_optimizer
                .get_or_create(module_key, rank, hidden_dim);

            // Get optimizer hyperparameters
            let beta1 = self.config.optimizer_config.beta1;
            let beta2 = self.config.optimizer_config.beta2;
            let epsilon = self.config.optimizer_config.epsilon;

            // Apply CPU-native Adam optimizer (state is checkpointable)
            let lora_a_flat: Vec<f32> = weights.lora_a.iter().flatten().copied().collect();
            let lora_b_flat: Vec<f32> = weights.lora_b.iter().flatten().copied().collect();

            let (new_lora_a, new_lora_b) = opt_state.adam_step(
                &lora_a_flat,
                &lora_b_flat,
                &accum_entry.0,
                &accum_entry.1,
                learning_rate,
                beta1,
                beta2,
                epsilon,
                rank,
                hidden_dim,
            );

            // Update weights in-place
            for r in 0..rank {
                for h in 0..hidden_dim {
                    weights.lora_a[r][h] = new_lora_a[r * hidden_dim + h];
                }
            }
            for h in 0..hidden_dim {
                for r in 0..rank {
                    weights.lora_b[h][r] = new_lora_b[h * rank + r];
                }
            }

            // Clear accumulation buffer
            accum_entry.0.fill(0.0);
            accum_entry.1.fill(0.0);
            accum_entry.2 = 0;
        }

        Ok(loss)
    }

    /// GPU backward pass with cross-entropy loss for a specific module.
    ///
    /// This is the multi-module version that updates weights for a single target module
    /// (e.g., q_proj, k_proj) rather than the legacy single-weight approach.
    ///
    /// Uses persistent per-module optimizers to maintain Adam momentum across steps.
    #[cfg(feature = "multi-backend")]
    fn backward_and_update_module_gpu_ce(
        &mut self,
        module_weights: &mut ModuleWeights,
        hidden: &[f32],
        target_tokens: &[u32],
        model: &adapteros_lora_mlx_ffi::MLXFFIModel,
        seed: u64,
        module_key: &str, // Module key for optimizer lookup (e.g., "layer_0.q_proj")
    ) -> Result<f32> {
        use adapteros_lora_mlx_ffi::MLXFFITensor;

        let rank = self.config.rank;
        let hidden_dim = self.config.hidden_dim;
        let alpha = self.config.alpha;
        let learning_rate = self.get_current_lr();

        // Get output projection (lm_head) weights from base model
        let output_proj = model.get_weight("lm_head.weight")?;

        // Convert hidden state to MLX tensor [1, hidden_dim]
        let hidden_tensor = MLXFFITensor::from_data(hidden, vec![1, hidden_dim])?;

        // Convert targets to MLX tensor [1, seq_len] as i32 for indexing
        let targets_i32: Vec<i32> = target_tokens.iter().map(|&t| t as i32).collect();
        let targets_tensor = MLXFFITensor::from_ints(&targets_i32, vec![1, target_tokens.len()])?;

        // Convert module LoRA A weights to tensor [rank, hidden_dim]
        let lora_a_flat: Vec<f32> = module_weights.lora_a.iter().flatten().copied().collect();
        let lora_a_tensor = MLXFFITensor::from_data(&lora_a_flat, vec![rank, hidden_dim])?;

        // Convert module LoRA B weights to tensor [hidden_dim, rank]
        let lora_b_flat: Vec<f32> = module_weights.lora_b.iter().flatten().copied().collect();
        let lora_b_tensor = MLXFFITensor::from_data(&lora_b_flat, vec![hidden_dim, rank])?;

        // Compute loss and gradients on GPU using cross-entropy
        let result = mlx_lora_backward_ce_gpu(
            &hidden_tensor,
            &output_proj,
            &targets_tensor,
            &lora_a_tensor,
            &lora_b_tensor,
            alpha,
            rank,
            LOSS_IGNORE_INDEX,
            seed,
        )?;

        let loss = result.loss;
        let accumulation_steps = self.gradient_accumulation_steps();

        // Get mutable references to gradient tensors for clipping
        let mut grad_a = result.grad_a;
        let mut grad_b = result.grad_b;

        // Clip gradients before accumulation
        use adapteros_lora_mlx_ffi::training::mlx_clip_grad_norm_gpu;
        let grad_norm =
            mlx_clip_grad_norm_gpu(&mut [grad_a.clone_tensor()?, grad_b.clone_tensor()?], 1.0);
        if grad_norm > 1.0 {
            debug!(
                "GPU (CE multi-module) clipped gradient norm from {:.4} to 1.0 for {}",
                grad_norm, module_key
            );
        }

        // Extract gradients to CPU for accumulation
        let grad_a_cpu = grad_a.to_float_vec()?;
        let grad_b_cpu = grad_b.to_float_vec()?;

        // Get or create gradient accumulation buffer for this module
        let accum_entry = self
            .accumulated_gradients
            .entry(module_key.to_string())
            .or_insert_with(|| {
                let a_size = rank * hidden_dim;
                let b_size = hidden_dim * rank;
                (vec![0.0; a_size], vec![0.0; b_size], 0)
            });

        // Accumulate gradients (scale by 1/N for averaging)
        let scale = 1.0 / accumulation_steps as f32;
        for (i, &g) in grad_a_cpu.iter().enumerate() {
            accum_entry.0[i] += g * scale;
        }
        for (i, &g) in grad_b_cpu.iter().enumerate() {
            accum_entry.1[i] += g * scale;
        }
        accum_entry.2 += 1;

        // Only apply optimizer step when accumulation is complete
        if accum_entry.2 >= accumulation_steps {
            // Get optimizer state (CPU-side, checkpointable)
            let opt_state = self
                .multi_module_optimizer
                .get_or_create(module_key, rank, hidden_dim);

            // Get optimizer hyperparameters
            let beta1 = self.config.optimizer_config.beta1;
            let beta2 = self.config.optimizer_config.beta2;
            let epsilon = self.config.optimizer_config.epsilon;

            // Apply CPU-native Adam optimizer (state is checkpointable)
            let lora_a_flat: Vec<f32> = module_weights.lora_a.iter().flatten().copied().collect();
            let lora_b_flat: Vec<f32> = module_weights.lora_b.iter().flatten().copied().collect();

            let (new_lora_a, new_lora_b) = opt_state.adam_step(
                &lora_a_flat,
                &lora_b_flat,
                &accum_entry.0,
                &accum_entry.1,
                learning_rate,
                beta1,
                beta2,
                epsilon,
                rank,
                hidden_dim,
            );

            // Update module weights in-place
            for r in 0..rank {
                for h in 0..hidden_dim {
                    module_weights.lora_a[r][h] = new_lora_a[r * hidden_dim + h];
                }
            }
            for h in 0..hidden_dim {
                for r in 0..rank {
                    module_weights.lora_b[h][r] = new_lora_b[h * rank + r];
                }
            }

            // Clear accumulation buffer
            accum_entry.0.fill(0.0);
            accum_entry.1.fill(0.0);
            accum_entry.2 = 0;

            debug!(
                "Applied accumulated gradients for {} (accumulated {} steps)",
                module_key, accumulation_steps
            );
        }

        Ok(loss)
    }

    /// Check if GPU backward pass should be used for this training session.
    fn should_use_gpu_backward(&self) -> bool {
        // Only use GPU backward when:
        // 1. Config explicitly enables it
        // 2. Using MLX backend (which supports autograd)
        // 3. Multi-backend feature is enabled
        #[cfg(feature = "multi-backend")]
        {
            self.config.use_gpu_backward
                && matches!(self.selected_backend, Some(TrainingBackend::Mlx))
        }
        #[cfg(not(feature = "multi-backend"))]
        {
            false
        }
    }

    /// Decide whether to use cross-entropy loss and chunked mode based on vocab size.
    ///
    /// Returns: (use_cross_entropy, use_chunked, vocab_threshold, force_ce, force_legacy)
    ///
    /// Cross-entropy loss is now enabled by default for all vocabulary sizes thanks to
    /// the chunked implementation which handles large vocabularies (>100K tokens) by
    /// processing in memory-efficient chunks with log-sum-exp numerical stability.
    fn resolve_cross_entropy_loss(&self) -> (bool, bool, usize, bool, bool) {
        let force_ce = std::env::var("AOS_TRAIN_FORCE_CE").ok().as_deref() == Some("1");
        let force_legacy = std::env::var("AOS_TRAIN_LEGACY_LOSS").ok().as_deref() == Some("1");
        let vocab_threshold = std::env::var("AOS_TRAIN_CE_CHUNK_THRESHOLD")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(CHUNKED_CE_VOCAB_THRESHOLD);

        // CE loss is now always enabled unless explicitly forced to legacy mode
        let use_cross_entropy = !force_legacy;

        // Use chunked mode for large vocabularies (memory-efficient, numerically stable)
        let use_chunked = self.config.vocab_size > vocab_threshold;

        (
            use_cross_entropy,
            use_chunked,
            vocab_threshold,
            force_ce,
            force_legacy,
        )
    }

    /// Create optimizer based on OptimizerConfig settings.
    ///
    /// Uses the configured optimizer type (SGD, Adam, AdamW) with the
    /// appropriate hyperparameters from the config.
    #[cfg(feature = "multi-backend")]
    fn create_optimizer(&self, learning_rate: f32) -> Result<MlxOptimizer> {
        let opt_config = &self.config.optimizer_config;
        match opt_config.optimizer_type {
            OptimizerType::Sgd => {
                MlxOptimizer::sgd(learning_rate, opt_config.momentum, opt_config.weight_decay)
            }
            OptimizerType::Adam | OptimizerType::AdamW => {
                // AdamW uses the same MLX function but with weight_decay > 0
                MlxOptimizer::adam(
                    learning_rate,
                    opt_config.beta1,
                    opt_config.beta2,
                    opt_config.epsilon,
                    opt_config.weight_decay,
                )
            }
        }
    }

    /// Get current learning rate from scheduler, or fallback to config
    fn get_current_lr(&self) -> f32 {
        self.lr_scheduler
            .as_ref()
            .map(|s| s.get_lr())
            .unwrap_or(self.config.learning_rate)
    }

    /// Step the learning rate scheduler (call after each training step)
    fn step_lr_scheduler(&mut self) {
        if let Some(ref mut scheduler) = self.lr_scheduler {
            scheduler.step();
        }
        self.global_step += 1;
    }

    /// Get gradient accumulation steps (defaults to 1 = no accumulation)
    fn gradient_accumulation_steps(&self) -> usize {
        self.config.gradient_accumulation_steps.unwrap_or(1).max(1) as usize
    }

    /// MoE-aware backward pass with routing-weighted gradients.
    ///
    /// Gradients are scaled by the routing weights for active experts.
    /// For routing-weighted shared LoRA:
    /// `grad_scale = sum(routing_weight[e]) for e in active_experts`
    #[cfg(test)]
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
        let learning_rate = self.get_current_lr();
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
    #[cfg(test)]
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
            let loss = self.compute_loss_ce(&output, &example.target_tokens);
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

    /// Train one batch for MoE models with GPU-accelerated backward pass.
    ///
    /// For training, we simulate routing by distributing examples across experts
    /// using deterministic routing based on the example index and MoE config.
    /// Gradients are scaled by the routing weight sum before accumulation.
    #[cfg(all(not(test), feature = "multi-backend"))]
    #[allow(dead_code)]
    fn train_batch_moe(
        &mut self,
        weights: &mut LoRAWeights,
        batch: &[PreparedExample],
        epoch_seed: u64,
    ) -> Result<f32> {
        use adapteros_lora_mlx_ffi::training::mlx_clip_grad_norm_gpu;
        use adapteros_lora_mlx_ffi::MLXFFITensor;

        let moe_config = self.config.moe_config.as_ref().ok_or_else(|| {
            AosError::Training("MoE batch training requires moe_config".to_string())
        })?;

        let batch_start = Instant::now();
        let mut batch_loss = 0.0;
        let batch_tokens = self.tokens_in_batch(batch);
        let num_experts_per_token = moe_config.num_experts_per_token;

        let rank = self.config.rank;
        let hidden_dim = self.config.hidden_dim;
        let alpha = self.config.alpha;
        let learning_rate = self.get_current_lr();
        let accumulation_steps = self.gradient_accumulation_steps();

        // Get base model for GPU backward pass
        let model = self.base_model.clone().ok_or_else(|| {
            AosError::Training(
                "Base model required for MoE GPU backward pass with cross-entropy loss".to_string(),
            )
        })?;

        // Get output projection weights once for the batch
        let output_proj = model.get_weight("lm_head.weight")?;

        for (idx, example) in batch.iter().enumerate() {
            // Simulate routing weights (deterministic based on example and expert index)
            // In production, these would come from the actual router
            let routing_weights: Vec<f32> = (0..num_experts_per_token)
                .map(|e| {
                    let seed = (idx * 1000 + e) as f32;
                    let weight = (seed.sin().abs() + 0.5) / num_experts_per_token as f32;
                    weight.min(1.0)
                })
                .collect();

            // Normalize to sum to ~1.0
            let sum: f32 = routing_weights.iter().sum();
            let routing_scale: f32 = if sum > 0.0 {
                sum / num_experts_per_token as f32
            } else {
                1.0 / num_experts_per_token as f32
            };

            // Get hidden state from example
            let hidden = if let Some(ref preprocessed) = example.preprocessed {
                preprocessed.clone()
            } else {
                example.scaled_input.clone()
            };

            let hidden_slice = if hidden.len() >= hidden_dim {
                &hidden[hidden.len().saturating_sub(hidden_dim)..]
            } else {
                return Err(AosError::Training(format!(
                    "Hidden state size mismatch: {} < {}",
                    hidden.len(),
                    hidden_dim
                )));
            };

            // Prepare target tokens
            if example.target_tokens.is_empty() {
                return Err(AosError::Training(
                    "Training example missing target tokens".to_string(),
                ));
            }
            let mut target_tokens = example.target_tokens.as_slice();
            let mut target_token_buf = [0u32; 1];
            if target_tokens.len() > 1 {
                target_token_buf[0] = *target_tokens.last().ok_or_else(|| {
                    AosError::Training("Training example missing target tokens".to_string())
                })?;
                target_tokens = &target_token_buf;
            }

            // Convert to MLX tensors
            let hidden_tensor = MLXFFITensor::from_data(hidden_slice, vec![1, hidden_dim])?;
            let targets_i32: Vec<i32> = target_tokens.iter().map(|&t| t as i32).collect();
            let targets_tensor =
                MLXFFITensor::from_ints(&targets_i32, vec![1, target_tokens.len()])?;
            let lora_a_flat: Vec<f32> = weights.lora_a.iter().flatten().copied().collect();
            let lora_a_tensor = MLXFFITensor::from_data(&lora_a_flat, vec![rank, hidden_dim])?;
            let lora_b_flat: Vec<f32> = weights.lora_b.iter().flatten().copied().collect();
            let lora_b_tensor = MLXFFITensor::from_data(&lora_b_flat, vec![hidden_dim, rank])?;

            // Compute loss and gradients on GPU
            let result = mlx_lora_backward_ce_gpu(
                &hidden_tensor,
                &output_proj,
                &targets_tensor,
                &lora_a_tensor,
                &lora_b_tensor,
                alpha,
                rank,
                LOSS_IGNORE_INDEX,
                epoch_seed.wrapping_add(idx as u64),
            )?;

            let loss = result.loss;
            batch_loss += loss;

            // Get mutable references to gradient tensors for clipping
            let mut grad_a = result.grad_a;
            let mut grad_b = result.grad_b;

            // Clip gradients before accumulation
            let grad_norm =
                mlx_clip_grad_norm_gpu(&mut [grad_a.clone_tensor()?, grad_b.clone_tensor()?], 1.0);
            if grad_norm > 1.0 {
                debug!("MoE GPU clipped gradient norm from {:.4} to 1.0", grad_norm);
            }

            // Extract gradients to CPU for routing-scaled accumulation
            let grad_a_cpu = grad_a.to_float_vec()?;
            let grad_b_cpu = grad_b.to_float_vec()?;

            // Use "default" key for legacy single-module path
            let module_key = "default";
            let accum_entry = self
                .accumulated_gradients
                .entry(module_key.to_string())
                .or_insert_with(|| {
                    let a_size = rank * hidden_dim;
                    let b_size = hidden_dim * rank;
                    (vec![0.0; a_size], vec![0.0; b_size], 0)
                });

            // Accumulate gradients with routing scale applied
            // Scale by routing_scale / accumulation_steps for proper MoE weighting
            let scale = routing_scale / accumulation_steps as f32;
            for (i, &g) in grad_a_cpu.iter().enumerate() {
                accum_entry.0[i] += g * scale;
            }
            for (i, &g) in grad_b_cpu.iter().enumerate() {
                accum_entry.1[i] += g * scale;
            }
            accum_entry.2 += 1;

            // Apply optimizer step when accumulation is complete
            if accum_entry.2 >= accumulation_steps {
                let opt_state = self
                    .multi_module_optimizer
                    .get_or_create(module_key, rank, hidden_dim);

                let beta1 = self.config.optimizer_config.beta1;
                let beta2 = self.config.optimizer_config.beta2;
                let epsilon = self.config.optimizer_config.epsilon;

                let lora_a_flat: Vec<f32> = weights.lora_a.iter().flatten().copied().collect();
                let lora_b_flat: Vec<f32> = weights.lora_b.iter().flatten().copied().collect();

                let (new_lora_a, new_lora_b) = opt_state.adam_step(
                    &lora_a_flat,
                    &lora_b_flat,
                    &accum_entry.0,
                    &accum_entry.1,
                    learning_rate,
                    beta1,
                    beta2,
                    epsilon,
                    rank,
                    hidden_dim,
                );

                // Update weights in-place
                for r in 0..rank {
                    for h in 0..hidden_dim {
                        weights.lora_a[r][h] = new_lora_a[r * hidden_dim + h];
                    }
                }
                for h in 0..hidden_dim {
                    for r in 0..rank {
                        weights.lora_b[h][r] = new_lora_b[h * rank + r];
                    }
                }

                // Clear accumulation buffer
                accum_entry.0.fill(0.0);
                accum_entry.1.fill(0.0);
                accum_entry.2 = 0;
            }

            // Step LR scheduler after each training step
            self.step_lr_scheduler();
        }

        // Update performance metrics
        let batch_time_ms = batch_start.elapsed().as_millis() as u64;
        {
            let mut metrics = self.performance_metrics.write();
            metrics.total_cpu_time_ms += batch_time_ms;
            metrics.cpu_operations += batch.len() as u64;
            metrics.total_examples_processed += batch.len() as u64;
            metrics.total_tokens_processed += batch_tokens;
            metrics.total_batches += 1;
        }

        Ok(batch_loss / batch.len() as f32)
    }

    /// MoE training stub for non-multi-backend builds.
    #[cfg(all(not(test), not(feature = "multi-backend")))]
    #[allow(dead_code)]
    fn train_batch_moe(
        &mut self,
        _weights: &mut LoRAWeights,
        _batch: &[PreparedExample],
        _epoch_seed: u64,
    ) -> Result<f32> {
        Err(AosError::Training(
            "MoE training requires multi-backend feature".to_string(),
        ))
    }

    /// Generate unique adapter ID
    fn generate_adapter_id() -> String {
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("microlora_{}", timestamp)
    }

    /// Explicitly release GPU kernel resources
    ///
    /// This method should be called when training completes or fails to ensure
    /// GPU resources are released promptly. While Drop will also release them,
    /// calling this explicitly provides better control and logging.
    pub fn release_kernels(&mut self) {
        if let Some(kernels) = self.kernels.take() {
            tracing::info!("Training complete, releasing GPU kernels");
            drop(kernels);
        }
    }
}

impl Drop for MicroLoRATrainer {
    fn drop(&mut self) {
        if let Some(kernels) = self.kernels.take() {
            tracing::debug!("Releasing GPU kernel resources");
            // The kernels will be dropped here, releasing GPU resources
            drop(kernels);
        }
    }
}

#[cfg(test)]
mod tests;
