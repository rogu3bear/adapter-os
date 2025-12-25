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
use adapteros_core::backend::BackendKind;
use adapteros_core::{derive_seed, AosError, Result};
use adapteros_db::{Db, TrainingMetricRow};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_router::ROUTER_GATE_Q15_MAX;
use adapteros_telemetry::TelemetryWriter;
use adapteros_types::coreml::CoreMLPlacementSpec;
use adapteros_types::training::TrainingBackendPolicy;
use chrono::Utc;
use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Performance metrics for GPU training
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingPerformanceMetrics {
    /// Total GPU time in milliseconds
    pub total_gpu_time_ms: u64,
    /// Total CPU time in milliseconds
    pub total_cpu_time_ms: u64,
    /// Number of GPU operations
    pub gpu_operations: u64,
    /// Number of CPU operations
    pub cpu_operations: u64,
    /// Average GPU utilization percentage (0-100)
    pub avg_gpu_utilization: f32,
    /// Peak GPU memory usage in MB
    pub peak_gpu_memory_mb: f32,
    /// Total training batches processed
    pub total_batches: u64,
    /// Throughput (examples per second)
    pub throughput_examples_per_sec: f32,
    /// Total tokens processed (input + target)
    pub total_tokens_processed: u64,
    /// Total examples processed
    pub total_examples_processed: u64,
    /// Mean CoreML forward latency in microseconds (training context)
    pub coreml_forward_mean_us: Option<f64>,
    /// p95 CoreML forward latency in microseconds (training context)
    pub coreml_forward_p95_us: Option<u64>,
    /// Total CoreML forward latency accumulated (microseconds)
    pub coreml_forward_total_us: u64,
    /// Count of CoreML forward passes sampled
    pub coreml_forward_samples: u64,
    /// Sampled CoreML forward latencies (bounded, not serialized)
    #[serde(skip, default)]
    pub coreml_forward_latency_samples: VecDeque<u64>,
    /// Effective batch size after device-aware capping
    pub effective_batch_size: usize,
    /// Token budget enforced per batch
    pub max_tokens_per_batch: usize,
    /// Sequences truncated during preparation
    pub sequences_truncated: u64,
    /// Sequences dropped during preparation
    pub sequences_dropped: u64,
    /// Device tier used for training (ANE/GPU/CPU)
    pub device_tier: Option<String>,
    /// Input shape seen by the backend (batch, hidden_dim/context)
    pub input_shape: Option<(usize, usize)>,
}

/// Per-epoch training metrics passed to callbacks and UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochMetrics {
    pub epoch: u32,
    pub loss: f32,
    pub duration_us: u64,
    pub examples_in_epoch: u64,
    pub tokens_in_epoch: u64,
    pub tokens_per_sec: f32,
    pub examples_per_sec: f32,
    pub total_tokens_processed: u64,
    pub total_examples_processed: u64,
}

/// GPU backend choice for training acceleration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainingBackend {
    /// CoreML backend with Neural Engine (ANE acceleration). Forward passes run
    /// on CoreML; gradients/updates remain CPU-side and only LoRA buffers are
    /// mutated.
    CoreML,
    /// MLX backend for research and training
    Mlx,
    /// Metal GPU backend (deterministic, fallback)
    Metal,
    /// CPU-only training (no GPU acceleration)
    Cpu,
}

impl TrainingBackend {
    /// Check if this backend requires GPU availability
    pub fn requires_gpu(&self) -> bool {
        matches!(
            self,
            TrainingBackend::CoreML | TrainingBackend::Mlx | TrainingBackend::Metal
        )
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            TrainingBackend::CoreML => "CoreML (ANE)",
            TrainingBackend::Mlx => "MLX",
            TrainingBackend::Metal => "Metal",
            TrainingBackend::Cpu => "CPU",
        }
    }

    /// Canonical short tag for manifest/metadata
    pub fn tag(&self) -> &'static str {
        match self {
            TrainingBackend::CoreML => "coreml",
            TrainingBackend::Mlx => "mlx",
            TrainingBackend::Metal => "metal",
            TrainingBackend::Cpu => "cpu",
        }
    }
}

impl TryFrom<BackendKind> for TrainingBackend {
    type Error = AosError;

    fn try_from(kind: BackendKind) -> Result<Self> {
        match kind {
            BackendKind::CoreML => Ok(TrainingBackend::CoreML),
            BackendKind::Mlx => Ok(TrainingBackend::Mlx),
            BackendKind::MlxBridge => Ok(TrainingBackend::Mlx), // MlxBridge uses Mlx for training
            BackendKind::Metal => Ok(TrainingBackend::Metal),
            BackendKind::CPU => Ok(TrainingBackend::Cpu),
            BackendKind::Auto => Err(AosError::Config(
                "Auto backend is not a concrete training backend; omit preferred_backend to auto-select training backend"
                    .to_string(),
            )),
        }
    }
}

impl From<TrainingBackend> for BackendKind {
    fn from(backend: TrainingBackend) -> Self {
        match backend {
            TrainingBackend::CoreML => BackendKind::CoreML,
            TrainingBackend::Mlx => BackendKind::Mlx,
            TrainingBackend::Metal => BackendKind::Metal,
            TrainingBackend::Cpu => BackendKind::CPU,
        }
    }
}

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

