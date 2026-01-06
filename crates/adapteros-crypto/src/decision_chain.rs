//! Decision chain hash computation for cryptographic audit receipts.
//!
//! Provides deterministic hashing of router decision events for
//! inclusion in Merkle bundles and audit trails.
//!
//! # Design Constraints
//!
//! - Hash only deterministic router events (exclude timestamps/durations)
//! - Use canonical encoding (JCS) for deterministic serialization
//! - Receipts must be stable across runs with same deterministic inputs
//!
//! # Committed Fields
//!
//! The decision chain hash commits to:
//! - Per-token router decisions (adapter indices, Q15 gates)
//! - Policy mask digests
//! - Entropy values (as Q15 fixed-point)
//! - Hash chain linkage between decisions

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

/// A single router decision event for hashing.
///
/// Contains only deterministic fields - no timestamps, durations,
/// or other non-reproducible data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouterEventDigest {
    /// Token position (0-indexed)
    pub step: usize,
    /// Input token ID (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_id: Option<u32>,
    /// Selected adapter indices (sorted for determinism)
    pub adapter_indices: Vec<u16>,
    /// Q15 fixed-point gate values (corresponding to adapter_indices)
    pub gates_q15: Vec<i16>,
    /// Entropy as Q15 fixed-point (entropy * 32767)
    pub entropy_q15: i16,
    /// BLAKE3 hash of the policy mask applied (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_mask_digest_b3: Option<B3Hash>,
    /// Hash chain link to previous decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<B3Hash>,
}

impl RouterEventDigest {
    /// Create a new router event digest.
    pub fn new(
        step: usize,
        input_token_id: Option<u32>,
        adapter_indices: Vec<u16>,
        gates_q15: Vec<i16>,
        entropy_q15: i16,
        policy_mask_digest_b3: Option<B3Hash>,
        previous_hash: Option<B3Hash>,
    ) -> Self {
        Self {
            step,
            input_token_id,
            adapter_indices,
            gates_q15,
            entropy_q15,
            policy_mask_digest_b3,
            previous_hash,
        }
    }

    /// Compute the canonical bytes for this event using JCS.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_jcs::to_vec(self).expect("RouterEventDigest should always serialize")
    }

    /// Compute the BLAKE3 hash of this event.
    pub fn hash(&self) -> B3Hash {
        B3Hash::hash(&self.canonical_bytes())
    }
}

/// Builder for computing decision chain hash from router events.
///
/// Collects router decision events and computes a Merkle root
/// over their hashes for inclusion in audit bundles.
#[derive(Debug, Default)]
pub struct DecisionChainBuilder {
    /// Collected event digests
    events: Vec<RouterEventDigest>,
    /// Running hash chain (last event's hash)
    last_hash: Option<B3Hash>,
}

