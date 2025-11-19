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
pub mod error;
pub mod timeout;
pub mod hash;
pub mod id;
pub mod identity;
pub mod index_snapshot;
pub mod lifecycle;
pub mod naming;
pub mod plugins;
pub mod policy;
pub mod seed;
pub mod stack;
pub mod tenant_snapshot;
pub mod training;
pub mod version;

pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState, SharedCircuitBreaker, StandardCircuitBreaker,
};
pub use timeout::TimeoutExt;
pub use error::{AosError, Result, ResultExt};
pub use hash::B3Hash;
pub use id::CPID;
pub use lifecycle::{LifecycleState, LifecycleTransition, SemanticVersion, TransitionReason};
pub use naming::{AdapterName, ForkType, StackName};
pub use plugins::{Plugin, PluginConfig, PluginHealth, PluginStatus};
pub use policy::DriftPolicy;
pub use stack::compute_stack_hash;
pub use seed::{
    clear_seed_registry, derive_adapter_seed, derive_seed, derive_seed_full, derive_seed_indexed,
    derive_seed_typed, hash_adapter_dir, SeedLabel,
};
pub use training::{TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate};
pub use version::VersionInfo;

/// RNG module version for determinism tracking
/// @deprecated Use `version::RNG_MODULE_VERSION` instead
pub const RNG_MODULE_VERSION: &str = "1.0.0-chacha20";

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{
        AdapterName, AosError, B3Hash, CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState,
        DriftPolicy, ForkType, LifecycleState, LifecycleTransition, Result, ResultExt, SemanticVersion,
        StackName, TransitionReason, CPID, SharedCircuitBreaker, StandardCircuitBreaker, TrainingConfig,
        TrainingJob, TrainingJobStatus, TrainingTemplate, VersionInfo,
    };
}
