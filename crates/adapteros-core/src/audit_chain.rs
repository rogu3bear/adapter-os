//! Standalone audit chain verification for policy audit entries.
//!
//! This module provides functions to verify the integrity of policy audit chains
//! without requiring database access. It can be used by CLI tools, external
//! verification systems, or background health checks.
//!
//! # Hash Chain Algorithm
//!
//! Each entry's hash is computed as:
//! ```text
//! entry_hash = BLAKE3(
//!     id | timestamp | tenant_id | policy_pack_id | hook | decision |
//!     reason | request_id | user_id | resource_type | resource_id |
//!     metadata_json | previous_hash
//! )
//! ```
//!
//! Fields are joined with "|" separator. Optional fields use empty string if None.
//!
//! # Example
//!
//! ```ignore
//! use adapteros_core::audit_chain::{verify_audit_chain, AuditEntry};
//!
//! let entries = vec![
//!     AuditEntry { /* ... */ },
//!     AuditEntry { /* ... */ },
//! ];
//!
//! let result = verify_audit_chain(&entries);
//! if result.divergence_detected {
//!     eprintln!("Chain tampered at index {}", result.first_invalid_index.unwrap());
//! }
//! ```

use crate::B3Hash;
use serde::{Deserialize, Serialize};

/// A policy audit entry for standalone verification.
///
/// This struct mirrors `PolicyAuditDecision` from adapteros-db but is defined
/// in core to allow verification without database dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique entry ID
    pub id: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Policy pack ID
    pub policy_pack_id: String,
    /// Policy hook (e.g., "adapter.register", "training.start")
    pub hook: String,
    /// Decision ("allow" or "deny")
    pub decision: String,
    /// Reason for decision (optional)
    #[serde(default)]
    pub reason: Option<String>,
    /// Request ID (optional)
    #[serde(default)]
    pub request_id: Option<String>,
    /// User ID (optional)
    #[serde(default)]
    pub user_id: Option<String>,
    /// Resource type (optional)
    #[serde(default)]
    pub resource_type: Option<String>,
    /// Resource ID (optional)
    #[serde(default)]
    pub resource_id: Option<String>,
    /// Metadata JSON (optional)
    #[serde(default)]
    pub metadata_json: Option<String>,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// BLAKE3 hash of this entry
    pub entry_hash: String,
    /// Hash of previous entry (None for first entry)
    #[serde(default)]
    pub previous_hash: Option<String>,
    /// Sequential chain position (starts at 1)
    pub chain_sequence: i64,
}

/// Result of audit chain verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditChainResult {
    /// Overall validity of the chain
    pub is_valid: bool,
    /// Number of entries checked
    pub entries_checked: usize,
    /// Index of first invalid entry (if any)
    pub first_invalid_index: Option<usize>,
    /// Sequence number of first invalid entry (if any)
    pub first_invalid_sequence: Option<i64>,
    /// Whether a divergence (tampering) was detected
    pub divergence_detected: bool,
    /// Description of the first validation failure
    pub error_message: Option<String>,
    /// Tenant ID for this chain (if single-tenant)
    pub tenant_id: Option<String>,
}

impl Default for AuditChainResult {
    fn default() -> Self {
        Self {
            is_valid: true,
            entries_checked: 0,
            first_invalid_index: None,
            first_invalid_sequence: None,
            divergence_detected: false,
            error_message: None,
            tenant_id: None,
        }
    }
}

impl AuditChainResult {
    /// Create a successful result.
    pub fn success(entries_checked: usize, tenant_id: Option<String>) -> Self {
        Self {
            is_valid: true,
            entries_checked,
            first_invalid_index: None,
            first_invalid_sequence: None,
            divergence_detected: false,
            error_message: None,
            tenant_id,
        }
    }

    /// Create a failure result.
    pub fn failure(
        entries_checked: usize,
        first_invalid_index: usize,
        first_invalid_sequence: i64,
        error_message: String,
        tenant_id: Option<String>,
    ) -> Self {
        Self {
            is_valid: false,
            entries_checked,
            first_invalid_index: Some(first_invalid_index),
            first_invalid_sequence: Some(first_invalid_sequence),
            divergence_detected: true,
            error_message: Some(error_message),
            tenant_id,
        }
    }
}

/// Compute the canonical hash for an audit entry.
///
/// This must match the algorithm used by `adapteros-db/src/policy_audit.rs`.
pub fn compute_entry_hash(entry: &AuditEntry) -> B3Hash {
    let entry_data = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        entry.id,
        entry.timestamp,
        entry.tenant_id,
        entry.policy_pack_id,
        entry.hook,
        entry.decision,
        entry.reason.as_deref().unwrap_or(""),
        entry.request_id.as_deref().unwrap_or(""),
        entry.user_id.as_deref().unwrap_or(""),
        entry.resource_type.as_deref().unwrap_or(""),
        entry.resource_id.as_deref().unwrap_or(""),
        entry.metadata_json.as_deref().unwrap_or(""),
        entry.previous_hash.as_deref().unwrap_or(""),
    );
    B3Hash::hash(entry_data.as_bytes())
}

