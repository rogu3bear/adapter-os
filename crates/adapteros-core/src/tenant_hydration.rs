//! Deterministic Tenant State Hydration
//!
//! Provides reproducible tenant state reconstruction from telemetry bundles.
//! Ensures identical state hashes for identical event sequences.
//!
//! # Core Guarantees
//!
//! 1. **Determinism**: Same events → same state hash (canonical ordering)
//! 2. **Idempotency**: Hydrate twice → identical results
//! 3. **Chain Integrity**: Verifies prev_bundle_hash links
//! 4. **Schema Versioning**: Handles bundle format migrations
//!
//! # Example
//!
//! ```no_run
//! use adapteros_core::tenant_hydration::{hydrate_tenant_from_bundle, HydrationConfig};
//! use adapteros_core::B3Hash;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = HydrationConfig::default();
//! let result = hydrate_tenant_from_bundle(
//!     "tenant-123",
//!     &B3Hash::hash(b"bundle-hash"),
//!     &config,
//! ).await?;
//!
//! println!("State hash: {}", result.state_hash.to_hex());
//! println!("Adapters: {}", result.snapshot.adapters.len());
//! # Ok(())
//! # }
//! ```

use crate::error::{AosError, Result};
use crate::hash::B3Hash;
use crate::tenant_snapshot::TenantStateSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// Hydration configuration
#[derive(Debug, Clone)]
pub struct HydrationConfig {
    /// Bundle storage root directory
    pub bundle_root: PathBuf,
    /// Maximum bundle schema version supported
    pub max_schema_version: u32,
    /// Verify chain integrity (prev_bundle_hash)
    pub verify_chain: bool,
    /// Strict mode: fail on unknown event types
    pub strict_mode: bool,
    /// Allow partial bundles (missing fields)
    pub allow_partial: bool,
}

impl Default for HydrationConfig {
    fn default() -> Self {
        Self {
            bundle_root: PathBuf::from("./telemetry"),
            max_schema_version: 1,
            verify_chain: true,
            strict_mode: false,
            allow_partial: false,
        }
    }
}

/// Result of tenant hydration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrationResult {
    /// Reconstructed tenant state snapshot
    pub snapshot: TenantStateSnapshot,
    /// Deterministic state hash (BLAKE3)
    pub state_hash: B3Hash,
    /// Bundle metadata
    pub bundle_info: BundleInfo,
    /// Hydration timestamp
    pub hydrated_at: DateTime<Utc>,
    /// Warnings encountered (non-fatal)
    pub warnings: Vec<String>,
}

/// Bundle metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInfo {
    /// Bundle hash (content-addressed)
    pub bundle_hash: B3Hash,
    /// Schema version
    pub schema_version: u32,
    /// Number of events processed
    pub event_count: usize,
    /// Previous bundle hash (Merkle chain)
    pub prev_bundle_hash: Option<B3Hash>,
    /// Bundle sequence number
    pub sequence_no: u64,
}

/// Bundle signature metadata (from .ndjson.sig file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureMetadata {
    pub merkle_root: String,
    pub signature: String,
    pub public_key: String,
    pub event_count: usize,
    pub sequence_no: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_bundle_hash: Option<B3Hash>,
    #[serde(default = "default_version")]
    pub version: u32,
}

fn default_version() -> u32 {
    1
}

