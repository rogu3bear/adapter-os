//! Telemetry types

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use utoipa::ToSchema;

/// Telemetry event
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryEvent {
    pub event_type: String,
    pub timestamp: String,
    pub data: serde_json::Value,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
}

/// Telemetry bundle response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryBundleResponse {
    pub bundle_id: String,
    pub created_at: String,
    pub event_count: u64,
    pub size_bytes: u64,
    pub signature: String,
}

/// Export telemetry bundle request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportTelemetryBundleRequest {
    pub bundle_id: String,
    pub format: String, // "json", "ndjson", "csv"
}

/// Verify bundle signature request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyBundleSignatureRequest {
    pub bundle_id: String,
    pub expected_signature: String,
}

/// Bundle verification response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BundleVerificationResponse {
    pub bundle_id: String,
    pub verified: bool,
    pub signature_match: bool,
    pub timestamp: String,
}

/// Canonical bundle metadata (consolidates lib.rs, bundle_store.rs, and bundle.rs versions)
///
/// Per Artifacts Ruleset #13: All bundles must be signed with Ed25519
/// This type provides a single source of truth for bundle signature metadata across AdapterOS.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BundleMetadata {
    // Core fields (required)
    /// Content hash of the bundle (BLAKE3)
    pub bundle_hash: B3Hash,
    /// Merkle root of all events in the bundle
    pub merkle_root: B3Hash,
    /// Number of events in the bundle
    pub event_count: usize,

    // Signature fields (required per Artifacts Ruleset #13)
    /// Ed25519 signature over bundle_hash (hex-encoded)
    pub signature: String,
    /// Ed25519 public key for verification (hex-encoded)
    pub public_key: String,
    /// Key identifier: blake3(pubkey)[..16]
    pub key_id: String,
    /// Schema version for forward compatibility
    pub schema_version: u32,
    /// Signature timestamp (microseconds since epoch)
    pub signed_at_us: u64,

    // Extended fields (optional - used by BundleStore)
    /// Control Plane ID
    pub cpid: Option<String>,
    /// Tenant ID
    pub tenant_id: Option<String>,
    /// Sequence number within tenant/cpid
    pub sequence_no: Option<u64>,
    /// Bundle creation timestamp
    pub created_at: SystemTime,
    /// Previous bundle hash for chain verification
    pub prev_bundle_hash: Option<B3Hash>,
    /// Mark bundle as incident-related (never evicted)
    pub is_incident_bundle: bool,
    /// Mark bundle as promotion-related (provenance)
    pub is_promotion_bundle: bool,
    /// Custom tags for categorization
    pub tags: Vec<String>,
}

impl BundleMetadata {
    /// Check if this bundle has verifiable signature metadata
    pub fn is_verifiable(&self) -> bool {
        self.schema_version > 0 && !self.signature.is_empty() && !self.public_key.is_empty()
    }

    /// Check if this is a protected bundle (cannot be evicted)
    pub fn is_protected(&self) -> bool {
        self.is_incident_bundle || self.is_promotion_bundle
    }
}
