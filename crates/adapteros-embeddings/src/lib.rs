//! Deterministic embedding generation with model hashing and seed tracking
//!
//! Provides:
//! - EmbeddingModel trait for pluggable backends
//! - MLX implementation (production)
//! - Determinism verification

pub mod config;
pub mod determinism;
pub mod model;

#[cfg(feature = "training")]
pub mod lora;
#[cfg(feature = "training")]
pub mod training;

pub use model::{Embedding, EmbeddingModel, EmbeddingProvider};

// Re-export from MLX FFI when available
#[cfg(feature = "mlx")]
pub use adapteros_lora_mlx_ffi::MLXEmbeddingModel;
