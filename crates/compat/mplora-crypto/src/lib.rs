#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-crypto instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-crypto`
//!
//! This crate has been renamed to `adapteros-crypto`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-crypto = "0.2"
//! ```

pub use adapteros_crypto::*;
