//! Neural Network Layers for adapteros-mlx
//!
//! This module provides layer abstractions that wrap the underlying Array
//! operations and can optionally delegate to ANE acceleration.
//!
//! # Layers
//!
//! - [`LayerNorm`] - Layer normalization
//! - [`RMSNorm`] - Root Mean Square normalization (LLaMA-style)
//! - [`MultiHeadAttention`] - Multi-head self-attention
//! - [`MLP`] - Feed-forward network / MLP block
//!
//! # ANE Acceleration
//!
//! Layers accept an optional `AneAccelerator` reference. When provided and
//! conditions are met (batch size >= threshold), certain operations will
//! execute on the Neural Engine for better power efficiency.
//!
//! ```ignore
//! let norm = LayerNorm::new(512, 1e-5)?;
//! let output = norm.forward(&input, Some(&ane_accel))?;
//! ```

pub mod norm;
pub mod attention;
pub mod mlp;

pub use norm::{LayerNorm, RMSNorm};
pub use attention::MultiHeadAttention;
pub use mlp::MLP;
