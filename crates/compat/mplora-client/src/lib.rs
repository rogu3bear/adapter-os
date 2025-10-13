#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-client instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-client`
//!
//! This crate has been renamed to `adapteros-client`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-client = "0.2"
//! ```

pub use adapteros_client::*;
