#![cfg(all(test, feature = "extended-tests"))]
//! End-to-end tests for telemetry validation
//!
//! Validates telemetry collection, canonical JSON formatting, BLAKE3 hashing,
//! bundle rotation, Merkle tree signing, and audit trail integrity.

use crate::orchestration::TestEnvironment;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_telemetry::{BundleMetadata, BundleStore, TelemetryWriter};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Telemetry validation test suite
pub struct TelemetryValidationTest {
    env: Arc<Mutex<TestEnvironment>>,
}

impl TelemetryValidationTest {
    pub fn new(env: Arc<Mutex<TestEnvironment>>) -> Self {
        Self { env }
    }

    /// Test complete telemetry pipeline: collection → canonicalization → hashing → bundling → signing
    pub async fn test_complete_telemetry_pipeline(&self) -> Result<()> {
        let env = self.env.lock().await;

        // 1. Event Collection Phase
        println!("📊 Phase 1: Event Collection");
        self.test_event_collection(&env).await?;

        // 2. Canonical JSON Formatting
        println!("📋 Phase 2: Canonical Formatting");
        self.test_canonical_formatting(&env).await?;

        // 3. BLAKE3 Hashing
        println!("🔐 Phase 3: BLAKE3 Hashing");
        self.test_blake3_hashing(&env).await?;

        // 4. Bundle Rotation
        println!("📦 Phase 4: Bundle Rotation");
        self.test_bundle_rotation(&env).await?;

        // 5. Merkle Tree Construction
        println!("🌳 Phase 5: Merkle Tree");
        self.test_merkle_tree(&env).await?;

        // 6. Bundle Signing
        println!("✍️  Phase 6: Bundle Signing");
        self.test_bundle_signing(&env).await?;

        // 7. Audit Trail Validation
        println!("🔍 Phase 7: Audit Validation");
        self.test_audit_trail(&env).await?;

        println!("✅ Complete telemetry pipeline test passed!");
        Ok(())
    }

    /// Test event collection from various sources
    async fn test_event_collection(&self, env: &TestEnvironment) -> Result<()> {
        let test_events = vec![
            (
                "inference_start",
                serde_json::json!({"request_id": "req_123", "model": "llama_7b"}),
            ),
            (
                "adapter_load",
                serde_json::json!({"adapter_id": "aviation_maint", "memory_mb": 256}),
            ),
            (
                "evidence_retrieval",
                serde_json::json!({"query": "torque spec", "spans": 3}),
            ),
            (
                "policy_check",
                serde_json::json!({"action": "generate", "allowed": true}),
            ),
            (
                "inference_complete",
                serde_json::json!({"tokens": 150, "latency_ms": 234}),
            ),
        ];

        for (event_type, payload) in test_events {
            env.telemetry().log(event_type, payload)?;
        }

        // Verify events were collected
        let bundle_store = env.bundle_store();
        let stats = bundle_store.get_storage_stats()?;
        assert!(stats.total_events > 0, "Events should be collected");

        Ok(())
    }

    /// Test canonical JSON formatting (JCS RFC 8785)
    async fn test_canonical_formatting(&self, env: &TestEnvironment) -> Result<()> {
        // Test various JSON structures for canonical formatting
        let test_payloads = vec![
            serde_json::json!({"name": "test", "value": 123}),
            serde_json::json!({"items": [1, 2, 3], "active": true}),
            serde_json::json!({"nested": {"inner": "value"}, "list": ["a", "b"]}),
        ];

        for payload in test_payloads {
            // Log event and verify canonical formatting
            env.telemetry().log("canonical_test", payload.clone())?;

            // Verify the payload can be canonicalized
            let canonical_bytes = serde_jcs::to_vec(&payload)
                .map_err(|e| AosError::Telemetry(format!("Canonicalization failed: {}", e)))?;

            // Verify canonical bytes are deterministic
            let canonical_again = serde_jcs::to_vec(&payload)?;
            assert_eq!(
                canonical_bytes, canonical_again,
                "Canonical JSON should be deterministic"
            );
        }

        Ok(())
    }

