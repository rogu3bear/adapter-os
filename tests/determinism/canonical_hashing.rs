<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! Canonical hashing verification tests for AdapterOS determinism
//!
//! Verifies that canonical JSON serialization and BLAKE3 hashing produce
//! deterministic, tamper-evident hashes for all data structures.

use super::utils::*;
use adapteros_core::B3Hash;

/// Test basic canonical JSON hashing
#[test]
fn test_canonical_json_hashing() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Test simple object
    let json = r#"{"name":"test","value":42,"active":true}"#;
    let hash = B3Hash::hash(json.as_bytes());

    verifier.verify_json_hash(json, &hash).unwrap();

    // Same JSON should produce same hash
    let hash2 = B3Hash::hash(json.as_bytes());
    assert_eq!(hash, hash2, "Same JSON should produce same hash");
}

/// Test canonical JSON field ordering
#[test]
fn test_canonical_field_ordering() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Different field orderings should produce same canonical hash
    let json1 = r#"{"a":1,"b":2,"c":3}"#;
    let json2 = r#"{"c":3,"b":2,"a":1}"#;
    let json3 = r#"{"b":2,"c":3,"a":1}"#;

    // Parse and re-serialize to ensure canonical form
    let val1: serde_json::Value = serde_json::from_str(json1).unwrap();
    let val2: serde_json::Value = serde_json::from_str(json2).unwrap();
    let val3: serde_json::Value = serde_json::from_str(json3).unwrap();

    let canonical1 = serde_json::to_string(&val1).unwrap();
    let canonical2 = serde_json::to_string(&val2).unwrap();
    let canonical3 = serde_json::to_string(&val3).unwrap();

    // Canonical forms should be identical
    assert_eq!(canonical1, canonical2);
    assert_eq!(canonical2, canonical3);

    // Hashes should be identical
    let hash1 = B3Hash::hash(canonical1.as_bytes());
    let hash2 = B3Hash::hash(canonical2.as_bytes());
    let hash3 = B3Hash::hash(canonical3.as_bytes());

    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);
}

/// Test canonical JSON whitespace handling
#[test]
fn test_canonical_whitespace_handling() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Different whitespace should produce same canonical hash
    let json_variants = vec![
        r#"{"key":"value"}"#,
        r#"{"key": "value"}"#,
        r#"{ "key": "value" }"#,
        r#"{
            "key": "value"
        }"#,
        r#"{"key":"value"        }"#,
    ];

    let mut hashes = Vec::new();
    for json in &json_variants {
        let val: serde_json::Value = serde_json::from_str(json).unwrap();
        let canonical = serde_json::to_string(&val).unwrap();
        let hash = B3Hash::hash(canonical.as_bytes());
        hashes.push(hash);
    }

    // All should be identical
    for i in 1..hashes.len() {
        assert_eq!(hashes[0], hashes[i], "Whitespace variations should produce identical canonical hashes");
    }
}

/// Test canonical JSON number formatting
#[test]
fn test_canonical_number_formatting() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Different number formats should produce same canonical representation
    let numbers = vec![
        "42",
        "42.0",
        "4.2e1",
        "42000000000000000000000000000000000000e-30", // Very large number
    ];

    let mut canonical_nums = Vec::new();
    for num_str in &numbers {
        let json = format!(r#"{{"num":{}}}"#, num_str);
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let canonical = serde_json::to_string(&val).unwrap();
        canonical_nums.push(canonical);
    }

    // All should be identical (serde_json normalizes numbers)
    for i in 1..canonical_nums.len() {
        assert_eq!(canonical_nums[0], canonical_nums[i],
                  "Number formats should be canonicalized identically");
    }
}

/// Test canonical JSON boolean and null handling
#[test]
fn test_canonical_boolean_null_handling() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Test boolean and null values
    let json = r#"{"bool":true,"null":null,"false":false}"#;
    let hash = B3Hash::hash(json.as_bytes());

    // Same values should produce same hash
    let json2 = r#"{"bool":true,"null":null,"false":false}"#;
    let hash2 = B3Hash::hash(json2.as_bytes());

    assert_eq!(hash, hash2, "Boolean and null values should be canonical");
}

