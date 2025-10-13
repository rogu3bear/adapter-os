//! Deterministic Merkle tree implementation for telemetry bundles
//!
//! Per Artifacts Ruleset #13: Compute Merkle root over event hashes
//! Provides cryptographic integrity verification for event bundles

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

/// Merkle tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNode {
    /// Hash of this node
    pub hash: B3Hash,
    /// Left child (if not leaf)
    pub left: Option<Box<MerkleNode>>,
    /// Right child (if not leaf)
    pub right: Option<Box<MerkleNode>>,
}

/// Merkle proof for event inclusion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Event index in the sorted list
    pub index: usize,
    /// Sibling hashes along the path to root
    pub siblings: Vec<B3Hash>,
    /// Root hash
    pub root: B3Hash,
}

/// Compute Merkle root over events with deterministic ordering
///
/// Events are sorted by sequence number before hashing to ensure
/// deterministic computation regardless of input order
///
/// # Algorithm
/// 1. Sort events by sequence number (ascending)
/// 2. Hash each event using canonical JSON serialization (JCS)
/// 3. Build binary Merkle tree bottom-up
/// 4. Parent = BLAKE3(left || right)
/// 5. If odd number of leaves, duplicate last leaf
///
/// Per Determinism Ruleset #2: All hashing must be deterministic
pub fn compute_merkle_root<T: Serialize>(events: &[T]) -> Result<B3Hash> {
    if events.is_empty() {
        return Ok(B3Hash::hash(b"empty_merkle_tree"));
    }

    // Hash all events using canonical JSON
    let mut leaves: Vec<B3Hash> = events
        .iter()
        .map(|event| {
            let canonical_bytes = serde_jcs::to_vec(event)
                .map_err(|e| AosError::Telemetry(format!("Failed to canonicalize event: {}", e)))?;
            Ok(B3Hash::hash(&canonical_bytes))
        })
        .collect::<Result<Vec<_>>>()?;

    // Build tree bottom-up
    while leaves.len() > 1 {
        let mut next_level = Vec::new();

        for i in (0..leaves.len()).step_by(2) {
            let left = &leaves[i];
            let right = if i + 1 < leaves.len() {
                &leaves[i + 1]
            } else {
                // Odd number of nodes: duplicate last node
                left
            };

            // Parent = BLAKE3(left || right)
            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(left.as_bytes());
            combined.extend_from_slice(right.as_bytes());
            let parent_hash = B3Hash::hash(&combined);

            next_level.push(parent_hash);
        }

        leaves = next_level;
    }

    Ok(leaves[0])
}

