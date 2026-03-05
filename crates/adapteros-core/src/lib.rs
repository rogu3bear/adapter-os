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
pub mod adapter_type;
pub mod archive;
pub mod backend;
pub mod build_info;
pub mod circuit_breaker;
pub mod circuit_breaker_registry;
pub mod clock;
pub mod codebase_versioning;
pub mod context_digest;
pub mod context_hash;
pub mod context_id;
pub mod context_manifest;
pub mod crypto_receipt;
pub mod debug_bypass;
pub mod defaults;
pub mod deployment_verification;
pub mod determinism;
pub mod determinism_mode;
pub mod drain;
pub mod error;
pub mod error_codes;
pub mod error_helpers;
pub mod error_macros;
pub mod errors;
pub mod evidence_envelope;
pub mod evidence_verifier;
pub mod feature_guards;
pub mod fusion_interval;
pub mod guard_common;
pub mod identity;
pub mod ids;
pub mod index_snapshot;
pub mod integrity_mode;
pub mod io_utils;
pub mod jitter;
pub mod json;
pub mod lifecycle;
pub mod model_format;
pub mod model_import_status;
pub mod naming;
pub mod path_normalization;
pub mod path_security;
pub mod path_utils;
pub mod paths;
pub mod plugin_events;
pub mod plugins;
pub mod policy;
pub mod prefix_kv_key;
pub mod preflight;
pub mod receipt_digest;
pub mod receipt_merkle;
pub mod recovery;
pub mod redaction;
pub mod retry_policy;
pub mod seed;
pub mod seed_guard;
pub mod seed_override;
pub mod serde_helpers;
pub mod singleflight;
pub mod stack;
pub mod status;
pub mod telemetry;
pub mod tenant;
pub mod tenant_isolation;
pub mod tenant_snapshot;
pub mod third_party_verification;
pub mod time;
pub mod timeout;
pub mod tokenizer_config;
pub mod training;
pub mod training_receipt_digest;
pub mod validation;
pub mod version;
pub mod worker_status;

#[cfg(feature = "cache-attestation")]
pub mod cache_attestation;

#[cfg(test)]
pub(crate) mod test_support;

pub use adapter_repo_paths::{
    adapter_fs_path, adapter_fs_path_with_root, resolve_adapter_roots_from_strings,
    RepoAdapterPaths, ResolveError, VersionStrategy, DEFAULT_CACHE_DIR, DEFAULT_REPO_DIR,
};
pub use adapter_store::{
    AdapterCacheKey, AdapterPins, AdapterRecord, AdapterSnapshot, AdapterStore,
};
pub use adapter_type::{AdapterType, AdapterTypeParseError};
pub use adapteros_infra_common as constants;
/// Re-export hash module for compatibility with `adapteros_core::hash::B3Hash` imports
pub use adapteros_infra_common::hash;
pub use adapteros_infra_common::{
    bytes_to_gb, bytes_to_mb, canonical_adapter_sort, canonical_score_comparator,
    cosine_similarity, decode_q15_gate, dot_product, encode_q15_gate,
    extract_repo_identifier_from_metadata, gb_to_bytes, kb_to_bytes, l2_norm, mb_to_bytes,
    normalize, normalize_path_segments, normalize_repo_id, normalize_repo_slug, sanitize_optional,
    sanitize_repo_identifier, sanitize_repo_slug, validate_seed_bytes, validate_seed_bytes_soft,
    B3Hash, BYTES_PER_GB, BYTES_PER_KB, BYTES_PER_MB, CPID, DEFAULT_TIMEOUT_SECS,
    EPS as VECTOR_EPS, Q15_GATE_DENOMINATOR, SLOW_TIMEOUT_SECS,
};
pub use backend::BackendKind;
pub use build_info::BuildInfo;
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState,
    SharedCircuitBreaker, StandardCircuitBreaker,
};
pub use circuit_breaker_registry::CircuitBreakerRegistry;
pub use clock::{Clock, SystemClock};
pub use codebase_versioning::{
    evaluate_versioning, should_auto_version, VersionBump, VersioningContext, VersioningDecision,
    VersioningPolicy, VersioningReason, DEFAULT_VERSIONING_THRESHOLD, MAX_VERSIONING_THRESHOLD,
    MIN_VERSIONING_THRESHOLD,
};
pub use model_format::{discover_model_dirs, ModelFormat};
pub use model_import_status::ModelImportStatus;
pub use naming::{AdapterName, ForkType, StackName};

