//! Telemetry KV models
//!
//! Mirrors SQL telemetry tables while preserving deterministic ordering and
//! tenant isolation in KV storage.

use serde::{Deserialize, Serialize};

/// Telemetry event stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEventKv {
    pub id: String,
    pub tenant_id: String,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub timestamp: String,
    pub source: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub priority: Option<String>,
    /// Deterministic sort key: timestamp-normalized + id
    pub seq: String,
    pub created_at: String,
}

/// Telemetry bundle metadata stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryBundleKv {
    pub id: String,
    pub tenant_id: String,
    pub cpid: String,
    pub path: String,
    pub merkle_root_b3: String,
    pub start_seq: i64,
    pub end_seq: i64,
    pub event_count: i64,
    pub created_at: String,
    /// Optional detached signature for bundle payload
    pub signature_b64: Option<String>,
    /// Number of chunks persisted (if chunked)
    pub chunk_count: Option<u32>,
    /// Chunk size used for persistence (bytes)
    pub chunk_size_bytes: Option<u32>,
}

/// Default chunk size for bundle payload persistence (512 KiB)
pub const DEFAULT_BUNDLE_CHUNK_SIZE: usize = 512 * 1024;
