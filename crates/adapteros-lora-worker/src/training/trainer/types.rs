use adapteros_core::backend::BackendKind;
use adapteros_core::{AosError, Result};
use adapteros_types::coreml::CoreMLPlacementSpec;
use adapteros_types::training::TrainingBackendPolicy;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;

/// Optimizer type selection for training.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OptimizerType {
    /// Stochastic Gradient Descent with optional momentum
    Sgd,
    /// Adam optimizer with bias correction (default)
    #[default]
    Adam,
    /// AdamW optimizer (Adam with decoupled weight decay)
    AdamW,
}

/// Configuration for the training optimizer.
///
/// This configuration is used when GPU backward pass is enabled to configure
/// the optimizer used for weight updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    /// Type of optimizer to use
    #[serde(default)]
    pub optimizer_type: OptimizerType,
    /// First moment decay for Adam/AdamW (typically 0.9)
    #[serde(default = "default_beta1")]
    pub beta1: f32,
    /// Second moment decay for Adam/AdamW (typically 0.999)
    #[serde(default = "default_beta2")]
    pub beta2: f32,
    /// Numerical stability constant for Adam/AdamW (typically 1e-8)
    #[serde(default = "default_epsilon")]
    pub epsilon: f32,
    /// Weight decay factor (0.0 to disable)
    #[serde(default)]
    pub weight_decay: f32,
    /// Momentum factor for SGD (0.0 for vanilla SGD)
    #[serde(default)]
    pub momentum: f32,
}

fn default_beta1() -> f32 {
    0.9
}

fn default_beta2() -> f32 {
    0.999
}

fn default_epsilon() -> f32 {
    1e-8
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            optimizer_type: OptimizerType::default(),
            beta1: default_beta1(),
            beta2: default_beta2(),
            epsilon: default_epsilon(),
            weight_decay: 0.0,
            momentum: 0.0,
        }
    }
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_loss: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_perplexity: Option<f32>,
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
    /// Default (None) means: MLX -> Metal -> CPU (if GPU optional).
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
    /// Enable early stopping based on validation loss.
    #[serde(default)]
    pub early_stopping: Option<bool>,
    /// Patience for early stopping (epochs without improvement).
    #[serde(default)]
    pub patience: Option<u32>,
    /// Minimum delta for validation loss improvement.
    #[serde(default)]
    pub min_delta: Option<f32>,
    /// Deterministic training/test harness configuration
    #[serde(default)]
    pub determinism: Option<DeterminismConfig>,
    /// MoE (Mixture of Experts) training configuration
    /// When set, enables MoE-aware training with routing-weighted LoRA
    #[serde(default)]
    pub moe_config: Option<MoETrainingConfig>,
    /// Enable GPU-accelerated backward pass (gradient computation) via MLX.
    /// When true and using MLX backend, gradients and optimizer steps run on GPU.
    /// When false (default), gradients are computed on CPU even with GPU forward pass.
    /// Note: GPU backward may not be bit-exact with CPU backward.
    #[serde(default)]
    pub use_gpu_backward: bool,
    /// Optimizer configuration for GPU training.
    /// Only used when `use_gpu_backward` is true.
    #[serde(default)]
    pub optimizer_config: OptimizerConfig,
    /// Path to base model for real hidden state extraction during training.
    /// REQUIRED: Training without a base model produces incorrect adapters.
    /// The trainer loads the base model and extracts actual hidden states
    /// from the specified layer for proper cross-entropy loss computation.
    #[serde(default)]
    pub base_model_path: Option<PathBuf>,
    /// Hidden state layer pattern to extract from the base model (e.g., "layer_31_output").
    /// If not specified, defaults to the last transformer layer's output.
    #[serde(default)]
    pub hidden_state_layer: Option<String>,
    /// Fraction of dataset to use for validation (0.0-0.5, default 0.2).
    /// Validation loss is used for convergence detection and early stopping.
    #[serde(default = "default_validation_split")]
    pub validation_split: f32,
}

