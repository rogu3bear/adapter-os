#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-server-api instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-server-api`
//!
//! This crate has been renamed to `adapteros-server-api`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-server-api = "0.2"
//! ```

pub use adapteros_server_api::*;
