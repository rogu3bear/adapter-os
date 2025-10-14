//! Error types for AdapterOS

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AosError>;

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

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Resource exhaustion: {0}")]
    ResourceExhaustion(String),

    #[error("Memory pressure: {0}")]
    MemoryPressure(String),

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("Resource unavailable: {0}")]
    Unavailable(String),

    #[error("Performance violation: {0}")]
    PerformanceViolation(String),

    #[error("RAG error: {0}")]
    Rag(String),

    #[error("Lifecycle error: {0}")]
    Lifecycle(String),

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

    #[error("Anomaly detected: {0}")]
    Anomaly(String),

    #[error("Promotion error: {0}")]
    Promotion(String),

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
    InvalidResponse {
        reason: String,
    },

    #[error("Feature disabled: {feature} - {reason}")]
    FeatureDisabled {
        feature: String,
        reason: String,
        alternative: Option<String>,
    },

    #[error("Worker not responding at {path}")]
    WorkerNotResponding {
        path: std::path::PathBuf,
    },

    #[error("Timeout waiting for response after {duration:?}")]
    Timeout {
        duration: std::time::Duration,
    },

    #[error("Encryption failed: {reason}")]
    EncryptionFailed {
        reason: String,
    },

    #[error("Decryption failed: {reason}")]
    DecryptionFailed {
        reason: String,
    },

    #[error("Invalid sealed data: {reason}")]
    InvalidSealedData {
        reason: String,
    },

    #[error("Database error: {operation}")]
    DatabaseError {
        operation: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("{0}")]
    Other(String),
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

// Conversion from DeterministicExecutorError
impl From<adapteros_deterministic_exec::DeterministicExecutorError> for AosError {
    fn from(err: adapteros_deterministic_exec::DeterministicExecutorError) -> Self {
        AosError::DeterministicExecutor(err.to_string())
    }
}

// Conversion from sqlx::Error for database operations
#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for AosError {
    fn from(err: sqlx::Error) -> Self {
        AosError::Sqlx(err.to_string())
    }
}
