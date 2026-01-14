//! Reproducible replay: Re-execute inference using a receipt as the specification.
//!
//! This module validates the determinism guarantee by:
//! 1. Extracting input tokens and configuration from an original receipt
//! 2. Re-executing inference with identical inputs and configuration
//! 3. Comparing the replay receipt digest against the original
//!
//! Divergence indicates non-determinism or configuration drift.
//!
//! # Stop Conditions
//! - Replay execution completes (success or verified divergence)
//! - Required model/adapter version unavailable
//! - Divergence detected mid-execution
//!
//! # Next Conditions
//! - On match: reproducibility verified
//! - On mismatch: report divergence with diagnostics
//! - On unavailable version: report replay impossible

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use adapteros_core::B3Hash;

/// Error types for reproducible replay operations
#[derive(Error, Debug)]
pub enum ReproducibleReplayError {
    /// Required model version is not available
    #[error("Model version unavailable: {model_id} (hash: {expected_hash})")]
    ModelUnavailable {
        model_id: String,
        expected_hash: String,
    },

    /// Required adapter version is not available
    #[error("Adapter version unavailable: {adapter_id} (hash: {expected_hash})")]
    AdapterUnavailable {
        adapter_id: String,
        expected_hash: String,
    },

    /// Configuration cannot be extracted from receipt
    #[error("Invalid receipt: {reason}")]
    InvalidReceipt { reason: String },

    /// Execution failed during replay
    #[error("Replay execution failed: {reason}")]
    ExecutionFailed { reason: String },

    /// Divergence detected mid-execution
    #[error("Divergence detected at token {token_index}: {reason}")]
    DivergenceDetected { token_index: u32, reason: String },

    /// I/O error during replay
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Result type for reproducible replay operations
pub type ReproducibleReplayResult<T> = Result<T, ReproducibleReplayError>;

/// Specification extracted from an original receipt for replay.
///
/// Contains all inputs and configuration needed to reproduce the inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproducibleReplaySpec {
    /// Original receipt digest (BLAKE3 hex)
    pub original_receipt_digest: String,

    /// Context digest from original (BLAKE3 hex)
    pub context_digest: String,

    /// Input tokens extracted from receipt/storage
    pub input_tokens: Vec<u32>,

    /// Model configuration
    pub model: ModelSpec,

    /// Adapter configurations (in routing order)
    pub adapters: Vec<AdapterSpec>,

    /// Sampling parameters
    pub sampling_params: SamplingParams,

    /// Backend that must be used for deterministic replay
    pub required_backend: String,

    /// Manifest hash that must match
    pub required_manifest_hash: String,

    /// Request seed for deterministic execution
    pub request_seed: Option<[u8; 32]>,

    /// Router seed (hex string) for deterministic adapter selection
    pub router_seed: Option<String>,

    /// Stop policy configuration (if any)
    pub stop_policy: Option<serde_json::Value>,

    /// Schema version used for receipt computation
    pub receipt_schema_version: u8,

    /// Expected output tokens (for verification, optional)
    pub expected_output_tokens: Option<Vec<u32>>,

    /// Expected run_head hash (BLAKE3 hex) for chain verification
    pub expected_run_head_hash: Option<String>,
}

/// Model specification for reproducible replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Model identifier
    pub id: String,
    /// Content hash (BLAKE3 hex) for version verification
    pub hash: String,
}

/// Adapter specification for reproducible replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSpec {
    /// Adapter identifier
    pub id: String,
    /// Content hash (BLAKE3 hex) for version verification
    pub hash: String,
    /// Gate value (Q15 format) used in original inference
    pub gate_q15: Option<i16>,
}

/// Sampling parameters for deterministic replay
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SamplingParams {
    pub temperature: Option<f32>,
    pub top_k: Option<u32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub seed: Option<u64>,
}

/// Result of a reproducible replay execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayVerificationResult {
    /// Whether the replay produced an identical receipt
    pub verified: bool,

    /// Match status
    pub status: VerificationStatus,

    /// Original receipt digest (BLAKE3 hex)
    pub original_receipt_digest: String,

    /// Replay receipt digest (BLAKE3 hex)
    pub replay_receipt_digest: String,

    /// Output tokens from replay execution
    pub replay_output_tokens: Vec<u32>,

    /// Divergence diagnostics (if not verified)
    pub diagnostics: Option<DivergenceDiagnostics>,

    /// Execution statistics
    pub stats: ReplayExecutionStats,
}

