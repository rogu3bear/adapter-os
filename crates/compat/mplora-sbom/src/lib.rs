#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-sbom instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-sbom`
//!
//! This crate has been renamed to `adapteros-sbom`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-sbom = "0.2"
//! ```

pub use adapteros_sbom::*;
