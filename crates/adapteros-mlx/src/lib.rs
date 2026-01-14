//! AdapterOS MLX - mlx-rs Abstraction Layer
//!
//! **DEPRECATED**: This crate is no longer used. The `mlx-rs-backend` feature
//! has been removed from the codebase. The production backend is C++ FFI
//! (`adapteros-lora-mlx-ffi` with `mlx` feature).
//!
//! This crate remains in the workspace for reference only and may be removed
//! in a future cleanup.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │              adapteros-mlx                      │
//! │  (unified API for all tensor operations)       │
//! ├─────────────────────────────────────────────────┤
//! │                                                 │
//! │  ┌─────────────────┐  ┌─────────────────────┐  │
//! │  │  ANE Accelerator │  │   MLX GPU Backend  │  │
//! │  │  (CoreML/ANE)   │  │   (mlx-rs 0.25)    │  │
//! │  │                 │  │                     │  │
//! │  │  LayerNorm ✓    │  │  All operations    │  │
//! │  │  RMSNorm ✓      │  │  (fallback path)   │  │
//! │  │  Softmax ✓      │  │                     │  │
//! │  └─────────────────┘  └─────────────────────┘  │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! # Backend Selection
//!
//! | Backend | When Used | Determinism |
//! |---------|-----------|-------------|
//! | **CoreML ANE** | Production, batch ≥ 32, macOS 15+ | Fixed-point arithmetic |
//! | **MLX GPU** | Development, small batch, fallback | Seeded RNG (when seeded) |
//!
//! ANE provides power efficiency for production. MLX GPU provides flexibility
//! for development and training.
//!
//! **Note:** End-to-end determinism depends on proper HKDF seeding at the system level
//! (see `adapteros-core::seed`). This crate provides the seeding API but does not
//! enforce that callers use it correctly.
//!
//! # Requirements
//!
//! - Apple Silicon (M1/M2/M3/M4)
//! - macOS 13+ (MLX), macOS 15+ (ANE via MLTensor)
//!
//! # Feature Flags
//!
//! - `coreml-ane` - Enable CoreML/ANE acceleration (recommended for production)
//!
//! # Usage
//!
//! ## Basic Array Operations
//!
//! ```ignore
//! use adapteros_mlx::{Array, Dtype};
//!
//! let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;
//! let transposed = arr.transpose()?;
//! let data = transposed.to_vec_f32()?;
//! ```
//!
//! ## Normalization Layers
//!
//! ```ignore
//! use adapteros_mlx::{Array, RMSNorm, LayerNorm};
//!
//! let norm = RMSNorm::new(512, 1e-5)?;
//! let x = Array::from_f32(&input_data, &[batch, seq_len, 512])?;
//! let normalized = norm.forward(&x, None)?;
//! ```
//!
//! ## ANE Acceleration (Production)
//!
//! ```ignore
//! use adapteros_mlx::{AneAccelerator, AneConfig, is_ane_available};
//!
//! // Check ANE availability
//! if is_ane_available() {
//!     let config = AneConfig::production();
//!     if let Some(ane) = AneAccelerator::try_new(config) {
//!         // ANE-accelerated ops (batch ≥ 32)
//!         let normalized = ane.layernorm(&x, &weight, &bias, 1e-5)?;
//!         let probs = ane.softmax(&logits, -1)?;
//!
//!         // Verify determinism
//!         let report = ane.attest();
//!         assert!(report.deterministic);
//!     }
//! }
//! ```
//!
//! ## Seeding for Determinism
//!
//! ```ignore
//! use adapteros_mlx::set_seed;
//!
//! set_seed(42)?;  // Seed MLX RNG for reproducible results
//! ```
//!
//! # Testing
//!
//! Run tests single-threaded (Metal command buffer constraint):
//!
//! ```bash
//! cargo test -p adapteros-mlx -- --test-threads=1
//! cargo test -p adapteros-mlx --features coreml-ane -- --test-threads=1
//! ```

pub mod ane;
mod array;
mod device;
mod error;
pub mod layers;

use std::sync::atomic::{AtomicBool, Ordering};

pub use array::{Array, Dtype};
pub use device::Device;
pub use error::MlxError;

// Re-export commonly used layer types
pub use layers::{LayerNorm, MultiHeadAttention, RMSNorm, MLP};

// Re-export ANE types
pub use ane::{is_ane_available, AneAccelerator, AneConfig};

/// Result type for MLX operations
pub type Result<T> = std::result::Result<T, MlxError>;

/// Global initialization flag
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the MLX runtime (idempotent)
///
/// This function verifies that mlx-rs is functional by performing a minimal
/// operation. Safe to call multiple times.
///
/// NOTE: Due to Metal command buffer threading issues, this runs on a dedicated
/// thread rather than a tokio worker thread. This ensures proper Metal device
/// initialization.
pub fn runtime_init() -> Result<()> {
    if INITIALIZED.load(Ordering::SeqCst) {
        return Ok(());
    }

    tracing::debug!("adapteros-mlx: Starting runtime initialization on dedicated thread");

    // Run MLX initialization on a dedicated thread to avoid tokio worker thread
    // conflicts with Metal command buffers
    let result = std::thread::spawn(|| {
        // First, explicitly set the default device to GPU
        // This ensures the MLX C library's Metal device is properly initialized
        tracing::debug!("adapteros-mlx: (thread) Setting default device to GPU");
        let gpu_device = mlx_rs::Device::gpu();
        mlx_rs::Device::set_default(&gpu_device);
        tracing::debug!("adapteros-mlx: (thread) GPU device set as default");

        tracing::debug!("adapteros-mlx: (thread) Creating test array");
        let test_result = Array::from_f32(&[1.0], &[1]);
        match test_result {
            Ok(test) => {
                tracing::debug!("adapteros-mlx: (thread) Array created, now evaluating");
                match test.to_vec_f32() {
                    Ok(data) => {
                        tracing::debug!("adapteros-mlx: (thread) to_vec_f32 succeeded: {:?}", data);
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!("adapteros-mlx: (thread) to_vec_f32 failed: {}", e);
                        Err(e)
                    }
                }
            }
            Err(e) => {
                tracing::error!("adapteros-mlx: (thread) Array creation failed: {}", e);
                Err(e)
            }
        }
    })
    .join()
    .map_err(|_| MlxError::InitializationFailed("Thread panicked during MLX init".to_string()))?;

    match result {
        Ok(()) => {
            INITIALIZED.store(true, Ordering::SeqCst);
            tracing::info!("adapteros-mlx runtime initialized (mlx-rs 0.25)");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Seed the MLX RNG (best-effort determinism for stochastic ops).
pub fn set_seed(seed: u64) -> Result<()> {
    mlx_rs::random::seed(seed).map_err(|e| MlxError::Upstream(format!("seed: {e}")))?;
    Ok(())
}

/// Check if the MLX runtime is initialized
pub fn runtime_is_initialized() -> bool {
    INITIALIZED.load(Ordering::SeqCst)
}

/// MLX backend info
pub fn backend_info() -> &'static str {
    "mlx-rs 0.25 (Apple Silicon GPU)"
}