impl ReplayVerificationResult {
    /// Create a verified (matching) result
    pub fn verified(
        original_digest: String,
        replay_digest: String,
        output_tokens: Vec<u32>,
        stats: ReplayExecutionStats,
    ) -> Self {
        Self {
            verified: true,
            status: VerificationStatus::Verified,
            original_receipt_digest: original_digest,
            replay_receipt_digest: replay_digest,
            replay_output_tokens: output_tokens,
            diagnostics: None,
            stats,
        }
    }

    /// Create a divergent (non-matching) result
    pub fn divergent(
        original_digest: String,
        replay_digest: String,
        output_tokens: Vec<u32>,
        diagnostics: DivergenceDiagnostics,
        stats: ReplayExecutionStats,
    ) -> Self {
        Self {
            verified: false,
            status: VerificationStatus::Divergent,
            original_receipt_digest: original_digest,
            replay_receipt_digest: replay_digest,
            replay_output_tokens: output_tokens,
            diagnostics: Some(diagnostics),
            stats,
        }
    }

    /// Create an unavailable result (replay impossible)
    pub fn unavailable(original_digest: String, reason: String) -> Self {
        Self {
            verified: false,
            status: VerificationStatus::Unavailable,
            original_receipt_digest: original_digest,
            replay_receipt_digest: String::new(),
            replay_output_tokens: Vec::new(),
            diagnostics: Some(DivergenceDiagnostics {
                divergence_type: DivergenceType::ConfigurationDrift,
                first_divergent_token: None,
                output_digest_match: false,
                run_head_match: false,
                context_digest_match: false,
                field_mismatches: vec![FieldMismatch {
                    field: "availability".to_string(),
                    expected: "available".to_string(),
                    actual: reason,
                }],
                possible_causes: vec!["Required model or adapter version is not available".to_string()],
            }),
            stats: ReplayExecutionStats::default(),
        }
    }
}

/// Verification status after replay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// Receipt digests match exactly - reproducibility verified
    Verified,
    /// Receipt digests differ - divergence detected
    Divergent,
    /// Replay impossible (missing model/adapter version)
    Unavailable,
}

impl fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Verified => write!(f, "verified"),
            Self::Divergent => write!(f, "divergent"),
            Self::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// Detailed diagnostics for divergence cases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergenceDiagnostics {
    /// Type of divergence detected
    pub divergence_type: DivergenceType,

    /// First token index where divergence was detected (if applicable)
    pub first_divergent_token: Option<u32>,

    /// Whether output digest matched
    pub output_digest_match: bool,

    /// Whether run_head hash chain matched
    pub run_head_match: bool,

    /// Whether context digest matched
    pub context_digest_match: bool,

    /// Specific field mismatches in the receipt
    pub field_mismatches: Vec<FieldMismatch>,

    /// Possible causes of the divergence
    pub possible_causes: Vec<String>,
}

impl DivergenceDiagnostics {
    /// Create diagnostics for an output divergence
    pub fn output_divergence(
        first_divergent_token: u32,
        expected_output_digest: &str,
        actual_output_digest: &str,
    ) -> Self {
        Self {
            divergence_type: DivergenceType::OutputMismatch,
            first_divergent_token: Some(first_divergent_token),
            output_digest_match: false,
            run_head_match: false,
            context_digest_match: true,
            field_mismatches: vec![FieldMismatch {
                field: "output_digest".to_string(),
                expected: expected_output_digest.to_string(),
                actual: actual_output_digest.to_string(),
            }],
            possible_causes: vec![
                "Non-deterministic model execution".to_string(),
                "Different backend implementation".to_string(),
                "Floating point precision differences".to_string(),
            ],
        }
    }