/// Verify the integrity of an audit chain.
///
/// Checks:
/// 1. Each entry's hash matches its computed hash (detects tampering)
/// 2. Each entry's previous_hash matches the prior entry's entry_hash (detects deletion)
/// 3. Chain sequence numbers are monotonically increasing (detects insertion/reorder)
///
/// # Arguments
/// * `entries` - Audit entries sorted by chain_sequence ascending
///
/// # Returns
/// Verification result with details on any failures
///
/// # Example
/// ```ignore
/// let result = verify_audit_chain(&entries);
/// if !result.is_valid {
///     println!("Chain broken at sequence {}", result.first_invalid_sequence.unwrap());
/// }
/// ```
pub fn verify_audit_chain(entries: &[AuditEntry]) -> AuditChainResult {
    if entries.is_empty() {
        return AuditChainResult::success(0, None);
    }

    // Get tenant from first entry for single-tenant chains
    let tenant_id = Some(entries[0].tenant_id.clone());

    let mut prev_hash: Option<String> = None;
    let mut prev_seq = 0i64;

    for (idx, entry) in entries.iter().enumerate() {
        // Check sequence monotonicity
        let expected_seq = prev_seq + 1;
        if entry.chain_sequence != expected_seq {
            tracing::error!(
                tenant_id = %entry.tenant_id,
                entry_id = %entry.id,
                expected_seq = expected_seq,
                actual_seq = entry.chain_sequence,
                "Audit chain sequence gap detected"
            );
            return AuditChainResult::failure(
                idx + 1,
                idx,
                entry.chain_sequence,
                format!(
                    "Sequence gap: expected {}, got {}",
                    expected_seq, entry.chain_sequence
                ),
                tenant_id,
            );
        }

        // Check previous_hash linkage
        match (&prev_hash, &entry.previous_hash) {
            (Some(expected), Some(actual)) if expected != actual => {
                tracing::error!(
                    tenant_id = %entry.tenant_id,
                    entry_id = %entry.id,
                    expected_prev_hash = %expected,
                    actual_prev_hash = %actual,
                    "Audit chain previous_hash mismatch"
                );
                return AuditChainResult::failure(
                    idx + 1,
                    idx,
                    entry.chain_sequence,
                    format!(
                        "Previous hash mismatch: expected {}, got {}",
                        expected, actual
                    ),
                    tenant_id,
                );
            }
            (Some(expected), None) => {
                tracing::error!(
                    tenant_id = %entry.tenant_id,
                    entry_id = %entry.id,
                    expected_prev_hash = %expected,
                    "Audit chain previous_hash should not be NULL"
                );
                return AuditChainResult::failure(
                    idx + 1,
                    idx,
                    entry.chain_sequence,
                    format!("Previous hash missing: expected {}", expected),
                    tenant_id,
                );
            }
            (None, Some(actual)) => {
                tracing::error!(
                    tenant_id = %entry.tenant_id,
                    entry_id = %entry.id,
                    actual_prev_hash = %actual,
                    "First entry should have NULL previous_hash"
                );
                return AuditChainResult::failure(
                    idx + 1,
                    idx,
                    entry.chain_sequence,
                    "First entry has non-null previous_hash".to_string(),
                    tenant_id,
                );
            }
            _ => {}
        }

        // Recompute and verify entry hash
        let computed_hash = compute_entry_hash(entry);
        let computed_hex = computed_hash.to_hex();

        if computed_hex != entry.entry_hash {
            tracing::error!(
                tenant_id = %entry.tenant_id,
                entry_id = %entry.id,
                computed_hash = %computed_hex,
                stored_hash = %entry.entry_hash,
                "Audit entry hash mismatch - possible tampering"
            );
            return AuditChainResult::failure(
                idx + 1,
                idx,
                entry.chain_sequence,
                format!(
                    "Entry hash mismatch (tampering detected): computed {}, stored {}",
                    computed_hex, entry.entry_hash
                ),
                tenant_id,
            );
        }

        // Update for next iteration
        prev_hash = Some(entry.entry_hash.clone());
        prev_seq = entry.chain_sequence;
    }

    AuditChainResult::success(entries.len(), tenant_id)
}

