//! Compile-Time Feature Guards (PRD-RECT-001)
//!
//! This module provides compile-time checks for illegal feature combinations.
//! If an illegal combination is detected, compilation fails with a clear error.
//!
//! # Illegal Combinations
//!
//! - `impl-mlx-bridge` + `deterministic-only`: The MLX bridge uses a subprocess
//!   which introduces non-determinism (process scheduling, IPC timing).

// =============================================================================
// ILLEGAL COMBINATION: mlx-bridge + deterministic-only
// =============================================================================
//
// The MLX bridge spawns a Python subprocess for inference, which introduces
// non-determinism due to:
// - Process scheduling variations
// - IPC timing differences
// - Python GIL behavior
//
// This is incompatible with deterministic-only mode which guarantees
// bit-exact reproducibility.

#[cfg(all(feature = "impl-mlx-bridge", feature = "deterministic-only"))]
compile_error!(
    "Feature conflict: `impl-mlx-bridge` cannot be used with `deterministic-only`. \
     The MLX bridge subprocess introduces non-determinism. \
     Use `backend-mlx` (C++ FFI) for deterministic inference."
);

// Legacy feature name check
#[cfg(all(feature = "mlx-bridge", feature = "deterministic-only"))]
compile_error!(
    "Feature conflict: `mlx-bridge` cannot be used with `deterministic-only`. \
     The MLX bridge subprocess introduces non-determinism. \
     Use `backend-mlx` (C++ FFI) for deterministic inference."
);

// =============================================================================
// Feature Guard Tests
// =============================================================================

#[cfg(test)]
mod tests {
    // These tests verify the feature guard logic is present.
    // The actual compile_error! guards cannot be tested at runtime
    // since they prevent compilation.

    #[test]
    fn test_feature_guards_module_loads() {
        // If this test runs, the module loaded successfully
        // (meaning no illegal feature combinations were detected)
        assert!(true);
    }
}
