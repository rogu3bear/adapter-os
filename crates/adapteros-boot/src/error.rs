//! Boot-specific error types for the lifecycle builder and worker auth.

use crate::phase::BootPhase;

/// Errors that can occur during boot lifecycle management.
#[derive(Debug, thiserror::Error)]
pub enum BootError {
    /// Invalid phase transition attempted
    #[error("Invalid phase transition from {from:?} to {to:?}")]
    InvalidTransition { from: BootPhase, to: BootPhase },

    /// Phase timeout exceeded
    #[error("Timeout during {phase:?} phase")]
    Timeout { phase: BootPhase },

    /// Database connection failed
    #[error("Database connection failed: {0}")]
    DbConnection(String),

    /// Migration failed
    #[error("Migration failed: {0}")]
    Migration(String),

    /// Missing required dependency
    #[error("Missing required dependency: {0}")]
    MissingDependency(String),

    /// Key loading/generation failed
    #[error("Key operation failed: {0}")]
    KeyLoad(String),

    /// Boot report write failed
    #[error("Failed to write boot report: {0}")]
    ReportWrite(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Policy loading failed
    #[error("Policy loading failed: {0}")]
    PolicyLoad(String),

    /// Backend initialization failed
    #[error("Backend initialization failed: {0}")]
    BackendInit(String),

    /// Model loading failed
    #[error("Model loading failed: {0}")]
    ModelLoad(String),

    /// Worker discovery failed
    #[error("Worker discovery failed: {0}")]
    WorkerDiscovery(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Errors specific to worker authentication.
#[derive(Debug, thiserror::Error)]
pub enum WorkerAuthError {
    /// Invalid token format (not a valid JWT structure)
    #[error("Invalid token format")]
    InvalidFormat,

    /// Signature verification failed
    #[error("Invalid signature")]
    InvalidSignature,

    /// Token has expired
    #[error("Token expired")]
    Expired,

    /// Token not yet valid (nbf claim in future)
    #[error("Token not yet valid")]
    NotYetValid,

    /// Invalid issuer claim
    #[error("Invalid issuer: expected 'control-plane'")]
    InvalidIssuer,

    /// Invalid audience claim
    #[error("Invalid audience: expected 'worker'")]
    InvalidAudience,

    /// Replay attack detected (jti already seen)
    #[error("Replay detected: token already used")]
    ReplayDetected,

    /// Worker ID mismatch - token was generated for a different worker
    #[error("Worker ID mismatch: token generated for '{expected}' but received by '{got}'. Check load balancer routing configuration.")]
    WorkerIdMismatch {
        /// The worker ID that was expected (this worker's ID)
        expected: String,
        /// The worker ID in the token
        got: String,
    },

    /// Base64 decoding failed
    #[error("Base64 decode error: {0}")]
    Base64Decode(String),

    /// JSON parsing failed
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Key operation failed
    #[error("Key error: {0}")]
    KeyError(String),

    /// Keypair file is corrupted (wrong size or invalid data)
    #[error("Corrupted keypair file at '{path}': {reason}. In non-strict mode, the key will be regenerated. In strict mode, delete the file and restart, or use --regenerate-keys to force regeneration.")]
    CorruptedKeypair {
        /// Path to the corrupted keypair file
        path: String,
        /// Reason for corruption (e.g., "expected 32 bytes, got 16")
        reason: String,
    },

    /// Key file not found (transient error for container startup)
    #[error("Key file not found: {0}")]
    KeyNotFound(String),

    /// Empty request ID (JTI) not allowed
    #[error("Request ID (JTI) cannot be empty")]
    EmptyRequestId,

    /// Empty worker ID not allowed
    #[error("Worker ID cannot be empty")]
    EmptyWorkerId,

    /// TTL too short (must be at least 1 second)
    #[error("TTL must be at least {min} seconds, got {actual}")]
    TtlTooShort {
        /// Minimum required TTL
        min: u64,
        /// Actual TTL provided
        actual: u64,
    },

    /// Worker ID too long
    #[error("Worker ID exceeds maximum length of {max} characters, got {actual}")]
    WorkerIdTooLong {
        /// Maximum allowed length
        max: usize,
        /// Actual length provided
        actual: usize,
    },

    /// Request ID too long
    #[error("Request ID exceeds maximum length of {max} characters, got {actual}")]
    RequestIdTooLong {
        /// Maximum allowed length
        max: usize,
        /// Actual length provided
        actual: usize,
    },

    /// Invalid characters in identifier (must be ASCII printable)
    #[error("Identifier contains invalid characters (must be ASCII printable): {field}")]
    InvalidIdentifierChars {
        /// The field name that contains invalid characters
        field: String,
    },

    /// Unknown key ID in token header
    #[error("Unknown key ID: {0}. Key may have expired or rotation not synced.")]
    UnknownKeyId(String),
}

/// Result type alias for boot operations.
pub type BootResult<T> = Result<T, BootError>;

/// Result type alias for worker auth operations.
pub type WorkerAuthResult<T> = Result<T, WorkerAuthError>;