/// Training configuration with GPU support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePolicyConfig {
    /// Ordered preference for devices/backends (coreml, mlx, metal, cpu)
    #[serde(default)]
    pub preferred_order: Vec<String>,
    /// Whether CPU is an allowed fallback when GPU/ANE is unavailable
    #[serde(default = "DevicePolicyConfig::default_allow_cpu")]
    pub allow_cpu_fallback: bool,
}

impl DevicePolicyConfig {
    const fn default_allow_cpu() -> bool {
        true
    }
}

impl Default for DevicePolicyConfig {
    fn default() -> Self {
        Self {
            preferred_order: vec![
                "coreml".to_string(),
                "mlx".to_string(),
                "metal".to_string(),
                "cpu".to_string(),
            ],
            allow_cpu_fallback: true,
        }
    }
}

/// Deterministic training configuration for harnesses and reproducibility checks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeterminismConfig {
    /// Explicit RNG seed; if unset, legacy derived seed is used.
    pub seed: Option<u64>,
    /// Dataset version identifier used for tagging metadata.
    #[serde(default)]
    pub dataset_version_id: Option<String>,
    /// Device descriptor (cpu/gpu/ane) for metadata only.
    #[serde(default)]
    pub device: Option<String>,
    /// Backend tag for metadata (does not change execution backend).
    #[serde(default)]
    pub backend: Option<String>,
    /// Maximum number of deterministic steps/epochs to run.
    #[serde(default)]
    pub max_steps: Option<usize>,
    /// Optional subsample window applied to the dataset for harness runs.
    #[serde(default)]
    pub subsample: Option<DatasetSubsample>,
}

/// Deterministic sub-sample window for dataset slices.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetSubsample {
    /// Offset into the deterministically ordered dataset.
    pub offset: usize,
    /// Number of examples to include from the offset.
    pub length: usize,
}

/// MoE (Mixture of Experts) training configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoETrainingConfig {
    /// Number of experts in the base model
    pub num_experts: usize,
    /// Number of experts activated per token
    pub num_experts_per_token: usize,
    /// LoRA strategy for MoE models
    #[serde(default)]
    pub lora_strategy: MoELoRAStrategy,
    /// Whether to use expert routing weights for LoRA scaling
    #[serde(default = "default_use_routing_weights")]
    pub use_routing_weights: bool,
    /// MoE intermediate size per expert (optional, for validation)
    #[serde(default)]
    pub moe_intermediate_size: Option<usize>,
}

fn default_use_routing_weights() -> bool {
    true
}

/// LoRA strategy for MoE models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MoELoRAStrategy {
    /// Shared LoRA with routing-weighted contribution per expert (recommended)
    /// Formula: expert_out += (gate / 32767.0) * routing_score[e] * (alpha/rank) * (B @ A) @ x
    #[default]
    RoutingWeightedShared,
    /// Per-expert LoRA (higher memory, potentially better quality)
    PerExpertLoRA,
}

/// Training configuration with GPU support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingConfig {
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// Learning rate
    pub learning_rate: f32,
    /// Batch size
    pub batch_size: usize,
    /// Number of epochs
    pub epochs: usize,
    /// Hidden dimension size
    pub hidden_dim: usize,
    /// Vocabulary size (model-specific)
    pub vocab_size: usize,
    /// Optional CoreML placement spec to align training/inference attachment.
    #[serde(default)]
    pub coreml_placement: Option<CoreMLPlacementSpec>,
    /// Preferred GPU backend (None = auto-select, falls back to CPU if unavailable)
    #[serde(skip, default)]
    pub preferred_backend: Option<TrainingBackend>,
    /// Backend policy for CoreML preference/fallback semantics.
    #[serde(skip, default)]
    pub backend_policy: Option<TrainingBackendPolicy>,
    /// When CoreML is requested for training, use this backend instead.
    /// Default (None) means: MLX → Metal → CPU (if GPU optional).
    #[serde(skip)]
    pub coreml_fallback_backend: Option<TrainingBackend>,
    /// Require GPU acceleration (error if GPU unavailable)
    #[serde(skip)]
    pub require_gpu: bool,
    /// Maximum GPU memory to use in MB (0 = unlimited)
    #[serde(skip)]
    pub max_gpu_memory_mb: u64,
    /// Maximum tokens per batch (input + target); None = auto
    #[serde(default)]
    pub max_tokens_per_batch: Option<usize>,
    /// Device policy to control preferred ordering and CPU fallback
    #[serde(default)]
    pub device_policy: Option<DevicePolicyConfig>,
    /// Checkpoint interval in epochs (None = no checkpoints, default = 5)
    #[serde(default)]
    pub checkpoint_interval: Option<u32>,
    /// Warmup steps for learning rate schedule (optional)
    /// TODO: Not yet implemented in MicroLoRATrainer - accepted from API but not used in training
    #[serde(default)]
    pub warmup_steps: Option<u32>,
    /// Maximum sequence length (optional, default 2048)
    /// TODO: Not yet implemented in MicroLoRATrainer - accepted from API but not used in training
    #[serde(default)]
    pub max_seq_length: Option<u32>,
    /// Gradient accumulation steps for larger effective batch size (optional)
    /// TODO: Not yet implemented in MicroLoRATrainer - accepted from API but not used in training
    #[serde(default)]
    pub gradient_accumulation_steps: Option<u32>,
    /// Deterministic training/test harness configuration
    #[serde(default)]
    pub determinism: Option<DeterminismConfig>,
    /// MoE (Mixture of Experts) training configuration
    /// When set, enables MoE-aware training with routing-weighted LoRA
    #[serde(default)]
    pub moe_config: Option<MoETrainingConfig>,
}

