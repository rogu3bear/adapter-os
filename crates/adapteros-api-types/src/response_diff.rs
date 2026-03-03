//! Response provenance diff types.
//!
//! Types for comparing two inference responses to show what changed and why.
//! Used when a user asks the same question twice and gets a different answer
//! due to adapter training, collection changes, or policy updates.
//!
//! The data for comparison comes from `InferResponse` fields: `adapters_used`,
//! `citations`, `trace_id`, `deterministic_receipt`, `rag_evidence`, and the
//! replay metadata stored per-inference.

use serde::{Deserialize, Serialize};

use crate::schema_version;

// =============================================================================
// Response Provenance (what shaped a single response)
// =============================================================================

/// Provenance snapshot of a single inference response.
///
/// Captures the key factors that influenced a response, extracted from
/// `InferResponse` and `InferenceReplayMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ResponseProvenance {
    /// Trace/request ID of this inference.
    pub trace_id: String,
    /// When this inference was executed.
    pub inferred_at: String,
    /// Adapter IDs used during inference.
    pub adapters_used: Vec<String>,
    /// RAG collection ID used (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    /// Number of RAG citations sourced.
    pub citation_count: usize,
    /// Citation source files referenced.
    pub citation_sources: Vec<String>,
    /// Backend used (coreml, mlx, metal, etc).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_used: Option<String>,
    /// Model identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Determinism mode applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism_mode: Option<String>,
    /// Policy mask digest (hex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_mask_digest_b3: Option<String>,
    /// Tokens generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_generated: Option<usize>,
}

// =============================================================================
// Provenance Diff (what changed between two responses)
// =============================================================================

/// Request to compare the provenance of two inferences.
///
/// Maps to `POST /v1/inference/diff`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ProvenanceDiffRequest {
    /// Trace ID of the earlier inference (baseline).
    pub baseline_trace_id: String,
    /// Trace ID of the later inference (comparison).
    pub comparison_trace_id: String,
}

/// Result of comparing two inference responses' provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ProvenanceDiffResult {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Provenance of the baseline response.
    pub baseline: ResponseProvenance,
    /// Provenance of the comparison response.
    pub comparison: ResponseProvenance,
    /// What changed between the two.
    pub changes: Vec<ProvenanceChange>,
    /// Overall verdict.
    pub verdict: DiffVerdict,
}

/// A single change between two response provenances.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ProvenanceChange {
    /// What category of change this is.
    pub kind: ProvenanceChangeKind,
    /// Human-readable description of the change.
    pub description: String,
    /// The old value (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    /// The new value (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
}

/// Category of provenance change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceChangeKind {
    /// An adapter was added between the two inferences.
    AdapterAdded,
    /// An adapter was removed.
    AdapterRemoved,
    /// A citation source was added.
    CitationSourceAdded,
    /// A citation source was removed.
    CitationSourceRemoved,
    /// The RAG collection changed.
    CollectionChanged,
    /// The backend changed.
    BackendChanged,
    /// The model changed.
    ModelChanged,
    /// The policy configuration changed.
    PolicyChanged,
    /// The determinism mode changed.
    DeterminismChanged,
}

/// Overall verdict of a provenance diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DiffVerdict {
    /// No provenance changes detected; difference is inherent model variation.
    Identical,
    /// Adapter configuration changed (training, addition, removal).
    AdaptersChanged,
    /// Knowledge/citation sources changed.
    SourcesChanged,
    /// Both adapters and sources changed.
    MultipleChanges,
    /// Infrastructure changed (backend, model, policy).
    InfrastructureChanged,
}
