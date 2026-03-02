//! Replay proof types for "Why did you say that?" feature.
//!
//! UI-facing types for triggering deterministic replay of a previous inference
//! and displaying the comparison result inline in the conversation.
//!
//! The replay infrastructure lives in `adapteros-server-api::handlers::replay_inference`.
//! These types provide the API contract between the UI and that backend.

use serde::{Deserialize, Serialize};

use crate::schema_version;

// =============================================================================
// Replay Check (is this inference replayable?)
// =============================================================================

/// Response from checking whether an inference can be replayed.
///
/// Maps to `GET /v1/replay/check/{inference_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ReplayAvailability {
    /// The inference being checked.
    pub inference_id: String,
    /// Whether replay is possible and at what fidelity.
    pub mode: ReplayMode,
    /// Human-readable explanation of the mode decision.
    pub reason: String,
    /// Number of RAG documents from the original that are still available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_doc_count: Option<usize>,
    /// Number of RAG documents from the original that are missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_doc_count: Option<usize>,
    /// Whether the original adapters are still loadable.
    pub adapters_available: bool,
}

/// Fidelity level of a replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// Exact reproduction: same seed, same adapters, same docs, same backend.
    Exact,
    /// Approximate: minor environmental differences (e.g., different worker).
    Approximate,
    /// Degraded: some inputs are missing (RAG docs removed, adapter archived).
    Degraded,
    /// Replay not possible (metadata purged or legacy inference).
    Unavailable,
}

// =============================================================================
// Replay Execution (run the replay and compare)
// =============================================================================

/// Request to execute a replay.
///
/// Maps to `POST /v1/replay`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ReplayRequest {
    /// The inference to replay.
    pub inference_id: String,
}

/// Result of a replay execution, ready for side-by-side UI rendering.
///
/// Maps to the response from `POST /v1/replay`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ReplayProofResult {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// The original inference ID.
    pub inference_id: String,
    /// The replay execution ID (for audit trail).
    pub replay_id: String,
    /// Original response text.
    pub original_text: String,
    /// Replayed response text.
    pub replay_text: String,
    /// Match outcome.
    pub match_status: ReplayMatchStatus,
    /// Character position where divergence begins (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence_position: Option<usize>,
    /// Replay fidelity mode that was used.
    pub replay_mode: ReplayMode,
    /// Adapters used in the original inference.
    pub original_adapters: Vec<String>,
    /// Adapters used in the replay.
    pub replay_adapters: Vec<String>,
    /// Backend used in the original.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_backend: Option<String>,
    /// Backend used in the replay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_backend: Option<String>,
    /// Latency of the original inference (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_latency_ms: Option<u64>,
    /// Latency of the replay (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_latency_ms: Option<u64>,
    /// When the replay was executed.
    pub replayed_at: String,
}

/// Outcome of comparing original vs replayed response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReplayMatchStatus {
    /// Responses are byte-identical.
    Exact,
    /// Responses are semantically equivalent (>80% word overlap).
    Semantic,
    /// Responses have meaningfully diverged.
    Divergent,
}

// =============================================================================
// Replay History (list previous replays for an inference)
// =============================================================================

/// Summary of a past replay execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct ReplayHistoryEntry {
    /// Replay execution ID.
    pub replay_id: String,
    /// Match outcome of this replay.
    pub match_status: ReplayMatchStatus,
    /// Replay fidelity mode.
    pub replay_mode: ReplayMode,
    /// When this replay was executed.
    pub replayed_at: String,
}
