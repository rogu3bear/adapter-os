//! Citable artifact export types.
//!
//! Types for exporting an inference response as a standalone, verifiable
//! document suitable for reports, legal briefs, or evidence submission.
//!
//! The export bundles the response text, citations with source attribution,
//! adapter configuration, run receipt hash, and a verification link/QR payload.
//! Receipt verification uses the existing `receipt_verifier.rs` / `crypto_receipt.rs`
//! infrastructure.

use serde::{Deserialize, Serialize};

use crate::schema_version;

// =============================================================================
// Export Request / Response
// =============================================================================

/// Request to export an inference response as a citable artifact.
///
/// Maps to `POST /v1/inference/{trace_id}/export`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ExportArtifactRequest {
    /// Trace/request ID of the inference to export.
    pub trace_id: String,
    /// Desired output format.
    pub format: ExportFormat,
    /// Whether to include the full receipt verification data.
    #[serde(default = "default_true")]
    pub include_receipt: bool,
    /// Whether to include a QR code payload for offline verification.
    #[serde(default)]
    pub include_qr_payload: bool,
    /// Optional title for the exported document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional author attribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Output format for exported artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// Markdown document with embedded citation metadata.
    Markdown,
    /// JSON structured artifact (machine-readable).
    Json,
}

/// Exported citable artifact response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ExportArtifactResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Trace ID of the exported inference.
    pub trace_id: String,
    /// The rendered document content (Markdown or JSON string).
    pub content: String,
    /// MIME type of the content.
    pub content_type: String,
    /// Suggested filename for download.
    pub filename: String,
    /// BLAKE3 hash of the exported content (for integrity).
    pub content_hash_b3: String,
    /// Structured artifact metadata (always present regardless of format).
    pub artifact: CitableArtifact,
}

/// Structured citable artifact metadata.
///
/// Contains all the data needed to verify and attribute an AI-generated response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CitableArtifact {
    /// The inference response text.
    pub response_text: String,
    /// When the inference was executed.
    pub inferred_at: String,
    /// Citations with full attribution.
    pub citations: Vec<ExportCitation>,
    /// Adapter configuration used.
    pub adapter_config: AdapterConfigSummary,
    /// Receipt verification data (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptSummary>,
    /// QR code payload for offline verification (if requested).
    /// Contains a compact, URL-safe verification payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_payload: Option<String>,
    /// Verification URL for online receipt checking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_url: Option<String>,
}

/// Citation formatted for export with full source attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ExportCitation {
    /// Source document name.
    pub document_name: String,
    /// Source document path.
    pub file_path: String,
    /// Page number within the document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<u32>,
    /// Quoted text from the source.
    pub quote: String,
    /// Citation ID (BLAKE3 hash).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_id: Option<String>,
    /// Relevance score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance_score: Option<f64>,
    /// Source type: "rag" for retrieval, adapter ID for training lineage.
    pub source_type: String,
}

/// Adapter configuration summary for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AdapterConfigSummary {
    /// Adapter IDs used.
    pub adapter_ids: Vec<String>,
    /// Model identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Backend used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Stack ID (if using adapter stack).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
}

/// Receipt verification summary for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ReceiptSummary {
    /// Receipt digest (BLAKE3 hex).
    pub receipt_digest_b3: String,
    /// Run head hash (BLAKE3 hex).
    pub run_head_hash_b3: String,
    /// Output digest (BLAKE3 hex).
    pub output_digest_b3: String,
    /// Ed25519 signature of the receipt (hex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Schema version of the receipt.
    pub receipt_schema_version: String,
    /// Signing key ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_kid: Option<String>,
}
