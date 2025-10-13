#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-artifacts instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-artifacts`
//!
//! This crate has been renamed to `adapteros-artifacts`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-artifacts = "0.2"
//! ```

pub use adapteros_artifacts::*;