    /// Create diagnostics for a routing divergence
    pub fn routing_divergence(expected_adapters: &[String], actual_adapters: &[String]) -> Self {
        Self {
            divergence_type: DivergenceType::RoutingMismatch,
            first_divergent_token: Some(0),
            output_digest_match: false,
            run_head_match: false,
            context_digest_match: true,
            field_mismatches: vec![FieldMismatch {
                field: "adapter_selection".to_string(),
                expected: expected_adapters.join(","),
                actual: actual_adapters.join(","),
            }],
            possible_causes: vec![
                "Router seed not preserved".to_string(),
                "Adapter availability changed".to_string(),
                "Router algorithm changed".to_string(),
            ],
        }
    }

    /// Create diagnostics for a context mismatch
    pub fn context_mismatch(expected: &str, actual: &str) -> Self {
        Self {
            divergence_type: DivergenceType::ConfigurationDrift,
            first_divergent_token: None,
            output_digest_match: false,
            run_head_match: false,
            context_digest_match: false,
            field_mismatches: vec![FieldMismatch {
                field: "context_digest".to_string(),
                expected: expected.to_string(),
                actual: actual.to_string(),
            }],
            possible_causes: vec![
                "Model or adapter content changed".to_string(),
                "Policy mask changed".to_string(),
                "Backend configuration changed".to_string(),
            ],
        }
    }
}

/// Type of divergence detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceType {
    /// Output tokens differ from original
    OutputMismatch,
    /// Adapter routing produced different selection
    RoutingMismatch,
    /// Run head hash chain diverged
    RunHeadDivergence,
    /// Configuration drift (model/adapter/policy changed)
    ConfigurationDrift,
    /// Stop condition triggered at different point
    StopConditionMismatch,
}

impl fmt::Display for DivergenceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputMismatch => write!(f, "output_mismatch"),
            Self::RoutingMismatch => write!(f, "routing_mismatch"),
            Self::RunHeadDivergence => write!(f, "run_head_divergence"),
            Self::ConfigurationDrift => write!(f, "configuration_drift"),
            Self::StopConditionMismatch => write!(f, "stop_condition_mismatch"),
        }
    }
}

/// A specific field mismatch in receipt comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMismatch {
    /// Field name that mismatched
    pub field: String,
    /// Expected value (from original receipt)
    pub expected: String,
    /// Actual value (from replay receipt)
    pub actual: String,
}

/// Execution statistics for replay
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayExecutionStats {
    /// Total replay execution time in milliseconds
    pub execution_time_ms: u64,
    /// Number of tokens generated
    pub tokens_generated: u32,
    /// Number of hash verifications performed
    pub hash_verifications: u32,
    /// Original execution time (if known)
    pub original_execution_time_ms: Option<u64>,
}

/// Compare two receipt digests and determine verification status
pub fn compare_receipt_digests(original: &B3Hash, replay: &B3Hash) -> VerificationStatus {
    if original == replay {
        VerificationStatus::Verified
    } else {
        VerificationStatus::Divergent
    }
}

/// Compare receipt digests given hex strings
pub fn compare_receipt_digests_hex(original_hex: &str, replay_hex: &str) -> VerificationStatus {
    if original_hex == replay_hex {
        VerificationStatus::Verified
    } else {
        VerificationStatus::Divergent
    }
}

/// Check if a model is available for replay
pub trait ModelAvailabilityChecker {
    /// Check if a model with the given ID and hash is available
    fn is_model_available(&self, model_id: &str, expected_hash: &str) -> bool;
}

/// Check if adapters are available for replay
pub trait AdapterAvailabilityChecker {
    /// Check if an adapter with the given ID and hash is available
    fn is_adapter_available(&self, adapter_id: &str, expected_hash: &str) -> bool;
}

/// Executor for reproducible replay
///
/// This trait defines the interface for executing a reproducible replay.
/// Implementations should use the provided spec to execute inference
/// and return the verification result.
pub trait ReproducibleReplayExecutor {
    /// Execute reproducible replay from the given specification
    fn execute(
        &self,
        spec: &ReproducibleReplaySpec,
    ) -> impl std::future::Future<Output = ReproducibleReplayResult<ReplayVerificationResult>> + Send;