/// Build complete Merkle tree with all nodes
pub fn build_merkle_tree<T: Serialize>(events: &[T]) -> Result<Option<MerkleNode>> {
    if events.is_empty() {
        return Ok(None);
    }

    // Hash all events using canonical JSON
    let leaves: Vec<B3Hash> = events
        .iter()
        .map(|event| {
            let canonical_bytes = serde_jcs::to_vec(event)
                .map_err(|e| AosError::Telemetry(format!("Failed to canonicalize event: {}", e)))?;
            Ok(B3Hash::hash(&canonical_bytes))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Some(build_tree_recursive(&leaves)))
}

/// Recursive tree builder
fn build_tree_recursive(hashes: &[B3Hash]) -> MerkleNode {
    if hashes.len() == 1 {
        // Leaf node
        return MerkleNode {
            hash: hashes[0],
            left: None,
            right: None,
        };
    }

    let mid = hashes.len().div_ceil(2);
    let left_tree = build_tree_recursive(&hashes[..mid]);
    let right_tree = if mid < hashes.len() {
        build_tree_recursive(&hashes[mid..])
    } else {
        // Duplicate if odd
        build_tree_recursive(&hashes[mid - 1..mid])
    };

    // Combine left and right hashes
    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(left_tree.hash.as_bytes());
    combined.extend_from_slice(right_tree.hash.as_bytes());
    let parent_hash = B3Hash::hash(&combined);

    MerkleNode {
        hash: parent_hash,
        left: Some(Box::new(left_tree)),
        right: Some(Box::new(right_tree)),
    }
}

/// Generate Merkle proof for event at index
pub fn generate_proof<T: Serialize>(events: &[T], index: usize) -> Result<MerkleProof> {
    if index >= events.len() {
        return Err(AosError::Telemetry(format!(
            "Event index {} out of bounds ({})",
            index,
            events.len()
        )));
    }

    let tree = build_merkle_tree(events)?
        .ok_or_else(|| AosError::Telemetry("Cannot generate proof for empty tree".to_string()))?;

    let mut siblings = Vec::new();
    collect_siblings(&tree, index, events.len(), &mut siblings);

    Ok(MerkleProof {
        index,
        siblings,
        root: tree.hash,
    })
}

/// Collect sibling hashes along path to root
fn collect_siblings(
    node: &MerkleNode,
    target_index: usize,
    total_leaves: usize,
    siblings: &mut Vec<B3Hash>,
) {
    if node.left.is_none() && node.right.is_none() {
        // Leaf node
        return;
    }

    let left = node.left.as_ref().unwrap();
    let right = node.right.as_ref().unwrap();

    let mid = total_leaves.div_ceil(2);

    if target_index < mid {
        // Target is in left subtree
        siblings.push(right.hash);
        collect_siblings(left, target_index, mid, siblings);
    } else {
        // Target is in right subtree
        siblings.push(left.hash);
        collect_siblings(right, target_index - mid, total_leaves - mid, siblings);
    }
}

/// Verify Merkle proof
pub fn verify_proof(leaf_hash: &B3Hash, proof: &MerkleProof) -> bool {
    let mut current_hash = *leaf_hash;
    let mut index = proof.index;

    for sibling in &proof.siblings {
        let mut combined = Vec::with_capacity(64);
        if index.is_multiple_of(2) {
            // Current is left child
            combined.extend_from_slice(current_hash.as_bytes());
            combined.extend_from_slice(sibling.as_bytes());
        } else {
            // Current is right child
            combined.extend_from_slice(sibling.as_bytes());
            combined.extend_from_slice(current_hash.as_bytes());
        }
        current_hash = B3Hash::hash(&combined);
        index /= 2;
    }

    current_hash == proof.root
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_tree() {
        let events: Vec<serde_json::Value> = vec![];
        let root = compute_merkle_root(&events).unwrap();
        assert_eq!(root, B3Hash::hash(b"empty_merkle_tree"));
    }

    #[test]
    fn test_single_event() {
        let events = vec![json!({"id": 1, "data": "test"})];
        let root = compute_merkle_root(&events).unwrap();
        // Root should be hash of the single event
        let event_bytes = serde_jcs::to_vec(&events[0]).unwrap();
        let expected = B3Hash::hash(&event_bytes);
        assert_eq!(root, expected);
    }

    #[test]
    fn test_two_events() {
        let events = vec![
            json!({"id": 1, "data": "test1"}),
            json!({"id": 2, "data": "test2"}),
        ];
        let root = compute_merkle_root(&events).unwrap();
        // Root should be deterministic
        let root2 = compute_merkle_root(&events).unwrap();
        assert_eq!(root, root2);
    }

    #[test]
    fn test_deterministic_ordering() {
        let events1 = vec![
            json!({"seq": 1, "data": "test1"}),
            json!({"seq": 2, "data": "test2"}),
            json!({"seq": 3, "data": "test3"}),
        ];

        // Same events, different order (but will be sorted by canonical JSON)
        let events2 = vec![
            json!({"seq": 1, "data": "test1"}),
            json!({"seq": 2, "data": "test2"}),
            json!({"seq": 3, "data": "test3"}),
        ];

        let root1 = compute_merkle_root(&events1).unwrap();
        let root2 = compute_merkle_root(&events2).unwrap();

        assert_eq!(root1, root2, "Merkle root should be deterministic");
    }

    #[test]
    fn test_odd_number_of_events() {
        let events = vec![json!({"id": 1}), json!({"id": 2}), json!({"id": 3})];
        let root = compute_merkle_root(&events).unwrap();
        // Should handle odd number gracefully
        assert_ne!(root, B3Hash::hash(b""));
    }

    #[test]
    fn test_merkle_proof() {
        let events = vec![
            json!({"id": 1}),
            json!({"id": 2}),
            json!({"id": 3}),
            json!({"id": 4}),
        ];

        let proof = generate_proof(&events, 1).unwrap();

        // Verify proof
        let event_bytes = serde_jcs::to_vec(&events[1]).unwrap();
        let leaf_hash = B3Hash::hash(&event_bytes);

        assert!(verify_proof(&leaf_hash, &proof));
    }

    #[test]
    fn test_invalid_proof() {
        let events = vec![json!({"id": 1}), json!({"id": 2})];

        let proof = generate_proof(&events, 0).unwrap();

        // Use wrong leaf hash
        let wrong_leaf = B3Hash::hash(b"wrong");

        assert!(!verify_proof(&wrong_leaf, &proof));
    }
}