/// Test canonical JSON array handling
#[test]
fn test_canonical_array_handling() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Arrays should maintain order
    let json1 = r#"{"arr":[1,2,3,4]}"#;
    let json2 = r#"{"arr":[1,2,3,4]}"#;
    let json3 = r#"{"arr":[4,3,2,1]}"#; // Different order

    let hash1 = B3Hash::hash(json1.as_bytes());
    let hash2 = B3Hash::hash(json2.as_bytes());
    let hash3 = B3Hash::hash(json3.as_bytes());

    assert_eq!(hash1, hash2, "Identical arrays should hash identically");
    assert_ne!(hash1, hash3, "Different array orders should hash differently");
}

/// Test canonical JSON nested object handling
#[test]
fn test_canonical_nested_objects() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Nested objects should be canonicalized recursively
    let json1 = r#"{"outer":{"inner":{"deep":"value","num":123}}}"#;
    let json2 = r#"{"outer":{"inner":{"num":123,"deep":"value"}}}"#; // Different inner field order

    let val1: serde_json::Value = serde_json::from_str(json1).unwrap();
    let val2: serde_json::Value = serde_json::from_str(json2).unwrap();

    let canonical1 = serde_json::to_string(&val1).unwrap();
    let canonical2 = serde_json::to_string(&val2).unwrap();

    // Should be identical due to canonical field ordering
    assert_eq!(canonical1, canonical2, "Nested objects should be canonicalized");

    let hash1 = B3Hash::hash(canonical1.as_bytes());
    let hash2 = B3Hash::hash(canonical2.as_bytes());
    assert_eq!(hash1, hash2, "Canonical nested objects should hash identically");
}

/// Test BLAKE3 hash properties
#[test]
fn test_blake3_hash_properties() {
    // Test collision resistance
    let data1 = b"test_data_1";
    let data2 = b"test_data_2";

    let hash1 = B3Hash::hash(data1);
    let hash2 = B3Hash::hash(data2);

    assert_ne!(hash1, hash2, "BLAKE3 should be collision resistant");

    // Test determinism
    let hash1_again = B3Hash::hash(data1);
    assert_eq!(hash1, hash1_again, "BLAKE3 should be deterministic");

    // Test avalanche effect (small input change -> large output change)
    let data3 = b"test_data_3"; // One byte different from data2
    let hash3 = B3Hash::hash(data3);

    // Count differing bits
    let bytes1 = hash2.as_bytes();
    let bytes3 = hash3.as_bytes();
    let mut diff_bits = 0;

    for i in 0..32 {
        let xor = bytes1[i] ^ bytes3[i];
        diff_bits += xor.count_ones() as usize;
    }

    // Should have roughly half the bits different (avalanche effect)
    assert!(diff_bits > 100, "BLAKE3 should exhibit avalanche effect ({} differing bits)", diff_bits);
}