/// Verify multiple tenant audit chains.
///
/// Groups entries by tenant_id and verifies each chain independently.
/// Returns all results even if some chains have diverged.
///
/// # Arguments
/// * `entries` - Audit entries (will be sorted by tenant_id, chain_sequence)
///
/// # Returns
/// Map of tenant_id -> verification result
pub fn verify_audit_chains_by_tenant(
    entries: &[AuditEntry],
) -> std::collections::BTreeMap<String, AuditChainResult> {
    use std::collections::BTreeMap;

    let mut results = BTreeMap::new();

    if entries.is_empty() {
        return results;
    }

    // Group by tenant
    let mut per_tenant: BTreeMap<String, Vec<&AuditEntry>> = BTreeMap::new();
    for entry in entries {
        per_tenant
            .entry(entry.tenant_id.clone())
            .or_default()
            .push(entry);
    }

    // Verify each tenant's chain
    for (tenant_id, mut chain) in per_tenant {
        // Sort by chain_sequence
        chain.sort_by_key(|e| e.chain_sequence);

        // Convert to owned for verification
        let owned_chain: Vec<AuditEntry> = chain.into_iter().cloned().collect();
        let result = verify_audit_chain(&owned_chain);
        results.insert(tenant_id, result);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        id: &str,
        tenant_id: &str,
        seq: i64,
        prev_hash: Option<&str>,
    ) -> AuditEntry {
        let mut entry = AuditEntry {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            policy_pack_id: "policy-1".to_string(),
            hook: "test.hook".to_string(),
            decision: "allow".to_string(),
            reason: None,
            request_id: None,
            user_id: None,
            resource_type: None,
            resource_id: None,
            metadata_json: None,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            entry_hash: String::new(), // Will compute
            previous_hash: prev_hash.map(|s| s.to_string()),
            chain_sequence: seq,
        };
        entry.entry_hash = compute_entry_hash(&entry).to_hex();
        entry
    }

    #[test]
    fn test_empty_chain_is_valid() {
        let result = verify_audit_chain(&[]);
        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 0);
    }

    #[test]
    fn test_single_entry_valid() {
        let entry = make_entry("e1", "tenant-1", 1, None);
        let result = verify_audit_chain(&[entry]);
        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 1);
    }

    #[test]
    fn test_chain_with_multiple_entries() {
        let e1 = make_entry("e1", "tenant-1", 1, None);
        let e2 = make_entry("e2", "tenant-1", 2, Some(&e1.entry_hash));
        let e3 = make_entry("e3", "tenant-1", 3, Some(&e2.entry_hash));

        let result = verify_audit_chain(&[e1, e2, e3]);
        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 3);
    }

    #[test]
    fn test_detects_hash_tampering() {
        let e1 = make_entry("e1", "tenant-1", 1, None);
        let mut e2 = make_entry("e2", "tenant-1", 2, Some(&e1.entry_hash));

        // Tamper with the decision after hash was computed
        e2.decision = "deny".to_string();
        // Note: entry_hash is stale, doesn't match the tampered content

        let result = verify_audit_chain(&[e1, e2]);
        assert!(!result.is_valid);
        assert!(result.divergence_detected);
        assert_eq!(result.first_invalid_index, Some(1));
        assert!(result.error_message.unwrap().contains("hash mismatch"));
    }

    #[test]
    fn test_detects_sequence_gap() {
        let e1 = make_entry("e1", "tenant-1", 1, None);
        let e3 = make_entry("e3", "tenant-1", 3, Some(&e1.entry_hash)); // skipped 2

        let result = verify_audit_chain(&[e1, e3]);
        assert!(!result.is_valid);
        assert!(result.divergence_detected);
        assert_eq!(result.first_invalid_sequence, Some(3));
        assert!(result.error_message.unwrap().contains("Sequence gap"));
    }

    #[test]
    fn test_detects_broken_linkage() {
        let e1 = make_entry("e1", "tenant-1", 1, None);
        let e2 = make_entry("e2", "tenant-1", 2, Some("wrong_hash")); // wrong prev_hash

        let result = verify_audit_chain(&[e1, e2]);
        assert!(!result.is_valid);
        assert!(result.divergence_detected);
        assert!(result.error_message.unwrap().contains("Previous hash mismatch"));
    }

    #[test]
    fn test_detects_first_entry_with_prev_hash() {
        let mut e1 = make_entry("e1", "tenant-1", 1, Some("should_be_none"));
        e1.entry_hash = compute_entry_hash(&e1).to_hex(); // Recompute with prev_hash

        let result = verify_audit_chain(&[e1]);
        assert!(!result.is_valid);
        assert!(result.error_message.unwrap().contains("First entry"));
    }

    #[test]
    fn test_multi_tenant_verification() {
        let t1_e1 = make_entry("t1-e1", "tenant-1", 1, None);
        let t1_e2 = make_entry("t1-e2", "tenant-1", 2, Some(&t1_e1.entry_hash));
        let t2_e1 = make_entry("t2-e1", "tenant-2", 1, None);

        let results = verify_audit_chains_by_tenant(&[t1_e1, t1_e2, t2_e1]);

        assert_eq!(results.len(), 2);
        assert!(results["tenant-1"].is_valid);
        assert_eq!(results["tenant-1"].entries_checked, 2);
        assert!(results["tenant-2"].is_valid);
        assert_eq!(results["tenant-2"].entries_checked, 1);
    }
}
