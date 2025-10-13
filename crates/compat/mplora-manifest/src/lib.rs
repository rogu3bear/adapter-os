#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-manifest instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-manifest`
//!
//! This crate has been renamed to `adapteros-manifest`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-manifest = "0.2"
//! ```

pub use adapteros_manifest::*;
