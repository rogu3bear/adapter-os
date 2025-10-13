#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-lora-plan instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-plan`
//!
//! This crate has been renamed to `adapteros-lora-plan`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-lora-plan = "0.2"
//! ```

pub use adapteros_lora_plan::*;
