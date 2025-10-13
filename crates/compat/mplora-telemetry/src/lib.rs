#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-telemetry instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-telemetry`
//!
//! This crate has been renamed to `adapteros-telemetry`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-telemetry = "0.2"
//! ```

pub use adapteros_telemetry::*;
