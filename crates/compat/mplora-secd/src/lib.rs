#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-secd instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-secd`
//!
//! This crate has been renamed to `adapteros-secd`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-secd = "0.2"
//! ```

pub use adapteros_secd::*;
