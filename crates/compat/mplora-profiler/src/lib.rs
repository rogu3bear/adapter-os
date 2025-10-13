#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-profiler instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-profiler`
//!
//! This crate has been renamed to `adapteros-profiler`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-profiler = "0.2"
//! ```

pub use adapteros_profiler::*;