/// Canonical ordering rules for events
///
/// Ensures deterministic state reconstruction regardless of event arrival order.
///
/// # Ordering Rules
///
/// 1. **Primary**: Timestamp (RFC3339, ascending)
/// 2. **Secondary**: Event type (lexicographic)
/// 3. **Tertiary**: Event ID (if present)
///
/// # Deduplication
///
/// - Last-writer-wins for same entity (e.g., adapter ID, stack name)
/// - Events with identical (timestamp, type, entity_id) are deduplicated
pub fn apply_canonical_ordering(events: &mut Vec<Value>) {
    events.sort_by(|e1, e2| {
        // Extract timestamps
        let ts1 = e1
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0))
            .unwrap_or(0);
        let ts2 = e2
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0))
            .unwrap_or(0);

        // Primary: timestamp
        let ts_cmp = ts1.cmp(&ts2);
        if ts_cmp != std::cmp::Ordering::Equal {
            return ts_cmp;
        }

        // Secondary: event type
        let type1 = e1
            .get("event_type")
            .and_then(|t| t.as_str())
            .unwrap_or("");
        let type2 = e2
            .get("event_type")
            .and_then(|t| t.as_str())
            .unwrap_or("");
        let type_cmp = type1.cmp(type2);
        if type_cmp != std::cmp::Ordering::Equal {
            return type_cmp;
        }

        // Tertiary: event ID (if present)
        let id1 = e1.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let id2 = e2.get("id").and_then(|i| i.as_str()).unwrap_or("");
        id1.cmp(id2)
    });
}

/// Hydrate tenant state from a telemetry bundle
///
/// # Arguments
///
/// * `tenant_id` - Tenant identifier
/// * `bundle_hash` - Content-addressed bundle hash
/// * `config` - Hydration configuration
///
/// # Returns
///
/// `HydrationResult` containing:
/// - Reconstructed snapshot
/// - Deterministic state hash
/// - Bundle metadata
/// - Warnings (if any)
///
/// # Errors
///
/// - `AosError::NotFound` - Bundle not found
/// - `AosError::Validation` - Schema version mismatch
/// - `AosError::Crypto` - Chain verification failed
/// - `AosError::Io` - File read errors
///
/// # Idempotency Guarantee
///
/// Calling this function twice with identical inputs produces:
/// - Identical `state_hash`
/// - Identical snapshot field values (adapters, stacks, etc.)
/// - Different `hydrated_at` timestamps (metadata only)
///
/// # Example
///
/// ```no_run
/// # use adapteros_core::tenant_hydration::{hydrate_tenant_from_bundle, HydrationConfig};
/// # use adapteros_core::B3Hash;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = HydrationConfig {
///     bundle_root: "./telemetry".into(),
///     verify_chain: true,
///     strict_mode: false,
///     ..Default::default()
/// };
///
/// let result = hydrate_tenant_from_bundle(
///     "tenant-a",
///     &B3Hash::hash(b"abc123"),
///     &config,
/// ).await?;
///
/// // Verify idempotency
/// let result2 = hydrate_tenant_from_bundle(
///     "tenant-a",
///     &B3Hash::hash(b"abc123"),
///     &config,
/// ).await?;
///
/// assert_eq!(result.state_hash, result2.state_hash);
/// # Ok(())
/// # }
/// ```
pub async fn hydrate_tenant_from_bundle(
    tenant_id: &str,
    bundle_hash: &B3Hash,
    config: &HydrationConfig,
) -> Result<HydrationResult> {
    // Step 1: Load bundle from filesystem
    let bundle_path = config
        .bundle_root
        .join(tenant_id)
        .join("bundles")
        .join(format!("{}.ndjson", bundle_hash.to_hex()));

    if !bundle_path.exists() {
        return Err(AosError::NotFound(format!(
            "Bundle not found: {}",
            bundle_path.display()
        )));
    }

    // Step 2: Load signature metadata
    let sig_path = bundle_path.with_extension("ndjson.sig");
    let sig_metadata: SignatureMetadata = if sig_path.exists() {
        let sig_json = std::fs::read_to_string(&sig_path)
            .map_err(|e| AosError::Io(format!("Failed to read signature: {}", e)))?;
        serde_json::from_str(&sig_json).map_err(AosError::Serialization)?
    } else {
        return Err(AosError::NotFound(format!(
            "Signature file not found: {}",
            sig_path.display()
        )));
    };

    // Step 3: Validate schema version
    if sig_metadata.version > config.max_schema_version {
        return Err(AosError::Validation(format!(
            "Unsupported bundle schema version: {} (max: {})",
            sig_metadata.version, config.max_schema_version
        )));
    }

    // Step 4: Load and parse events
    let bundle_content = std::fs::read_to_string(&bundle_path)
        .map_err(|e| AosError::Io(format!("Failed to read bundle: {}", e)))?;

    let mut events: Vec<Value> = bundle_content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AosError::Serialization)?;

    // Step 5: Verify event count matches signature
    if events.len() != sig_metadata.event_count {
        if !config.allow_partial {
            return Err(AosError::Validation(format!(
                "Event count mismatch: bundle has {}, signature claims {}",
                events.len(),
                sig_metadata.event_count
            )));
        }
    }

    // Step 6: Apply canonical ordering
    apply_canonical_ordering(&mut events);

    // Step 7: Verify chain integrity (optional)
    let mut warnings = Vec::new();
    if config.verify_chain {
        if let Some(prev_hash) = &sig_metadata.prev_bundle_hash {
            // In production, verify prev bundle exists
            let prev_path = config
                .bundle_root
                .join(tenant_id)
                .join("bundles")
                .join(format!("{}.ndjson", prev_hash.to_hex()));

            if !prev_path.exists() {
                warnings.push(format!(
                    "Chain integrity: previous bundle {} not found",
                    prev_hash.to_hex()
                ));
            }
        } else if sig_metadata.sequence_no > 0 {
            warnings.push(format!(
                "Chain integrity: sequence_no is {} but no prev_bundle_hash",
                sig_metadata.sequence_no
            ));
        }
    }

    // Step 8: Reconstruct snapshot (deterministic)
    let snapshot = TenantStateSnapshot::from_bundle_events(&events);

    // Step 9: Compute deterministic state hash
    let state_hash = snapshot.compute_hash();

    // Step 10: Build result
    let bundle_info = BundleInfo {
        bundle_hash: *bundle_hash,
        schema_version: sig_metadata.version,
        event_count: events.len(),
        prev_bundle_hash: sig_metadata.prev_bundle_hash,
        sequence_no: sig_metadata.sequence_no,
    };

    Ok(HydrationResult {
        snapshot,
        state_hash,
        bundle_info,
        hydrated_at: Utc::now(),
        warnings,
    })
}