impl DecisionChainBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a router decision event to the chain.
    ///
    /// The event's `previous_hash` will be set to the last event's hash
    /// to maintain the hash chain.
    pub fn push_event(
        &mut self,
        step: usize,
        input_token_id: Option<u32>,
        adapter_indices: Vec<u16>,
        gates_q15: Vec<i16>,
        entropy_q15: i16,
        policy_mask_digest_b3: Option<B3Hash>,
    ) {
        let event = RouterEventDigest::new(
            step,
            input_token_id,
            adapter_indices,
            gates_q15,
            entropy_q15,
            policy_mask_digest_b3,
            self.last_hash,
        );

        // Update the hash chain
        self.last_hash = Some(event.hash());
        self.events.push(event);
    }

    /// Add a pre-built event to the chain.
    ///
    /// Use this when you already have a RouterEventDigest with
    /// the correct previous_hash set.
    pub fn push_raw_event(&mut self, event: RouterEventDigest) {
        self.last_hash = Some(event.hash());
        self.events.push(event);
    }

    /// Get the number of events in the chain.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get the last event's hash (for chain verification).
    pub fn last_hash(&self) -> Option<B3Hash> {
        self.last_hash
    }

    /// Get all events in the chain.
    pub fn events(&self) -> &[RouterEventDigest] {
        &self.events
    }

    /// Compute the decision chain hash.
    ///
    /// Returns `BLAKE3(MerkleRoot(router_event_digests))`.
    ///
    /// If the chain is empty, returns a hash of the sentinel value
    /// "empty_decision_chain" for deterministic behavior.
    pub fn finalize(&self) -> B3Hash {
        if self.events.is_empty() {
            return B3Hash::hash(b"empty_decision_chain");
        }

        // Compute leaf hashes
        let mut leaves: Vec<B3Hash> = self.events.iter().map(|e| e.hash()).collect();

        // Build Merkle tree bottom-up
        while leaves.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in leaves.chunks(2) {
                let hash = if chunk.len() == 2 {
                    // Parent = BLAKE3(left || right)
                    let mut combined = Vec::with_capacity(64);
                    combined.extend_from_slice(chunk[0].as_bytes());
                    combined.extend_from_slice(chunk[1].as_bytes());
                    B3Hash::hash(&combined)
                } else {
                    // Odd node: duplicate for pairing
                    let mut combined = Vec::with_capacity(64);
                    combined.extend_from_slice(chunk[0].as_bytes());
                    combined.extend_from_slice(chunk[0].as_bytes());
                    B3Hash::hash(&combined)
                };
                next_level.push(hash);
            }

            leaves = next_level;
        }

        // Final decision chain hash = BLAKE3(merkle_root)
        B3Hash::hash(leaves[0].as_bytes())
    }

    /// Verify the internal hash chain integrity.
    ///
    /// Returns `true` if all events properly link to their predecessors.
    pub fn verify_chain(&self) -> bool {
        let mut expected_prev: Option<B3Hash> = None;

        for event in &self.events {
            if event.previous_hash != expected_prev {
                return false;
            }
            expected_prev = Some(event.hash());
        }

        true
    }
}

/// Environment identity hash for audit receipts.
///
/// Commits to the execution environment identity for reproducibility
/// verification and audit trails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentIdentity {
    /// Git commit hash or build hash (hex string, 40 chars for git)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit_hash: Option<String>,
    /// Build timestamp (Unix seconds, for build identification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_timestamp: Option<u64>,
    /// Backend identity string (e.g., "mlx-0.21.0", "coreml-17.4")
    pub backend_identity: String,
    /// BLAKE3 hash of MLX bridge script content (if subprocess used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mlx_bridge_script_hash: Option<B3Hash>,
    /// Model identity hash (BLAKE3 of model manifest or weights)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_identity_hash: Option<B3Hash>,
}

impl EnvironmentIdentity {
    /// Create a new environment identity.
    pub fn new(backend_identity: impl Into<String>) -> Self {
        Self {
            git_commit_hash: None,
            build_timestamp: None,
            backend_identity: backend_identity.into(),
            mlx_bridge_script_hash: None,
            model_identity_hash: None,
        }
    }

    /// Set the git commit hash.
    pub fn with_git_commit(mut self, commit_hash: impl Into<String>) -> Self {
        self.git_commit_hash = Some(commit_hash.into());
        self
    }

    /// Set the build timestamp.
    pub fn with_build_timestamp(mut self, timestamp: u64) -> Self {
        self.build_timestamp = Some(timestamp);
        self
    }

    /// Set the MLX bridge script hash.
    pub fn with_mlx_bridge_hash(mut self, hash: B3Hash) -> Self {
        self.mlx_bridge_script_hash = Some(hash);
        self
    }

    /// Set the model identity hash.
    pub fn with_model_identity(mut self, hash: B3Hash) -> Self {
        self.model_identity_hash = Some(hash);
        self
    }

