#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-api instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-api`
//!
//! This crate has been renamed to `adapteros-api`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-api = "0.2"
//! ```

pub use adapteros_api::*;