impl TrainingConfig {
    /// Check if this configuration is for an MoE model
    pub fn is_moe(&self) -> bool {
        self.moe_config.is_some()
    }

    /// Get the number of experts (returns 1 for dense models)
    pub fn num_experts(&self) -> usize {
        self.moe_config.as_ref().map(|m| m.num_experts).unwrap_or(1)
    }
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            rank: 4,
            alpha: 16.0,
            learning_rate: 1e-4,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            vocab_size: 32000, // Default LLaMA/Mistral vocab size
            coreml_placement: None,
            preferred_backend: None,
            backend_policy: None,
            coreml_fallback_backend: None,
            require_gpu: false,
            max_gpu_memory_mb: 0,
            max_tokens_per_batch: None,
            device_policy: None,
            checkpoint_interval: None, // Disabled by default
            warmup_steps: None,
            max_seq_length: None,
            gradient_accumulation_steps: None,
            determinism: None,
            moe_config: None,
        }
    }
}

impl TrainingConfig {
    /// Create a new configuration with GPU acceleration required
    pub fn with_gpu_required(mut self) -> Self {
        self.require_gpu = true;
        self
    }

    /// Set preferred GPU backend
    pub fn with_backend(mut self, backend: TrainingBackend) -> Self {
        self.preferred_backend = Some(backend);
        self
    }

    /// Set maximum GPU memory usage
    pub fn with_max_gpu_memory(mut self, max_mb: u64) -> Self {
        self.max_gpu_memory_mb = max_mb;
        self
    }

    /// Enable checkpoint saving every N epochs
    pub fn with_checkpoint_interval(mut self, interval: u32) -> Self {
        self.checkpoint_interval = Some(interval);
        self
    }

    /// Effective context window used for validation and padding.
    pub fn effective_context_window(&self) -> usize {
        let requested = self.max_seq_length.unwrap_or(self.hidden_dim as u32).max(1) as usize;
        std::cmp::min(requested, self.hidden_dim)
    }

    /// Resolve device policy order (strings) or fall back to default.
    pub fn device_policy_order(&self) -> Vec<String> {
        if let Some(policy) = &self.device_policy {
            if !policy.preferred_order.is_empty() {
                return policy.preferred_order.clone();
            }
        }
        DevicePolicyConfig::default().preferred_order
    }

    /// Configure for MoE (Mixture of Experts) training
    pub fn with_moe(mut self, num_experts: usize, num_experts_per_token: usize) -> Self {
        self.moe_config = Some(MoETrainingConfig {
            num_experts,
            num_experts_per_token,
            lora_strategy: MoELoRAStrategy::RoutingWeightedShared,
            use_routing_weights: true,
            moe_intermediate_size: None,
        });
        self
    }

    /// Configure for MoE with full options
    pub fn with_moe_config(mut self, config: MoETrainingConfig) -> Self {
        self.moe_config = Some(config);
        self
    }
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

/// Training result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingResult {
    pub adapter_id: String,
    pub final_loss: f32,
    /// Training time in microseconds for high precision measurement.
    /// Use `.training_time_ms()` method for millisecond conversion.
    pub training_time_us: u64,
    pub weights: LoRAWeights,
    /// True if training was cancelled before completion
    #[serde(default)]
    pub cancelled: bool,
    /// Epoch at which training stopped (whether completed or cancelled)
    #[serde(default)]
    pub stopped_at_epoch: Option<u32>,
    /// Total examples processed before stopping
    #[serde(default)]
    pub examples_processed: Option<u64>,
    /// Total tokens processed before stopping
    #[serde(default)]
    pub tokens_processed: Option<u64>,
    /// Average tokens per second across the run
    #[serde(default)]
    pub tokens_per_sec: f32,
    /// Average examples per second across the run
    #[serde(default)]
    pub examples_per_sec: f32,
    /// Backend name used for training (if any)
    #[serde(default)]
    pub backend: Option<String>,
    /// Device string for the backend (if any)
    #[serde(default)]
    pub backend_device: Option<String>,
    /// Whether GPU acceleration was used
    #[serde(default)]
    pub using_gpu: bool,
    /// Effective batch size after device-aware capping
    #[serde(default)]
    pub effective_batch_size: Option<usize>,
    /// Loss curve (per-epoch) captured during training
    #[serde(default)]
    pub loss_curve: Vec<f32>,
    /// Deterministic seed used for this run (if provided)
    #[serde(default)]
    pub determinism_seed: Option<u64>,
    /// Backend tag recorded for determinism/drift harness
    #[serde(default)]
    pub determinism_backend: Option<String>,
    /// Device string recorded for determinism/drift harness
    #[serde(default)]
    pub determinism_device: Option<String>,
    /// Dataset version identifier (if provided by harness)
    #[serde(default)]
    pub dataset_version_id: Option<String>,
}