    /// Compute the canonical bytes for this identity using JCS.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_jcs::to_vec(self).expect("EnvironmentIdentity should always serialize")
    }

    /// Compute the BLAKE3 hash of this environment identity.
    pub fn hash(&self) -> B3Hash {
        B3Hash::hash(&self.canonical_bytes())
    }

    /// Try to detect environment from build-time or runtime info.
    ///
    /// Attempts to populate git commit from `GIT_COMMIT_HASH` env var
    /// and build timestamp from `BUILD_TIMESTAMP` env var.
    pub fn detect(backend_identity: impl Into<String>) -> Self {
        let mut env = Self::new(backend_identity);

        // Try to get git commit from environment
        if let Ok(commit) = std::env::var("GIT_COMMIT_HASH") {
            if commit.len() >= 7 {
                env.git_commit_hash = Some(commit);
            }
        }

        // Try to get build timestamp from environment
        if let Ok(ts) = std::env::var("BUILD_TIMESTAMP") {
            if let Ok(timestamp) = ts.parse::<u64>() {
                env.build_timestamp = Some(timestamp);
            }
        }

        env
    }
}

/// Merkle bundle commit structure for audit receipts.
///
/// Contains all hashes that are committed to the Merkle bundle
/// for a given inference run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MerkleBundleCommits {
    /// BLAKE3 hash of the request (prompt + system + params)
    pub request_hash: B3Hash,
    /// BLAKE3 hash of the manifest (adapter stack configuration)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<B3Hash>,
    /// Stable IDs of adapters in the stack (sorted)
    pub adapter_stack_stable_ids: Vec<String>,
    /// Decision chain hash (Merkle root of router decisions)
    pub decision_chain_hash: B3Hash,
    /// Environment/backend identity hash
    pub backend_identity_hash: B3Hash,
    /// Model identity hash (weights/manifest hash)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_identity_hash: Option<B3Hash>,
}

impl MerkleBundleCommits {
    /// Create a new bundle commits structure.
    pub fn new(
        request_hash: B3Hash,
        decision_chain_hash: B3Hash,
        backend_identity_hash: B3Hash,
        adapter_stack_stable_ids: Vec<String>,
    ) -> Self {
        Self {
            request_hash,
            manifest_hash: None,
            adapter_stack_stable_ids,
            decision_chain_hash,
            backend_identity_hash,
            model_identity_hash: None,
        }
    }

    /// Set the manifest hash.
    pub fn with_manifest_hash(mut self, hash: B3Hash) -> Self {
        self.manifest_hash = Some(hash);
        self
    }

    /// Set the model identity hash.
    pub fn with_model_identity(mut self, hash: B3Hash) -> Self {
        self.model_identity_hash = Some(hash);
        self
    }

    /// Compute the canonical bytes for bundle commits using JCS.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_jcs::to_vec(self).expect("MerkleBundleCommits should always serialize")
    }

    /// Compute the combined BLAKE3 hash of all commits.
    ///
    /// This is the hash that gets signed in the bundle signature.
    pub fn combined_hash(&self) -> B3Hash {
        B3Hash::hash(&self.canonical_bytes())
    }

    /// Get the leaf hashes for Merkle tree construction.
    ///
    /// Returns individual hashes suitable for building a Merkle tree.
    pub fn leaf_hashes(&self) -> Vec<B3Hash> {
        let mut leaves = vec![
            self.request_hash,
            self.decision_chain_hash,
            self.backend_identity_hash,
        ];

        if let Some(ref manifest_hash) = self.manifest_hash {
            leaves.push(*manifest_hash);
        }

        if let Some(ref model_hash) = self.model_identity_hash {
            leaves.push(*model_hash);
        }

        // Hash the adapter stack IDs
        if !self.adapter_stack_stable_ids.is_empty() {
            let adapter_bytes = self.adapter_stack_stable_ids.join(",");
            leaves.push(B3Hash::hash(adapter_bytes.as_bytes()));
        }

        leaves
    }

    /// Compute the Merkle root of all commits.
    pub fn merkle_root(&self) -> B3Hash {
        let mut leaves = self.leaf_hashes();

        if leaves.is_empty() {
            return B3Hash::hash(b"empty_bundle_commits");
        }

        while leaves.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in leaves.chunks(2) {
                let hash = if chunk.len() == 2 {
                    let mut combined = Vec::with_capacity(64);
                    combined.extend_from_slice(chunk[0].as_bytes());
                    combined.extend_from_slice(chunk[1].as_bytes());
                    B3Hash::hash(&combined)
                } else {
                    let mut combined = Vec::with_capacity(64);
                    combined.extend_from_slice(chunk[0].as_bytes());
                    combined.extend_from_slice(chunk[0].as_bytes());
                    B3Hash::hash(&combined)
                };
                next_level.push(hash);
            }

            leaves = next_level;
        }

        leaves[0]
    }
}

