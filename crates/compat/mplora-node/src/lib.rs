#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-node instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-node`
//!
//! This crate has been renamed to `adapteros-node`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-node = "0.2"
//! ```

pub use adapteros_node::*;
