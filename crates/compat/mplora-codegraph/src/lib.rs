#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-codegraph instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-codegraph`
//!
//! This crate has been renamed to `adapteros-codegraph`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-codegraph = "0.2"
//! ```

pub use adapteros_codegraph::*;
