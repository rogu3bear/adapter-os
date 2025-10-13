#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-db instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-db`
//!
//! This crate has been renamed to `adapteros-db`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-db = "0.2"
//! ```

pub use adapteros_db::*;
