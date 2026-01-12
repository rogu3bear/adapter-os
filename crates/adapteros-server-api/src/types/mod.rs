//! API types for the adapteros-server-api crate.
//!
//! This module provides all request, response, and internal types used by the API layer.
//! Types are organized into submodules by category but are re-exported at this level
//! for backward compatibility with `pub use types::*` in lib.rs.

pub mod auth;
pub mod code_policy;
pub mod context;
pub mod conversion;
pub mod error;
pub mod event_applier;
pub mod replay;
pub mod request;
pub mod response;
pub mod sampling;
pub mod session;
pub mod telemetry;

// Re-export the run_envelope submodule
pub mod run_envelope;
pub use run_envelope::{new_run_envelope, set_policy_mask, set_router_seed, set_worker_context};

// Re-export everything from submodules FIRST - these are our local definitions
// Note: auth and conversion modules contain only comments/impls, nothing to re-export
pub use code_policy::*;
pub use context::*;
pub use error::*;
pub use event_applier::*;
pub use replay::*;
pub use request::*;
pub use response::*;
pub use sampling::*;
pub use session::*;
pub use telemetry::*;

// Re-export shared API types from adapteros-api-types.
pub use adapteros_api_types::*;

// Re-export dataset chunked upload response types for API tests.
pub use crate::handlers::datasets::{
    CompleteChunkedUploadResponse, InitiateChunkedUploadResponse, UploadChunkResponse,
};
