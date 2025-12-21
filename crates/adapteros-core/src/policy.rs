//! Policy configuration types

use serde::{Deserialize, Serialize};

/// Drift policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftPolicy {
    /// OS build version tolerance (0 = exact match required)
    pub os_build_tolerance: u8,
    /// GPU driver version tolerance (0 = exact match required)
    pub gpu_driver_tolerance: u8,
    /// Environment hash tolerance (0 = exact match required)
    pub env_hash_tolerance: u8,
    /// Allow warning-level drift
    pub allow_warnings: bool,
    /// Block on critical drift
    pub block_on_critical: bool,
}

impl Default for DriftPolicy {
    fn default() -> Self {
        Self {
            os_build_tolerance: 0,
            gpu_driver_tolerance: 0,
            env_hash_tolerance: 0,
            allow_warnings: true,
            block_on_critical: true,
        }
    }
}
