#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-metrics-exporter instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-metrics-exporter`
//!
//! This crate has been renamed to `adapteros-metrics-exporter`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-metrics-exporter = "0.2"
//! ```

pub use adapteros_metrics_exporter::*;
