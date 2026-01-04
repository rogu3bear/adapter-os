//! # Unified Inference Execution Core
//!
//! This module provides `InferenceCore` - the **ONLY** path to execute inference.
//! All handlers (standard, streaming, batch, replay) **MUST** use this module.
//!
//! ## Module Structure
//!
//! The inference core is split into the following submodules:
//!
//! - **core**: The main `InferenceCore` struct and `route_and_infer()` pipeline
//! - **adapters**: Adapter resolution and validation helpers
//! - **determinism**: Determinism mode resolution and validation
//! - **policy**: Execution policy resolution (determinism + routing + golden)
//! - **replay**: Replay guarantee computation and runtime guards
//! - **validation**: Request validation helpers
//!
//! ## Usage
//!
//! ```rust,ignore
//! use adapteros_server_api::inference_core::{InferenceCore, InferenceRequestInternal};
//!
//! let core = InferenceCore::new(&app_state);
//! let result = core.route_and_infer(request, None, None, None).await?;
//! ```

mod adapters;
mod core;
mod determinism;
mod diag;
mod policy;
mod replay;
mod validation;

#[cfg(test)]
mod tests;

// Primary public API
pub use core::InferenceCore;

// Re-export types from crate::types for convenience
// Note: InferenceError, InferenceRequestInternal, InferenceResult are defined in types.rs
// and re-exported at the crate level via `pub use types::*`

// Policy resolution (used by handlers)
pub use policy::{
    resolve_tenant_execution_policy, ExecutionPolicyResolved, GoldenPolicyResolved,
    RoutingPolicyResolved,
};

// Determinism helpers
pub use determinism::{
    compute_strict_mode, resolve_determinism_mode, validate_strict_mode_constraints,
};

// Replay functionality
pub use replay::{compute_replay_guarantee, enforce_strict_runtime_guards};

// Validation
pub use validation::{parse_pinned_adapter_ids, validate_pinned_within_effective_set};

// Adapter helpers (internal use, exposed for testing)
pub use adapters::{map_router_decision_chain, map_router_decisions, parse_routing_mode};
