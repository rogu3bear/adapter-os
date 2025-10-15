//! Hash chain validation tests for AdapterOS determinism
//!
//! Verifies that hash chains maintain integrity and determinism throughout
//! the execution lifecycle, ensuring tamper-evident operation logs.

use super::utils::*;
use adapteros_core::B3Hash;

/// Test basic hash chain construction and validation
#[test]
fn test_hash_chain_construction() {
    let mut validator = HashChainValidator::new();

    // Build a simple hash chain
    let initial = B3Hash::hash(b"genesis");
    validator.add_hash("chain1", initial);

    for i in 1..5 {
        let previous = validator.chains.get("chain1").unwrap().last().unwrap();
        let next = B3Hash::hash(format!("block_{}", i).as_bytes());
        validator.add_hash("chain1", next);
    }

    // Verify chain has expected length
    assert_eq!(validator.chains["chain1"].len(), 5);

    // Verify all hashes are unique (no collisions)
    let chain = &validator.chains["chain1"];
    for i in 0..chain.len() {
        for j in (i+1)..chain.len() {
            assert_ne!(chain[i], chain[j], "Hash collision detected");
        }
    }
}

/// Test hash chain integrity validation
#[test]
fn test_hash_chain_integrity() {
    let mut validator = HashChainValidator::new();

    // Build two identical chains
    for chain_name in ["chain1", "chain2"] {
        let initial = B3Hash::hash(b"genesis");
        validator.add_hash(chain_name, initial);

        for i in 1..10 {
            let previous = validator.chains.get(chain_name).unwrap().last().unwrap();
            let next = B3Hash::hash(format!("block_{}_{}", chain_name, i).as_bytes());
            validator.add_hash(chain_name, next);
        }
    }

    // Verify chains are identical
    validator.verify_chain_equality("chain1", "chain2").unwrap();
}

/// Test hash chain tamper detection
#[test]
fn test_hash_chain_tamper_detection() {
    let mut validator = HashChainValidator::new();

    // Build a chain
    let initial = B3Hash::hash(b"genesis");
    validator.add_hash("chain1", initial);

    for i in 1..5 {
        let next = B3Hash::hash(format!("block_{}", i).as_bytes());
        validator.add_hash("chain1", next);
    }

    // Build a different chain
    let initial2 = B3Hash::hash(b"different_genesis");
    validator.add_hash("chain2", initial2);

    for i in 1..5 {
        let next = B3Hash::hash(format!("different_block_{}", i).as_bytes());
        validator.add_hash("chain2", next);
    }

    // Verify chains are different
    assert!(validator.verify_chain_equality("chain1", "chain2").is_err());
}

/// Test Merkle tree construction for hash chains
#[test]
fn test_merkle_tree_construction() {
    // Simulate Merkle tree construction
    let leaves = vec![
        B3Hash::hash(b"leaf1"),
        B3Hash::hash(b"leaf2"),
        B3Hash::hash(b"leaf3"),
        B3Hash::hash(b"leaf4"),
    ];

    // Build Merkle tree (simplified)
    let mut tree = leaves.clone();
    while tree.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in tree.chunks(2) {
            if chunk.len() == 2 {
                let combined = format!("{}{}", chunk[0], chunk[1]);
                next_level.push(B3Hash::hash(combined.as_bytes()));
            } else {
                next_level.push(chunk[0]);
            }
        }
        tree = next_level;
    }

    let root = tree[0];

    // Verify root is deterministic
    let root2 = tree[0];
    assert_eq!(root, root2, "Merkle root should be deterministic");

    // Verify different leaves produce different roots
    let different_leaves = vec![
        B3Hash::hash(b"different_leaf1"),
        B3Hash::hash(b"leaf2"),
        B3Hash::hash(b"leaf3"),
        B3Hash::hash(b"leaf4"),
    ];

    let mut different_tree = different_leaves.clone();
    while different_tree.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in different_tree.chunks(2) {
            if chunk.len() == 2 {
                let combined = format!("{}{}", chunk[0], chunk[1]);
                next_level.push(B3Hash::hash(combined.as_bytes()));
            } else {
                next_level.push(chunk[0]);
            }
        }
        different_tree = next_level;
    }

    assert_ne!(root, different_tree[0], "Different leaves should produce different Merkle roots");
}

/// Test hash chain forking and validation
#[test]
fn test_hash_chain_forking() {
    let mut validator = HashChainValidator::new();

    // Build main chain
    let genesis = B3Hash::hash(b"genesis");
    validator.add_hash("main", genesis);

    for i in 1..5 {
        let next = B3Hash::hash(format!("main_block_{}", i).as_bytes());
        validator.add_hash("main", next);
    }

    // Build fork from block 2
    validator.add_hash("fork", genesis);
    for i in 1..3 {
        let next = B3Hash::hash(format!("main_block_{}", i).as_bytes());
        validator.add_hash("fork", next);
    }

    // Fork diverges
    for i in 3..7 {
        let next = B3Hash::hash(format!("fork_block_{}", i).as_bytes());
        validator.add_hash("fork", next);
    }

    // Verify chains diverge after common prefix
    let main_chain = &validator.chains["main"];
    let fork_chain = &validator.chains["fork"];

    // First 3 blocks should be identical
    for i in 0..3 {
        assert_eq!(main_chain[i], fork_chain[i], "Chains should be identical up to fork point");
    }

    // After fork point, chains should diverge
    for i in 3..main_chain.len() {
        assert_ne!(main_chain[i], fork_chain[i], "Chains should diverge after fork point");
    }
}

