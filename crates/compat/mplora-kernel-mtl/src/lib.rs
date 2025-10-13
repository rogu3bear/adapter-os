#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-lora-kernel-mtl instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-kernel-mtl`
//!
//! This crate has been renamed to `adapteros-lora-kernel-mtl`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-lora-kernel-mtl = "0.2"
//! ```

pub use adapteros_lora_kernel_mtl::*;
