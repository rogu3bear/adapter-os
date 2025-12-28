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

pub mod adapter_repo_paths;
pub mod adapter_store;
pub mod backend;
pub mod circuit_breaker;
pub mod circuit_breaker_registry;
pub mod constants;
pub mod context_hash;
pub mod context_manifest;
pub mod defaults;
pub mod determinism;
pub mod determinism_mode;
pub mod error;
pub mod error_helpers;
pub mod errors;
pub mod evidence_envelope;
pub mod evidence_verifier;
pub mod fusion_interval;
pub mod guard_common;
pub mod hash;
pub mod id;
pub mod identity;
pub mod index_snapshot;
pub mod json;
pub mod lifecycle;
pub mod naming;
pub mod path_security;
pub mod paths;
pub mod plugin_events;
pub mod plugins;
pub mod policy;
pub mod prefix_kv_key;
pub mod redaction;
pub mod retry_policy;
pub mod seed;
pub mod seed_guard;
pub mod singleflight;
pub mod stack;
pub mod status;
pub mod telemetry;
pub mod tenant;
pub mod tenant_isolation;
pub mod tenant_snapshot;
pub mod time;
pub mod timeout;
pub mod training;
pub mod validation;
pub mod version;
pub mod worker_status;

pub use adapter_repo_paths::{
    adapter_fs_path, adapter_fs_path_with_root, resolve_adapter_roots_from_strings,
    AdapterPaths as RepoAdapterPaths, ResolveError, VersionStrategy, DEFAULT_CACHE_DIR,
    DEFAULT_REPO_DIR,
};
pub use adapter_store::{
    AdapterCacheKey, AdapterPins, AdapterRecord, AdapterSnapshot, AdapterStore,
};
pub use backend::BackendKind;
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState,
    SharedCircuitBreaker, StandardCircuitBreaker,
};
pub use circuit_breaker_registry::CircuitBreakerRegistry;
pub use constants::{
    bytes_to_gb, bytes_to_mb, gb_to_bytes, kb_to_bytes, mb_to_bytes, BYTES_PER_GB, BYTES_PER_KB,
    BYTES_PER_MB, DEFAULT_TIMEOUT_SECS, SLOW_TIMEOUT_SECS,
};
pub use context_hash::{compute_context_hash, ChunkRef};
pub use context_manifest::{
    ContextAdapterEntry, ContextAdapterEntryV1, ContextManifest, ContextManifestV1,
};
pub use determinism::{
    derive_router_seed, derive_router_tiebreak_seed, derive_sampler_seed, expand_u64_seed,
    DeterminismContext, DeterminismSource,
};
pub use determinism_mode::DeterminismMode;
pub use error::{AosError, Result, ResultExt};
// Re-export categorical error types for structured error handling
pub use errors::{
    AosAdapterError, AosAuthError, AosCryptoError, AosInternalError, AosModelError,
    AosNetworkError, AosOperationsError, AosPolicyError, AosResourceError, AosStorageError,
    AosValidationError, CacheBudgetExceededInfo,
};
pub use evidence_envelope::{
    compute_key_id, BundleMetadataRef, EvidenceEnvelope, EvidenceScope, InferenceReceiptRef,
    PolicyAuditRef, EVIDENCE_ENVELOPE_SCHEMA_VERSION,
};
pub use evidence_verifier::{
    evidence_chain_divergence, is_evidence_chain_divergence, ChainVerificationResult,
    EnvelopeVerificationResult, EvidenceVerifier, EVIDENCE_CHAIN_DIVERGED_CODE,
};
pub use fusion_interval::FusionInterval;
pub use guard_common::{GuardConfig, GuardLogLevel};
pub use hash::B3Hash;
pub use id::CPID;
pub use lifecycle::{LifecycleState, LifecycleTransition, SemanticVersion, TransitionReason};
pub use naming::{AdapterName, ForkType, StackName};
pub use path_security::{
    is_forbidden_tmp_path, is_forbidden_tmp_path_str, reject_forbidden_tmp_path,
    reject_forbidden_tmp_path_like, FORBIDDEN_TMP_PREFIXES,
};
pub use paths::{get_adapter_path, get_default_adapters_root, AdapterPaths};
pub use plugin_events::{
    AdapterEvent, AuditEvent, InferenceEvent, MetricsTickEvent, PluginEvent, PolicyViolationEvent,
    TrainingJobEvent,
};
pub use plugins::{EventHookType, Plugin, PluginConfig, PluginHealth, PluginStatus};
pub use policy::DriftPolicy;
pub use prefix_kv_key::{
    compute_prefix_kv_key, compute_tokenizer_manifest_hash, encode_prefix_tokens,
    PrefixKvKeyBuilder,
};
pub use seed::{
    clear_seed_registry, derive_adapter_seed, derive_request_seed, derive_seed, derive_seed_full,
    derive_seed_indexed, derive_seed_typed, hash_adapter_dir, ExecutionProfile, SeedLabel,
    SeedMode,
};
pub use seed_guard::SeedScopeGuard;
pub use stack::compute_stack_hash;
pub use status::{AdapterOSStatus, HealthCheckResult, HealthStatus, ServiceStatus};
pub use telemetry::{
    audit_chain_divergence_event, audit_export_tamper_event, determinism_violation_event,
    dual_write_divergence_event, emit_observability_event, policy_override_event,
    receipt_mismatch_event, strict_mode_failure_event, DeterminismViolationKind,
    ObservabilityDetail, ObservabilityEvent, ObservabilityEventKind, ObservabilitySeverity,
    AUDIT_DIVERGENCE_ERROR, AUDIT_DIVERGENCE_METRIC, DETERMINISM_VIOLATION_METRIC,
    POLICY_DENY_OVERRIDE_ERROR, POLICY_OVERRIDE_METRIC, RECEIPT_MISMATCH_ERROR,
    RECEIPT_MISMATCH_METRIC, STRICT_DETERMINISM_ERROR, STRICT_DETERMINISM_METRIC,
};
pub use tenant::{TenantContext, TenantId, WorkspaceId};
pub use tenant_isolation::{
    TenantIsolationAction, TenantIsolationConfig, TenantIsolationDecision, TenantIsolationEngine,
    TenantIsolationReason, TenantIsolationRequest, TenantIsolationTarget, TenantIsolationVerdict,
    TenantIsolationViolation, TenantPrincipal, TENANT_ISOLATION_ERROR_CODE,
};
pub use timeout::TimeoutExt;
pub use training::{TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate};
pub use version::VersionInfo;
pub use worker_status::{WorkerStatus, WorkerStatusTransition};

