//! CoreML configuration

use serde::{Deserialize, Serialize};

/// CoreML compute unit preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeUnits {
    /// CPU only (deterministic, slowest)
    CpuOnly,
    /// CPU and GPU (may be non-deterministic)
    CpuAndGpu,
    /// CPU and Neural Engine (deterministic, power-efficient)
    #[default]
    CpuAndNeuralEngine,
    /// All available units (optimal performance, determinism varies)
    All,
}

/// CoreML backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLConfig {
    /// Preferred compute units
    pub compute_units: ComputeUnitsConfig,
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Enable profiling
    pub enable_profiling: bool,
    /// Model cache directory
    pub cache_dir: Option<String>,
    /// Require ANE availability (enforced in production mode)
    #[serde(default)]
    pub require_ane: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputeUnitsConfig {
    CpuOnly,
    CpuAndGpu,
    CpuAndNeuralEngine,
    All,
}

impl From<ComputeUnitsConfig> for ComputeUnits {
    fn from(config: ComputeUnitsConfig) -> Self {
        match config {
            ComputeUnitsConfig::CpuOnly => ComputeUnits::CpuOnly,
            ComputeUnitsConfig::CpuAndGpu => ComputeUnits::CpuAndGpu,
            ComputeUnitsConfig::CpuAndNeuralEngine => ComputeUnits::CpuAndNeuralEngine,
            ComputeUnitsConfig::All => ComputeUnits::All,
        }
    }
}

impl Default for CoreMLConfig {
    fn default() -> Self {
        Self {
            compute_units: ComputeUnitsConfig::CpuAndNeuralEngine,
            max_batch_size: 32,
            enable_profiling: false,
            cache_dir: None,
            require_ane: false,
        }
    }
}

/// Model-specific parameters for CoreML inference
///
/// These parameters configure the attention mechanism and model architecture.
/// They should be loaded from the model's config.json file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLModelParams {
    /// Hidden layer dimension
    pub hidden_size: usize,
    /// Number of attention heads
    pub num_attention_heads: usize,
    /// Number of key-value heads (for GQA - Grouped Query Attention)
    pub num_key_value_heads: usize,
    /// FFN intermediate size
    pub intermediate_size: usize,
    /// RoPE (Rotary Position Embedding) theta parameter
    pub rope_theta: f32,
    /// Maximum sequence length
    pub max_seq_len: usize,
}

impl CoreMLModelParams {
    /// Create model params from individual values
    ///
    /// # Arguments
    /// * `hidden_size` - Hidden layer dimension
    /// * `num_attention_heads` - Number of attention heads
    /// * `num_key_value_heads` - Number of KV heads (for GQA)
    /// * `intermediate_size` - FFN intermediate size
    /// * `rope_theta` - RoPE theta parameter
    /// * `max_seq_len` - Maximum sequence length
    pub fn new(
        hidden_size: usize,
        num_attention_heads: usize,
        num_key_value_heads: usize,
        intermediate_size: usize,
        rope_theta: f32,
        max_seq_len: usize,
    ) -> Self {
        Self {
            hidden_size,
            num_attention_heads,
            num_key_value_heads,
            intermediate_size,
            rope_theta,
            max_seq_len,
        }
    }

    /// Compute head dimension
    pub fn head_dim(&self) -> usize {
        self.hidden_size / self.num_attention_heads
    }

    /// Compute number of KV groups (for GQA)
    pub fn kv_groups(&self) -> usize {
        self.num_attention_heads / self.num_key_value_heads
    }
}

impl Default for CoreMLModelParams {
    /// Default parameters for Qwen2.5-7B model
    fn default() -> Self {
        Self {
            hidden_size: 3584,
            num_attention_heads: 28,
            num_key_value_heads: 4,
            intermediate_size: 18944,
            rope_theta: 1000000.0,
            max_seq_len: 32768,
        }
    }
}
