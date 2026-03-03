//! Diagnostics API types for tenant-safe diagnostic run queries.
//!
//! These types support the /diag/* endpoints for querying diagnostic runs
//! and events with strict tenant isolation.

use serde::{Deserialize, Serialize};

/// Query parameters for listing diagnostic runs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema, utoipa::IntoParams))]
pub struct ListDiagRunsQuery {
    /// Return runs started after this Unix timestamp (milliseconds)
    pub since: Option<i64>,
    /// Maximum number of runs to return (default: 50, max: 200)
    pub limit: Option<u32>,
    /// Cursor for pagination (run ID to start after)
    pub after: Option<String>,
    /// Filter by status: running, completed, failed, cancelled
    pub status: Option<String>,
}

/// Query parameters for listing diagnostic events.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema, utoipa::IntoParams))]
pub struct ListDiagEventsQuery {
    /// Return events with sequence number > after_seq
    pub after_seq: Option<i64>,
    /// Maximum number of events to return (default: 100, max: 1000)
    pub limit: Option<u32>,
    /// Filter by event type (e.g., "stage_enter", "router_decision")
    pub event_type: Option<String>,
    /// Filter by severity: trace, debug, info, warn, error
    pub severity: Option<String>,
}

/// Freshness status for determinism diagnostics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeterminismFreshnessStatus {
    /// Latest check is within freshness threshold.
    Fresh,
    /// Latest check exists but is older than freshness threshold.
    Stale,
    /// No trustworthy timestamp is available.
    #[default]
    Unknown,
}

/// Machine-readable reason for determinism freshness classification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeterminismFreshnessReason {
    RecentRun,
    StaleLastRun,
    MissingLastRun,
    InvalidLastRunFormat,
    FutureLastRun,
    NoDeterminismChecks,
    QueryError,
}

/// Determinism check status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DeterminismStatusResponse {
    /// Last persisted determinism check timestamp.
    pub last_run: Option<String>,
    /// Determinism result: "pass" | "fail" | null.
    pub result: Option<String>,
    /// Number of runs used for latest determinism check.
    pub runs: Option<usize>,
    /// Divergence count for latest determinism check.
    pub divergences: Option<usize>,
    /// Freshness classification for determinism status.
    pub freshness_status: DeterminismFreshnessStatus,
    /// Machine-readable reason for freshness status.
    pub freshness_reason: DeterminismFreshnessReason,
    /// Age of `last_run` in seconds when parseable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freshness_age_seconds: Option<i64>,
    /// Threshold in seconds after which a check is stale.
    pub stale_after_seconds: i64,
}

/// Summary of a diagnostic run (no sensitive content).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagRunResponse {
    /// Unique run identifier
    pub id: String,
    /// Trace ID for correlation
    pub trace_id: String,
    /// Run status: running, completed, failed, cancelled
    pub status: String,
    /// When the run started (Unix timestamp in milliseconds)
    pub started_at_unix_ms: i64,
    /// When the run completed (Unix timestamp in milliseconds), if completed
    pub completed_at_unix_ms: Option<i64>,
    /// Hash of the request (for correlation, no content)
    pub request_hash: String,
    /// Whether the request hash has been cryptographically verified
    #[serde(default)]
    pub request_hash_verified: Option<bool>,
    /// Hash of the manifest used
    pub manifest_hash: Option<String>,
    /// Whether the manifest hash has been cryptographically verified
    #[serde(default)]
    pub manifest_hash_verified: Option<bool>,
    /// Total number of events in this run
    pub total_events_count: i64,
    /// Number of events dropped due to buffer overflow
    pub dropped_events_count: i64,
    /// Duration in milliseconds (if completed)
    pub duration_ms: Option<i64>,
    /// When this record was created
    pub created_at: String,
}

/// Diagnostic event (sanitized - no prompt/output content).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagEventResponse {
    /// Event sequence number within the run
    pub seq: i64,
    /// Monotonic timestamp in microseconds
    pub mono_us: i64,
    /// Event type (e.g., "stage_enter", "router_decision", "policy_check")
    pub event_type: String,
    /// Severity level: trace, debug, info, warn, error
    pub severity: String,
    /// Sanitized event payload (no prompt/output content)
    pub payload: serde_json::Value,
}

