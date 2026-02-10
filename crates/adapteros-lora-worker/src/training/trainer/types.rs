use adapteros_core::backend::BackendKind;
use adapteros_core::{AosError, Result};
use adapteros_types::coreml::CoreMLPlacementSpec;
use adapteros_types::training::{TrainingBackendPolicy, TRAINING_DATA_CONTRACT_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
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
            BackendKind::ModelServer => Ok(TrainingBackend::Mlx), // ModelServer uses Mlx for training
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

/// Optional compression choices for cached preprocessing tensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreprocessCompression {
    /// No compression (store f32 tensors).
    None,
    /// Q15 fixed-point compression (i16 + scale).
    Q15,
}

impl PreprocessCompression {
    pub fn as_str(&self) -> &'static str {
        match self {
            PreprocessCompression::None => "none",
            PreprocessCompression::Q15 => "q15",
        }
    }
}

/// Output feature selection for preprocessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PreprocessOutputFeature {
    /// Emit per-token embedding features.
    Embedding,
    /// Emit the last hidden state token.
    HiddenStateLast,
    /// Emit a pooled (mean) hidden state.
    #[default]
    Pooled,
}

impl PreprocessOutputFeature {
    pub fn as_str(&self) -> &'static str {
        match self {
            PreprocessOutputFeature::Embedding => "embedding",
            PreprocessOutputFeature::HiddenStateLast => "hidden_state_last",
            PreprocessOutputFeature::Pooled => "pooled",
        }
    }
}

/// Optional preprocessing stage for tokenized inputs.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PreprocessingConfig {
    /// Explicitly enable preprocessing when set to true.
    #[serde(default)]
    pub enabled: bool,
    /// Optional CoreML model identifier (resolved via model cache).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_model_id: Option<String>,
    /// Optional CoreML model path for preprocessing (mlpackage or mlmodelc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_model_path: Option<PathBuf>,
    /// Output feature selection (embedding/hidden_state_last/pooled).
    #[serde(default)]
    pub output_feature: PreprocessOutputFeature,
    /// Layer key aligned to hidden_state_layer naming (optional override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_key: Option<String>,
    /// Maximum sequence length for preprocessing (0 = use input length).
    #[serde(default)]
    pub max_seq_len: u32,
    /// Batch size hint for preprocessing (0 = no batching).
    #[serde(default)]
    pub batch_size: u32,
    /// Optional feature compression to apply to cached tensors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<PreprocessCompression>,
    /// Optional cache directory override (defaults to dataset artifacts root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<PathBuf>,
    /// Optional seed to pin preprocessing determinism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
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
    /// Training data contract version.
    pub training_contract_version: String,
    /// Explicit pad token ID.
    pub pad_token_id: u32,
    /// Explicit ignore index for loss masking (-1 disables masking).
    pub ignore_index: i32,
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
    /// Warmup steps before reaching target learning rate.
    ///
    /// When set to a value greater than 0, enables a cosine decay learning rate
    /// schedule with linear warmup. The learning rate increases linearly from 0
    /// to the target `learning_rate` over the specified warmup steps, then decays
    /// following a cosine schedule.
    ///
    /// When unset or set to 0, training uses a constant learning rate.
    #[serde(default)]
    pub warmup_steps: Option<u32>,
    /// Maximum sequence length for training examples.
    ///
    /// When set, truncates training sequences to this maximum length. Sequences
    /// longer than this value are truncated from the end.
    ///
    /// Defaults to 2048 when not specified.
    #[serde(default)]
    pub max_seq_length: Option<u32>,
    /// Gradient accumulation steps for larger effective batch size.
    ///
    /// Accumulates gradients over N steps before updating weights. This enables
    /// training with larger effective batch sizes without increasing memory usage.
    ///
    /// Effective batch size = `batch_size` × `gradient_accumulation_steps`.
    ///
    /// Defaults to 1 (no accumulation) when not specified.
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
    /// When false, gradients are computed on CPU using the proxy loss path.
    /// Note: GPU backward may not be bit-exact with CPU backward.
    #[serde(default = "default_use_gpu_backward")]
    pub use_gpu_backward: bool,
    /// Optimizer configuration for GPU training.
    /// Only used when `use_gpu_backward` is true.
    #[serde(default)]
    pub optimizer_config: OptimizerConfig,
    /// Path to base model for real hidden state extraction during training.
    /// REQUIRED for GPU backward or validation. CPU proxy training
    /// (use_gpu_backward=false) skips base model loading and uses scaled-token
    /// MSE loss instead.
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
    /// Optional preprocessing stage for tokenized inputs (disabled by default).
    #[serde(default)]
    pub preprocessing: Option<PreprocessingConfig>,
    /// Target modules for LoRA injection (e.g., ["q_proj", "k_proj", "v_proj"]).
    /// When multi_module_training is enabled, separate weights are trained for each target.
    #[serde(default = "default_targets")]
    pub targets: Vec<String>,
    /// Enable multi-module training (train separate weights per target module).
    /// When false (default), trains a single A/B pair applied to all targets.
    #[serde(default)]
    pub multi_module_training: bool,
    /// Layer indices for LoRA injection (e.g., [0, 8, 16, 24, 31]).
    /// If empty, defaults to last layer only for backward compatibility.
    /// When combined with multi_module_training, trains separate weights for each (layer, module) pair.
    #[serde(default)]
    pub lora_layer_indices: Vec<usize>,
    /// MLX framework version used for training (captured at training start).
    /// Required for training reproducibility - ensures identical software stack
    /// can be reconstructed for deterministic replay or debugging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mlx_version: Option<String>,
}

