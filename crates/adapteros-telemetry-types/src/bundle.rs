//! Bundle metadata types for AdapterOS telemetry system
//!
//! This module provides the canonical BundleMetadata type used across
//! the telemetry system for bundle storage, verification, and retention.

#[cfg(feature = "server")]
use adapteros_core::B3Hash;

// For WASM builds, use the simple type alias from adapteros-types
#[cfg(not(feature = "server"))]
pub use adapteros_types::routing::B3Hash;

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Canonical bundle metadata (consolidates lib.rs, bundle_store.rs, and bundle.rs versions)
///
/// Per Artifacts Ruleset #13: All bundles must be signed with Ed25519
/// This type provides a single source of truth for bundle signature metadata across AdapterOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
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
    #[cfg_attr(feature = "utoipa", schema(value_type = u64, example = 1700000000))]
    pub created_at: SystemTime,
    /// Previous bundle hash for chain verification
    pub prev_bundle_hash: Option<B3Hash>,
    /// Mark bundle as incident-related (never evicted)
    pub is_incident_bundle: bool,
    /// Mark bundle as promotion-related (provenance)
    pub is_promotion_bundle: bool,
    /// Custom tags for categorization
    pub tags: Vec<String>,

    // Stack versioning fields (PRD-03)
    /// Stack ID associated with this bundle's events
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version when bundle was created
    #[serde(default)]
    pub stack_version: Option<i64>,
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

/// Bundle metadata for retention policy decisions
///
/// This is a simplified metadata type used specifically for retention
/// policy evaluation. It contains only the fields needed for retention decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionBundleMetadata {
    pub bundle_id: String,
    pub cpid: String,
    pub created_at: u64,
    pub size_bytes: u64,
    pub last_accessed: u64,
    pub bundle_type: BundleType,
    pub incident_id: Option<String>,
    pub promotion_id: Option<String>,
}

/// Type of bundle for retention decisions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BundleType {
    /// Regular inference bundle
    Inference,
    /// Incident investigation bundle
    Incident,
    /// Control plane promotion bundle
    Promotion,
    /// Audit trail bundle
    Audit,
}

/// Bundle metadata for trace schema
///
/// This is a specialized metadata type for trace bundles with
/// compression and custom metadata support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceBundleMetadata {
    /// Creation timestamp
    pub created_at: u128,
    /// Number of events in bundle
    pub event_count: usize,
    /// Total size in bytes
    pub total_size_bytes: u64,
    /// Compression used
    pub compression: String,
    /// Signature of the bundle
    pub signature: Option<String>,
    /// Additional metadata
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}