/// Response for listing diagnostic runs with pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListDiagRunsResponse {
    /// Schema version for API compatibility
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// List of diagnostic runs
    pub runs: Vec<DiagRunResponse>,
    /// Total count of matching runs (for pagination UI)
    pub total_count: i64,
    /// Next cursor for pagination (last run ID, if more results exist)
    pub next_cursor: Option<String>,
    /// Whether more results are available
    pub has_more: bool,
}

/// Response for listing diagnostic events with pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListDiagEventsResponse {
    /// Schema version for API compatibility
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// List of diagnostic events
    pub events: Vec<DiagEventResponse>,
    /// Last sequence number in this page (for cursor-based pagination)
    pub last_seq: Option<i64>,
    /// Whether more events are available
    pub has_more: bool,
}

/// Request body for comparing two diagnostic runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagDiffRequest {
    /// First trace ID to compare (baseline)
    pub trace_id_a: String,
    /// Second trace ID to compare (comparison)
    pub trace_id_b: String,
    /// Whether to include timing differences
    #[serde(default = "default_true")]
    pub include_timing: bool,
    /// Whether to include event-by-event comparison
    #[serde(default = "default_true")]
    pub include_events: bool,
    /// Whether to include router step comparison (for deterministic divergence)
    #[serde(default = "default_true")]
    pub include_router_steps: bool,
}

fn default_true() -> bool {
    true
}

/// Difference between two diagnostic runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagDiffResponse {
    /// Schema version for API compatibility
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// Run A summary
    pub run_a: DiagRunResponse,
    /// Run B summary
    pub run_b: DiagRunResponse,
    /// Anchor comparison (deterministic hashes)
    pub anchor_comparison: AnchorComparison,
    /// First divergence point (if any)
    pub first_divergence: Option<FirstDivergence>,
    /// Overall diff summary
    pub summary: DiagDiffSummary,
    /// Event-level differences (if requested)
    pub event_diffs: Option<Vec<EventDiff>>,
    /// Timing differences (if requested)
    pub timing_diffs: Option<Vec<TimingDiff>>,
    /// Router step differences (if requested and divergence found)
    pub router_step_diffs: Option<Vec<RouterStepDiff>>,
}

/// Comparison of deterministic anchors between two runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AnchorComparison {
    /// Whether request_hash matches (same input)
    pub request_hash_match: bool,
    /// Whether manifest_hash matches (same model/config)
    pub manifest_hash_match: bool,
    /// Whether decision_chain_hash matches (same routing decisions)
    pub decision_chain_hash_match: bool,
    /// Whether backend_identity_hash matches (same execution environment)
    pub backend_identity_hash_match: bool,
    /// Whether model_identity_hash matches (same model weights)
    pub model_identity_hash_match: bool,
    /// Whether all deterministic anchors match
    pub all_anchors_match: bool,
    /// Run A request hash
    pub request_hash_a: String,
    /// Run B request hash
    pub request_hash_b: String,
    /// Run A decision chain hash
    pub decision_chain_hash_a: Option<String>,
    /// Run B decision chain hash
    pub decision_chain_hash_b: Option<String>,
}

/// First divergence point between two runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct FirstDivergence {
    /// Divergence category: "anchor", "stage", "router_step"
    pub category: String,
    /// The specific stage where divergence occurred
    pub stage: Option<String>,
    /// Router step index where divergence occurred (for router_step category)
    pub router_step: Option<u32>,
    /// Brief description of the divergence
    pub description: String,
    /// Run A's value at divergence point
    pub value_a: Option<serde_json::Value>,
    /// Run B's value at divergence point
    pub value_b: Option<serde_json::Value>,
}

/// Summary of differences between two runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagDiffSummary {
    /// Whether the runs have the same status
    pub status_match: bool,
    /// Whether the runs have the same event count
    pub event_count_match: bool,
    /// Difference in total duration (B - A) in milliseconds
    pub duration_diff_ms: Option<i64>,
    /// Number of event type mismatches
    pub event_type_mismatches: i64,
    /// Number of severity level changes
    pub severity_changes: i64,
    /// Whether the runs are considered deterministically equivalent
    pub equivalent: bool,
    /// Divergence reason if not equivalent
    pub divergence_reason: Option<String>,
}