fn default_validation_split() -> f32 {
    0.2
}

fn default_targets() -> Vec<String> {
    vec!["q_proj".to_string(), "v_proj".to_string()]
}

fn default_use_gpu_backward() -> bool {
    true
}

impl TrainingConfig {
    /// Check if this configuration is for an MoE model
    pub fn is_moe(&self) -> bool {
        self.moe_config.is_some()
    }

    /// Whether this configuration requires a base model to run training.
    pub fn requires_base_model(&self) -> bool {
        self.use_gpu_backward || self.validation_split > 0.0 || self.multi_module_training
    }

    /// Get the number of experts (returns 1 for dense models)
    pub fn num_experts(&self) -> usize {
        self.moe_config.as_ref().map(|m| m.num_experts).unwrap_or(1)
    }

    /// Render a concise summary for logging and resume checks.
    pub fn summary(&self) -> String {
        format!(
            "rank={}, alpha={}, lr={}, batch_size={}, epochs={}, hidden_dim={}, vocab_size={}, contract_version={}, pad_token_id={}, ignore_index={}, validation_split={}, hidden_state_layer={:?}, early_stopping={:?}, patience={:?}, min_delta={:?}, optimizer={:?}, beta1={}, beta2={}, epsilon={}, weight_decay={}, momentum={}, use_gpu_backward={}",
            self.rank,
            self.alpha,
            self.learning_rate,
            self.batch_size,
            self.epochs,
            self.hidden_dim,
            self.vocab_size,
            self.training_contract_version,
            self.pad_token_id,
            self.ignore_index,
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

    /// Compute a deterministic BLAKE3 hash of the training configuration.
    ///
    /// This hash covers all fields that affect training outcomes: hyperparameters,
    /// optimizer settings, determinism config, and target modules. It deliberately
    /// excludes runtime-only fields (checkpoint paths, GPU memory limits) that do
    /// not affect the mathematical result of training.
    ///
    /// Two configs that produce the same `canonical_hash` will produce identical
    /// training results given the same dataset and base model.
    pub fn canonical_hash(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"TrainingConfig_v1");
        // Hyperparameters
        hasher.update(&(self.rank as u64).to_le_bytes());
        hasher.update(&self.alpha.to_le_bytes());
        hasher.update(&self.learning_rate.to_le_bytes());
        hasher.update(&(self.batch_size as u64).to_le_bytes());
        hasher.update(&(self.epochs as u64).to_le_bytes());
        hasher.update(&(self.hidden_dim as u64).to_le_bytes());
        hasher.update(&(self.vocab_size as u64).to_le_bytes());
        hasher.update(self.training_contract_version.as_bytes());
        hasher.update(&self.pad_token_id.to_le_bytes());
        hasher.update(&self.ignore_index.to_le_bytes());
        hasher.update(&self.validation_split.to_le_bytes());
        hasher.update(&(self.use_gpu_backward as u8).to_le_bytes());
        // Optimizer
        hasher.update(&(self.optimizer_config.optimizer_type as u8).to_le_bytes());
        hasher.update(&self.optimizer_config.beta1.to_le_bytes());
        hasher.update(&self.optimizer_config.beta2.to_le_bytes());
        hasher.update(&self.optimizer_config.epsilon.to_le_bytes());
        hasher.update(&self.optimizer_config.weight_decay.to_le_bytes());
        hasher.update(&self.optimizer_config.momentum.to_le_bytes());
        // Schedule
        hasher.update(&self.warmup_steps.unwrap_or(0).to_le_bytes());
        hasher.update(&self.gradient_accumulation_steps.unwrap_or(1).to_le_bytes());
        hasher.update(&self.max_seq_length.unwrap_or(0).to_le_bytes());
        // Early stopping
        hasher.update(&(self.early_stopping.unwrap_or(false) as u8).to_le_bytes());
        hasher.update(&self.patience.unwrap_or(0).to_le_bytes());
        hasher.update(&self.min_delta.unwrap_or(0.0).to_le_bytes());
        // Hidden state layer
        if let Some(ref layer) = self.hidden_state_layer {
            hasher.update(layer.as_bytes());
        }
        // Target modules (sorted — BTreeMap-style determinism)
        let mut sorted_targets = self.targets.clone();
        sorted_targets.sort();
        for target in &sorted_targets {
            hasher.update(target.as_bytes());
        }
        hasher.update(&(self.multi_module_training as u8).to_le_bytes());
        // Layer indices (sorted for determinism)
        let mut sorted_layers = self.lora_layer_indices.clone();
        sorted_layers.sort();
        for idx in &sorted_layers {
            hasher.update(&(*idx as u64).to_le_bytes());
        }
        // Determinism seed (if provided)
        if let Some(ref det) = self.determinism {
            if let Some(seed) = det.seed {
                hasher.update(&seed.to_le_bytes());
            }
            if let Some(ref version_id) = det.dataset_version_id {
                hasher.update(version_id.as_bytes());
            }
        }
        // MoE config
        if let Some(ref moe) = self.moe_config {
            hasher.update(&(moe.num_experts as u64).to_le_bytes());
            hasher.update(&(moe.num_experts_per_token as u64).to_le_bytes());
        }
        hasher.finalize().to_hex().to_string()
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
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            pad_token_id: 0,
            ignore_index: 0,
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
            preprocessing: None,
            targets: default_targets(),
            multi_module_training: false,
            lora_layer_indices: Vec::new(),
            mlx_version: None,
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
    /// REQUIRED for GPU backward or validation. CPU proxy training (use_gpu_backward=false)
    /// does not load the base model and uses scaled-token MSE loss instead.
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

    /// Set the MLX framework version for reproducibility tracking.
    /// This should be captured at training start from the runtime environment.
    pub fn with_mlx_version(mut self, version: impl Into<String>) -> Self {
        self.mlx_version = Some(version.into());
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
    /// BLAKE3 hash of deterministic train/validation split
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_hash_b3: Option<String>,
    /// Number of examples in training split
    #[serde(default)]
    pub train_example_count: u64,
    /// Number of examples in validation split
    #[serde(default)]
    pub validation_example_count: u64,
    /// Token count in training split
    #[serde(default)]
    pub train_token_count: u64,
    /// Token count in validation split
    #[serde(default)]
    pub validation_token_count: u64,
    /// Best validation loss achieved and the epoch
    #[serde(default)]
    pub best_validation: Option<(f32, u32)>,
    /// Final validation loss (if validation_split > 0)
    #[serde(default)]
    pub final_validation_loss: Option<f32>,
    /// MLX framework version used for this training run.
    /// Captured at training start for reproducibility verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mlx_version: Option<String>,
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

/// Per-module LoRA weights for multi-layer training.
///
/// Each target module (q_proj, k_proj, v_proj, etc.) has its own A/B matrices
/// when multi-module training is enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleWeights {
    /// Down-projection matrix (rank × hidden_dim)
    pub lora_a: Vec<Vec<f32>>,
    /// Up-projection matrix (hidden_dim × rank)
    pub lora_b: Vec<Vec<f32>>,
}

impl ModuleWeights {
    /// Create new module weights with given dimensions
    pub fn new(rank: usize, hidden_dim: usize) -> Self {
        Self {
            lora_a: vec![vec![0.0; hidden_dim]; rank],
            lora_b: vec![vec![0.0; rank]; hidden_dim],
        }
    }

    /// Check if weights are empty (uninitialized)
    pub fn is_empty(&self) -> bool {
        self.lora_a.is_empty() || self.lora_b.is_empty()
    }
}

/// Per-module optimizer state for Adam momentum tracking.
///
/// Maintains first and second moment estimates for each module's A and B matrices,
/// enabling proper Adam optimization across training steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleOptimizerState {
    /// First moment (mean) for A matrix
    pub m_a: Vec<Vec<f32>>,
    /// Second moment (variance) for A matrix
    pub v_a: Vec<Vec<f32>>,
    /// First moment (mean) for B matrix
    pub m_b: Vec<Vec<f32>>,
    /// Second moment (variance) for B matrix
    pub v_b: Vec<Vec<f32>>,
    /// Number of optimization steps taken
    pub step: u64,
}

impl ModuleOptimizerState {
    /// Create new optimizer state with zero-initialized moments
    pub fn new(rank: usize, hidden_dim: usize) -> Self {
        Self {
            m_a: vec![vec![0.0; hidden_dim]; rank],
            v_a: vec![vec![0.0; hidden_dim]; rank],
            m_b: vec![vec![0.0; rank]; hidden_dim],
            v_b: vec![vec![0.0; rank]; hidden_dim],
            step: 0,
        }
    }

    /// Increment step counter
    pub fn increment_step(&mut self) {
        self.step += 1;
    }

    /// Perform Adam optimizer update on weights using gradients.
    ///
    /// This is a CPU-native Adam implementation that updates m/v moments
    /// and applies the Adam update rule:
    ///
    /// m = β₁ * m + (1 - β₁) * g
    /// v = β₂ * v + (1 - β₂) * g²
    /// m̂ = m / (1 - β₁^t)
    /// v̂ = v / (1 - β₂^t)
    /// w = w - lr * m̂ / (√v̂ + ε)
    ///
    /// Returns updated weights (lora_a, lora_b) in flattened form.
    #[allow(clippy::too_many_arguments)]
    pub fn adam_step(
        &mut self,
        lora_a_flat: &[f32],
        lora_b_flat: &[f32],
        grad_a_flat: &[f32],
        grad_b_flat: &[f32],
        learning_rate: f32,
        beta1: f32,
        beta2: f32,
        epsilon: f32,
        rank: usize,
        hidden_dim: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        self.step += 1;
        let t = self.step as f32;

        // Bias correction factors
        let bc1 = 1.0 - beta1.powf(t);
        let bc2 = 1.0 - beta2.powf(t);

        // Update A weights
        let mut new_a = vec![0.0f32; rank * hidden_dim];
        for r in 0..rank {
            for h in 0..hidden_dim {
                let idx = r * hidden_dim + h;
                let g = grad_a_flat[idx];

                // Update moments
                self.m_a[r][h] = beta1 * self.m_a[r][h] + (1.0 - beta1) * g;
                self.v_a[r][h] = beta2 * self.v_a[r][h] + (1.0 - beta2) * g * g;

                // Bias-corrected estimates
                let m_hat = self.m_a[r][h] / bc1;
                let v_hat = self.v_a[r][h] / bc2;

                // Update weight
                new_a[idx] = lora_a_flat[idx] - learning_rate * m_hat / (v_hat.sqrt() + epsilon);
            }
        }

        // Update B weights
        let mut new_b = vec![0.0f32; hidden_dim * rank];
        for h in 0..hidden_dim {
            for r in 0..rank {
                let idx = h * rank + r;
                let g = grad_b_flat[idx];

                // Update moments
                self.m_b[h][r] = beta1 * self.m_b[h][r] + (1.0 - beta1) * g;
                self.v_b[h][r] = beta2 * self.v_b[h][r] + (1.0 - beta2) * g * g;

                // Bias-corrected estimates
                let m_hat = self.m_b[h][r] / bc1;
                let v_hat = self.v_b[h][r] / bc2;

                // Update weight
                new_b[idx] = lora_b_flat[idx] - learning_rate * m_hat / (v_hat.sqrt() + epsilon);
            }
        }

        (new_a, new_b)
    }
}

/// Multi-module optimizer state container.
///
/// Holds per-module optimizer states and configuration for multi-module training.
/// Uses BTreeMap for deterministic iteration order during gradient updates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultiModuleOptimizerState {
    /// Per-module optimizer states keyed by module name (q_proj, k_proj, etc.)
    /// Uses BTreeMap for deterministic iteration order.
    pub module_states: BTreeMap<String, ModuleOptimizerState>,
}

impl MultiModuleOptimizerState {
    /// Create new multi-module optimizer state
    pub fn new() -> Self {
        Self {
            module_states: BTreeMap::new(),
        }
    }

