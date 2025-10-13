#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-lora-quant instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-quant`
//!
//! This crate has been renamed to `adapteros-lora-quant`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-lora-quant = "0.2"
//! ```

pub use adapteros_lora_quant::*;
