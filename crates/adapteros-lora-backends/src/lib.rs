//! Unified Compute Backends Facade for AdapterOS
//!
//! This crate provides a unified interface over multiple LoRA inference backends:
//!
//! - **Metal**: GPU acceleration via Metal Performance Shaders (macOS)
//! - **CoreML**: Neural Engine acceleration via CoreML (macOS 13+)
//! - **MLX**: Apple's ML framework via C++ FFI
//! - **Profiler**: Kernel performance profiling
//!
//! # Feature Flags
//!
//! - `metal` (default): Enable Metal GPU backend
//! - `coreml`: Enable CoreML/Neural Engine backend
//! - `mlx`: Enable MLX backend
//! - `profiler`: Enable kernel profiling
//! - `all`: Enable all backends
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_lora_backends::prelude::*;
//!
//! // Create backend based on available features
//! let backend = MetalKernels::new()?;
//! ```

// Re-export the kernel API (always available)
pub use adapteros_lora_kernel_api as api;
pub use adapteros_lora_kernel_api::{
    attestation, blend_and_forward_reference, AdapterLookup, BackendHealth, BackendMetrics,
    FusedKernels, GpuBufferFingerprint, IoBuffers, LiquidBlendRequest, LiquidBlendStats,
    LiquidKernel, LiquidPrecision, LiquidSlice, LiquidTensor, MockKernels, MploraConfig,
    MploraKernels, RouterRing, LIQUID_MAX_ADAPTERS,
};

// Backend implementations (feature-gated)
#[cfg(feature = "metal")]
pub use adapteros_lora_kernel_mtl as metal;

#[cfg(feature = "coreml")]
pub use adapteros_lora_kernel_coreml as coreml;

#[cfg(feature = "mlx")]
pub use adapteros_lora_mlx_ffi as mlx;

#[cfg(feature = "profiler")]
pub use adapteros_lora_kernel_prof as profiler;

/// Backend selection hint
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendHint {
    /// Use Metal GPU backend
    Metal,
    /// Use CoreML/Neural Engine backend
    CoreML,
    /// Use MLX backend
    Mlx,
    /// Auto-select best available backend
    #[default]
    Auto,
}

/// Check if a backend is available at compile time
pub fn is_backend_available(hint: BackendHint) -> bool {
    match hint {
        BackendHint::Metal => cfg!(feature = "metal"),
        BackendHint::CoreML => cfg!(feature = "coreml"),
        BackendHint::Mlx => cfg!(feature = "mlx"),
        BackendHint::Auto => {
            cfg!(feature = "metal") || cfg!(feature = "coreml") || cfg!(feature = "mlx")
        }
    }
}

/// List all available backends based on compile-time features
#[allow(clippy::vec_init_then_push)] // cfg attributes require push pattern
pub fn available_backends() -> Vec<BackendHint> {
    let mut backends = Vec::new();

    #[cfg(feature = "metal")]
    backends.push(BackendHint::Metal);

    #[cfg(feature = "coreml")]
    backends.push(BackendHint::CoreML);

    #[cfg(feature = "mlx")]
    backends.push(BackendHint::Mlx);

    backends
}

/// Convenience prelude for common imports
pub mod prelude {
    pub use super::{available_backends, is_backend_available, BackendHint};
    pub use adapteros_lora_kernel_api::{
        BackendHealth, BackendMetrics, FusedKernels, IoBuffers, MockKernels, RouterRing,
    };

    #[cfg(feature = "metal")]
    pub use adapteros_lora_kernel_mtl::MetalKernels;

    #[cfg(feature = "coreml")]
    pub use adapteros_lora_kernel_coreml::CoreMLBackend;

    #[cfg(feature = "mlx")]
    pub use adapteros_lora_mlx_ffi::MLXFFIBackend;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_availability() {
        // At least one backend should be available
        assert!(
            !available_backends().is_empty(),
            "At least one backend should be compiled"
        );
    }

    #[test]
    fn test_backend_hint_default() {
        assert_eq!(BackendHint::default(), BackendHint::Auto);
    }

    #[test]
    fn test_is_backend_available() {
        // Auto should be available if any backend is
        if !available_backends().is_empty() {
            assert!(is_backend_available(BackendHint::Auto));
        }
    }
}
