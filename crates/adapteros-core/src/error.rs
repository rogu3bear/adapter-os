//! Error types for adapterOS
//!
//! ## Error Message Standards
//!
//! When creating error messages, follow these conventions:
//!
//! 1. **Capitalization**: Start with a capital letter
//!    - ✅ `"Failed to load adapter: file not found"`
//!    - ❌ `"failed to load adapter: file not found"`
//!
//! 2. **Format**: Use "Action failed: reason" or "Entity state: details"
//!    - ✅ `"Dataset not found: {id}"`
//!    - ✅ `"Invalid configuration: vocab_size must be non-zero"`
//!    - ❌ `"dataset not found"`
//!
//! 3. **Dynamic values**: Use `format!()` for interpolation
//!    - ✅ `AosError::Config(format!("Port {} already in use", port))`
//!    - ❌ `AosError::Config("port already in use".to_string())`
//!
//! 4. **Static messages**: Use `.to_string()` without interpolation
//!    - ✅ `AosError::Config("Database connection required".to_string())`
//!
//! 5. **No trailing periods**: Error strings should not end with periods
//!    - ✅ `"Connection timeout after 30s"`
//!    - ❌ `"Connection timeout after 30s."`
//!
//! 6. **Be specific and actionable**: Include enough context to debug
//!    - ✅ `"Failed to verify GPU buffers for adapter 'code-review': hash mismatch"`
//!    - ❌ `"Verification failed"`

use crate::B3Hash;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zip::result::ZipError;

pub type Result<T> = std::result::Result<T, AosError>;

impl From<adapteros_infra_common::AosError> for AosError {
    fn from(err: adapteros_infra_common::AosError) -> Self {
        match err {
            adapteros_infra_common::AosError::InvalidHash(s) => AosError::InvalidHash(s),
            adapteros_infra_common::AosError::InvalidCPID(s) => AosError::InvalidCPID(s),
            adapteros_infra_common::AosError::Parse(s) => AosError::Parse(s),
            adapteros_infra_common::AosError::Validation(s) => AosError::Validation(s),
        }
    }
}

/// Core error type for adapterOS operations
///
/// All errors in adapterOS should use this enum to ensure consistent
/// error handling, logging, and user experience.
///
/// [source: crates/adapteros-core/src/error.rs L38-388]
/// [source: AGENTS.md#error-handling]
/// [source: docs/ERRORS.md#error-handling]
#[derive(Error, Debug)]
pub enum AosError {
    #[error("Invalid hash: {0}")]
    InvalidHash(String),

    #[error("Invalid CPID: {0}")]
    InvalidCPID(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Policy error: {0}")]
    Policy(String),

    #[error("Metal error: {0}")]
    Mtl(String),

    #[error("Replay error: {0}")]
    Replay(String),

    #[error("Verification error: {0}")]
    Verification(String),

    #[error("SQLx database error: {0}")]
    Sqlx(String),

    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("Registry error: {0}")]
    Registry(String),

    #[error("SQLite error: {0}")]
    Sqlite(String),

    #[error("Artifact error: {0}")]
    Artifact(String),

    #[error("Plan error: {0}")]
    Plan(String),

    #[error("Kernel error: {0}")]
    Kernel(String),

    #[error("Kernel layout mismatch for tensor '{tensor}': expected {expected}, got {got}")]
    KernelLayoutMismatch {
        tensor: String,
        expected: String,
        got: String,
    },

    #[error("CoreML error: {0}")]
    CoreML(String),

    /// CoreML export encountered unsupported operations in the model
    #[error("CoreML export unsupported ops in {model_path}: {ops:?}")]
    CoreMLUnsupportedOps {
        model_path: String,
        ops: Vec<String>,
    },

    /// CoreML package is missing required weight files
    #[error("CoreML package missing weights at {package_path}: {missing:?}")]
    CoreMLMissingWeights {
        package_path: String,
        missing: Vec<String>,
    },

    /// CoreML export destination already exists and is non-empty
    #[error("CoreML export path exists: {path} (contains {file_count} items)")]
    CoreMLExportPathExists { path: String, file_count: usize },

    /// CoreML export operation timed out
    #[error("CoreML export timeout after {duration:?}: {operation}")]
    CoreMLExportTimeout {
        operation: String,
        duration: std::time::Duration,
    },

    /// LoRA shape mismatch during fusion
    #[error("LoRA shape mismatch at layer {layer} {target}: A expected {expected_a} got {got_a}, B expected {expected_b} got {got_b}")]
    LoraShapeMismatch {
        layer: usize,
        target: String,
        expected_a: usize,
        got_a: usize,
        expected_b: usize,
        got_b: usize,
    },

    #[error("MLX error: {0}")]
    Mlx(String),

    #[error("Worker error: {0}")]
    Worker(String),

    #[error("Reasoning loop detected: {0}")]
    ReasoningLoop(String),

    #[error("Telemetry error: {0}")]
    Telemetry(String),

    #[error("Determinism violation: {0}")]
    DeterminismViolation(String),

    #[error("Egress violation: {0}")]
    EgressViolation(String),

    #[error("Isolation violation: {0}")]
    IsolationViolation(String),

