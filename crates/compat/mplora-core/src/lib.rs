#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-core instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-core`
//!
//! This crate has been renamed to `adapteros-core`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-core = "0.2"
//! ```

pub use adapteros_core::*;
