//! Core type definitions for adapterOS
//!
//! This crate provides pure data types without framework dependencies.
//! It serves as the single source of truth for domain types used across
//! the adapterOS control plane, worker nodes, and client libraries.
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
//! 1. **No framework dependencies** - Only serde, chrono, uuid
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

/// CoreML placement specification types
pub mod coreml;

/// Fusion interval policy shared across inference components
pub mod fusion;

/// Inference requests/responses and receipts
pub mod inference;

/// Prompt-injection detection utilities.
pub mod prompt_injection;

/// Manifest metadata shared between DB/API layers
pub mod manifest;

/// Repository assurance tiers
pub mod repository;

/// Router decision and candidate types
pub mod routing;

/// Telemetry event types
pub mod telemetry;

/// Common API patterns (requests, responses, streaming)
pub mod api;

/// Node and status types
pub mod nodes;

/// Tenant and usage types
pub mod tenants;

/// Re-exported types for convenience
///
/// # Purpose
///
/// This re-export strategy provides three import paths:
///
/// 1. **Specific imports** (recommended for clarity):
///    ```ignore
///    use adapteros_types::adapters::AdapterInfo;
///    use adapteros_types::core::Uuid;
///    use adapteros_types::routing::RouterDecision;
///    ```
///
/// 2. **Aggregate imports** (convenience):
///    ```ignore
///    use adapteros_types::AdapterInfo;  // From adapters
///    use adapteros_types::Uuid;         // From core (via pub use core::*)
///    use adapteros_types::RouterDecision;
///    ```
///
/// 3. **Module imports** (for namespace clarity):
///    ```ignore
///    use adapteros_types::{adapters, core, routing};
///    ```
///
/// The `pub use core::*` re-export provides domain-agnostic primitives (identity,
/// temporal, pagination) at the crate root for ergonomic access. All other modules
/// use explicit type re-exports to document the public API surface.
///
/// # Import Guidelines
///
/// - Use path 1 (specific) for new code when type origin matters for clarity
/// - Use path 2 (aggregate) for existing code to maintain backward compatibility
/// - Avoid mixing re-export sources in the same file—pick one strategy per file
pub use adapters::{
    AdapterInfo, AdapterMetadata, AdapterMetrics, AdapterState, LifecycleState,
    RegisterAdapterRequest,
};
/// Re-export all core primitives (identity, temporal, pagination) for ergonomic access.
/// These domain-agnostic types form the foundation for all domain-specific types.
pub use core::*;
pub use coreml::{
    CoreMLGating, CoreMLMode, CoreMLOpKind, CoreMLPlacementBinding, CoreMLPlacementShape,
    CoreMLPlacementSpec, CoreMLProjection, CoreMLTargetRef,
};
pub use fusion::FusionInterval;
pub use inference::{
    CancelSource, CancellationState, InferRequest, RunReceipt, StopReasonCode, STOP_Q15_DENOM,
};
pub use manifest::Manifest;
pub use nodes::{Node, NodeDetail};
pub use prompt_injection::{check_prompt_injection, PromptInjectionResult};
pub use repository::RepoTier;
pub use routing::{RouterCandidate, RouterDecision, RouterModelType};
pub use telemetry::{EventType, LogLevel, TelemetryBundle, TelemetryEvent, TelemetryFilters};
pub use tenants::{Tenant, TenantUsage};