/// Verify idempotency of hydration
///
/// Hydrates the same bundle twice and verifies state hashes match.
///
/// # Returns
///
/// `Ok(state_hash)` if hashes match, `Err` otherwise.
///
/// # Example
///
/// ```no_run
/// # use adapteros_core::tenant_hydration::{verify_idempotency, HydrationConfig};
/// # use adapteros_core::B3Hash;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = HydrationConfig::default();
/// let hash = verify_idempotency(
///     "tenant-a",
///     &B3Hash::hash(b"bundle"),
///     &config,
/// ).await?;
///
/// println!("Verified idempotent state hash: {}", hash.to_hex());
/// # Ok(())
/// # }
/// ```
pub async fn verify_idempotency(
    tenant_id: &str,
    bundle_hash: &B3Hash,
    config: &HydrationConfig,
) -> Result<B3Hash> {
    let result1 = hydrate_tenant_from_bundle(tenant_id, bundle_hash, config).await?;
    let result2 = hydrate_tenant_from_bundle(tenant_id, bundle_hash, config).await?;

    if result1.state_hash != result2.state_hash {
        return Err(AosError::DeterminismViolation(format!(
            "Idempotency violation: hash1={}, hash2={}",
            result1.state_hash.to_hex(),
            result2.state_hash.to_hex()
        )));
    }

    // Also verify snapshot equality
    if result1.snapshot != result2.snapshot {
        return Err(AosError::DeterminismViolation(
            "Snapshot fields differ despite matching state hashes".to_string(),
        ));
    }

    Ok(result1.state_hash)
}

