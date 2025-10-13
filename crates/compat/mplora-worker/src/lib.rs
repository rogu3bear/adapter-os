#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-lora-worker instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-worker`
//!
//! This crate has been renamed to `adapteros-lora-worker`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-lora-worker = "0.2"
//! ```

pub use adapteros_lora_worker::*;
