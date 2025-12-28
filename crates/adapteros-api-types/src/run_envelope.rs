use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Canonical execution context threaded through a run.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct RunEnvelope {
    /// Stable run identifier (UUID/ULID/string).
    pub run_id: String,
    /// Schema version marker for envelope compatibility.
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Workspace/tenant identifier.
    pub workspace_id: String,
    /// Actor initiating the run.
    pub actor: RunActor,
    /// BLAKE3 hash of the manifest used for this run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_hash_b3: Option<String>,
    /// Plan identifier if resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    /// Policy mask digest applied at the edge (BLAKE3 hex).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_mask_digest_b3: Option<String>,
    /// Router seed reference for routing determinism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router_seed: Option<String>,
    /// Global tick assigned to this run (if available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick: Option<u64>,
    /// Worker identifier selected for execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    /// Whether reasoning-aware routing was enabled.
    pub reasoning_mode: bool,
    /// Determinism version marker (legacy; use schema_version for schema compatibility).
    pub determinism_version: String,
    /// Boot trace identifier for the serving process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_trace_id: Option<String>,
    /// Envelope creation timestamp (RFC3339).
    #[schema(value_type = String)]
    pub created_at: DateTime<Utc>,
}

/// Initiating actor for a run envelope.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RunActor {
    /// Subject identifier (user id, dev-bypass, or anonymous).
    pub subject: String,
    /// Roles attached to the subject.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    /// Principal type (user, api_key, dev_bypass, internal_service, anonymous).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_type: Option<String>,
    /// Auth mode used for the request (bearer, api_key, dev_bypass, unauthenticated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
}