    #[error("Integrity violation: {0}")]
    IntegrityViolation(String),

    #[error("Chat template error: {0}")]
    ChatTemplate(String),

    #[error("Quantization error: {0}")]
    Quantization(String),

    #[error("Node error: {0}")]
    Node(String),

    #[error("Job error: {0}")]
    Job(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Authorization error: {0}")]
    Authz(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Resource exhaustion: {0}")]
    ResourceExhaustion(String),

    #[error("Memory pressure: {0}")]
    MemoryPressure(String),

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("Quota exceeded for resource '{resource}'")]
    QuotaExceeded {
        resource: String,
        /// Failure code string (e.g., "KV_QUOTA_EXCEEDED")
        failure_code: Option<String>,
    },

    /// Model cache budget exceeded during eviction
    ///
    /// This error occurs when the model cache cannot free enough memory
    /// to accommodate a new model load, typically because entries are
    /// pinned (base models) or active (in-flight inference).
    #[error("Model cache budget exceeded: needed {needed_mb} MB, freed {freed_mb} MB (pinned={pinned_count}, active={active_count}), max {max_mb} MB")]
    CacheBudgetExceeded {
        /// Memory needed in megabytes
        needed_mb: u64,
        /// Memory freed during eviction attempt in megabytes
        freed_mb: u64,
        /// Number of pinned entries that blocked eviction
        pinned_count: usize,
        /// Number of active entries that blocked eviction
        active_count: usize,
        /// Maximum cache budget in megabytes
        max_mb: u64,
        /// Optional model key identifier (for diagnostics)
        model_key: Option<String>,
    },

    #[error("Resource unavailable: {0}")]
    Unavailable(String),

    /// CPU usage exceeds limits and throttles the process
    #[error("CPU throttled: {reason} (usage: {usage_percent:.1}%, limit: {limit_percent:.1}%)")]
    CpuThrottled {
        /// Human-readable reason for throttling
        reason: String,
        /// Current CPU usage percentage
        usage_percent: f32,
        /// Configured CPU limit percentage
        limit_percent: f32,
        /// Recommended backoff duration in milliseconds
        backoff_ms: u64,
    },

    /// Memory usage hits OOM and the service may restart
    #[error("Out of memory: {reason} (used: {used_mb} MB, limit: {limit_mb} MB)")]
    OutOfMemory {
        /// Human-readable reason for OOM
        reason: String,
        /// Current memory usage in MB
        used_mb: u64,
        /// Memory limit in MB
        limit_mb: u64,
        /// Whether service restart is imminent
        restart_imminent: bool,
    },

    /// File descriptor limit is reached
    #[error("File descriptor limit reached: {current}/{limit} descriptors in use")]
    FileDescriptorExhausted {
        /// Current number of open file descriptors
        current: u64,
        /// Maximum allowed file descriptors
        limit: u64,
        /// Suggested action to resolve
        suggestion: String,
    },

    /// Thread pool is saturated
    #[error("Thread pool saturated: {active}/{max} threads busy, {queued} tasks queued")]
    ThreadPoolSaturated {
        /// Number of currently active threads
        active: usize,
        /// Maximum thread pool size
        max: usize,
        /// Number of tasks waiting in queue
        queued: usize,
        /// Estimated wait time in milliseconds
        estimated_wait_ms: u64,
    },

    /// GPU device is unavailable
    #[error("GPU unavailable: {reason}")]
    GpuUnavailable {
        /// Human-readable reason for unavailability
        reason: String,
        /// Device identifier if known
        device_id: Option<String>,
        /// Whether fallback to CPU is possible
        cpu_fallback_available: bool,
        /// Whether this is a transient condition that may recover
        is_transient: bool,
    },

    #[error("Performance violation: {0}")]
    PerformanceViolation(String),

    #[error("RAG error: {0}")]
    Rag(String),

    #[error("Lifecycle error: {0}")]
    Lifecycle(String),

    /// Preflight checks failed before an operation could proceed
    ///
    /// This error is returned when an operation (like adapter swap) is blocked
    /// by preflight validation checks. The message contains details about which
    /// checks failed and suggested remediation steps.
    #[error("Preflight failed: {0}")]
    PreflightFailed(String),

    /// Adapter is not loaded or not in a ready state for inference
    ///
    /// This error is returned when inference is attempted on an adapter
    /// that is in Unloaded or Cold state, rather than Warm, Hot, or Resident.
    #[error("Adapter not loaded: {adapter_id} is in {current_state} state (requires: warm, hot, or resident)")]
    AdapterNotLoaded {
        adapter_id: String,
        current_state: String,
    },

    /// Requested adapter is not present in the current manifest
    #[error("Adapter '{adapter_id}' not found in worker manifest. The adapter does not exist in the loaded model configuration. Available adapters in manifest: {available:?}. To fix this, ensure the adapter is registered in the manifest and the worker has loaded the correct manifest version.")]
    AdapterNotInManifest {
        adapter_id: String,
        available: Vec<String>,
    },

    /// Requested adapter is not part of the effective adapter set
    #[error("Adapter '{adapter_id}' is not in the effective adapter set. Effective adapter gates restrict which adapters can be used for this request. The adapter must be in the effective set: {effective_set:?}. To fix this, either add the adapter to the effective_adapter_ids list in your request, or remove it from pinned_adapter_ids.")]
    AdapterNotInEffectiveSet {
        adapter_id: String,
        effective_set: Vec<String>,
    },

    #[error("Adapter hash mismatch for {adapter_id}: expected {expected}, got {actual}")]
    AdapterHashMismatch {
        adapter_id: String,
        expected: B3Hash,
        actual: B3Hash,
    },

    #[error("Segment hash mismatch for {segment_id}")]
    SegmentHashMismatch { segment_id: u32 },

    #[error("Missing segment for backend '{backend}' and scope '{scope_path}'")]
    MissingSegment { backend: String, scope_path: String },

    #[error("Missing canonical segment (corrupted / needs retrain)")]
    MissingCanonicalSegment,

    #[error(
        "Per-layer hash mismatch for {adapter_id} at {layer_id}: expected {expected}, got {actual}"
    )]
    AdapterLayerHashMismatch {
        adapter_id: String,
        layer_id: String,
        expected: B3Hash,
        actual: B3Hash,
    },

    #[error("Git error: {0}")]
    Git(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Training error: {0}")]
    Training(String),

    #[error("Autograd error: {0}")]
    Autograd(String),

    #[error("Toolchain error: {0}")]
    Toolchain(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Deterministic executor error: {0}")]
    DeterministicExecutor(String),

    #[error("Base LLM error: {0}")]
    BaseLLM(String),

    #[error("System error: {0}")]
    System(String),

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("Anomaly detected: {0}")]
    Anomaly(String),

    #[error("Promotion error: {0}")]
    Promotion(String),

    #[error("System quarantined due to policy hash violations: {0}")]
    Quarantined(String),

    #[error("Policy hash mismatch for {pack_id}: expected {expected}, got {actual}")]
    PolicyHashMismatch {
        pack_id: String,
        expected: String,
        actual: String,
    },

    #[error("RNG error [seed:{seed_hash}|label:{label}|counter:{counter}]: {message}")]
    RngError {
        seed_hash: String,
        label: String,
        counter: u64,
        message: String,
    },

    #[error("UDS connection failed: {path}")]
    UdsConnectionFailed {
        path: std::path::PathBuf,
        #[source]
        source: anyhow::Error,
    },

    #[error("Invalid response from worker: {reason}")]
    InvalidResponse { reason: String },

    #[error("Feature disabled: {feature} - {reason}")]
    FeatureDisabled {
        feature: String,
        reason: String,
        alternative: Option<String>,
    },

    #[error("Worker not responding at {path}")]
    WorkerNotResponding { path: std::path::PathBuf },

    #[error("Timeout waiting for response after {duration:?}")]
    Timeout { duration: std::time::Duration },

    #[error("Circuit breaker is open for service '{service}'")]
    CircuitBreakerOpen { service: String },

    #[error("Circuit breaker is half-open for service '{service}'")]
    CircuitBreakerHalfOpen { service: String },

    /// Optimistic locking conflict - resource was modified by another process
    ///
    /// This error indicates a TOCTOU (time-of-check to time-of-use) race condition
    /// was detected. The caller should retry with fresh state.
    /// Maps to HTTP 409 Conflict.
    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Encryption failed: {reason}")]
    EncryptionFailed { reason: String },

    #[error("Decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    #[error("Invalid sealed data: {reason}")]
    InvalidSealedData { reason: String },

    #[error("Database error: {operation}")]
    DatabaseError {
        operation: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("Dual-write inconsistency for {entity_type} '{entity_id}': {reason}")]
    DualWriteInconsistency {
        entity_type: String,
        entity_id: String,
        reason: String,
    },

    #[error("Routing error: {0}")]
    Routing(String),

    #[error("Federation error: {0}")]
    Federation(String),

    #[error("Download failed for {repo_id}: {reason}")]
    DownloadFailed {
        repo_id: String,
        reason: String,
        is_resumable: bool,
    },

    #[error("Cache corruption at {path}: expected hash {expected}, got {actual}")]
    CacheCorruption {
        path: String,
        expected: String,
        actual: String,
    },

    /// Disk is full (ENOSPC) or quota exceeded (EDQUOT)
    ///
    /// This error occurs when the filesystem cannot allocate more space for a write
    /// operation. The error includes available space information when possible.
    #[error("Disk full at {path}: {details}")]
    DiskFull {
        /// Path where the write operation failed
        path: String,
        /// Human-readable description of the failure
        details: String,
        /// Bytes needed for the operation (if known)
        bytes_needed: Option<u64>,
        /// Bytes available on the filesystem (if known)
        bytes_available: Option<u64>,
    },

    /// Temporary directory does not exist or is inaccessible
    ///
    /// This error occurs when operations requiring a temporary directory fail
    /// because the directory doesn't exist, isn't writable, or can't be created.
    #[error("Temporary directory unavailable: {path} - {reason}")]
    TempDirUnavailable {
        /// Path to the temporary directory
        path: String,
        /// Reason why the directory is unavailable
        reason: String,
    },

    /// File permission denied (EACCES/EPERM)
    ///
    /// This error occurs when a file operation is denied due to insufficient
    /// permissions. The error includes the operation that was attempted and
    /// whether a permission fix was attempted.
    #[error("Permission denied for {path}: {operation} - {reason}")]
    PermissionDenied {
        /// Path to the file or directory
        path: String,
        /// Operation that was denied (e.g., "open", "create", "read", "write")
        operation: String,
        /// Detailed reason for the denial
        reason: String,
    },

    /// File path contains invalid characters for the operating system
    ///
    /// This error occurs when a file path contains characters that are not
    /// allowed on the current operating system (e.g., NUL on Unix, <>:"|?* on Windows).
    #[error("Invalid path characters in '{path}': {details}")]
    InvalidPathCharacters {
        /// The path containing invalid characters
        path: String,
        /// Description of the validation failure
        details: String,
        /// List of invalid characters found
        invalid_chars: Vec<char>,
    },

    /// File watcher events dropped due to channel overflow
    ///
    /// This error occurs when the file watcher's internal channel is full and
    /// events cannot be queued. A rescan may be triggered automatically.
    #[error("File watcher dropped {count} events in {window_secs}s")]
    WatcherEventsDropped {
        /// Number of events dropped
        count: u64,
        /// Time window in seconds over which events were dropped
        window_secs: u64,
    },

    #[error("Health check failed for model {model_id}: {reason} (attempt {retry_count})")]
    HealthCheckFailed {
        model_id: String,
        reason: String,
        retry_count: u32,
    },

    #[error("Model not found: {model_id}")]
    ModelNotFound { model_id: String },

    #[error("Model acquisition in progress: {model_id} is {state}")]
    ModelAcquisitionInProgress { model_id: String, state: String },

    #[error("{context}: {source}")]
    WithContext {
        context: String,
        #[source]
        source: Box<AosError>,
    },

    // =========================================================================
    // Build/Toolchain errors (Category 20)
    // =========================================================================
    /// Build toolchain version mismatch with CI
    #[error("Toolchain mismatch: {component} - expected {expected}, got {actual}")]
    ToolchainMismatch {
        /// Component that mismatches (e.g., "rust", "metal_sdk", "macos")
        component: String,
        /// Expected version
        expected: String,
        /// Actual version found
        actual: String,
        /// CI version if available
        ci_version: Option<String>,
    },

    /// Build cache is stale and may hide errors
    #[error("Build cache stale: {path} - {reason}")]
    StaleBuildCache {
        /// Path to the stale cache
        path: String,
        /// Reason why it's considered stale
        reason: String,
        /// Last modification time if available
        last_modified: Option<String>,
    },

    /// Lint step skipped due to missing target
    #[error("Lint target missing: {target} - run `cargo build --target {target}` first")]
    LintTargetMissing {
        /// The missing target
        target: String,
        /// The lint command that failed
        lint_command: String,
    },

    /// Cargo.lock is out of sync with Cargo.toml
    #[error("Cargo.lock out of sync with Cargo.toml: {details}")]
    LockfileOutOfSync {
        /// Description of the sync issue
        details: String,
        /// Affected crate name if known
        crate_name: Option<String>,
        /// Version in Cargo.toml
        toml_version: Option<String>,
        /// Version in Cargo.lock
        lock_version: Option<String>,
    },

    /// Workspace member path is invalid
    #[error("Workspace member path invalid: {member} at {path} - {reason}")]
    WorkspaceMemberPathInvalid {
        /// Workspace member name
        member: String,
        /// Path that is invalid
        path: String,
        /// Reason for the invalidity
        reason: String,
    },

    // =========================================================================
    // CLI errors (Category 21)
    // =========================================================================
    /// CLI uses a deprecated flag
    #[error(
        "Deprecated flag: --{flag} - use {replacement} instead (removed in {removal_version})"
    )]
    DeprecatedFlag {
        /// The deprecated flag name
        flag: String,
        /// The replacement flag or option
        replacement: String,
        /// Version when the flag will be removed
        removal_version: String,
    },

    /// CLI output format changed without version bump
    #[error(
        "CLI output format changed: {format} - expected schema version {expected}, got {actual}"
    )]
    OutputFormatMismatch {
        /// Output format (e.g., "json", "yaml", "table")
        format: String,
        /// Expected schema version
        expected: String,
        /// Actual schema version
        actual: String,
    },

    /// CLI cannot write to specified output directory
    #[error("Cannot write to {path}: {reason}")]
    CliWritePermissionDenied {
        /// Path that cannot be written to
        path: String,
        /// Reason for the denial
        reason: String,
        /// Operation that was attempted
        operation: String,
    },

    /// CLI received binary input when UTF-8 was expected
    #[error("Invalid input encoding: expected UTF-8, received binary data at byte {offset}")]
    InvalidInputEncoding {
        /// Byte offset where invalid encoding was found
        offset: usize,
        /// Context describing where input came from
        context: String,
        /// Suggested flag to use for binary input
        suggested_flag: Option<String>,
    },

    /// CLI attempted to retry a non-retriable error
    #[error("Retried non-retriable error: {error_type} - {reason}")]
    InvalidRetryAttempt {
        /// Type of the original error
        error_type: String,
        /// Reason why it's not retriable
        reason: String,
        /// The original error message
        original_error: String,
    },

    // =========================================================================
    // Rate limiting errors (Category 23)
    // =========================================================================
    /// Rate limiter configuration is missing
    #[error("Rate limiter not configured: {reason}")]
    RateLimiterNotConfigured {
        /// Reason for the missing configuration
        reason: String,
        /// Which limiter is affected
        limiter_name: String,
    },

    /// Rate limiter configuration is invalid
    #[error("Invalid rate limit config: {reason}")]
    InvalidRateLimitConfig {
        /// Reason for the invalid configuration
        reason: String,
        /// The invalid parameter
        parameter: String,
        /// The invalid value
        value: String,
    },

    /// Request rejected due to thundering herd protection
    #[error("Thundering herd rejected: {reason}")]
    ThunderingHerdRejected {
        /// Reason for the rejection
        reason: String,
        /// Recommended retry delay in milliseconds
        retry_after_ms: u64,
    },
}

