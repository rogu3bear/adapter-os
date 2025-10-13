#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-lora-lifecycle instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-lifecycle`
//!
//! This crate has been renamed to `adapteros-lora-lifecycle`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-lora-lifecycle = "0.2"
//! ```

pub use adapteros_lora_lifecycle::*;
