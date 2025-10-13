#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-cli instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-cli`
//!
//! This crate has been renamed to `adapteros-cli`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-cli = "0.2"
//! ```

pub use adapteros_cli::*;