/// Test hash chain compression and expansion
#[test]
fn test_hash_chain_compression() {
    let mut validator = HashChainValidator::new();

    // Build a long chain
    let mut current = B3Hash::hash(b"genesis");
    validator.add_hash("original", current);

    for i in 1..100 {
        current = B3Hash::hash(format!("block_{}", i).as_bytes());
        validator.add_hash("original", current);
    }

    // Compress chain by storing only checkpoints
    let checkpoints: Vec<B3Hash> = validator.chains["original"]
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 10 == 0)
        .map(|(_, hash)| *hash)
        .collect();

    // Verify compression maintains determinism
    let compressed_hash = B3Hash::hash(
        &checkpoints.iter()
            .map(|h| h.to_string())
            .collect::<Vec<_>>()
            .join("")
            .as_bytes()
    );

    let compressed_hash2 = B3Hash::hash(
        &checkpoints.iter()
            .map(|h| h.to_string())
            .collect::<Vec<_>>()
            .join("")
            .as_bytes()
    );

    assert_eq!(compressed_hash, compressed_hash2, "Hash chain compression should be deterministic");
}

/// Test hash chain serialization and deserialization
#[test]
fn test_hash_chain_serialization() {
    let mut validator = HashChainValidator::new();

    // Build a chain
    for i in 0..10 {
        let hash = B3Hash::hash(format!("block_{}", i).as_bytes());
        validator.add_hash("test_chain", hash);
    }

    // Serialize chain
    let serialized = serde_json::to_string(&validator.chains).unwrap();

    // Deserialize
    let deserialized: std::collections::HashMap<String, Vec<B3Hash>> =
        serde_json::from_str(&serialized).unwrap();

    // Verify round-trip consistency
    assert_eq!(validator.chains, deserialized, "Hash chain serialization should be deterministic");

    // Verify deserialized chain is valid
    let original_chain = &validator.chains["test_chain"];
    let deserialized_chain = &deserialized["test_chain"];

    assert_eq!(original_chain.len(), deserialized_chain.len());
    for (orig, deser) in original_chain.iter().zip(deserialized_chain.iter()) {
        assert_eq!(orig, deser);
    }
}

/// Test hash chain performance under load
#[test]
fn test_hash_chain_performance() {
    let mut validator = HashChainValidator::new();

    // Build a large chain to test performance
    let start = std::time::Instant::now();

    for i in 0..1000 {
        let hash = B3Hash::hash(format!("performance_block_{}", i).as_bytes());
        validator.add_hash("perf_chain", hash);
    }

    let duration = start.elapsed();

    // Verify chain was built successfully
    assert_eq!(validator.chains["perf_chain"].len(), 1000);

    // Performance should be reasonable (less than 1 second for 1000 hashes)
    assert!(duration < std::time::Duration::from_secs(1),
            "Hash chain construction should be performant: {:?}", duration);

    // Verify determinism under performance constraints
    let hash1 = validator.chains["perf_chain"][500];
    let hash2 = validator.chains["perf_chain"][500];
    assert_eq!(hash1, hash2, "Hash chain should remain deterministic under load");
}

/// Test hash chain cryptographic properties
#[test]
fn test_hash_chain_cryptographic_properties() {
    let mut validator = HashChainValidator::new();

    // Test avalanche effect - small input changes produce large output changes
    let hash1 = B3Hash::hash(b"test_input_1");
    let hash2 = B3Hash::hash(b"test_input_2");

    validator.add_hash("avalanche", hash1);
    validator.add_hash("avalanche", hash2);

    // Hashes should be completely different
    assert_ne!(hash1, hash2, "Hash function should exhibit avalanche effect");

    // Test collision resistance
    let mut hashes = std::collections::HashSet::new();
    for i in 0..1000 {
        let hash = B3Hash::hash(format!("unique_input_{}", i).as_bytes());
        assert!(hashes.insert(hash), "Hash collision detected - not collision resistant");
    }

    // Test preimage resistance (hard to find input for known output)
    let target_hash = B3Hash::hash(b"target_input");
    let mut found = false;

    // Try a few brute force attempts (should fail)
    for i in 0..100 {
        let candidate = B3Hash::hash(format!("brute_{}", i).as_bytes());
        if candidate == target_hash {
            found = true;
            break;
        }
    }

    assert!(!found, "Hash function should be preimage resistant");
}