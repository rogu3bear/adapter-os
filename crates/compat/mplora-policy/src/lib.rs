#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-policy instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-policy`
//!
//! This crate has been renamed to `adapteros-policy`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-policy = "0.2"
//! ```

pub use adapteros_policy::*;
