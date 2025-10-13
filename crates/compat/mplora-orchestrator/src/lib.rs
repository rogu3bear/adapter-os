#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-orchestrator instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-orchestrator`
//!
//! This crate has been renamed to `adapteros-orchestrator`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-orchestrator = "0.2"
//! ```

pub use adapteros_orchestrator::*;