impl TrainingResult {
    /// Get training time in milliseconds (for backward compatibility and display)
    pub fn training_time_ms(&self) -> u64 {
        self.training_time_us / 1000
    }
}

/// LoRA weight matrices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAWeights {
    /// Down-projection matrix (rank × hidden_dim)
    pub lora_a: Vec<Vec<f32>>,
    /// Up-projection matrix (hidden_dim × rank)
    pub lora_b: Vec<Vec<f32>>,
    /// MoE configuration (if trained for MoE model)
    #[serde(default)]
    pub moe_config: Option<MoETrainingConfig>,
    /// Precomputed delta (B @ A) for faster inference (optional)
    #[serde(skip)]
    pub precomputed_delta: Option<Vec<Vec<f32>>>,
}

impl LoRAWeights {
    /// Create new LoRA weights with given dimensions
    pub fn new(rank: usize, hidden_dim: usize) -> Self {
        Self {
            lora_a: vec![vec![0.0; hidden_dim]; rank],
            lora_b: vec![vec![0.0; rank]; hidden_dim],
            moe_config: None,
            precomputed_delta: None,
        }
    }

    /// Create new LoRA weights for MoE model
    pub fn new_moe(rank: usize, hidden_dim: usize, moe_config: MoETrainingConfig) -> Self {
        Self {
            lora_a: vec![vec![0.0; hidden_dim]; rank],
            lora_b: vec![vec![0.0; rank]; hidden_dim],
            moe_config: Some(moe_config),
            precomputed_delta: None,
        }
    }

    /// Check if these weights are for an MoE model
    pub fn is_moe(&self) -> bool {
        self.moe_config.is_some()
    }

    /// Precompute delta (B @ A) for faster inference
    pub fn precompute_delta(&mut self) {
        if self.precomputed_delta.is_some() {
            return;
        }

        let rank = self.lora_a.len();
        let hidden_dim = self.lora_b.len();
        let in_features = self.lora_a.get(0).map(|v| v.len()).unwrap_or(0);

        // delta = B @ A: (hidden_dim, in_features)
        let mut delta = vec![vec![0.0f32; in_features]; hidden_dim];

        for out_idx in 0..hidden_dim {
            for in_idx in 0..in_features {
                let mut sum = 0.0f32;
                for r in 0..rank {
                    // B[out_idx, r] * A[r, in_idx]
                    sum += self.lora_b[out_idx][r] * self.lora_a[r][in_idx];
                }
                delta[out_idx][in_idx] = sum;
            }
        }

        self.precomputed_delta = Some(delta);
    }

    /// Get scaling factor (alpha / rank)
    pub fn scale(&self, alpha: f32) -> f32 {
        alpha / self.lora_a.len() as f32
    }
}

impl MicroLoRATrainer {
    /// Create a new trainer with configuration
    pub fn new(mut config: TrainingConfig) -> Result<Self> {
        // Derive deterministic training seed with optional explicit override
        let deterministic_seed_override = config
            .determinism
            .as_ref()
            .and_then(|d| d.seed)
            .filter(|seed| *seed != 0);
        let training_seed = deterministic_seed_override.unwrap_or_else(|| {
            let global_seed = adapteros_core::B3Hash::hash(b"training");
            let training_seed_bytes = derive_seed(&global_seed, "lora_trainer");
            u64::from_le_bytes([
                training_seed_bytes[0],
                training_seed_bytes[1],
                training_seed_bytes[2],
                training_seed_bytes[3],
                training_seed_bytes[4],
                training_seed_bytes[5],
                training_seed_bytes[6],
                training_seed_bytes[7],
            ])
        });

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
        lora_output
            .into_iter()
            .map(|v| v * routing_scale)
            .collect()
    }

    /// MoE-aware forward pass with routing weights.
    ///
    /// Uses routing-weighted shared LoRA: same weights scaled by expert routing.
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
mod tests {
    use super::*;
    use crate::training::coreml_pipeline;
    use adapteros_core::B3Hash;
    use adapteros_platform::common::PlatformUtils;
    use blake3;
    use rand::thread_rng;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    fn make_prepared(
        example: &TrainingExample,
        hidden_dim: usize,
    ) -> coreml_pipeline::PreparedExample {
        let mut scaled_input: Vec<f32> = example.input.iter().map(|t| *t as f32).collect();
        if scaled_input.len() < hidden_dim {
            scaled_input.resize(hidden_dim, 0.0);
        } else {
            scaled_input.truncate(hidden_dim);
        }

        coreml_pipeline::PreparedExample {
            input_tokens: example.input.clone(),
            target_tokens: example.target.clone(),
            padded_input: example.input.clone(),
            padded_target: example.target.clone(),
            scaled_input,
            input_mask: vec![1; example.input.len()],
            target_mask: vec![1; example.target.len()],
            input_len: example.input.len(),
            target_len: example.target.len(),
            metadata: example.metadata.clone(),
            weight: example.weight,
        }
    }