/// Difference in a single event between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct EventDiff {
    /// Sequence number
    pub seq: i64,
    /// Difference type: added, removed, changed
    pub diff_type: String,
    /// Event type in run A (if present)
    pub event_type_a: Option<String>,
    /// Event type in run B (if present)
    pub event_type_b: Option<String>,
    /// Brief description of the difference
    pub description: String,
}

/// Timing difference between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TimingDiff {
    /// Stage or phase name
    pub stage: String,
    /// Duration in run A (microseconds)
    pub duration_us_a: Option<i64>,
    /// Duration in run B (microseconds)
    pub duration_us_b: Option<i64>,
    /// Difference (B - A) in microseconds
    pub diff_us: Option<i64>,
    /// Percentage change
    pub percent_change: Option<f64>,
}

/// Router step difference between two runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RouterStepDiff {
    /// Step index
    pub step_idx: u32,
    /// Whether this step matches between runs
    pub matches: bool,
    /// Is this the first divergent step?
    pub is_first_divergence: bool,
    /// Selected adapter stable IDs in run A
    pub selected_ids_a: Vec<u64>,
    /// Selected adapter stable IDs in run B
    pub selected_ids_b: Vec<u64>,
    /// Q15 scores in run A
    pub scores_q15_a: Vec<i16>,
    /// Q15 scores in run B
    pub scores_q15_b: Vec<i16>,
    /// Decision hash in run A
    pub decision_hash_a: Option<String>,
    /// Decision hash in run B
    pub decision_hash_b: Option<String>,
}

/// Request body for exporting diagnostic data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagExportRequest {
    /// Trace ID to export
    pub trace_id: String,
    /// Export format: json, ndjson, csv
    #[serde(default = "default_json_format")]
    pub format: String,
    /// Whether to include all events (default: true)
    #[serde(default = "default_true")]
    pub include_events: bool,
    /// Whether to include timing summary (default: true)
    #[serde(default = "default_true")]
    pub include_timing: bool,
    /// Whether to include metadata (default: true)
    #[serde(default = "default_true")]
    pub include_metadata: bool,
    /// Maximum events to export (default: 10000, max: 50000)
    pub max_events: Option<u32>,
}

fn default_json_format() -> String {
    "json".to_string()
}

/// Response for diagnostic export.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagExportResponse {
    /// Schema version for API compatibility
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// Export format used
    pub format: String,
    /// Run summary
    pub run: DiagRunResponse,
    /// Exported events (if include_events)
    pub events: Option<Vec<DiagEventResponse>>,
    /// Timing summary by stage (if include_timing)
    pub timing_summary: Option<Vec<StageTiming>>,
    /// Export metadata
    pub metadata: Option<ExportMetadata>,
}

/// Timing summary for a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct StageTiming {
    /// Stage name
    pub stage: String,
    /// Start time (mono_us from run start)
    pub start_us: i64,
    /// End time (mono_us from run start)
    pub end_us: Option<i64>,
    /// Duration in microseconds
    pub duration_us: Option<i64>,
    /// Whether the stage completed successfully
    pub success: bool,
}

/// Export metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ExportMetadata {
    /// When the export was generated
    pub exported_at: String,
    /// Total events exported
    pub events_exported: i64,
    /// Total events in run (may be more than exported if max_events hit)
    pub events_total: i64,
    /// Whether the export was truncated
    pub truncated: bool,
}

// ============================================================================
// Bundle Export Types
// ============================================================================

/// Request for creating a signed diagnostic bundle export.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagBundleExportRequest {
    /// Trace ID of the diagnostic run to export
    pub trace_id: String,
    /// Bundle format: "tar.zst" (default) or "zip"
    #[serde(default = "default_bundle_format")]
    pub format: String,
    /// Include evidence payload (requires explicit authorization)
    #[serde(default)]
    pub include_evidence: bool,
    /// Evidence authorization token (required if include_evidence is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_auth_token: Option<String>,
}

fn default_bundle_format() -> String {
    "tar.zst".to_string()
}

/// Response for bundle export request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagBundleExportResponse {
    /// Schema version for API compatibility
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// Unique export ID
    pub export_id: String,
    /// Bundle format used
    pub format: String,
    /// Bundle file size in bytes
    pub size_bytes: u64,
    /// BLAKE3 hash of the bundle file
    pub bundle_hash: String,
    /// Merkle root of included events
    pub merkle_root: String,
    /// Ed25519 signature (hex-encoded)
    pub signature: String,
    /// Public key used for signing (hex-encoded)
    pub public_key: String,
    /// Key ID (kid-{hash})
    pub key_id: String,
    /// Download URL (relative)
    pub download_url: String,
    /// When the export was created
    pub created_at: String,
    /// Bundle manifest
    pub manifest: BundleManifest,
}

