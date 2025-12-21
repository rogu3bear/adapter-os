//! CoreML-specific configuration types shared across worker and server
//!
//! This module defines a platform-agnostic CoreML compute preference enum that
//! can be surfaced through configuration and mapped onto the CoreML binding
//! (`ComputeUnits`) by downstream crates.

use adapteros_core::AosError;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// User-facing CoreML compute preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CoreMLComputePreference {
    /// CPU only (deterministic, slowest)
    CpuOnly,
    /// CPU + GPU (preferred default for throughput)
    #[default]
    CpuAndGpu,
    /// CPU + Neural Engine
    CpuAndNe,
    /// All available units (CPU + GPU + NE)
    All,
}

impl CoreMLComputePreference {
    /// Canonical snake_case string for logging/config echo
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreMLComputePreference::CpuOnly => "cpu_only",
            CoreMLComputePreference::CpuAndGpu => "cpu_and_gpu",
            CoreMLComputePreference::CpuAndNe => "cpu_and_ne",
            CoreMLComputePreference::All => "all",
        }
    }
}

impl Display for CoreMLComputePreference {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CoreMLComputePreference {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase();
        match normalized.as_str() {
            "cpu_only" | "cpu-only" => Ok(CoreMLComputePreference::CpuOnly),
            "cpu_and_gpu" | "cpu-gpu" | "cpu+gpu" => Ok(CoreMLComputePreference::CpuAndGpu),
            "cpu_and_ne"
            | "cpu_and_neural_engine"
            | "cpu-ne"
            | "cpu+ne"
            | "cpu_ne" => Ok(CoreMLComputePreference::CpuAndNe),
            "all" | "cpu_gpu_ne" | "cpu-gpu-ne" => Ok(CoreMLComputePreference::All),
            _ => Err(AosError::Config(format!(
                "Invalid CoreML compute preference '{}'. Valid values: cpu_only, cpu_and_gpu, cpu_and_ne, all",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_preferences() {
        assert_eq!(
            CoreMLComputePreference::from_str("cpu_only").unwrap(),
            CoreMLComputePreference::CpuOnly
        );
        assert_eq!(
            CoreMLComputePreference::from_str("cpu_and_gpu").unwrap(),
            CoreMLComputePreference::CpuAndGpu
        );
        assert_eq!(
            CoreMLComputePreference::from_str("cpu_and_neural_engine").unwrap(),
            CoreMLComputePreference::CpuAndNe
        );
        assert_eq!(
            CoreMLComputePreference::from_str("all").unwrap(),
            CoreMLComputePreference::All
        );
    }

    #[test]
    fn rejects_invalid_preference() {
        let err = CoreMLComputePreference::from_str("ne_only").unwrap_err();
        assert!(
            err.to_string()
                .contains("Invalid CoreML compute preference"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn default_prefers_cpu_and_gpu() {
        assert_eq!(
            CoreMLComputePreference::default(),
            CoreMLComputePreference::CpuAndGpu
        );
    }
}