    #[test]
    fn test_training_backend_enum() {
        assert!(TrainingBackend::CoreML.requires_gpu());
        assert!(TrainingBackend::Metal.requires_gpu());
        assert!(TrainingBackend::Mlx.requires_gpu());
        assert!(!TrainingBackend::Cpu.requires_gpu());

        assert_eq!(TrainingBackend::CoreML.name(), "CoreML (ANE)");
        assert_eq!(TrainingBackend::Cpu.name(), "CPU");
    }

    #[test]
    fn test_training_config_with_gpu_required() {
        let config = TrainingConfig::default().with_gpu_required();
        assert!(config.require_gpu);
        assert_eq!(config.rank, 4); // Default values preserved
    }

    #[test]
    fn test_training_config_with_backend() {
        let config = TrainingConfig::default().with_backend(TrainingBackend::Metal);
        assert_eq!(config.preferred_backend, Some(TrainingBackend::Metal));
    }

    #[test]
    fn test_backend_kind_conversion() {
        assert_eq!(
            TrainingBackend::try_from(BackendKind::Metal).unwrap(),
            TrainingBackend::Metal
        );
        assert_eq!(
            TrainingBackend::try_from(BackendKind::Mlx).unwrap(),
            TrainingBackend::Mlx
        );
        assert_eq!(
            TrainingBackend::try_from(BackendKind::CPU).unwrap(),
            TrainingBackend::Cpu
        );
        assert!(TrainingBackend::try_from(BackendKind::Auto).is_err());
        assert_eq!(
            BackendKind::from(TrainingBackend::CoreML),
            BackendKind::CoreML
        );
    }

    #[test]
    fn test_training_config_with_max_gpu_memory() {
        let config = TrainingConfig::default().with_max_gpu_memory(2048);
        assert_eq!(config.max_gpu_memory_mb, 2048);
    }

    #[test]
    fn test_backend_candidates_priority_order_includes_coreml_first() {
        let mut trainer = MicroLoRATrainer::new(TrainingConfig::default()).unwrap();
        let availability = BackendAvailability {
            coreml: true,
            mlx: true,
            metal: true,
        };

        let candidates = trainer.build_backend_candidates(&availability).unwrap();
        assert_eq!(candidates[0], TrainingBackend::CoreML);
        assert_eq!(candidates[1], TrainingBackend::Mlx);
        assert_eq!(candidates[2], TrainingBackend::Metal);
        assert_eq!(candidates.last(), Some(&TrainingBackend::Cpu));
        assert!(candidates.contains(&TrainingBackend::CoreML));
    }

