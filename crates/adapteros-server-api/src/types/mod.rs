//! API types for the adapteros-server-api crate.
//!
//! This module provides all request, response, and internal types used by the API layer.
//! Types are organized into submodules by category but are re-exported at this level
//! for backward compatibility with `pub use types::*` in lib.rs.
//!
//! # Conflicting Types
//!
//! Some types in this module have the same name as types in `adapteros-api-types` but
//! with different fields. These local versions are intentionally different:
//!
//! - `SpawnWorkerRequest`: Local version has extra fields (uid, gid, model_cache_max_mb,
//!   config_toml_path) needed for server-side worker spawning.

pub mod auth;
pub mod context;
pub mod conversion;
pub mod error;
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
// The local SpawnWorkerRequest is intentionally different from the api-types version
// Note: auth and conversion modules contain only comments/impls, nothing to re-export
pub use context::*;
pub use error::*;
pub use replay::*;
pub use request::*;
pub use response::*;
pub use sampling::*;
pub use session::*;
pub use telemetry::*;

// Re-export shared API types from adapteros-api-types.
// This comes AFTER local types to allow local types to shadow api-types when there are conflicts.
// However, due to Rust's glob re-export rules, same-named types will cause ambiguity errors.
// The conflicting types (SpawnWorkerRequest, GitStatusResponse) are NOT in api-types glob
// because we want to use our local versions with different fields.
//
// We exclude SpawnWorkerRequest and GitStatusResponse from the glob by re-exporting
// specific submodules instead of the conflicting ones.
pub use adapteros_api_types::*;