/// Test hash chain with canonical JSON
#[test]
fn test_hash_chain_with_canonical_json() {
    let mut validator = HashChainValidator::new();

    // Build a chain using canonical JSON
    let mut current_hash = B3Hash::hash(b"genesis");

    for i in 0..5 {
        let data = format!(r#"{{"step":{},"prev":"{}"}}"#, i, current_hash);
        let val: serde_json::Value = serde_json::from_str(&data).unwrap();
        let canonical = serde_json::to_string(&val).unwrap();
        current_hash = B3Hash::hash(canonical.as_bytes());
        validator.add_hash("canonical_chain", current_hash);
    }

    // Verify chain integrity
    let chain = &validator.chains["canonical_chain"];
    assert_eq!(chain.len(), 5);

    // Rebuild chain and verify identical
    let mut rebuilt_chain = Vec::new();
    let mut current_hash2 = B3Hash::hash(b"genesis");

    for i in 0..5 {
        let data = format!(r#"{{"step":{},"prev":"{}"}}"#, i, current_hash2);
        let val: serde_json::Value = serde_json::from_str(&data).unwrap();
        let canonical = serde_json::to_string(&val).unwrap();
        current_hash2 = B3Hash::hash(canonical.as_bytes());
        rebuilt_chain.push(current_hash2);
    }

    assert_eq!(chain, &rebuilt_chain, "Canonical JSON hash chains should be deterministic");
}

/// Test telemetry event canonical hashing
#[test]
fn test_telemetry_event_canonical_hashing() {
    use adapteros_telemetry::TelemetryEvent;

    let event1 = TelemetryEvent {
        event_type: "test_event".to_string(),
<<<<<<< HEAD
        kind: None,
=======
>>>>>>> integration-branch
        timestamp: 1234567890,
        data: serde_json::json!({"key": "value", "count": 42}),
    };

    let event2 = TelemetryEvent {
        event_type: "test_event".to_string(),
<<<<<<< HEAD
        kind: None,
=======
>>>>>>> integration-branch
        timestamp: 1234567890,
        data: serde_json::json!({"count": 42, "key": "value"}), // Different field order
    };

    // Serialize to canonical JSON
    let json1 = serde_json::to_string(&event1).unwrap();
    let json2 = serde_json::to_string(&event2).unwrap();

    let hash1 = B3Hash::hash(json1.as_bytes());
    let hash2 = B3Hash::hash(json2.as_bytes());

    // Should be identical due to canonical serialization
    assert_eq!(hash1, hash2, "Telemetry events should hash canonically");
}

/// Test evidence canonical hashing
#[test]
fn test_evidence_canonical_hashing() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence with different JSON orderings
    let evidence1 = vec!["function: calculate_total".to_string(), "file: math.rs".to_string()];
    let evidence2 = vec!["file: math.rs".to_string(), "function: calculate_total".to_string()];

    verifier.add_evidence("response1", evidence1);
    verifier.add_evidence("response2", evidence2);

    // Create responses that reference evidence
    let response1 = "Based on calculate_total in math.rs";
    let response2 = "Based on calculate_total in math.rs"; // Same content

    // Both should verify (order doesn't matter for evidence checking)
    verifier.verify_evidence_grounding("response1", response1).unwrap();
    verifier.verify_evidence_grounding("response2", response2).unwrap();
}

/// Test performance of canonical hashing
#[test]
fn test_canonical_hashing_performance() {
    let start = std::time::Instant::now();

    // Hash many JSON objects
    for i in 0..1000 {
        let json = format!(r#"{{"index":{},"data":"test_data_{}"}}"#, i, i);
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let canonical = serde_json::to_string(&val).unwrap();
        let _hash = B3Hash::hash(canonical.as_bytes());
    }

    let duration = start.elapsed();

    // Should be reasonably fast (< 500ms for 1000 operations)
    assert!(duration < std::time::Duration::from_millis(500),
            "Canonical hashing should be performant: {:?}", duration);
}

/// Test canonical hashing with complex nested structures
#[test]
fn test_complex_nested_structures() {
    let mut verifier = CanonicalHashingVerifier::new();

    // Complex nested structure
    let complex_json = r#"{
        "metadata": {
            "version": "1.0",
            "features": ["deterministic", "secure", "fast"],
            "config": {
                "timeout": 30000,
                "retries": 3,
                "endpoints": [
                    {"url": "https://api1.example.com", "weight": 1.0},
                    {"url": "https://api2.example.com", "weight": 2.0}
                ]
            }
        },
        "data": {
            "users": [
                {"id": 1, "name": "Alice", "active": true},
                {"id": 2, "name": "Bob", "active": false},
                {"id": 3, "name": "Charlie", "active": true}
            ],
            "stats": {
                "total_users": 3,
                "active_users": 2,
                "last_updated": 1234567890123
            }
        }
    }"#;

    let hash1 = B3Hash::hash(complex_json.as_bytes());

    // Parse and re-serialize
    let val: serde_json::Value = serde_json::from_str(complex_json).unwrap();
    let canonical = serde_json::to_string(&val).unwrap();
    let hash2 = B3Hash::hash(canonical.as_bytes());

    // Should be identical
    assert_eq!(hash1, hash2, "Complex nested structures should hash deterministically");
}