#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-system-metrics instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-system-metrics`
//!
//! This crate has been renamed to `adapteros-system-metrics`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-system-metrics = "0.2"
//! ```

pub use adapteros_system_metrics::*;