/// Bundle manifest describing contents and integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BundleManifest {
    /// Schema version for bundle format
    pub schema_version: String,
    /// Bundle format identifier
    pub format: String,
    /// When the bundle was created
    pub created_at: String,
    /// Trace ID of the exported run
    pub trace_id: String,
    /// Run ID
    pub run_id: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Run status at export time
    pub run_status: String,
    /// Files included in the bundle with their hashes
    pub files: Vec<BundleFileEntry>,
    /// Total size of all files (uncompressed)
    pub total_uncompressed_bytes: u64,
    /// Merkle root of events (BLAKE3 hex)
    pub events_merkle_root: String,
    /// Total events in bundle
    pub events_count: u64,
    /// Whether events were truncated
    pub events_truncated: bool,
    /// Whether evidence payload is included
    pub evidence_included: bool,
    /// Identity hashes for reproducibility verification
    pub identity: BundleIdentity,
}

/// File entry in bundle manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BundleFileEntry {
    /// Relative path within bundle
    pub path: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// BLAKE3 hash of file contents
    pub hash: String,
    /// MIME type
    pub content_type: String,
}

/// Identity hashes for determinism verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BundleIdentity {
    /// Request hash (input determinism)
    pub request_hash: String,
    /// Decision chain hash (routing determinism)
    pub decision_chain_hash: Option<String>,
    /// Backend identity hash (environment)
    pub backend_identity_hash: Option<String>,
    /// Model identity hash (weights)
    pub model_identity_hash: Option<String>,
    /// Adapter stack stable IDs
    pub adapter_stack_ids: Vec<String>,
    /// Code/build identity (git SHA or build hash)
    pub code_identity: Option<String>,
}

/// Request for verifying a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagBundleVerifyRequest {
    /// Bundle hash to verify
    pub bundle_hash: String,
    /// Expected signature (hex-encoded)
    pub signature: String,
    /// Public key to verify against (hex-encoded)
    pub public_key: String,
}

/// Response for bundle verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DiagBundleVerifyResponse {
    /// Schema version
    #[serde(default = "crate::schema_version")]
    pub schema_version: String,
    /// Whether the bundle signature is valid
    pub valid: bool,
    /// Verification result details
    pub result: VerificationResult,
    /// Warnings (non-fatal issues)
    pub warnings: Vec<String>,
}

/// Detailed verification result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VerificationResult {
    /// Signature verification passed
    pub signature_valid: bool,
    /// Manifest hash matches bundle
    pub manifest_hash_valid: bool,
    /// All file hashes match
    pub files_hash_valid: bool,
    /// Events merkle root matches
    pub merkle_root_valid: bool,
    /// Number of files verified
    pub files_verified: u32,
    /// Number of events verified
    pub events_verified: u64,
    /// Key ID used for signing
    pub key_id: String,
    /// When the bundle was signed
    pub signed_at: Option<String>,
}

/// Config snapshot subset for bundle export.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ConfigSnapshot {
    /// Server version
    pub server_version: String,
    /// API schema version
    pub api_schema_version: String,
    /// Active policy pack IDs
    pub active_policy_packs: Vec<String>,
    /// Router configuration (non-sensitive)
    pub router_config: RouterConfigSnapshot,
    /// Backend configuration (non-sensitive)
    pub backend_config: BackendConfigSnapshot,
}

/// Router configuration snapshot (non-sensitive fields only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RouterConfigSnapshot {
    /// K value for K-sparse routing
    pub k_sparse_value: Option<u32>,
    /// Determinism mode
    pub determinism_mode: String,
    /// Tie-break policy
    pub tie_break_policy: String,
}

/// Backend configuration snapshot (non-sensitive fields only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BackendConfigSnapshot {
    /// Backend type (mlx, coreml, metal)
    pub backend_type: String,
    /// Metal enabled
    pub metal_enabled: bool,
    /// CoreML enabled
    pub coreml_enabled: bool,
    /// ANE enabled
    pub ane_enabled: bool,
}