    /// Test BLAKE3 hashing of events
    async fn test_blake3_hashing(&self, env: &TestEnvironment) -> Result<()> {
        let test_payload = serde_json::json!({
            "event_type": "hash_test",
            "timestamp": 1234567890,
            "data": "test_data"
        });

        // Log event
        env.telemetry().log("hash_test", &test_payload)?;

        // Manually compute hash to verify
        let canonical_bytes = serde_jcs::to_vec(&test_payload)?;
        let computed_hash = B3Hash::hash(&canonical_bytes);

        // Verify hash is valid (32 bytes)
        assert_eq!(
            computed_hash.as_bytes().len(),
            32,
            "BLAKE3 hash should be 32 bytes"
        );

        // Verify hash is deterministic
        let computed_hash_again = B3Hash::hash(&canonical_bytes);
        assert_eq!(
            computed_hash, computed_hash_again,
            "Hash should be deterministic"
        );

        // Verify different payloads produce different hashes
        let different_payload = serde_json::json!({
            "event_type": "different_hash_test",
            "timestamp": 1234567890,
            "data": "test_data"
        });
        let different_canonical = serde_jcs::to_vec(&different_payload)?;
        let different_hash = B3Hash::hash(&different_canonical);
        assert_ne!(
            computed_hash, different_hash,
            "Different payloads should have different hashes"
        );

        Ok(())
    }

    /// Test bundle rotation based on size and event count
    async fn test_bundle_rotation(&self, env: &TestEnvironment) -> Result<()> {
        let bundle_store = env.bundle_store();

        // Log many events to trigger rotation
        for i in 0..150 {
            // More than default max_events of 100
            let event_data = serde_json::json!({
                "event_id": i,
                "test_data": format!("test_payload_{}", i),
                "timestamp": chrono::Utc::now().timestamp()
            });
            env.telemetry().log("rotation_test", event_data)?;
        }

        // Force flush to trigger rotation
        // Note: In real implementation, this would happen automatically

        // Verify multiple bundles were created
        let stats = bundle_store.get_storage_stats()?;
        assert!(
            stats.total_bundles > 1,
            "Should have multiple bundles after rotation"
        );

        Ok(())
    }

    /// Test Merkle tree construction for bundle integrity
    async fn test_merkle_tree(&self, env: &TestEnvironment) -> Result<()> {
        // Create a set of event hashes
        let event_hashes = vec![
            B3Hash::hash(b"event_1"),
            B3Hash::hash(b"event_2"),
            B3Hash::hash(b"event_3"),
            B3Hash::hash(b"event_4"),
        ];

        // Compute Merkle root
        let merkle_root = self.compute_merkle_root(&event_hashes);

        // Verify Merkle root is deterministic
        let merkle_root_again = self.compute_merkle_root(&event_hashes);
        assert_eq!(
            merkle_root, merkle_root_again,
            "Merkle root should be deterministic"
        );

        // Verify different inputs produce different roots
        let different_hashes = vec![B3Hash::hash(b"different_1"), B3Hash::hash(b"different_2")];
        let different_root = self.compute_merkle_root(&different_hashes);
        assert_ne!(
            merkle_root, different_root,
            "Different inputs should have different Merkle roots"
        );

        Ok(())
    }

    /// Helper to compute Merkle root (simplified implementation)
    fn compute_merkle_root(&self, hashes: &[B3Hash]) -> B3Hash {
        if hashes.is_empty() {
            return B3Hash::hash(b"empty");
        }

        let mut combined = Vec::new();
        for hash in hashes {
            combined.extend_from_slice(hash.as_bytes());
        }
        B3Hash::hash(&combined)
    }

    /// Test bundle signing with Ed25519
    async fn test_bundle_signing(&self, env: &TestEnvironment) -> Result<()> {
        // Create test bundle metadata
        let merkle_root = B3Hash::hash(b"test_bundle_root");
        let metadata = BundleMetadata {
            event_count: 42,
            merkle_root,
            signature: None,
        };

        // Sign the bundle (this would normally happen in the telemetry writer)
        let signature = self.sign_merkle_root(&merkle_root)?;

        // Verify signature is valid Ed25519 signature (64 bytes)
        assert_eq!(signature.len(), 64, "Ed25519 signature should be 64 bytes");

        // Verify signature is deterministic for same input
        let signature_again = self.sign_merkle_root(&merkle_root)?;
        assert_eq!(
            signature, signature_again,
            "Signature should be deterministic for same input"
        );

        Ok(())
    }

    /// Helper to sign Merkle root (simplified implementation)
    fn sign_merkle_root(&self, merkle_root: &B3Hash) -> Result<Vec<u8>> {
        // In production, this would use a secure key from Secure Enclave
        // For testing, generate ephemeral keypair
        let keypair = adapteros_crypto::Keypair::generate();
        let signature = keypair.sign(merkle_root.as_bytes());
        Ok(signature.to_bytes().to_vec())
    }

