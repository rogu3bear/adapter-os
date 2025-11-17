//! Stack Specification (PRD 3)
//!
//! Defines the canonical stack representation with deterministic hash computation.

use crate::B3Hash;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stack specification with content-addressed hash
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackSpec {
    /// Unique stack identifier
    pub stack_id: Uuid,
    /// Tenant owning this stack
    pub tenant_id: String,
    /// Adapter IDs in this stack (sorted, deduplicated)
    pub adapter_ids: Vec<String>,
    /// Generation counter (incremented on each activation)
    pub generation: u64,
    /// Content hash of (adapter_ids + per-adapter content hashes)
    pub hash: B3Hash,
}

impl StackSpec {
    /// Create a new stack spec with deduplication and sorting
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant owning this stack
    /// * `adapter_ids` - List of adapter IDs (will be sorted and deduplicated)
    /// * `adapter_hashes` - Map of adapter_id -> content hash
    ///
    /// # Returns
    /// * `Ok(StackSpec)` if all adapter IDs have corresponding hashes
    /// * `Err` if any adapter ID is missing from adapter_hashes
    pub fn new(
        tenant_id: String,
        adapter_ids: Vec<String>,
        adapter_hashes: &std::collections::HashMap<String, String>,
    ) -> Result<Self, String> {
        // Deduplicate and sort adapter IDs
        let mut sorted_ids = adapter_ids;
        sorted_ids.sort();
        sorted_ids.dedup();

        // Verify all adapters have hashes
        for id in &sorted_ids {
            if !adapter_hashes.contains_key(id) {
                return Err(format!("Missing hash for adapter: {}", id));
            }
        }

        // Compute content hash
        let hash = Self::compute_hash(&sorted_ids, adapter_hashes);

        Ok(Self {
            stack_id: Uuid::new_v4(),
            tenant_id,
            adapter_ids: sorted_ids,
            generation: 0,
            hash,
        })
    }

    /// Compute deterministic hash from adapter IDs and their content hashes
    ///
    /// Hash is computed as: BLAKE3(adapter_ids || per-adapter-hashes)
    /// where adapter_ids are sorted lexicographically
    pub fn compute_hash(
        adapter_ids: &[String],
        adapter_hashes: &std::collections::HashMap<String, String>,
    ) -> B3Hash {
        let mut pairs = Vec::new();
        for id in adapter_ids {
            if let Some(hash) = adapter_hashes.get(id) {
                pairs.push((id.clone(), hash.clone()));
            }
        }
        crate::compute_stack_hash(pairs)
    }

    /// Recompute hash (used after adapter content changes)
    pub fn recompute_hash(
        &mut self,
        adapter_hashes: &std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        for id in &self.adapter_ids {
            if !adapter_hashes.contains_key(id) {
                return Err(format!("Missing hash for adapter: {}", id));
            }
        }
        self.hash = Self::compute_hash(&self.adapter_ids, adapter_hashes);
        Ok(())
    }

    /// Bump generation counter (called on activation)
    pub fn advance_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }

    /// Validate stack invariants
    ///
    /// # Invariants
    /// - adapter_ids must be sorted
    /// - adapter_ids must not have duplicates
    pub fn validate(&self) -> Result<(), String> {
        // Check sorted
        let mut sorted = self.adapter_ids.clone();
        sorted.sort();
        if self.adapter_ids != sorted {
            return Err("adapter_ids must be sorted lexicographically".to_string());
        }

        // Check no duplicates
        let original_len = self.adapter_ids.len();
        let mut deduped = self.adapter_ids.clone();
        deduped.dedup();
        if deduped.len() != original_len {
            return Err("adapter_ids must not contain duplicates".to_string());
        }

        Ok(())
    }

    /// Check if this stack contains duplicate adapter IDs
    pub fn has_duplicates(&self) -> bool {
        let mut seen = std::collections::HashSet::new();
        for id in &self.adapter_ids {
            if !seen.insert(id) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn mock_adapter_hashes() -> HashMap<String, String> {
        let mut hashes = HashMap::new();
        hashes.insert("adapter-1".to_string(), "hash-1".to_string());
        hashes.insert("adapter-2".to_string(), "hash-2".to_string());
        hashes.insert("adapter-3".to_string(), "hash-3".to_string());
        hashes
    }

    #[test]
    fn test_new_sorts_and_dedups() {
        let hashes = mock_adapter_hashes();
        let spec = StackSpec::new(
            "tenant-1".to_string(),
            vec![
                "adapter-3".to_string(),
                "adapter-1".to_string(),
                "adapter-2".to_string(),
                "adapter-1".to_string(), // duplicate
            ],
            &hashes,
        )
        .unwrap();

        assert_eq!(
            spec.adapter_ids,
            vec!["adapter-1", "adapter-2", "adapter-3"]
        );
    }

    #[test]
    fn test_new_missing_hash() {
        let hashes = mock_adapter_hashes();
        let result = StackSpec::new(
            "tenant-1".to_string(),
            vec!["adapter-1".to_string(), "adapter-999".to_string()],
            &hashes,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Missing hash for adapter: adapter-999"));
    }

    #[test]
    fn test_validate_sorted() {
        let hashes = mock_adapter_hashes();
        let mut spec = StackSpec::new(
            "tenant-1".to_string(),
            vec!["adapter-1".to_string(), "adapter-2".to_string()],
            &hashes,
        )
        .unwrap();

        // Should be valid
        assert!(spec.validate().is_ok());

        // Manually break sorting
        spec.adapter_ids = vec!["adapter-2".to_string(), "adapter-1".to_string()];
        assert!(spec.validate().is_err());
    }

    #[test]
    fn test_validate_no_duplicates() {
        let hashes = mock_adapter_hashes();
        let mut spec = StackSpec::new(
            "tenant-1".to_string(),
            vec!["adapter-1".to_string()],
            &hashes,
        )
        .unwrap();

        // Manually add duplicate
        spec.adapter_ids = vec!["adapter-1".to_string(), "adapter-1".to_string()];
        assert!(spec.validate().is_err());
    }

    #[test]
    fn test_hash_stability() {
        let hashes = mock_adapter_hashes();

        let spec1 = StackSpec::new(
            "tenant-1".to_string(),
            vec!["adapter-1".to_string(), "adapter-2".to_string()],
            &hashes,
        )
        .unwrap();

        let spec2 = StackSpec::new(
            "tenant-1".to_string(),
            vec!["adapter-2".to_string(), "adapter-1".to_string()], // different order
            &hashes,
        )
        .unwrap();

        // Hashes should be identical (order-independent due to sorting)
        assert_eq!(spec1.hash, spec2.hash);
    }

    #[test]
    fn test_advance_generation() {
        let hashes = mock_adapter_hashes();
        let mut spec = StackSpec::new(
            "tenant-1".to_string(),
            vec!["adapter-1".to_string()],
            &hashes,
        )
        .unwrap();

        assert_eq!(spec.generation, 0);
        spec.advance_generation();
        assert_eq!(spec.generation, 1);
        spec.advance_generation();
        assert_eq!(spec.generation, 2);
    }

    #[test]
    fn test_has_duplicates() {
        let hashes = mock_adapter_hashes();
        let spec = StackSpec::new(
            "tenant-1".to_string(),
            vec!["adapter-1".to_string(), "adapter-2".to_string()],
            &hashes,
        )
        .unwrap();

        assert!(!spec.has_duplicates());

        let mut spec_with_dups = spec.clone();
        spec_with_dups.adapter_ids.push("adapter-1".to_string());
        assert!(spec_with_dups.has_duplicates());
    }
}