    /// Get or create optimizer state for a module
    pub fn get_or_create(
        &mut self,
        module_name: &str,
        rank: usize,
        hidden_dim: usize,
    ) -> &mut ModuleOptimizerState {
        self.module_states
            .entry(module_name.to_string())
            .or_insert_with(|| ModuleOptimizerState::new(rank, hidden_dim))
    }

    /// Get optimizer state for a module (if exists)
    pub fn get(&self, module_name: &str) -> Option<&ModuleOptimizerState> {
        self.module_states.get(module_name)
    }

    /// Get mutable optimizer state for a module (if exists)
    pub fn get_mut(&mut self, module_name: &str) -> Option<&mut ModuleOptimizerState> {
        self.module_states.get_mut(module_name)
    }
}

/// LoRA weight matrices with multi-module support.
///
/// Supports both single-module (legacy) and multi-module training:
/// - Single-module: Uses `lora_a` and `lora_b` directly
/// - Multi-module: Uses `modules` HashMap with per-target weights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAWeights {
    /// Per-module weights for multi-layer training (q_proj, k_proj, etc.)
    /// When populated, these take precedence over legacy single-module weights.
    /// Uses BTreeMap for deterministic iteration order during training/serialization.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, ModuleWeights>,
    /// Down-projection matrix (rank × hidden_dim) - legacy single-module
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lora_a: Vec<Vec<f32>>,
    /// Up-projection matrix (hidden_dim × rank) - legacy single-module
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lora_b: Vec<Vec<f32>>,
    /// MoE configuration (if trained for MoE model)
    #[serde(default)]
    pub moe_config: Option<MoETrainingConfig>,
    /// Precomputed delta (B @ A) for faster inference (optional)
    #[serde(skip)]
    pub precomputed_delta: Option<Vec<Vec<f32>>>,
}

