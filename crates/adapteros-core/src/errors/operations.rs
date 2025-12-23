//! Domain-specific operational errors
//!
//! Covers specialized subsystems: telemetry, federation, routing, workers, jobs, etc.

use thiserror::Error;

/// Domain-specific operational errors
#[derive(Error, Debug)]
pub enum AosOperationsError {
    /// Replay/verification error
    #[error("Replay error: {0}")]
    Replay(String),

    /// Verification error
    #[error("Verification error: {0}")]
    Verification(String),

    /// Plan execution error
    #[error("Plan error: {0}")]
    Plan(String),

    /// Worker process error
    #[error("Worker error: {0}")]
    Worker(String),

    /// Telemetry error
    #[error("Telemetry error: {0}")]
    Telemetry(String),

    /// Node error
    #[error("Node error: {0}")]
    Node(String),

    /// Job execution error
    #[error("Job error: {0}")]
    Job(String),

    /// RAG (retrieval-augmented generation) error
    #[error("RAG error: {0}")]
    Rag(String),

    /// Git operation error
    #[error("Git error: {0}")]
    Git(String),

    /// Promotion error
    #[error("Promotion error: {0}")]
    Promotion(String),

    /// Routing error
    #[error("Routing error: {0}")]
    Routing(String),

    /// Federation error
    #[error("Federation error: {0}")]
    Federation(String),

    /// Feature disabled
    #[error("Feature disabled: {feature} - {reason}")]
    FeatureDisabled {
        feature: String,
        reason: String,
        alternative: Option<String>,
    },

    /// Generic not found error
    #[error("Not found: {0}")]
    NotFound(String),
}
