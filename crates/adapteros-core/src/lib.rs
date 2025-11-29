//! AdapterOS Core Types
//!
//! Foundational types and utilities for the AdapterOS system.
//!
//! This crate provides:
//! - Error handling with [`AosError`] and [`Result`]
//! - Cryptographic hashing with [`B3Hash`] (BLAKE3)
//! - Checkpoint IDs with [`CPID`]
//! - Deterministic seed derivation for RNG
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::{B3Hash, CPID, derive_seed};
//!
//! // Hash some data
//! let hash = B3Hash::hash(b"hello world");
//! println!("Hash: {}", hash.to_hex());
//!
//! // Derive a checkpoint ID
//! let cpid = CPID::from_hash(&hash);
//! println!("CPID: {}", cpid);
//!
//! // Derive deterministic seeds
//! let seed = derive_seed(&hash, "component_a");
//! ```

pub mod circuit_breaker;
pub mod constants;
pub mod context_hash;
pub mod error;
pub mod error_helpers;
pub mod hash;
pub mod id;
pub mod identity;
pub mod index_snapshot;
pub mod json;
pub mod lifecycle;
pub mod naming;
pub mod paths;
pub mod plugin_events;
pub mod plugins;
pub mod policy;
pub mod retry_policy;
pub mod seed;
pub mod stack;
pub mod status;
pub mod tenant_snapshot;
pub mod time;
pub mod timeout;
pub mod training;
pub mod validation;
pub mod version;

pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState,
    SharedCircuitBreaker, StandardCircuitBreaker,
};
pub use constants::{
    bytes_to_gb, bytes_to_mb, gb_to_bytes, kb_to_bytes, mb_to_bytes, BYTES_PER_GB, BYTES_PER_KB,
    BYTES_PER_MB, DEFAULT_TIMEOUT_SECS, SLOW_TIMEOUT_SECS,
};
pub use context_hash::{compute_context_hash, ChunkRef};
pub use error::{AosError, Result, ResultExt};
pub use hash::B3Hash;
pub use id::CPID;
pub use lifecycle::{LifecycleState, LifecycleTransition, SemanticVersion, TransitionReason};
pub use naming::{AdapterName, ForkType, StackName};
pub use paths::{get_adapter_path, get_default_adapters_root, AdapterPaths};
pub use plugin_events::{
    AdapterEvent, AuditEvent, InferenceEvent, MetricsTickEvent, PluginEvent, PolicyViolationEvent,
    TrainingJobEvent,
};
pub use plugins::{EventHookType, Plugin, PluginConfig, PluginHealth, PluginStatus};
pub use policy::DriftPolicy;
pub use seed::{
    clear_seed_registry, derive_adapter_seed, derive_seed, derive_seed_full, derive_seed_indexed,
    derive_seed_typed, hash_adapter_dir, SeedLabel,
};
pub use stack::compute_stack_hash;
pub use status::{AdapterOSStatus, HealthCheckResult, HealthStatus, ServiceStatus};
pub use timeout::TimeoutExt;
pub use training::{TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate};
pub use version::VersionInfo;

/// RNG module version for determinism tracking
/// @deprecated Use `version::RNG_MODULE_VERSION` instead
pub const RNG_MODULE_VERSION: &str = "1.0.0-chacha20";

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{
        bytes_to_gb, bytes_to_mb, gb_to_bytes, kb_to_bytes, mb_to_bytes, AdapterEvent,
        AdapterName, AdapterOSStatus, AosError, AuditEvent, B3Hash, CircuitBreaker,
        CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState, DriftPolicy, EventHookType,
        ForkType, HealthCheckResult, HealthStatus, InferenceEvent, LifecycleState,
        LifecycleTransition, MetricsTickEvent, Plugin, PluginConfig, PluginEvent, PluginHealth,
        PluginStatus, PolicyViolationEvent, Result, ResultExt, SemanticVersion, ServiceStatus,
        SharedCircuitBreaker, StackName, StandardCircuitBreaker, TrainingConfig, TrainingJob,
        TrainingJobEvent, TrainingJobStatus, TrainingTemplate, TransitionReason, VersionInfo,
        BYTES_PER_GB, BYTES_PER_KB, BYTES_PER_MB, CPID, DEFAULT_TIMEOUT_SECS, SLOW_TIMEOUT_SECS,
    };
}
