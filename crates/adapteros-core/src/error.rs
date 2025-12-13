//! Error types for AdapterOS
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
use thiserror::Error;
use zip::result::ZipError;

pub type Result<T> = std::result::Result<T, AosError>;

/// Core error type for AdapterOS operations
///
/// All errors in AdapterOS should use this enum to ensure consistent
/// error handling, logging, and user experience.
///
/// [source: crates/adapteros-core/src/error.rs L38-388]
/// [source: CLAUDE.md#error-handling]
/// [source: docs/ARCHITECTURE_INDEX.md#error-handling]
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

    #[error("MLX error: {0}")]
    Mlx(String),

    #[error("Worker error: {0}")]
    Worker(String),

    #[error("Telemetry error: {0}")]
    Telemetry(String),

    #[error("Determinism violation: {0}")]
    DeterminismViolation(String),

    #[error("Egress violation: {0}")]
    EgressViolation(String),

    #[error("Isolation violation: {0}")]
    IsolationViolation(String),

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

    #[error("Resource unavailable: {0}")]
    Unavailable(String),

    #[error("Performance violation: {0}")]
    PerformanceViolation(String),

    #[error("RAG error: {0}")]
    Rag(String),

    #[error("Lifecycle error: {0}")]
    Lifecycle(String),

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
    #[error("Adapter '{adapter_id}' not found in manifest. Available adapters: {available:?}")]
    AdapterNotInManifest {
        adapter_id: String,
        available: Vec<String>,
    },

    /// Requested adapter is not part of the effective adapter set
    #[error("Adapter '{adapter_id}' is not in the effective adapter set: {effective_set:?}")]
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

    #[error("{0}")]
    Other(String),

    #[error("{context}: {source}")]
    WithContext {
        context: String,
        #[source]
        source: Box<AosError>,
    },
}

// Rusqlite conversions removed to avoid conflicts with sqlx
// If needed, implement these conversions in aos-registry directly

// Conversion from anyhow for CLI commands
impl From<anyhow::Error> for AosError {
    fn from(err: anyhow::Error) -> Self {
        AosError::Other(err.to_string())
    }
}

// Conversion from rusqlite for aos-registry
impl From<rusqlite::Error> for AosError {
    fn from(err: rusqlite::Error) -> Self {
        AosError::Sqlite(err.to_string())
    }
}

// Conversion from std::io::Error
impl From<std::io::Error> for AosError {
    fn from(err: std::io::Error) -> Self {
        AosError::Io(err.to_string())
    }
}

// Conversion from DeterministicExecutorError removed to avoid circular dependency

// Conversion from sqlx::Error for database operations
#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for AosError {
    fn from(err: sqlx::Error) -> Self {
        AosError::Sqlx(err.to_string())
    }
}

// Conversion from ZipError for archive operations (zip v1.x)
impl From<ZipError> for AosError {
    fn from(err: ZipError) -> Self {
        AosError::Io(format!("Zip operation failed: {}", err))
    }
}

// Note: DeterministicExecutorError conversion avoided to prevent circular dependency
// Handle in calling code with manual mapping

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_chaining() {
        let base: Result<()> = Err(AosError::Other("boom".to_string()));

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
                            AosError::Other(ref s) => assert_eq!(s, "boom"),
                            _ => panic!("expected innermost Other variant"),
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
}