    /// Check if replay is possible (all required versions available)
    fn check_availability(
        &self,
        spec: &ReproducibleReplaySpec,
    ) -> impl std::future::Future<Output = ReproducibleReplayResult<AvailabilityCheckResult>> + Send;
}

/// Result of an availability check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityCheckResult {
    /// Whether replay is possible
    pub available: bool,
    /// Model availability status
    pub model_available: bool,
    /// Adapter availability status (true if all adapters available)
    pub adapters_available: bool,
    /// Backend availability status
    pub backend_available: bool,
    /// List of unavailable components
    pub unavailable_components: Vec<UnavailableComponent>,
}

impl AvailabilityCheckResult {
    /// Create a fully available result
    pub fn all_available() -> Self {
        Self {
            available: true,
            model_available: true,
            adapters_available: true,
            backend_available: true,
            unavailable_components: Vec::new(),
        }
    }

    /// Create an unavailable result
    pub fn unavailable(components: Vec<UnavailableComponent>) -> Self {
        let model_available = !components.iter().any(|c| matches!(c.component_type, ComponentType::Model));
        let adapters_available = !components.iter().any(|c| matches!(c.component_type, ComponentType::Adapter));
        let backend_available = !components.iter().any(|c| matches!(c.component_type, ComponentType::Backend));

        Self {
            available: false,
            model_available,
            adapters_available,
            backend_available,
            unavailable_components: components,
        }
    }
}

/// An unavailable component for replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnavailableComponent {
    /// Type of component
    pub component_type: ComponentType,
    /// Component identifier
    pub id: String,
    /// Expected hash (if applicable)
    pub expected_hash: Option<String>,
    /// Reason for unavailability
    pub reason: String,
}

