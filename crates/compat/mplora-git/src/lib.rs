#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-git instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-git`
//!
//! This crate has been renamed to `adapteros-git`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-git = "0.2"
//! ```

pub use adapteros_git::*;