// ============================================================================
// Convenience constructors for common error patterns
// ============================================================================

impl AosError {
    /// Validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        AosError::Validation(msg.into())
    }

    /// Configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        AosError::Config(msg.into())
    }

    /// I/O error
    pub fn io(msg: impl Into<String>) -> Self {
        AosError::Io(msg.into())
    }

    /// Cryptographic error
    pub fn crypto(msg: impl Into<String>) -> Self {
        AosError::Crypto(msg.into())
    }

    /// Network error
    pub fn network(msg: impl Into<String>) -> Self {
        AosError::Network(msg.into())
    }

    /// HTTP error
    pub fn http(msg: impl Into<String>) -> Self {
        AosError::Http(msg.into())
    }

    /// Timeout error
    pub fn timeout(duration: std::time::Duration) -> Self {
        AosError::Timeout { duration }
    }

    /// Lifecycle error
    pub fn lifecycle(msg: impl Into<String>) -> Self {
        AosError::Lifecycle(msg.into())
    }

    /// Determinism violation
    pub fn determinism_violation(msg: impl Into<String>) -> Self {
        AosError::DeterminismViolation(msg.into())
    }

    /// Internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        AosError::Internal(msg.into())
    }

    /// Database error
    pub fn database(msg: impl Into<String>) -> Self {
        AosError::Database(msg.into())
    }

    /// Not found error
    pub fn not_found(msg: impl Into<String>) -> Self {
        AosError::NotFound(msg.into())
    }

    /// Worker error
    pub fn worker(msg: impl Into<String>) -> Self {
        AosError::Worker(msg.into())
    }

    /// Reasoning loop detected
    pub fn reasoning_loop(msg: impl Into<String>) -> Self {
        AosError::ReasoningLoop(msg.into())
    }

    /// Policy violation
    pub fn policy_violation(msg: impl Into<String>) -> Self {
        AosError::PolicyViolation(msg.into())
    }

    /// Integrity violation (tamper detection, hash mismatch)
    pub fn integrity_violation(msg: impl Into<String>) -> Self {
        AosError::IntegrityViolation(msg.into())
    }

    /// Circuit breaker open
    pub fn circuit_breaker_open(service: impl Into<String>) -> Self {
        AosError::CircuitBreakerOpen {
            service: service.into(),
        }
    }

    /// Dual-write inconsistency error (SQL committed but KV failed)
    pub fn dual_write_inconsistency(
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        AosError::DualWriteInconsistency {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            reason: reason.into(),
        }
    }

    /// Disk full error
    pub fn disk_full(
        path: impl Into<String>,
        details: impl Into<String>,
        bytes_needed: Option<u64>,
        bytes_available: Option<u64>,
    ) -> Self {
        AosError::DiskFull {
            path: path.into(),
            details: details.into(),
            bytes_needed,
            bytes_available,
        }
    }

    /// Temporary directory unavailable error
    pub fn temp_dir_unavailable(path: impl Into<String>, reason: impl Into<String>) -> Self {
        AosError::TempDirUnavailable {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Permission denied error
    pub fn permission_denied(
        path: impl Into<String>,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        AosError::PermissionDenied {
            path: path.into(),
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Invalid path characters error
    pub fn invalid_path_characters(
        path: impl Into<String>,
        details: impl Into<String>,
        invalid_chars: Vec<char>,
    ) -> Self {
        AosError::InvalidPathCharacters {
            path: path.into(),
            details: details.into(),
            invalid_chars,
        }
    }

    /// File watcher events dropped error
    pub fn watcher_events_dropped(count: u64, window_secs: u64) -> Self {
        AosError::WatcherEventsDropped { count, window_secs }
    }

    /// CoreML unsupported ops error
    pub fn coreml_unsupported_ops(model_path: impl Into<String>, ops: Vec<String>) -> Self {
        AosError::CoreMLUnsupportedOps {
            model_path: model_path.into(),
            ops,
        }
    }

    /// CoreML missing weights error
    pub fn coreml_missing_weights(package_path: impl Into<String>, missing: Vec<String>) -> Self {
        AosError::CoreMLMissingWeights {
            package_path: package_path.into(),
            missing,
        }
    }

    /// CoreML export path exists error
    pub fn coreml_export_path_exists(path: impl Into<String>, file_count: usize) -> Self {
        AosError::CoreMLExportPathExists {
            path: path.into(),
            file_count,
        }
    }

    /// CoreML export timeout error
    pub fn coreml_export_timeout(
        operation: impl Into<String>,
        duration: std::time::Duration,
    ) -> Self {
        AosError::CoreMLExportTimeout {
            operation: operation.into(),
            duration,
        }
    }

    /// LoRA shape mismatch error
    pub fn lora_shape_mismatch(
        layer: usize,
        target: impl Into<String>,
        expected_a: usize,
        got_a: usize,
        expected_b: usize,
        got_b: usize,
    ) -> Self {
        AosError::LoraShapeMismatch {
            layer,
            target: target.into(),
            expected_a,
            got_a,
            expected_b,
            got_b,
        }
    }

    // =========================================================================
    // Build/Toolchain error constructors (Category 20)
    // =========================================================================

    /// Toolchain version mismatch error
    pub fn toolchain_mismatch(
        component: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
        ci_version: Option<String>,
    ) -> Self {
        AosError::ToolchainMismatch {
            component: component.into(),
            expected: expected.into(),
            actual: actual.into(),
            ci_version,
        }
    }

    /// Stale build cache error
    pub fn stale_build_cache(
        path: impl Into<String>,
        reason: impl Into<String>,
        last_modified: Option<String>,
    ) -> Self {
        AosError::StaleBuildCache {
            path: path.into(),
            reason: reason.into(),
            last_modified,
        }
    }

    /// Lockfile out of sync error
    pub fn lockfile_out_of_sync(details: impl Into<String>) -> Self {
        AosError::LockfileOutOfSync {
            details: details.into(),
            crate_name: None,
            toml_version: None,
            lock_version: None,
        }
    }

    // =========================================================================
    // CLI error constructors (Category 21)
    // =========================================================================

    /// Deprecated flag error
    pub fn deprecated_flag(
        flag: impl Into<String>,
        replacement: impl Into<String>,
        removal_version: impl Into<String>,
    ) -> Self {
        AosError::DeprecatedFlag {
            flag: flag.into(),
            replacement: replacement.into(),
            removal_version: removal_version.into(),
        }
    }

    /// Invalid input encoding error
    pub fn invalid_input_encoding(
        offset: usize,
        context: impl Into<String>,
        suggested_flag: Option<String>,
    ) -> Self {
        AosError::InvalidInputEncoding {
            offset,
            context: context.into(),
            suggested_flag,
        }
    }

    /// CLI write permission denied error
    pub fn cli_write_permission_denied(
        path: impl Into<String>,
        reason: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        AosError::CliWritePermissionDenied {
            path: path.into(),
            reason: reason.into(),
            operation: operation.into(),
        }
    }

    // =========================================================================
    // Rate limiting error constructors (Category 23)
    // =========================================================================

    /// Rate limiter not configured error
    pub fn rate_limiter_not_configured(
        limiter_name: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        AosError::RateLimiterNotConfigured {
            limiter_name: limiter_name.into(),
            reason: reason.into(),
        }
    }

    /// Invalid rate limit config error
    pub fn invalid_rate_limit_config(
        parameter: impl Into<String>,
        value: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        AosError::InvalidRateLimitConfig {
            parameter: parameter.into(),
            value: value.into(),
            reason: reason.into(),
        }
    }

    /// Thundering herd rejection error
    pub fn thundering_herd_rejected(reason: impl Into<String>, retry_after_ms: u64) -> Self {
        AosError::ThunderingHerdRejected {
            reason: reason.into(),
            retry_after_ms,
        }
    }
}

/// Serializable representation of cache budget exceeded error
///
/// This struct is used to transport cache budget error details across
/// serialization boundaries (e.g., UDS, HTTP responses) where the full
/// `AosError` type cannot be directly serialized.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheBudgetExceededInfo {
    /// Memory needed in megabytes
    pub needed_mb: u64,
    /// Memory freed during eviction attempt in megabytes
    pub freed_mb: u64,
    /// Number of pinned entries that blocked eviction
    pub pinned_count: usize,
    /// Number of active entries that blocked eviction
    pub active_count: usize,
    /// Maximum cache budget in megabytes
    pub max_mb: u64,
    /// Optional model key identifier (for diagnostics)
    pub model_key: Option<String>,
}

impl CacheBudgetExceededInfo {
    /// Extract info from an `AosError::CacheBudgetExceeded` variant
    ///
    /// Returns `None` if the error is not a `CacheBudgetExceeded` variant.
    pub fn from_error(e: &AosError) -> Option<Self> {
        match e {
            AosError::CacheBudgetExceeded {
                needed_mb,
                freed_mb,
                pinned_count,
                active_count,
                max_mb,
                model_key,
            } => Some(Self {
                needed_mb: *needed_mb,
                freed_mb: *freed_mb,
                pinned_count: *pinned_count,
                active_count: *active_count,
                max_mb: *max_mb,
                model_key: model_key.clone(),
            }),
            _ => None,
        }
    }
}

// Rusqlite conversions removed to avoid conflicts with sqlx
// If needed, implement these conversions in aos-registry directly

// ============================================================================
// Error Chain Preservation
// ============================================================================
//
// These `From` implementations preserve the full error chain by capturing
// all causes in the error message. This is critical for debugging as it
// prevents losing context when errors are converted.
//
// The pattern used is:
// 1. Capture the root error message
// 2. Walk the error chain via `.source()` or anyhow's `.chain()`
// 3. Format as "root cause -> inner cause -> ... -> leaf cause"
//
// For structured error handling with proper `#[source]` support, use the
// structured variants like `UdsConnectionFailed`, `DatabaseError`, or
// `WithContext` which preserve the actual error objects.

/// Helper to format an error chain into a single string.
///
/// Walks the error's source chain and joins all messages with " -> ".
/// This preserves context that would otherwise be lost during error conversion.
fn format_error_chain(err: &dyn std::error::Error) -> String {
    let mut chain = vec![err.to_string()];
    let mut current = err.source();
    while let Some(cause) = current {
        chain.push(cause.to_string());
        current = cause.source();
    }
    chain.join(" -> ")
}

// Conversion from anyhow for CLI commands
//
// anyhow::Error provides `.chain()` which is more efficient than walking
// `.source()` since it's already collected.
impl From<anyhow::Error> for AosError {
    fn from(err: anyhow::Error) -> Self {
        let chain: String = err
            .chain()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        AosError::Internal(chain)
    }
}

// ============================================================================
// Error conversions using impl_error_from! macro
// ============================================================================
//
// Simple error conversions use the impl_error_from! macro to reduce boilerplate.
// The macro is defined in crates/adapteros-core/src/error_macros.rs

// Conversion from rusqlite for aos-registry
crate::impl_error_from!(rusqlite::Error => Sqlite, chain);

// Conversion from std::io::Error
crate::impl_error_from!(std::io::Error => Io, chain);

// Conversion from AosValidationError for validation-specific errors
impl From<crate::errors::AosValidationError> for AosError {
    fn from(err: crate::errors::AosValidationError) -> Self {
        use crate::errors::AosValidationError;
        match err {
            AosValidationError::ConfigFileNotFound { path, .. } => {
                AosError::Config(format!("Config file not found: {}", path))
            }
            AosValidationError::ConfigFilePermissionDenied { path, reason } => AosError::Config(
                format!("Config file permission denied: {} - {}", path, reason),
            ),
            AosValidationError::ConfigSchemaViolation {
                field,
                value,
                constraint,
                ..
            } => AosError::Config(format!(
                "Config schema violation: {} = '{}' - {}",
                field, value, constraint
            )),
            AosValidationError::EmptyEnvOverride { variable, .. } => AosError::Config(format!(
                "Empty environment override (empty or whitespace): {} - set a value or unset the variable",
                variable
            )),
            AosValidationError::BlankSecret { variable, reason } => {
                AosError::Config(format!("Invalid secret value for {}: {}", variable, reason))
            }
            // Map other validation errors to their base AosError types
            AosValidationError::Validation(msg) => AosError::Validation(msg),
            AosValidationError::InvalidManifest(msg) => AosError::InvalidManifest(msg),
            AosValidationError::Parse(msg) => AosError::Parse(msg),
            AosValidationError::Serialization(msg) => {
                AosError::Internal(format!("Serialization error: {}", msg))
            }
            AosValidationError::InvalidCPID(msg) => AosError::InvalidCPID(msg),
            AosValidationError::ChatTemplate(msg) => AosError::ChatTemplate(msg),
            AosValidationError::Config(msg) => AosError::Config(msg),
            AosValidationError::InvalidInput(msg) => AosError::Validation(msg),
            AosValidationError::MissingVersion => {
                AosError::Validation("Adapter version string is missing from metadata".to_string())
            }
            AosValidationError::UnknownManifestFields(fields) => {
                AosError::InvalidManifest(format!("Unknown required fields: {:?}", fields))
            }
            AosValidationError::TtlInPast => {
                AosError::Validation("Adapter pin TTL is in the past".to_string())
            }
            AosValidationError::MissingArtifacts(artifacts) => {
                AosError::Validation(format!("Missing required artifacts: {:?}", artifacts))
            }
        }
    }
}

// Conversion from DeterministicExecutorError removed to avoid circular dependency

// Conversion from sqlx::Error for database operations
#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for AosError {
    fn from(err: sqlx::Error) -> Self {
        AosError::Sqlx(format_error_chain(&err))
    }
}

// Conversion from ZipError for archive operations (zip v1.x)
crate::impl_error_from!(ZipError => Io, prefix = "Zip operation failed", chain);

// Note: DeterministicExecutorError conversion avoided to prevent circular dependency
// Handle in calling code with manual mapping

/// Format a full error chain into a human-readable string.
///
/// Walks the error's source chain and joins all messages with " -> ".
/// This is useful for logging or displaying errors with full context.
///
/// # Example
/// ```
/// use std::io;
/// use adapteros_core::error::error_chain_string;
///
/// let inner = io::Error::new(io::ErrorKind::NotFound, "file.txt");
/// let outer = io::Error::new(io::ErrorKind::Other, inner);
/// let chain = error_chain_string(&outer);
/// assert!(chain.contains(" -> "));
/// ```
pub fn error_chain_string(err: &dyn std::error::Error) -> String {
    format_error_chain(err)
}

/// Extension trait to attach context to results without disrupting error types
pub trait ResultExt<T> {
    fn context(self, ctx: impl Into<String>) -> Result<T>;
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, ctx: impl Into<String>) -> Result<T> {
        self.map_err(|e| AosError::WithContext {
            context: ctx.into(),
            source: Box::new(e),
        })
    }

    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.map_err(|e| AosError::WithContext {
            context: f(),
            source: Box::new(e),
        })
    }
}