fn default_validation_split() -> f32 {
    0.2
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

    /// Render a concise summary for logging and resume checks.
    pub fn summary(&self) -> String {
        format!(
            "rank={}, alpha={}, lr={}, batch_size={}, epochs={}, hidden_dim={}, validation_split={}, hidden_state_layer={:?}, early_stopping={:?}, patience={:?}, min_delta={:?}, optimizer={:?}, beta1={}, beta2={}, epsilon={}, weight_decay={}, momentum={}, use_gpu_backward={}",
            self.rank,
            self.alpha,
            self.learning_rate,
            self.batch_size,
            self.epochs,
            self.hidden_dim,
            self.validation_split,
            self.hidden_state_layer,
            self.early_stopping,
            self.patience,
            self.min_delta,
            self.optimizer_config.optimizer_type,
            self.optimizer_config.beta1,
            self.optimizer_config.beta2,
            self.optimizer_config.epsilon,
            self.optimizer_config.weight_decay,
            self.optimizer_config.momentum,
            self.use_gpu_backward
        )
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
            early_stopping: Some(false),
            patience: Some(5),
            min_delta: Some(0.001),
            determinism: None,
            moe_config: None,
            use_gpu_backward: true,
            optimizer_config: OptimizerConfig::default(),
            base_model_path: None,
            hidden_state_layer: None,
            validation_split: default_validation_split(),
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

    /// Configure base model path for real hidden state extraction during training.
    /// REQUIRED: The trainer will load the specified model and extract actual hidden
    /// states for proper cross-entropy loss computation. Training without a base model
    /// will fail with an error.
    pub fn with_base_model(mut self, path: impl Into<PathBuf>) -> Self {
        self.base_model_path = Some(path.into());
        self
    }

    /// Configure which hidden state layer to extract from the base model.
    /// If not specified, defaults to the last transformer layer's output.
    pub fn with_hidden_state_layer(mut self, layer: impl Into<String>) -> Self {
        self.hidden_state_layer = Some(layer.into());
        self
    }

    /// Configure validation split ratio (0.0-0.5).
    /// Validation loss is used for convergence detection and early stopping.
    pub fn with_validation_split(mut self, split: f32) -> Self {
        self.validation_split = split.clamp(0.0, 0.5);
        self
    }

    /// Enable GPU-accelerated backward pass via MLX.
    /// When enabled and using MLX backend, gradients and optimizer steps run on GPU.
    /// Note: GPU backward may produce different results than CPU (not bit-exact).
    pub fn with_gpu_backward(mut self, enabled: bool) -> Self {
        self.use_gpu_backward = enabled;
        self
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
    /// Validation loss per epoch (if validation_split > 0)
    #[serde(default)]
    pub validation_loss_curve: Vec<f32>,
    /// Training perplexity per epoch (exp(loss))
    #[serde(default)]
    pub train_perplexity_curve: Vec<f32>,
    /// Validation perplexity per epoch
    #[serde(default)]
    pub validation_perplexity_curve: Vec<f32>,
    /// Best validation loss achieved and the epoch
    #[serde(default)]
    pub best_validation: Option<(f32, u32)>,
    /// Final validation loss (if validation_split > 0)
    #[serde(default)]
    pub final_validation_loss: Option<f32>,
}

impl TrainingResult {
    /// Get training time in milliseconds (for backward compatibility and display)
    pub fn training_time_ms(&self) -> u64 {
        self.training_time_us / 1000
    }

    /// Check if validation metrics are available
    pub fn has_validation_metrics(&self) -> bool {
        !self.validation_loss_curve.is_empty()
    }

    /// Get the best epoch based on validation loss (if available)
    pub fn best_epoch(&self) -> Option<u32> {
        self.best_validation.map(|(_, epoch)| epoch)
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
        let in_features = self.lora_a.first().map(|v| v.len()).unwrap_or(0);

        // delta = B @ A: (hidden_dim, in_features)
        let mut delta = vec![vec![0.0f32; in_features]; hidden_dim];

        #[allow(clippy::needless_range_loop)]
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
