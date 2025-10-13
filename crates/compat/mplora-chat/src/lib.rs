#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-chat instead. This compatibility crate will be removed in 0.3.0."
)]

//! Compatibility shim for `mplora-chat`
//!
//! This crate has been renamed to `adapteros-chat`.
//! Please update your `Cargo.toml` to use the new name:
//!
//! ```toml
//! [dependencies]
//! adapteros-chat = "0.2"
//! ```

pub use adapteros_chat::*;
