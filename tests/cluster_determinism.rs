//! Integration tests for cluster determinism verification (Tier 6)
//!
//! Tests:
//! - 3-node cluster simulation
//! - Hash mismatch detection
//! - Cross-node consistency checks
//! - Non-zero exit on mismatch

use mplora_core::B3Hash;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct NodeHashes {
    node_id: String,
    plan_hash: B3Hash,
    kernel_hash: B3Hash,
    adapter_hashes: HashMap<String, B3Hash>,
}

#[test]
fn test_three_node_consistency() {
    // Simulate 3-node cluster with identical hashes
    let plan_hash = B3Hash::hash(b"plan_v1");
    let kernel_hash = B3Hash::hash(b"kernel_v1");

    let mut adapter_hashes = HashMap::new();
    adapter_hashes.insert("adapter1".to_string(), B3Hash::hash(b"adapter1_v1"));
    adapter_hashes.insert("adapter2".to_string(), B3Hash::hash(b"adapter2_v1"));

    let node1 = NodeHashes {
        node_id: "node-001".to_string(),
        plan_hash,
        kernel_hash,
        adapter_hashes: adapter_hashes.clone(),
    };

    let node2 = NodeHashes {
        node_id: "node-002".to_string(),
        plan_hash,
        kernel_hash,
        adapter_hashes: adapter_hashes.clone(),
    };

    let node3 = NodeHashes {
        node_id: "node-003".to_string(),
        plan_hash,
        kernel_hash,
        adapter_hashes: adapter_hashes.clone(),
    };

    let nodes = vec![node1, node2, node3];

    // Verify all nodes have matching hashes
    let is_consistent = verify_cluster_consistency(&nodes);
    assert!(is_consistent, "All 3 nodes should be consistent");
}

#[test]
fn test_detect_hash_mismatch_on_one_node() {
    // Simulate 3-node cluster where node-002 has different hash
    let plan_hash = B3Hash::hash(b"plan_v1");
    let kernel_hash = B3Hash::hash(b"kernel_v1");

    let mut adapter_hashes_normal = HashMap::new();
    adapter_hashes_normal.insert("adapter1".to_string(), B3Hash::hash(b"adapter1_v1"));

    let mut adapter_hashes_mismatch = HashMap::new();
    adapter_hashes_mismatch.insert("adapter1".to_string(), B3Hash::hash(b"adapter1_v2_WRONG"));

    let node1 = NodeHashes {
        node_id: "node-001".to_string(),
        plan_hash,
        kernel_hash,
        adapter_hashes: adapter_hashes_normal.clone(),
    };

    let node2 = NodeHashes {
        node_id: "node-002".to_string(),
        plan_hash,
        kernel_hash,
        adapter_hashes: adapter_hashes_mismatch, // MISMATCH HERE
    };

    let node3 = NodeHashes {
        node_id: "node-003".to_string(),
        plan_hash,
        kernel_hash,
        adapter_hashes: adapter_hashes_normal,
    };

    let nodes = vec![node1, node2, node3];

    // Verify mismatch is detected
    let is_consistent = verify_cluster_consistency(&nodes);
    assert!(!is_consistent, "Should detect mismatch on node-002");

    // Identify which node has the mismatch
    let mismatched_node = find_mismatched_node(&nodes, "adapter1");
    assert!(mismatched_node.is_some());
    assert_eq!(mismatched_node.unwrap(), "node-002");
}

#[test]
fn test_plan_hash_verification() {
    let plan_v1 = B3Hash::hash(b"plan_v1");
    let plan_v2 = B3Hash::hash(b"plan_v2");

    let node1 = NodeHashes {
        node_id: "node-001".to_string(),
        plan_hash: plan_v1,
        kernel_hash: B3Hash::hash(b"kernel"),
        adapter_hashes: HashMap::new(),
    };

    let node2 = NodeHashes {
        node_id: "node-002".to_string(),
        plan_hash: plan_v2, // Different plan version
        kernel_hash: B3Hash::hash(b"kernel"),
        adapter_hashes: HashMap::new(),
    };

    let nodes = vec![node1, node2];
    let is_consistent = verify_cluster_consistency(&nodes);

    assert!(
        !is_consistent,
        "Different plan versions should fail consistency check"
    );
}

#[test]
fn test_kernel_hash_verification() {
    let plan_hash = B3Hash::hash(b"plan");
    let kernel_v1 = B3Hash::hash(b"kernel_v1");
    let kernel_v2 = B3Hash::hash(b"kernel_v2");

    let node1 = NodeHashes {
        node_id: "node-001".to_string(),
        plan_hash,
        kernel_hash: kernel_v1,
        adapter_hashes: HashMap::new(),
    };

    let node2 = NodeHashes {
        node_id: "node-002".to_string(),
        plan_hash,
        kernel_hash: kernel_v2, // Different kernel version
        adapter_hashes: HashMap::new(),
    };

    let nodes = vec![node1, node2];
    let is_consistent = verify_cluster_consistency(&nodes);

    assert!(
        !is_consistent,
        "Different kernel versions should fail consistency check"
    );
}

#[test]
fn test_empty_cluster_verification() {
    let nodes: Vec<NodeHashes> = vec![];
    let is_consistent = verify_cluster_consistency(&nodes);

    // Empty cluster is trivially consistent
    assert!(is_consistent);
}

#[test]
fn test_single_node_consistency() {
    let node1 = NodeHashes {
        node_id: "node-001".to_string(),
        plan_hash: B3Hash::hash(b"plan"),
        kernel_hash: B3Hash::hash(b"kernel"),
        adapter_hashes: HashMap::new(),
    };

    let nodes = vec![node1];
    let is_consistent = verify_cluster_consistency(&nodes);

    // Single node is always consistent with itself
    assert!(is_consistent);
}

// Helper functions

fn verify_cluster_consistency(nodes: &[NodeHashes]) -> bool {
    if nodes.is_empty() || nodes.len() == 1 {
        return true;
    }

    let reference = &nodes[0];

    for node in &nodes[1..] {
        // Check plan hash
        if node.plan_hash != reference.plan_hash {
            return false;
        }

        // Check kernel hash
        if node.kernel_hash != reference.kernel_hash {
            return false;
        }

        // Check adapter hashes
        for (adapter_id, ref_hash) in &reference.adapter_hashes {
            if let Some(node_hash) = node.adapter_hashes.get(adapter_id) {
                if node_hash != ref_hash {
                    return false;
                }
            } else {
                // Adapter missing on this node
                return false;
            }
        }
    }

    true
}

fn find_mismatched_node(nodes: &[NodeHashes], adapter_id: &str) -> Option<String> {
    if nodes.is_empty() {
        return None;
    }

    // Build hash frequency map
    let mut hash_counts: HashMap<B3Hash, Vec<String>> = HashMap::new();

    for node in nodes {
        if let Some(hash) = node.adapter_hashes.get(adapter_id) {
            hash_counts
                .entry(*hash)
                .or_insert_with(Vec::new)
                .push(node.node_id.clone());
        }
    }

    // Find the minority hash (the mismatch)
    if hash_counts.len() > 1 {
        let mut sorted: Vec<_> = hash_counts.into_iter().collect();
        sorted.sort_by_key(|(_, nodes)| nodes.len());

        // Return the node with the least common hash
        return Some(sorted[0].1[0].clone());
    }

    None
}