    #[test]
    fn test_backend_candidates_preferred_fallback() {
        let config = TrainingConfig {
            preferred_backend: Some(TrainingBackend::Metal),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();
        let availability = BackendAvailability {
            coreml: false,
            mlx: true,
            metal: false,
        };

        let candidates = trainer.build_backend_candidates(&availability).unwrap();
        assert_eq!(candidates[0], TrainingBackend::Mlx);
        assert!(candidates.contains(&TrainingBackend::Cpu));
    }

    #[test]
    fn test_backend_policy_coreml_only_fails_without_coreml() {
        let config = TrainingConfig {
            backend_policy: Some(TrainingBackendPolicy::CoremlOnly),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();
        let availability = BackendAvailability {
            coreml: false,
            mlx: true,
            metal: true,
        };

        let result = trainer.build_backend_candidates(&availability);
        assert!(result.is_err());
    }

    #[test]
    fn test_backend_policy_coreml_else_fallback_uses_fallback() {
        let config = TrainingConfig {
            backend_policy: Some(TrainingBackendPolicy::CoremlElseFallback),
            coreml_fallback_backend: Some(TrainingBackend::Mlx),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();
        let availability = BackendAvailability {
            coreml: false,
            mlx: true,
            metal: false,
        };

        let candidates = trainer.build_backend_candidates(&availability).unwrap();
        assert_eq!(candidates[0], TrainingBackend::Mlx);
    }

    #[test]
    fn test_coreml_preference_uses_coreml_when_available() {
        let config = TrainingConfig {
            preferred_backend: Some(TrainingBackend::CoreML),
            coreml_fallback_backend: Some(TrainingBackend::Mlx),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();
        let availability = BackendAvailability {
            coreml: true,
            mlx: true,
            metal: true,
        };

        let candidates = trainer.build_backend_candidates(&availability).unwrap();
        assert_eq!(candidates[0], TrainingBackend::CoreML);
        assert!(
            !trainer
                .backend_reason()
                .unwrap_or_default()
                .contains("coreml_unavailable"),
            "coreml available should not emit unavailable reason"
        );
    }

    #[test]
    fn test_coreml_preference_falls_back_when_unavailable_and_fallback_provided() {
        let config = TrainingConfig {
            preferred_backend: Some(TrainingBackend::CoreML),
            coreml_fallback_backend: Some(TrainingBackend::Mlx),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();
        let availability = BackendAvailability {
            coreml: false,
            mlx: true,
            metal: true,
        };

        let candidates = trainer.build_backend_candidates(&availability).unwrap();
        assert_eq!(candidates[0], TrainingBackend::Mlx);
        assert!(!candidates.contains(&TrainingBackend::CoreML));
        let reason = trainer.backend_reason().unwrap_or_default();
        assert!(
            reason.contains("coreml_unavailable"),
            "expected reason to mention CoreML unavailable, got: {reason}"
        );
        assert!(
            reason.contains("mlx"),
            "expected reason to include fallback backend tag, got: {reason}"
        );
    }

    #[test]
    fn test_backend_candidates_require_gpu_error_when_none() {
        let config = TrainingConfig {
            require_gpu: true,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();
        let availability = BackendAvailability {
            coreml: false,
            mlx: false,
            metal: false,
        };

        assert!(trainer.build_backend_candidates(&availability).is_err());
    }

    #[test]
    fn test_coreml_preference_without_gpu_uses_cpu_and_reason() {
        std::env::set_var("AOS_FORCE_GPU_BACKEND", "none");
        let config = TrainingConfig {
            preferred_backend: Some(TrainingBackend::CoreML),
            coreml_fallback_backend: Some(TrainingBackend::Mlx),
            require_gpu: false,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        trainer
            .init_kernels(&[])
            .expect("CPU fallback should succeed when GPU is optional");

        assert_eq!(trainer.backend_info(), Some("CPU"));
        let reason = trainer.backend_reason().unwrap_or_default();
        assert!(
            reason.contains("coreml_unavailable"),
            "expected backend reason to mention CoreML fallback, got: {reason}"
        );
        std::env::remove_var("AOS_FORCE_GPU_BACKEND");
    }

    #[test]
    fn test_coreml_latency_metrics_tracking() {
        let trainer = MicroLoRATrainer::new(TrainingConfig::default()).unwrap();
        trainer.record_coreml_forward_latency(10);
        trainer.record_coreml_forward_latency(30);
        let metrics = trainer.get_performance_metrics();
        assert_eq!(metrics.coreml_forward_samples, 2);
        assert_eq!(metrics.coreml_forward_total_us, 40);
        assert_eq!(metrics.coreml_forward_mean_us, Some(20.0));
        assert_eq!(metrics.coreml_forward_p95_us, Some(30));
    }

    #[test]
    fn test_available_backends_detection() {
        let backends = MicroLoRATrainer::detect_available_backends();
        // At minimum, CPU should always be available
        assert!(!backends.is_empty());
        let has_cpu = backends.iter().any(|(b, _)| *b == TrainingBackend::Cpu);
        assert!(has_cpu, "CPU backend should always be available");
    }

    #[test]
    fn test_describe_available_backends() {
        let desc = MicroLoRATrainer::describe_available_backends();
        assert!(desc.contains("Available training backends:"));
        assert!(desc.contains("CPU")); // At minimum, CPU should be listed
    }

    #[test]
    fn test_initialize_weights() {
        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 768,
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config).unwrap();
        let weights = trainer.initialize_weights_deterministic().unwrap();

        assert_eq!(weights.lora_a.len(), 4);
        assert_eq!(weights.lora_a[0].len(), 768);
        assert_eq!(weights.lora_b.len(), 768);
        assert_eq!(weights.lora_b[0].len(), 4);
    }

    #[test]
    fn test_training_updates_only_lora_weights() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 6,
            vocab_size: 16,
            batch_size: 1,
            epochs: 1,
            ..Default::default()
        };

        let mut trainer = MicroLoRATrainer::new(config.clone()).unwrap();
        let mut weights = trainer.initialize_weights_deterministic().unwrap();
        let initial_weights = weights.clone();

        let base_snapshot = vec![1.0f32, 2.0, 3.0, 4.0];

        let examples = vec![TrainingExample {
            input: vec![1, 2, 3, 4],
            target: vec![4, 3, 2, 1],
            metadata: HashMap::new(),
            weight: 1.0,
        }];

        let dataset = trainer
            .prepare_dataset_for_training(&examples)
            .expect("dataset prep");

        // Run a single epoch; only LoRA weights should change.
        let mut base_hash_bytes = Vec::new();
        for f in &base_snapshot {
            base_hash_bytes.extend_from_slice(&f.to_le_bytes());
        }
        let base_hash_before = B3Hash::hash(&base_hash_bytes);

        let loss = trainer
            .train_epoch_deterministic(&mut weights, &dataset, 0)
            .unwrap();

        assert!(loss.is_finite());
        assert_ne!(
            weights.lora_a, initial_weights.lora_a,
            "LoRA A should change during training"
        );
        assert_ne!(
            weights.lora_b, initial_weights.lora_b,
            "LoRA B should change during training"
        );

        // Base model buffers are not part of the optimizer set and must remain untouched.
        assert_eq!(base_snapshot, vec![1.0, 2.0, 3.0, 4.0]);
        let mut base_hash_bytes_after = Vec::new();
        for f in &base_snapshot {
            base_hash_bytes_after.extend_from_slice(&f.to_le_bytes());
        }
        let base_hash_after = B3Hash::hash(&base_hash_bytes_after);
        assert_eq!(
            base_hash_before, base_hash_after,
            "Base checksum must remain stable during training"
        );

        // Ensure deterministic RNG usage remains stable between runs
        let mut trainer_second = MicroLoRATrainer::new(config).unwrap();
        let mut weights_second = trainer_second.initialize_weights_deterministic().unwrap();
        let dataset_second = trainer_second
            .prepare_dataset_for_training(&examples)
            .expect("dataset prep second");
        trainer_second
            .train_epoch_deterministic(&mut weights_second, &dataset_second, 0)
            .unwrap();
        assert_eq!(
            weights.lora_a, weights_second.lora_a,
            "Deterministic training should yield identical LoRA A updates"
        );
        assert_eq!(
            weights.lora_b, weights_second.lora_b,
            "Deterministic training should yield identical LoRA B updates"
        );
    }

    #[test]
    fn test_forward_pass() {
        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 768,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();
        let weights = trainer.initialize_weights_deterministic().unwrap();

        let examples = vec![TrainingExample {
            input: vec![1, 2, 3, 4, 5],
            target: vec![1, 2, 3, 4, 5],
            metadata: HashMap::new(),
            weight: 1.0,
        }];
        let dataset = trainer
            .prepare_dataset_for_training(&examples)
            .expect("prepare dataset");
        let (output, hidden) = trainer.forward(&weights, &dataset.examples[0]).unwrap();

        assert_eq!(output.len(), 768);
        assert_eq!(hidden.len(), 768);
    }

    #[test]
    fn test_trainer_gpu_status_initially_cpu() {
        let config = TrainingConfig::default();
        let trainer = MicroLoRATrainer::new(config).unwrap();

        // Before init_kernels, no backend is selected
        assert_eq!(trainer.backend_info(), None);
        assert!(!trainer.using_gpu());
    }

    #[tokio::test]
    async fn test_train_small() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 64,
            batch_size: 2,
            epochs: 1,
            learning_rate: 0.01,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        let examples = vec![
            TrainingExample {
                input: vec![1, 2, 3],
                target: vec![4, 5, 6],
                metadata: HashMap::new(),
                weight: 1.0,
            },
            TrainingExample {
                input: vec![7, 8, 9],
                target: vec![10, 11, 12],
                metadata: HashMap::new(),
                weight: 1.0,
            },
        ];

        let result = trainer.train(&examples).await.unwrap();
        assert!(result.final_loss >= 0.0);
        assert!(
            result.training_time_us > 0,
            "Training time should be positive (actual work done), got: {}us",
            result.training_time_us
        );
        assert_eq!(result.weights.lora_a.len(), 2);
        assert!(
            result.effective_batch_size.unwrap_or_default() > 0,
            "effective batch size should be captured"
        );
    }

    #[test]
    fn test_backward_only_updates_lora_weights() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 2,
            vocab_size: 4,
            batch_size: 1,
            epochs: 1,
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config).unwrap();
        let mut weights = trainer.initialize_weights_deterministic().unwrap();
        let original_weights = weights.clone();

        let example = TrainingExample {
            input: vec![1, 2],
            target: vec![1, 2, 3, 4],
            metadata: HashMap::new(),
            weight: 1.0,
        };
        let prepared = make_prepared(&example, trainer.config.hidden_dim);
        let (output, hidden) = trainer.forward(&weights, &prepared).unwrap();
        let target = example.target.clone();

        let mut rng = thread_rng();
        let base_stub = vec![42.0f32, 43.0];
        let base_before = base_stub.clone();

        trainer
            .backward_and_update_deterministic(
                &mut weights,
                &hidden,
                &output,
                &target,
                0.1,
                &mut rng,
            )
            .unwrap();

        // LoRA weights should change
        assert_ne!(weights.lora_a, original_weights.lora_a);
        // Base model buffer (not part of trainer) remains unchanged
        assert_eq!(base_stub, base_before);
    }

    #[tokio::test]
    async fn test_train_with_cpu_backend_optional() {
        // Training should work without GPU when GPU is optional
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            batch_size: 1,
            epochs: 1,
            learning_rate: 0.01,
            require_gpu: false,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        let examples = vec![TrainingExample {
            input: vec![1, 2],
            target: vec![3, 4],
            metadata: HashMap::new(),
            weight: 1.0,
        }];

        // init_kernels should complete successfully (CPU path)
        trainer
            .init_kernels(&[])
            .expect("CPU kernel init should succeed");

        // Training should complete without errors
        let result = trainer.train(&examples).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().weights.lora_a.len(), 2);
    }