impl LoRAWeights {
    /// Create new LoRA weights with given dimensions (single-module mode)
    pub fn new(rank: usize, hidden_dim: usize) -> Self {
        Self {
            modules: BTreeMap::new(),
            lora_a: vec![vec![0.0; hidden_dim]; rank],
            lora_b: vec![vec![0.0; rank]; hidden_dim],
            moe_config: None,
            precomputed_delta: None,
        }
    }

    /// Create new multi-module LoRA weights for specified targets
    pub fn new_multi_module(rank: usize, hidden_dim: usize, targets: &[String]) -> Self {
        let mut modules = BTreeMap::new();
        for target in targets {
            modules.insert(target.clone(), ModuleWeights::new(rank, hidden_dim));
        }
        Self {
            modules,
            lora_a: Vec::new(),
            lora_b: Vec::new(),
            moe_config: None,
            precomputed_delta: None,
        }
    }

    /// Create new multi-layer LoRA weights for specified targets and layer indices.
    ///
    /// Creates a separate weight set for each (layer, module) combination.
    /// Module keys follow the pattern `layer_{idx}.{module}` (e.g., `layer_0.q_proj`).
    ///
    /// # Arguments
    /// * `rank` - LoRA rank dimension
    /// * `hidden_dim` - Hidden dimension size
    /// * `targets` - Target module names (e.g., ["q_proj", "v_proj"])
    /// * `layer_indices` - Layer indices for LoRA injection (e.g., [0, 16, 31])
    pub fn new_multi_layer(
        rank: usize,
        hidden_dim: usize,
        targets: &[String],
        layer_indices: &[usize],
    ) -> Self {
        let mut modules = BTreeMap::new();
        for layer_idx in layer_indices {
            for target in targets {
                let key = format!("layer_{}.{}", layer_idx, target);
                modules.insert(key, ModuleWeights::new(rank, hidden_dim));
            }
        }
        Self {
            modules,
            lora_a: Vec::new(),
            lora_b: Vec::new(),
            moe_config: None,
            precomputed_delta: None,
        }
    }

