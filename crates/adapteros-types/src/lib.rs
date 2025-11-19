//! Core type definitions for AdapterOS
//!
//! This crate provides pure data types without framework dependencies.
//! It serves as the single source of truth for domain types used across
//! the AdapterOS control plane, worker nodes, and client libraries.
//!
//! # Architecture
//!
//! - `core` - Domain-agnostic primitives (identity, timestamps, pagination)
//! - `adapters` - Adapter lifecycle and metadata types
//! - `training` - Training job and configuration types
//! - `routing` - Router decision and candidate types
//! - `telemetry` - Telemetry event types
//! - `api` - Common API request/response patterns
//!
//! # Design Principles
//!
//! 1. **No framework dependencies** - Only serde, chrono, uuid, blake3
//! 2. **snake_case serialization** - Consistent with REST API conventions
//! 3. **Explicit field naming** - No ambiguity in JSON serialization
//! 4. **Versioned schemas** - All types include schema version metadata

#![warn(missing_docs)]
#![deny(unsafe_code)]

/// Schema version for type definitions
pub const TYPES_SCHEMA_VERSION: &str = "1.0";

/// Core primitives (identity, temporal, pagination)
pub mod core;

/// Adapter lifecycle and metadata types
pub mod adapters;

/// Training job and configuration types
pub mod training;

/// Router decision and candidate types
pub mod routing;

/// Telemetry event types
pub mod telemetry;

/// Common API patterns (requests, responses, streaming)
pub mod api;

// Re-export commonly used types for convenience
pub use adapters::{AdapterMetadata, LifecycleState, RegisterAdapterRequest};
pub use core::*;
pub use routing::{RouterCandidate, RouterDecision};