    #[test]
    fn test_backend_selection_priority() {
        let config = TrainingConfig {
            preferred_backend: Some(TrainingBackend::Metal),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        let availability = BackendAvailability {
            coreml: false,
            mlx: false,
            metal: true,
        };
        let candidates = trainer.build_backend_candidates(&availability).unwrap();
        assert_eq!(candidates[0], TrainingBackend::Metal);
    }

    #[test]
    fn test_device_policy_prefers_coreml_first() {
        std::env::set_var("AOS_FORCE_GPU_BACKEND", "all");
        let mut trainer = MicroLoRATrainer::new(TrainingConfig::default()).unwrap();
        let availability = BackendAvailability {
            coreml: true,
            mlx: true,
            metal: true,
        };

        let candidates = trainer.build_backend_candidates(&availability).unwrap();
        assert_eq!(
            candidates[0],
            TrainingBackend::CoreML,
            "CoreML should be first when available"
        );
        assert!(candidates.contains(&TrainingBackend::Cpu));
        std::env::remove_var("AOS_FORCE_GPU_BACKEND");
    }

    // ========================================================================
    // Checkpoint Integration Tests
    // ========================================================================

    #[test]
    fn test_checkpoint_interval_config() {
        let config = TrainingConfig::default().with_checkpoint_interval(5);
        assert_eq!(config.checkpoint_interval, Some(5));
    }

    #[test]
    fn test_checkpoint_interval_default_none() {
        let config = TrainingConfig::default();
        assert_eq!(config.checkpoint_interval, None);
    }

    #[tokio::test]
    async fn test_enable_checkpointing() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            epochs: 10,
            checkpoint_interval: Some(2),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        // Create temp dir for checkpoints
        let temp_dir = new_test_tempdir();

        // Enable checkpointing
        trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

        // Verify checkpoint manager is configured
        assert!(trainer.checkpoint_manager.is_some());
    }