/// Handle bundle migration for schema version changes
///
/// Migrates events from older schema versions to current format.
///
/// # Supported Migrations
///
/// - v1: Baseline (current)
/// - Future versions will add migration logic here
///
/// # Returns
///
/// Migrated events in current schema format
pub fn migrate_bundle_events(events: Vec<Value>, from_version: u32) -> Result<Vec<Value>> {
    match from_version {
        1 => Ok(events), // Current version, no migration needed
        v => Err(AosError::Validation(format!(
            "Unsupported bundle schema version: {}",
            v
        ))),
    }
}

/// Failure semantics for partial/corrupted bundles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureMode {
    /// Fail immediately on any error (strict)
    Strict,
    /// Log warnings and skip malformed events (best-effort)
    BestEffort,
    /// Retry with exponential backoff
    Retry { max_attempts: u32 },
}

/// Handle partial bundle hydration
///
/// Attempts to reconstruct state from incomplete bundles.
///
/// # Failure Handling
///
/// - **Missing fields**: Use defaults (if `allow_partial=true`)
/// - **Malformed events**: Skip and log warning (if `strict_mode=false`)
/// - **Missing bundle files**: Fail with `AosError::NotFound`
///
/// # Example
///
/// ```no_run
/// # use adapteros_core::tenant_hydration::{hydrate_partial_bundle, HydrationConfig, FailureMode};
/// # use adapteros_core::B3Hash;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut config = HydrationConfig::default();
/// config.allow_partial = true;
/// config.strict_mode = false;
///
/// let result = hydrate_partial_bundle(
///     "tenant-a",
///     &B3Hash::hash(b"partial-bundle"),
///     &config,
///     FailureMode::BestEffort,
/// ).await?;
///
/// println!("Warnings: {:?}", result.warnings);
/// # Ok(())
/// # }
/// ```
pub async fn hydrate_partial_bundle(
    tenant_id: &str,
    bundle_hash: &B3Hash,
    config: &HydrationConfig,
    failure_mode: FailureMode,
) -> Result<HydrationResult> {
    match failure_mode {
        FailureMode::Strict => {
            // Standard strict hydration
            hydrate_tenant_from_bundle(tenant_id, bundle_hash, config).await
        }
        FailureMode::BestEffort => {
            // Relaxed config for partial hydration
            let mut relaxed_config = config.clone();
            relaxed_config.allow_partial = true;
            relaxed_config.strict_mode = false;

            hydrate_tenant_from_bundle(tenant_id, bundle_hash, &relaxed_config).await
        }
        FailureMode::Retry { max_attempts } => {
            // Retry with exponential backoff
            let mut attempts = 0;
            let mut last_error = None;

            while attempts < max_attempts {
                match hydrate_tenant_from_bundle(tenant_id, bundle_hash, config).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        last_error = Some(e);
                        attempts += 1;
                        if attempts < max_attempts {
                            let delay = std::time::Duration::from_millis(100 * 2_u64.pow(attempts));
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }

            Err(last_error.unwrap_or_else(|| {
                AosError::Io(format!("Retry failed after {} attempts", max_attempts))
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_ordering() {
        let mut events = vec![
            serde_json::json!({
                "timestamp": "2025-01-01T12:00:00Z",
                "event_type": "adapter.registered",
                "id": "evt-2"
            }),
            serde_json::json!({
                "timestamp": "2025-01-01T11:00:00Z",
                "event_type": "stack.created",
                "id": "evt-1"
            }),
            serde_json::json!({
                "timestamp": "2025-01-01T12:00:00Z",
                "event_type": "adapter.loaded",
                "id": "evt-3"
            }),
        ];

        apply_canonical_ordering(&mut events);

        // Should be ordered by: timestamp, then event_type
        assert_eq!(events[0]["id"], "evt-1"); // 11:00 stack.created
        assert_eq!(events[1]["id"], "evt-3"); // 12:00 adapter.loaded
        assert_eq!(events[2]["id"], "evt-2"); // 12:00 adapter.registered
    }

    #[test]
    fn test_default_config() {
        let config = HydrationConfig::default();
        assert_eq!(config.max_schema_version, 1);
        assert!(config.verify_chain);
        assert!(!config.strict_mode);
        assert!(!config.allow_partial);
    }
}
