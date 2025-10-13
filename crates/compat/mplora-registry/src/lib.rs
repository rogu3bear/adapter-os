#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-registry instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-registry`
//!
//! This crate has been renamed to `adapteros-registry`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-registry = "0.2"
//! ```

pub use adapteros_registry::*;