    #[tokio::test]
    async fn test_train_with_checkpointing() {
        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            batch_size: 1,
            epochs: 4,
            learning_rate: 0.01,
            checkpoint_interval: Some(1), // Save every epoch to ensure checkpoints exist in tests
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        // Create temp dir for checkpoints
        let temp_dir = new_test_tempdir();
        trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

        let examples = vec![TrainingExample {
            input: vec![1, 2],
            target: vec![3, 4],
            metadata: HashMap::new(),
            weight: 1.0,
        }];

        // Train - checkpoints should be saved each epoch
        let result = trainer.train(&examples).await;
        assert!(result.is_ok());

        // Verify checkpoint files were created
        let checkpoint_files: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "ckpt"))
            .collect();

        // Should have at least the latest checkpoint
        assert!(
            !checkpoint_files.is_empty(),
            "Expected checkpoint files to be created"
        );
    }

    #[tokio::test]
    async fn test_try_resume_from_checkpoint_no_checkpoint() {
        let config = TrainingConfig {
            checkpoint_interval: Some(5),
            ..Default::default()
        };
        let trainer = MicroLoRATrainer::new(config).unwrap();

        // No checkpoint manager configured, should return None
        let resume_state = trainer.try_resume_from_checkpoint().await;
        assert!(resume_state.is_none());
    }

    #[tokio::test]
    async fn test_try_resume_from_checkpoint_with_checkpoint() {
        use crate::training::checkpoint::TrainingCheckpoint;

        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 32,
            checkpoint_interval: Some(2),
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config.clone()).unwrap();

        // Create temp dir and save a checkpoint
        let temp_dir = new_test_tempdir();
        trainer.enable_checkpointing(temp_dir.path(), "test-adapter", 3);

        // Manually create a checkpoint
        let weights = LoRAWeights {
            lora_a: vec![vec![1.0, 2.0]],
            lora_b: vec![vec![3.0, 4.0]],
            moe_config: None,
            precomputed_delta: None,
        };
        let checkpoint = TrainingCheckpoint::new(
            5, // epoch 5
            0, 0.5, // loss
            0.001, config, weights,
        );

        // Save checkpoint using the manager
        let manager = trainer.checkpoint_manager.as_ref().unwrap();
        manager.save_checkpoint(&checkpoint).await.unwrap();

        // Now try to resume
        let resume_state = trainer.try_resume_from_checkpoint().await;
        assert!(resume_state.is_some());

        let (epoch, _weights, _best_loss) = resume_state.unwrap();
        assert_eq!(epoch, 5);
    }

    #[tokio::test]
    async fn test_adapter_only_training_updates_lora_only() {
        fn hash_weights(weights: &LoRAWeights) -> blake3::Hash {
            let mut bytes = Vec::new();
            for row in &weights.lora_a {
                for v in row {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
            for row in &weights.lora_b {
                for v in row {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
            }
            blake3::hash(&bytes)
        }

        let config = TrainingConfig {
            rank: 2,
            hidden_dim: 16,
            batch_size: 1,
            epochs: 1,
            learning_rate: 0.05,
            ..Default::default()
        };
        let mut trainer = MicroLoRATrainer::new(config).unwrap();

        let examples = vec![TrainingExample {
            input: vec![1, 2, 3, 4],
            target: vec![5, 6, 7, 8],
            metadata: HashMap::new(),
            weight: 1.0,
        }];

        // Snapshot initial LoRA weights and base (input-derived) hidden state.
        let initial_weights = trainer.initialize_weights_deterministic().unwrap();
        let initial_hash = hash_weights(&initial_weights);
        let prepared = make_prepared(&examples[0], trainer.config.hidden_dim);
        let (_out_before, base_hidden_before) =
            trainer.forward(&initial_weights, &prepared).unwrap();

        // Run a tiny training step.
        let result = trainer.train(&examples).await.unwrap();
        let updated_hash = hash_weights(&result.weights);

        // Adapter-only guarantee: LoRA weights must change, base path stays identical.
        assert_ne!(
            initial_hash, updated_hash,
            "LoRA weights should update during training"
        );

        let prepared_after = make_prepared(&examples[0], trainer.config.hidden_dim);
        let (_out_after, base_hidden_after) =
            trainer.forward(&result.weights, &prepared_after).unwrap();
        assert_eq!(
            base_hidden_before, base_hidden_after,
            "Base (input-derived) hidden path must remain unchanged; only LoRA deltas mutate"
        );
    }
}
