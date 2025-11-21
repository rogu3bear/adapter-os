//! CoreML configuration

use serde::{Deserialize, Serialize};

/// CoreML compute unit preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