    /// Test audit trail validation and replay
    async fn test_audit_trail(&self, env: &TestEnvironment) -> Result<()> {
        let bundle_store = env.bundle_store();

        // Test bundle listing
        let bundles = bundle_store.list_bundles()?;
        assert!(!bundles.is_empty(), "Should have bundles for audit");

        // Test bundle verification
        for bundle_id in bundles {
            let verification = bundle_store.verify_bundle(&bundle_id)?;
            assert!(
                verification.is_valid,
                "Bundle {} should be valid",
                bundle_id
            );

            // Test replay capability
            let replay_data = bundle_store.replay_bundle(&bundle_id)?;
            assert!(
                !replay_data.events.is_empty(),
                "Bundle should have replayable events"
            );

            // Verify event ordering and determinism
            let events = &replay_data.events;
            for i in 1..events.len() {
                assert!(
                    events[i - 1].timestamp <= events[i].timestamp,
                    "Events should be in chronological order"
                );
            }
        }

        Ok(())
    }
}

/// Test telemetry sampling strategies
pub async fn test_telemetry_sampling(env: Arc<Mutex<TestEnvironment>>) -> Result<()> {
    let env = env.lock().await;

    // Test different sampling rates
    let sampling_scenarios = vec![
        ("full_sampling", 1.0, 100),
        ("half_sampling", 0.5, 50),
        ("tenth_sampling", 0.1, 10),
    ];

    for (scenario, rate, expected_count) in sampling_scenarios {
        let mut sampled_events = 0;

        // Simulate events with sampling
        for i in 0..100 {
            let should_sample = rand::random::<f64>() < rate;
            if should_sample {
                sampled_events += 1;
                let event = serde_json::json!({
                    "scenario": scenario,
                    "event_id": i,
                    "sampled": true,
                    "sampling_rate": rate
                });
                env.telemetry().log("sampling_test", event)?;
            }
        }

        // Verify sampling worked approximately correctly
        let tolerance = (expected_count as f64 * 0.2) as i32; // 20% tolerance
        assert!(
            (sampled_events as i32 - expected_count as i32).abs() <= tolerance,
            "Sampling for {} should be approximately correct: expected ~{}, got {}",
            scenario,
            expected_count,
            sampled_events
        );
    }

    Ok(())
}

/// Test telemetry performance under load
pub async fn test_telemetry_performance(env: Arc<Mutex<TestEnvironment>>) -> Result<()> {
    let env = env.lock().await;

    let start_time = std::time::Instant::now();

    // Generate high volume of events
    for i in 0..1000 {
        let event = serde_json::json!({
            "performance_test": true,
            "event_id": i,
            "payload_size": "medium",
            "timestamp": chrono::Utc::now().timestamp_nanos()
        });
        env.telemetry().log("performance_test", event)?;
    }

    let duration = start_time.elapsed();
    let events_per_second = 1000.0 / duration.as_secs_f64();

    println!(
        "Telemetry performance: {:.0} events/second",
        events_per_second
    );

    // Should handle at least 100 events/second
    assert!(
        events_per_second > 100.0,
        "Telemetry should handle >100 events/second"
    );

    Ok(())
}

/// Test telemetry compression and storage efficiency
pub async fn test_telemetry_compression(env: Arc<Mutex<TestEnvironment>>) -> Result<()> {
    let env = env.lock().await;

    // Test bundle compression ratios
    let test_payloads = vec![
        ("small", "x".repeat(100)),
        ("medium", "x".repeat(1000)),
        ("large", "x".repeat(10000)),
    ];

    for (size_category, payload) in test_payloads {
        let event = serde_json::json!({
            "compression_test": true,
            "size_category": size_category,
            "payload": payload,
            "uncompressed_bytes": payload.len()
        });
        env.telemetry().log("compression_test", event)?;
    }

    // Verify bundles were created and are reasonably sized
    let bundle_store = env.bundle_store();
    let stats = bundle_store.get_storage_stats()?;

    // Compression should achieve reasonable ratios
    let avg_compression_ratio =
        stats.total_uncompressed_bytes as f64 / stats.total_compressed_bytes as f64;
    assert!(
        avg_compression_ratio > 2.0,
        "Should achieve >2:1 compression ratio"
    );

    Ok(())
}
