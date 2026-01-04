//! AdapterOS UDS Protocol Types (PRD-RECT-002)
//!
//! This crate defines the canonical types for Unix Domain Socket communication
//! between the control plane and worker processes.
//!
//! # Type Categories
//!
//! - **Request types**: `WorkerInferRequest`, `PatchProposalInferRequest`
//! - **Response types**: `WorkerInferResponse`
//! - **Stream types**: `StreamToken`, `StreamFrame`
//!
//! # Migration Status
//!
//! This crate is being introduced to consolidate UDS protocol types.
//! Currently, types are duplicated between:
//! - `adapteros-server-api::types` (control plane side)
//! - `adapteros-lora-worker::response_types` (worker side)
//!
//! The consolidation will proceed incrementally:
//! 1. Define canonical types here
//! 2. Re-export from original locations for backward compatibility
//! 3. Migrate consumers to use this crate directly
//!
//! # Schema Stability
//!
//! Changes to these types affect wire compatibility. Use JSON schema
//! snapshot tests to detect breaking changes.

pub mod stream;

pub use stream::{StreamFrame, StreamToken, WorkerStreamEvent};

/// Protocol version for UDS communication.
///
/// Increment when making breaking changes to the protocol.
pub const PROTOCOL_VERSION: u32 = 1;

/// SSE event types used in UDS streaming responses.
pub mod sse_events {
    /// Token event - emitted for each generated token
    pub const TOKEN: &str = "token";
    /// Complete event - emitted when generation finishes
    pub const COMPLETE: &str = "complete";
    /// Error event - emitted on generation failure
    pub const ERROR: &str = "error";
    /// Signal event - emitted for worker-to-client signals
    pub const SIGNAL: &str = "signal";
}