// Manual implementation of JsonSchema for AosError to avoid complex trait bounds on variants
#[cfg(any(feature = "schemars", feature = "schemars-support"))]
impl schemars::JsonSchema for AosError {
    fn schema_name() -> String {
        "AosError".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        let mut schema = String::json_schema(gen).into_object();
        schema.metadata().description = Some("adapterOS error message".to_string());
        schema.into()
    }
}

// Manual implementation of ToSchema for AosError
#[cfg(feature = "utoipa")]
impl utoipa::ToSchema for AosError {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("AosError")
    }
}

#[cfg(feature = "utoipa")]
impl utoipa::PartialSchema for AosError {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::SchemaType::Type(
                utoipa::openapi::schema::Type::String,
            ))
            .description(Some("adapterOS error message".to_string()))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_chaining() {
        let base: Result<()> = Err(AosError::Internal("boom".to_string()));

        let err = base
            .context("while doing A")
            .with_context(|| "processing request".to_string())
            .unwrap_err();

        // Ensure nesting structure is present and ordered
        match &err {
            AosError::WithContext { context, source } => {
                assert_eq!(context, "processing request");
                match source.as_ref() {
                    AosError::WithContext { context, source } => {
                        assert_eq!(context, "while doing A");
                        match source.as_ref() {
                            AosError::Internal(ref s) => assert_eq!(s, "boom"),
                            _ => panic!("expected innermost Internal variant"),
                        }
                    }
                    _ => panic!("expected inner WithContext variant"),
                }
            }
            _ => panic!("expected outer WithContext variant"),
        }

        // Also validate Display formatting produces a sensible chain
        let display = format!("{}", err);
        assert!(display.contains("processing request:"));
        assert!(display.contains("while doing A:"));
        assert!(display.contains("boom"));
    }

    #[test]
    fn test_error_chain_string_single_error() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "config.toml");
        let chain = error_chain_string(&err);
        assert_eq!(chain, "config.toml");
    }

    // Custom error type for testing error chains
    #[derive(Debug)]
    struct ChainedError {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    }

    impl std::fmt::Display for ChainedError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for ChainedError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source
                .as_ref()
                .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
        }
    }

    #[test]
    fn test_error_chain_string_nested_errors() {
        // Create a proper chain with source
        let inner = ChainedError {
            message: "missing file".to_string(),
            source: None,
        };
        let outer = ChainedError {
            message: "failed to read config".to_string(),
            source: Some(Box::new(inner)),
        };
        let chain = error_chain_string(&outer);

        // Should contain both messages joined by " -> "
        assert!(chain.contains("missing file"), "chain was: {}", chain);
        assert!(
            chain.contains("failed to read config"),
            "chain was: {}",
            chain
        );
        assert!(chain.contains(" -> "), "chain was: {}", chain);
        // Verify order: outer first, then inner
        assert_eq!(chain, "failed to read config -> missing file");
    }

    #[test]
    fn test_io_error_conversion_preserves_chain() {
        // io::Error with a custom source that has its own source
        let inner = ChainedError {
            message: "database.sqlite not found".to_string(),
            source: None,
        };
        let outer = std::io::Error::other(inner);

        let aos_err: AosError = outer.into();

        match aos_err {
            AosError::Io(msg) => {
                // The outer message is the inner error's Display
                assert!(
                    msg.contains("database.sqlite"),
                    "inner cause should be preserved, got: {}",
                    msg
                );
            }
            _ => panic!("expected Io variant"),
        }
    }

    #[test]
    fn test_anyhow_error_conversion_preserves_chain() {
        // Build an anyhow error chain
        let root = anyhow::anyhow!("root cause");
        let middle = root.context("middle context");
        let outer = middle.context("outer context");

        let aos_err: AosError = outer.into();

        match aos_err {
            AosError::Internal(msg) => {
                assert!(
                    msg.contains("outer context"),
                    "outer context missing, got: {}",
                    msg
                );
                assert!(
                    msg.contains("middle context"),
                    "middle context missing, got: {}",
                    msg
                );
                assert!(
                    msg.contains("root cause"),
                    "root cause missing, got: {}",
                    msg
                );
                assert!(
                    msg.contains(" -> "),
                    "chain separator missing, got: {}",
                    msg
                );
            }
            _ => panic!("expected Internal variant"),
        }
    }

    #[test]
    fn test_error_chain_string_public_api() {
        // Test the public API function
        let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let formatted = error_chain_string(&err);
        assert_eq!(formatted, "access denied");
    }
}