/// Type of component required for replay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentType {
    Model,
    Adapter,
    Backend,
    Manifest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_status_display() {
        assert_eq!(VerificationStatus::Verified.to_string(), "verified");
        assert_eq!(VerificationStatus::Divergent.to_string(), "divergent");
        assert_eq!(VerificationStatus::Unavailable.to_string(), "unavailable");
    }

    #[test]
    fn test_divergence_type_display() {
        assert_eq!(DivergenceType::OutputMismatch.to_string(), "output_mismatch");
        assert_eq!(DivergenceType::RoutingMismatch.to_string(), "routing_mismatch");
        assert_eq!(DivergenceType::RunHeadDivergence.to_string(), "run_head_divergence");
        assert_eq!(DivergenceType::ConfigurationDrift.to_string(), "configuration_drift");
        assert_eq!(DivergenceType::StopConditionMismatch.to_string(), "stop_condition_mismatch");
    }

    #[test]
    fn test_compare_receipt_digests_hex_match() {
        let digest = "abc123def456";
        assert_eq!(
            compare_receipt_digests_hex(digest, digest),
            VerificationStatus::Verified
        );
    }

    #[test]
    fn test_compare_receipt_digests_hex_mismatch() {
        assert_eq!(
            compare_receipt_digests_hex("abc123", "def456"),
            VerificationStatus::Divergent
        );
    }

    #[test]
    fn test_replay_verification_result_verified() {
        let result = ReplayVerificationResult::verified(
            "orig".to_string(),
            "orig".to_string(),
            vec![1, 2, 3],
            ReplayExecutionStats::default(),
        );
        assert!(result.verified);
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(result.diagnostics.is_none());
    }

    #[test]
    fn test_replay_verification_result_divergent() {
        let diagnostics = DivergenceDiagnostics::output_divergence(5, "expected", "actual");
        let result = ReplayVerificationResult::divergent(
            "orig".to_string(),
            "replay".to_string(),
            vec![1, 2, 3, 4, 5],
            diagnostics,
            ReplayExecutionStats::default(),
        );
        assert!(!result.verified);
        assert_eq!(result.status, VerificationStatus::Divergent);
        assert!(result.diagnostics.is_some());
        let diag = result.diagnostics.unwrap();
        assert_eq!(diag.divergence_type, DivergenceType::OutputMismatch);
        assert_eq!(diag.first_divergent_token, Some(5));
    }

    #[test]
    fn test_replay_verification_result_unavailable() {
        let result = ReplayVerificationResult::unavailable(
            "orig".to_string(),
            "Model qwen2.5-7b not found".to_string(),
        );
        assert!(!result.verified);
        assert_eq!(result.status, VerificationStatus::Unavailable);
    }

    #[test]
    fn test_availability_check_all_available() {
        let result = AvailabilityCheckResult::all_available();
        assert!(result.available);
        assert!(result.model_available);
        assert!(result.adapters_available);
        assert!(result.backend_available);
        assert!(result.unavailable_components.is_empty());
    }

    #[test]
    fn test_availability_check_model_unavailable() {
        let components = vec![UnavailableComponent {
            component_type: ComponentType::Model,
            id: "qwen2.5-7b".to_string(),
            expected_hash: Some("abc123".to_string()),
            reason: "Model archived".to_string(),
        }];
        let result = AvailabilityCheckResult::unavailable(components);
        assert!(!result.available);
        assert!(!result.model_available);
        assert!(result.adapters_available);
        assert!(result.backend_available);
    }

    #[test]
    fn test_diagnostics_output_divergence() {
        let diag = DivergenceDiagnostics::output_divergence(10, "exp", "act");
        assert_eq!(diag.divergence_type, DivergenceType::OutputMismatch);
        assert_eq!(diag.first_divergent_token, Some(10));
        assert!(!diag.output_digest_match);
        assert!(diag.context_digest_match);
        assert!(!diag.possible_causes.is_empty());
    }

    #[test]
    fn test_diagnostics_routing_divergence() {
        let expected = vec!["adapter-a".to_string(), "adapter-b".to_string()];
        let actual = vec!["adapter-c".to_string()];
        let diag = DivergenceDiagnostics::routing_divergence(&expected, &actual);
        assert_eq!(diag.divergence_type, DivergenceType::RoutingMismatch);
        assert_eq!(diag.first_divergent_token, Some(0));
        assert_eq!(diag.field_mismatches[0].field, "adapter_selection");
    }

    #[test]
    fn test_diagnostics_context_mismatch() {
        let diag = DivergenceDiagnostics::context_mismatch("expected_ctx", "actual_ctx");
        assert_eq!(diag.divergence_type, DivergenceType::ConfigurationDrift);
        assert!(!diag.context_digest_match);
        assert!(diag.first_divergent_token.is_none());
    }

    #[test]
    fn test_sampling_params_default() {
        let params = SamplingParams::default();
        assert!(params.temperature.is_none());
        assert!(params.top_k.is_none());
        assert!(params.top_p.is_none());
        assert!(params.max_tokens.is_none());
        assert!(params.seed.is_none());
    }

    #[test]
    fn test_reproducible_replay_spec_serialization() {
        let spec = ReproducibleReplaySpec {
            original_receipt_digest: "abc123".to_string(),
            context_digest: "def456".to_string(),
            input_tokens: vec![1, 2, 3],
            model: ModelSpec {
                id: "qwen2.5-7b".to_string(),
                hash: "modelhash".to_string(),
            },
            adapters: vec![AdapterSpec {
                id: "code-adapter".to_string(),
                hash: "adapterhash".to_string(),
                gate_q15: Some(16384),
            }],
            sampling_params: SamplingParams {
                temperature: Some(0.7),
                max_tokens: Some(100),
                ..Default::default()
            },
            required_backend: "mlx".to_string(),
            required_manifest_hash: "manifesthash".to_string(),
            request_seed: None,
            router_seed: Some("routerseed".to_string()),
            stop_policy: None,
            receipt_schema_version: 5,
            expected_output_tokens: Some(vec![4, 5, 6]),
            expected_run_head_hash: Some("runhead".to_string()),
        };

        let json = serde_json::to_string(&spec).expect("serialization failed");
        let parsed: ReproducibleReplaySpec = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(parsed.original_receipt_digest, spec.original_receipt_digest);
        assert_eq!(parsed.input_tokens, spec.input_tokens);
        assert_eq!(parsed.model.id, spec.model.id);
        assert_eq!(parsed.adapters.len(), 1);
        assert_eq!(parsed.receipt_schema_version, 5);
    }
}
