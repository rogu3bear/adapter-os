//! AdapterOS MLX - Single Source of Truth
//!
//! Real GPU inference on Apple Silicon. No stubs, no demo mode.
//! NOTE: The mlx-rs backend is deprecated/unsupported in production; this crate remains for reference.
//!
//! # Overview
//!
//! This crate wraps `mlx-rs` and provides a stable internal API for all
//! MLX operations in AdapterOS. It is the ONLY place where mlx-rs should
//! be used directly.
//!
//! # Requirements
//!
//! - Apple Silicon (M1/M2/M3/M4)
//! - macOS 13+
//!
//! # Usage
//!
//! ## Array Operations
//!
//! ```ignore
//! use adapteros_mlx::{Array, Dtype};
//!
//! let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;
//! let transposed = arr.transpose()?;
//! let data = transposed.to_vec_f32()?;
//! ```
//!
//! ## Normalization
//!
//! ```ignore
//! use adapteros_mlx::{Array, RMSNorm};
//!
//! let norm = RMSNorm::new(512, 1e-5)?;
//! let x = Array::from_f32(&input_data, &[batch, seq_len, 512])?;
//! let normalized = norm.forward(&x, None)?;
//! ```
//!
//! ## Layers
//!
//! ```ignore
//! use adapteros_mlx::{Array, MultiHeadAttention, MLP};
//!
//! let attn = MultiHeadAttention::new(512, 8)?; // 8 heads
//! let ffn = MLP::new(512, 2048, Activation::SiLU, true)?; // Gated FFN
//! ```
//!
//! ## ANE Acceleration (Optional)
//!
//! ```ignore
//! use adapteros_mlx::{AneAccelerator, AneConfig};
//!
//! // Try to create ANE accelerator (returns None if unavailable)
//! let config = AneConfig::production();
//! if let Some(ane) = AneAccelerator::try_new(config) {
//!     // ANE-accelerated operations for batch >= 32
//!     let normalized = ane.layernorm(&x, &weight, &bias, 1e-5)?;
//! }
//! ```
//!
//! # Testing
//!
//! Due to Metal command buffer threading issues in the cargo test harness,
//! run tests single-threaded:
//!
//! ```bash
//! cargo test -p adapteros-mlx -- --test-threads=1
//! ```

mod array;
mod device;
mod error;
pub mod ane;
pub mod layers;

use std::sync::atomic::{AtomicBool, Ordering};

pub use array::{Array, Dtype};
pub use device::Device;
pub use error::MlxError;

// Re-export commonly used layer types
pub use layers::{LayerNorm, RMSNorm, MultiHeadAttention, MLP};

// Re-export ANE types
pub use ane::{AneAccelerator, AneConfig, is_ane_available};

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

/// Check if the MLX runtime is initialized
pub fn runtime_is_initialized() -> bool {
    INITIALIZED.load(Ordering::SeqCst)
}

/// MLX backend info
pub fn backend_info() -> &'static str {
    "mlx-rs 0.25 (Apple Silicon GPU)"
}
