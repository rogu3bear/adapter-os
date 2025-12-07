//! Compatibility shim that re-exports the canonical request ID utilities.
//!
//! The definitive implementation lives in `crate::request_id`; this module
//! preserves the previous import path used by handlers and middleware.
pub use crate::request_id::{request_id_middleware, RequestId};
