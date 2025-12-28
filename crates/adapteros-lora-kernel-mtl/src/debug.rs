//! Deterministic debug mode for kernel execution tracing
//!
//! When enabled via AOS_DETERMINISTIC_DEBUG=1 environment variable,
//! this module logs seed chains, kernel dispatches, and parameter hashes
//! without leaking sensitive tensor data.

use adapteros_core::B3Hash;
use serde::Serialize;

/// Kernel debugger for determinism tracing
pub struct KernelDebugger {
    enabled: bool,
}

impl KernelDebugger {
    /// Create debugger from environment
    ///
    /// Checks for AOS_DETERMINISTIC_DEBUG environment variable
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("AOS_DETERMINISTIC_DEBUG")
                .map(|v| v == "1")
                .unwrap_or(false),
        }
    }

    /// Create debugger with explicit enable state
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Check if debug mode is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Log seed derivation in HKDF chain
    ///
    /// Only logs the label and hash, no raw seed bytes
    pub fn log_seed_chain(&self, label: &str, seed_hash: &B3Hash) {
        if self.enabled {
            tracing::debug!(
                label = %label,
                hash = %seed_hash.to_hex(),
                "HKDF seed chain"
            );
        }
    }

    /// Log kernel dispatch with hashed parameters
    ///
    /// Parameters are hashed to prevent data leakage while
    /// maintaining reproducibility verification
    pub fn log_kernel_dispatch(&self, kernel_name: &str, params: &KernelParams) {
        if self.enabled {
            let param_json = serde_json::to_vec(params).unwrap_or_default();
            let param_hash = B3Hash::hash(&param_json);
            tracing::debug!(
                kernel = %kernel_name,
                params_hash = %param_hash.to_hex(),
                "Kernel dispatch"
            );
        }
    }

    /// Log buffer allocation without data
    pub fn log_buffer_allocation(&self, name: &str, size_bytes: u64) {
        if self.enabled {
            tracing::debug!(
                buffer_name = %name,
                size_bytes = size_bytes,
                "Buffer allocation"
            );
        }
    }

    /// Log adapter activation
    pub fn log_adapter_activation(&self, adapter_id: u32, gate_q15: u16) {
        if self.enabled {
            let gate_f32 = gate_q15 as f32 / 32767.0;
            tracing::debug!(
                adapter_id = adapter_id,
                gate_f32 = gate_f32,
                gate_q15 = gate_q15,
                "Adapter activation"
            );
        }
    }
}

/// Kernel dispatch parameters for hashing
///
/// Contains only shape/configuration information, no tensor data
#[derive(Debug, Serialize)]
pub struct KernelParams {
    pub grid_size: (u64, u64, u64),
    pub threadgroup_size: (u64, u64, u64),
    pub buffer_count: usize,
}

impl KernelParams {
    /// Create params from Metal dispatch configuration
    pub fn new(grid: (u64, u64, u64), threadgroup: (u64, u64, u64), buffers: usize) -> Self {
        Self {
            grid_size: grid,
            threadgroup_size: threadgroup,
            buffer_count: buffers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_debugger_disabled_by_default() {
        let _guard = env_lock().lock().expect("env lock");
        // Ensure the environment variable is cleared before testing
        std::env::remove_var("AOS_DETERMINISTIC_DEBUG");

        // Verify the environment variable is actually unset
        assert!(std::env::var("AOS_DETERMINISTIC_DEBUG").is_err());

        let debugger = KernelDebugger::from_env();
        assert!(!debugger.is_enabled());
    }

    #[test]
    fn test_debugger_enabled_from_env() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("AOS_DETERMINISTIC_DEBUG", "1");
        let debugger = KernelDebugger::from_env();
        assert!(debugger.is_enabled());
        std::env::remove_var("AOS_DETERMINISTIC_DEBUG");
    }

    #[test]
    fn test_kernel_params_serialization() {
        let params = KernelParams::new((256, 1, 1), (16, 16, 1), 5);
        let json = serde_json::to_string(&params).expect("KernelParams should serialize to JSON");
        assert!(json.contains("grid_size"));
        assert!(json.contains("threadgroup_size"));
    }

    #[test]
    fn test_logging_doesnt_panic() {
        let debugger = KernelDebugger::new(true);
        let hash = B3Hash::hash(b"test");

        debugger.log_seed_chain("test_label", &hash);

        let params = KernelParams::new((1, 1, 1), (1, 1, 1), 0);
        debugger.log_kernel_dispatch("test_kernel", &params);

        debugger.log_buffer_allocation("test_buffer", 1024);
        debugger.log_adapter_activation(42, 16384);
    }
}