    /// Create new LoRA weights for MoE model
    pub fn new_moe(rank: usize, hidden_dim: usize, moe_config: MoETrainingConfig) -> Self {
        Self {
            modules: BTreeMap::new(),
            lora_a: vec![vec![0.0; hidden_dim]; rank],
            lora_b: vec![vec![0.0; rank]; hidden_dim],
            moe_config: Some(moe_config),
            precomputed_delta: None,
        }
    }

    /// Check if this is multi-module training mode
    pub fn is_multi_module(&self) -> bool {
        !self.modules.is_empty()
    }

    /// Check if these weights are for an MoE model
    pub fn is_moe(&self) -> bool {
        self.moe_config.is_some()
    }

    /// Get module weights by name (returns None for legacy single-module)
    pub fn get_module(&self, module_name: &str) -> Option<&ModuleWeights> {
        self.modules.get(module_name)
    }

    /// Get mutable module weights by name
    pub fn get_module_mut(&mut self, module_name: &str) -> Option<&mut ModuleWeights> {
        self.modules.get_mut(module_name)
    }

    /// Get or create module weights for a target
    pub fn get_or_create_module(
        &mut self,
        module_name: &str,
        rank: usize,
        hidden_dim: usize,
    ) -> &mut ModuleWeights {
        self.modules
            .entry(module_name.to_string())
            .or_insert_with(|| ModuleWeights::new(rank, hidden_dim))
    }

    /// Get list of all module names in multi-module mode.
    /// Returns names in deterministic sorted order (BTreeMap provides this inherently).
    pub fn module_names(&self) -> Vec<&str> {
        // BTreeMap iterates in sorted key order, ensuring deterministic output
        self.modules.keys().map(|s| s.as_str()).collect()
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