pub use context_manifest::{
    ContextAdapterEntry, ContextAdapterEntryV1, ContextManifest, ContextManifestV1,
};
pub use crypto_receipt::{
    compute_adapter_config_hash, CancelSource, CancellationReceipt, CancellationReceiptBuilder,
    EquipmentProfile, ReceiptGenerator, RoutingRecord, CANCELLATION_RECEIPT_SCHEMA_VERSION,
    CRYPTO_RECEIPT_SCHEMA_VERSION,
};
pub use deployment_verification::{
    check_coreml_hash, check_manifest_hash, check_no_session_conflict, check_repo_clean,
    verify_codebase_deployment, AdapterVerificationState, DeploymentCheck, DeploymentCheckResult,
};
pub use determinism::{
    derive_router_seed, derive_router_tiebreak_seed, derive_sampler_seed, expand_u64_seed,
    DeterminismContext, DeterminismSource,
};
pub use determinism_mode::DeterminismMode;
pub use drain::{
    phase_for_elapsed, should_emit_warning_sample, DrainPhase, DrainPhaseConfig, DrainStats,
};
pub use error::{AosError, Result, ResultExt};
pub use integrity_mode::IntegrityMode;
// Re-export categorical error types for structured error handling
pub use crypto_receipt::compute_input_digest as compute_input_digest_v2;
pub use errors::{
    AosAdapterError, AosAuthError, AosCryptoError, AosInternalError, AosModelError,
    AosNetworkError, AosOperationsError, AosPolicyError, AosResourceError, AosStorageError,
    AosValidationError, CacheBudgetExceededInfo,
};
pub use evidence_envelope::{
    compute_key_id, compute_unavailable_pinned_set_digest_b3, pinned_degradation_telemetry_ref_ids,
    BundleMetadataRef, EvidenceEnvelope, EvidenceScope, InferenceReceiptRef,
    PinnedDegradationEvidence, PolicyAuditRef, EVIDENCE_ENVELOPE_SCHEMA_VERSION,
};
pub use evidence_verifier::{
    evidence_chain_divergence, is_evidence_chain_divergence, ChainVerificationResult,
    EnvelopeVerificationResult, EvidenceVerifier, EVIDENCE_CHAIN_DIVERGED_CODE,
};
pub use fusion_interval::FusionInterval;
pub use guard_common::{GuardConfig, GuardLogLevel};
pub use io_utils::{
    check_disk_space, classify_and_convert_io_error, classify_io_error, ensure_temp_dir,
    get_available_space, validate_path_characters, IoErrorKind, TempFileGuard,
    DEFAULT_DISK_SPACE_MARGIN,
};
pub use jitter::{check_probability_by_id, compute_backoff_with_jitter, compute_jitter_delay};
pub use lifecycle::{
    validate_deterministic_transition, LifecycleError, LifecycleState, LifecycleTransition,
    SemanticVersion, TransitionReason,
};
pub use path_normalization::{
    compare_paths_deterministic, normalize_path_for_sorting, normalize_path_str,
};
pub use path_security::{
    is_forbidden_tmp_path, is_forbidden_tmp_path_str, reject_forbidden_tmp_path,
    reject_forbidden_tmp_path_like, FORBIDDEN_TMP_PREFIXES,
};
pub use path_utils::{
    absolutize_path, find_project_root, rebase_var_path, resolve_var_dir, resolve_var_tmp_dir,
    tempdir_in_var,
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
pub use preflight::{
    is_maintenance_mode, run_preflight, ActiveUniquenessResult, CheckStatus, PreflightAdapterData,
    PreflightAuditEvent, PreflightCheck, PreflightCheckFailure, PreflightConfig, PreflightDbOps,
    PreflightErrorCode, PreflightResult, SimpleAdapterData,
};
pub use receipt_digest::{compute_output_digest, hash_token_decision, update_run_head};
pub use seed::{
    clear_seed_registry, derive_adapter_seed, derive_request_seed, derive_seed, derive_seed_full,
    derive_seed_indexed, derive_seed_typed, derive_typed_seed, derive_typed_seed_full,
    hash_adapter_dir, ExecutionProfile, SeedLabel, SeedMode, TypedSeed,
};
pub use seed_guard::SeedScopeGuard;
pub use stack::compute_stack_hash;
pub use status::{adapterOSStatus, HealthCheckResult, HealthStatus, ServiceStatus};
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
pub use tokenizer_config::SpecialTokenMap;
pub use training::{TrainingConfig, TrainingJob, TrainingJobStatus, TrainingTemplate};
pub use version::{
    AlgorithmVersionBundle, IncompatibilitySeverity, VersionIncompatibility, VersionInfo,
    HASH_ALGORITHM_VERSION, HKDF_ALGORITHM_VERSION, PARSER_ALGORITHM_VERSION,
    PATH_NORMALIZATION_VERSION,
};
pub use worker_status::{WorkerStatus, WorkerStatusTransition};

#[cfg(feature = "cache-attestation")]
pub use cache_attestation::{
    create_cache_attestation, CacheAttestation, CacheAttestationBuilder,
    CACHE_ATTESTATION_SCHEMA_VERSION,
};

/// RNG module version for determinism tracking
/// @deprecated Use `version::RNG_MODULE_VERSION` instead
pub const RNG_MODULE_VERSION: &str = "1.0.0-chacha20";

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{
        adapterOSStatus, bytes_to_gb, bytes_to_mb, gb_to_bytes, kb_to_bytes, mb_to_bytes,
        AdapterEvent, AdapterName, AdapterType, AosError, AuditEvent, B3Hash, BackendKind,
        CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics, CircuitState, DriftPolicy,
        EventHookType, ExecutionProfile, ForkType, HealthCheckResult, HealthStatus, InferenceEvent,
        LifecycleState, LifecycleTransition, MetricsTickEvent, ModelFormat, ModelImportStatus,
        ObservabilityEvent, ObservabilityEventKind, ObservabilitySeverity, Plugin, PluginConfig,
        PluginEvent, PluginHealth, PluginStatus, PolicyViolationEvent, Result, ResultExt, SeedMode,
        SemanticVersion, ServiceStatus, SharedCircuitBreaker, StackName, StandardCircuitBreaker,
        TrainingConfig, TrainingJob, TrainingJobEvent, TrainingJobStatus, TrainingTemplate,
        TransitionReason, VersionInfo, WorkerStatus, WorkerStatusTransition, BYTES_PER_GB,
        BYTES_PER_KB, BYTES_PER_MB, CPID, DEFAULT_TIMEOUT_SECS, DETERMINISM_VIOLATION_METRIC,
        SLOW_TIMEOUT_SECS,
    };

    // Recovery orchestrator types
    pub use crate::recovery::{
        FallbackConfig, RecoveryClassifier, RecoveryConfig, RecoveryError, RecoveryOrchestrator,
        RecoveryOrchestratorBuilder, RecoveryOutcome, RecoveryStats,
    };
}