/// Verify a bundle signature against the commits.
///
/// This function reconstructs the expected bundle hash from the commits
/// and verifies it against the provided signature.
pub fn verify_bundle_commits(
    commits: &MerkleBundleCommits,
    expected_merkle_root: &B3Hash,
) -> Result<bool> {
    let computed_root = commits.merkle_root();
    if computed_root != *expected_merkle_root {
        return Err(AosError::Crypto(format!(
            "Merkle root mismatch: expected {}, got {}",
            expected_merkle_root.to_hex(),
            computed_root.to_hex()
        )));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_chain_empty() {
        let builder = DecisionChainBuilder::new();
        let hash = builder.finalize();

        // Should return deterministic hash for empty chain
        assert_eq!(hash, B3Hash::hash(b"empty_decision_chain"));
    }

    #[test]
    fn test_decision_chain_single_event() {
        let mut builder = DecisionChainBuilder::new();
        builder.push_event(
            0,
            Some(42),
            vec![0, 1, 2],
            vec![16384, 8192, 8191],
            24576, // ~0.75 entropy
            None,
        );

        assert_eq!(builder.len(), 1);
        assert!(builder.last_hash().is_some());

        let hash = builder.finalize();
        assert_ne!(hash, B3Hash::hash(b"empty_decision_chain"));
    }

    #[test]
    fn test_decision_chain_deterministic() {
        // Build two identical chains
        let mut builder1 = DecisionChainBuilder::new();
        let mut builder2 = DecisionChainBuilder::new();

        for step in 0..5 {
            let adapters = vec![0, 1];
            let gates = vec![16384, 16383];

            builder1.push_event(
                step,
                Some(step as u32),
                adapters.clone(),
                gates.clone(),
                24576,
                None,
            );
            builder2.push_event(step, Some(step as u32), adapters, gates, 24576, None);
        }

        let hash1 = builder1.finalize();
        let hash2 = builder2.finalize();

        assert_eq!(hash1, hash2, "Decision chain hash should be deterministic");
    }

    #[test]
    fn test_decision_chain_different_inputs() {
        let mut builder1 = DecisionChainBuilder::new();
        let mut builder2 = DecisionChainBuilder::new();

        builder1.push_event(0, Some(42), vec![0, 1], vec![16384, 16383], 24576, None);
        builder2.push_event(0, Some(43), vec![0, 1], vec![16384, 16383], 24576, None); // Different token

        let hash1 = builder1.finalize();
        let hash2 = builder2.finalize();

        assert_ne!(
            hash1, hash2,
            "Different inputs should produce different hashes"
        );
    }

    #[test]
    fn test_decision_chain_hash_chain_integrity() {
        let mut builder = DecisionChainBuilder::new();

        for step in 0..3 {
            builder.push_event(step, Some(step as u32), vec![0], vec![32767], 16384, None);
        }

        assert!(builder.verify_chain(), "Hash chain should be valid");

        // Verify each event links to previous
        let events = builder.events();
        assert!(
            events[0].previous_hash.is_none(),
            "First event should have no previous"
        );
        assert!(
            events[1].previous_hash.is_some(),
            "Second event should link to first"
        );
        assert_eq!(
            events[1].previous_hash.unwrap(),
            events[0].hash(),
            "Second event should link to first's hash"
        );
    }

    #[test]
    fn test_environment_identity_hash() {
        let env1 = EnvironmentIdentity::new("mlx-0.21.0")
            .with_git_commit("abc123def456")
            .with_build_timestamp(1704067200);

        let env2 = EnvironmentIdentity::new("mlx-0.21.0")
            .with_git_commit("abc123def456")
            .with_build_timestamp(1704067200);

        assert_eq!(
            env1.hash(),
            env2.hash(),
            "Same environment should hash the same"
        );

        let env3 = EnvironmentIdentity::new("mlx-0.22.0")
            .with_git_commit("abc123def456")
            .with_build_timestamp(1704067200);

        assert_ne!(
            env1.hash(),
            env3.hash(),
            "Different backend should hash differently"
        );
    }

    #[test]
    fn test_merkle_bundle_commits() {
        let request_hash = B3Hash::hash(b"request");
        let decision_hash = B3Hash::hash(b"decisions");
        let backend_hash = B3Hash::hash(b"backend");

        let commits = MerkleBundleCommits::new(
            request_hash,
            decision_hash,
            backend_hash,
            vec!["adapter-1".to_string(), "adapter-2".to_string()],
        )
        .with_manifest_hash(B3Hash::hash(b"manifest"))
        .with_model_identity(B3Hash::hash(b"model"));

        let root = commits.merkle_root();
        assert_ne!(root, B3Hash::zero(), "Merkle root should not be zero");

        // Verify determinism
        let commits2 = MerkleBundleCommits::new(
            request_hash,
            decision_hash,
            backend_hash,
            vec!["adapter-1".to_string(), "adapter-2".to_string()],
        )
        .with_manifest_hash(B3Hash::hash(b"manifest"))
        .with_model_identity(B3Hash::hash(b"model"));

        assert_eq!(
            commits.merkle_root(),
            commits2.merkle_root(),
            "Same commits should produce same root"
        );
    }

    #[test]
    fn test_verify_bundle_commits() {
        let request_hash = B3Hash::hash(b"request");
        let decision_hash = B3Hash::hash(b"decisions");
        let backend_hash = B3Hash::hash(b"backend");

        let commits = MerkleBundleCommits::new(
            request_hash,
            decision_hash,
            backend_hash,
            vec!["adapter-1".to_string()],
        );

        let expected_root = commits.merkle_root();
        assert!(verify_bundle_commits(&commits, &expected_root).is_ok());

        // Wrong root should fail
        let wrong_root = B3Hash::hash(b"wrong");
        assert!(verify_bundle_commits(&commits, &wrong_root).is_err());
    }

    #[test]
    fn test_router_event_digest_canonical() {
        let event1 = RouterEventDigest::new(
            0,
            Some(42),
            vec![0, 1, 2],
            vec![16384, 8192, 8191],
            24576,
            None,
            None,
        );

        let event2 = RouterEventDigest::new(
            0,
            Some(42),
            vec![0, 1, 2],
            vec![16384, 8192, 8191],
            24576,
            None,
            None,
        );

        // Same events should have same canonical bytes
        assert_eq!(event1.canonical_bytes(), event2.canonical_bytes());
        assert_eq!(event1.hash(), event2.hash());
    }

    #[test]
    fn test_router_event_with_policy_mask() {
        let policy_hash = B3Hash::hash(b"policy_mask");

        let mut builder = DecisionChainBuilder::new();
        builder.push_event(
            0,
            Some(42),
            vec![0, 1],
            vec![16384, 16383],
            24576,
            Some(policy_hash),
        );

        let events = builder.events();
        assert_eq!(events[0].policy_mask_digest_b3, Some(policy_hash));
    }

    #[test]
    fn test_decision_chain_stability_across_runs() {
        // This test verifies that the hash is stable across multiple "runs"
        // with the same deterministic inputs
        let compute_hash = || {
            let mut builder = DecisionChainBuilder::new();
            for step in 0..10 {
                builder.push_event(
                    step,
                    Some(step as u32 * 100 + 42),
                    vec![0, 1, 2],
                    vec![10922, 10923, 10922], // ~1/3 each in Q15
                    16384,                     // ~0.5 entropy
                    None,
                );
            }
            builder.finalize()
        };

        let hash1 = compute_hash();
        let hash2 = compute_hash();
        let hash3 = compute_hash();

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }
}