/// RNG module version for determinism tracking
/// @deprecated Use `version::RNG_MODULE_VERSION` instead
pub const RNG_MODULE_VERSION: &str = "1.0.0-chacha20";

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{
        bytes_to_gb, bytes_to_mb, gb_to_bytes, kb_to_bytes, mb_to_bytes, AdapterEvent, AdapterName,
        AdapterOSStatus, AosError, AuditEvent, B3Hash, BackendKind, CircuitBreaker,
        CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState, DriftPolicy, EventHookType,
        ExecutionProfile, ForkType, HealthCheckResult, HealthStatus, InferenceEvent,
        LifecycleState, LifecycleTransition, MetricsTickEvent, ObservabilityEvent,
        ObservabilityEventKind, ObservabilitySeverity, Plugin, PluginConfig, PluginEvent,
        PluginHealth, PluginStatus, PolicyViolationEvent, Result, ResultExt, SeedMode,
        SemanticVersion, ServiceStatus, SharedCircuitBreaker, StackName, StandardCircuitBreaker,
        TrainingConfig, TrainingJob, TrainingJobEvent, TrainingJobStatus, TrainingTemplate,
        TransitionReason, VersionInfo, WorkerStatus, WorkerStatusTransition, BYTES_PER_GB,
        BYTES_PER_KB, BYTES_PER_MB, CPID, DEFAULT_TIMEOUT_SECS, DETERMINISM_VIOLATION_METRIC,
        SLOW_TIMEOUT_SECS,
    };
}
