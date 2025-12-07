#![cfg(all(test, feature = "extended-tests"))]
//! Specialized verification utilities for determinism testing
//!
//! Provides common utilities for:
//! - Deterministic execution simulation
//! - Hash chain validation
//! - Platform fingerprinting
//! - Event sequence comparison
//! - HKDF seeding verification

use std::collections::HashMap;
use std::sync::Arc;
use adapteros_core::{B3Hash, CPID, derive_seed};
use adapteros_deterministic_exec::DeterministicExecutor;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryEvent {
    pub event_type: String,
    pub timestamp: i64,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventSequence {
    pub events: Vec<TelemetryEvent>,
    pub metadata: HashMap<String, String>,
}

/// Deterministic execution context for testing
pub struct DeterminismTestContext {
    pub global_seed: [u8; 32],
    pub executor: DeterministicExecutor,
    pub event_log: Vec<TelemetryEvent>,
}

impl DeterminismTestContext {
    /// Create a new test context with a fixed global seed
    pub fn new() -> Self {
        let global_seed = [0x42; 32]; // Fixed test seed
        let executor = DeterministicExecutor::new(global_seed);
        Self {
            global_seed,
            executor,
            event_log: Vec::new(),
        }
    }

    /// Execute a deterministic task and capture events
    pub async fn execute_task<F, Fut>(&mut self, task_name: &str, task: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let task_id = self.executor.spawn_deterministic(task_name, task()).await?;
        let events = self.executor.get_event_log();
        self.event_log.extend(events);
        Ok(())
    }
}

/// Hash chain validator for verifying deterministic hash sequences
pub struct HashChainValidator {
    pub chains: HashMap<String, Vec<B3Hash>>,
}

impl HashChainValidator {
    pub fn new() -> Self {
        Self {
            chains: HashMap::new(),
        }
    }

    /// Add a hash to a named chain
    pub fn add_hash(&mut self, chain_name: &str, hash: B3Hash) {
        self.chains.entry(chain_name.to_string()).or_insert_with(Vec::new).push(hash);
    }

    /// Verify that two hash chains are identical
    pub fn verify_chain_equality(&self, chain1: &str, chain2: &str) -> Result<(), String> {
        let chain1_hashes = self.chains.get(chain1).ok_or_else(|| format!("Chain {} not found", chain1))?;
        let chain2_hashes = self.chains.get(chain2).ok_or_else(|| format!("Chain {} not found", chain2))?;

        if chain1_hashes.len() != chain2_hashes.len() {
            return Err(format!("Chain lengths differ: {} vs {}", chain1_hashes.len(), chain2_hashes.len()));
        }

        for (i, (h1, h2)) in chain1_hashes.iter().zip(chain2_hashes.iter()).enumerate() {
            if h1 != h2 {
                return Err(format!("Hash mismatch at position {}: {} vs {}", i, h1, h2));
            }
        }

        Ok(())
    }
}

/// Platform fingerprint for cross-platform validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformFingerprint {
    pub os: String,
    pub arch: String,
    pub compiler: String,
    pub features: Vec<String>,
}

impl PlatformFingerprint {
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            compiler: "rustc".to_string(), // Simplified
            features: vec!["deterministic".to_string()], // Test features
        }
    }

    pub fn hash(&self) -> B3Hash {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        let hash_value = hasher.finish();

        // Convert u64 to [u8; 32] for B3Hash
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&hash_value.to_le_bytes());
        B3Hash::from_bytes(bytes)
    }
}

/// Event sequence comparator for determinism verification
pub struct EventSequenceComparator {
    pub sequences: HashMap<String, EventSequence>,
}

impl EventSequenceComparator {
    pub fn new() -> Self {
        Self {
            sequences: HashMap::new(),
        }
    }

    /// Add an event sequence for comparison
    pub fn add_sequence(&mut self, name: &str, sequence: EventSequence) {
        self.sequences.insert(name.to_string(), sequence);
    }

    /// Compare two event sequences for determinism
    pub fn compare_sequences(&self, seq1: &str, seq2: &str) -> Result<(), String> {
        let s1 = self.sequences.get(seq1).ok_or_else(|| format!("Sequence {} not found", seq1))?;
        let s2 = self.sequences.get(seq2).ok_or_else(|| format!("Sequence {} not found", seq2))?;

        if s1.events.len() != s2.events.len() {
            return Err(format!("Sequence lengths differ: {} vs {}", s1.events.len(), s2.events.len()));
        }

        for (i, (e1, e2)) in s1.events.iter().zip(s2.events.iter()).enumerate() {
            if e1 != e2 {
                return Err(format!("Event mismatch at position {}: {:?} vs {:?}", i, e1, e2));
            }
        }

        Ok(())
    }
}

/// HKDF seeding verifier for deterministic randomness
pub struct HkdfSeedingVerifier {
    pub global_seed: [u8; 32],
}

impl HkdfSeedingVerifier {
    pub fn new(global_seed: [u8; 32]) -> Self {
        Self { global_seed }
    }

    /// Verify that HKDF-derived seeds are deterministic
    pub fn verify_seed_derivation(&self, label: &str, expected_seed: &[u8; 32]) -> Result<(), String> {
        let derived = derive_seed(&B3Hash::from_bytes(self.global_seed), label);
        if derived != *expected_seed {
            return Err(format!("Seed derivation mismatch for label '{}'", label));
        }
        Ok(())
    }

    /// Verify that multiple derivations with same label are identical
    pub fn verify_seed_consistency(&self, label: &str) -> Result<(), String> {
        let seed1 = derive_seed(&B3Hash::from_bytes(self.global_seed), label);
        let seed2 = derive_seed(&B3Hash::from_bytes(self.global_seed), label);

        if seed1 != seed2 {
            return Err(format!("Inconsistent seed derivation for label '{}'", label));
        }
        Ok(())
    }
}

/// Canonical hashing verifier for deterministic serialization
pub struct CanonicalHashingVerifier {
    pub hasher: B3Hash,
}

impl CanonicalHashingVerifier {
    pub fn new() -> Self {
        Self {
            hasher: B3Hash::new(),
        }
    }

    /// Verify that canonical JSON produces consistent hashes
    pub fn verify_json_hash(&self, json_str: &str, expected_hash: &B3Hash) -> Result<(), String> {
        // Parse and re-serialize to ensure canonical form
        let value: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let canonical = serde_json::to_string(&value)
            .map_err(|e| format!("JSON serialize error: {}", e))?;

        let hash = B3Hash::hash(canonical.as_bytes());
        if hash != *expected_hash {
            return Err(format!("Hash mismatch: expected {}, got {}", expected_hash, hash));
        }
        Ok(())
    }
}

/// Evidence-grounded response verifier
pub struct EvidenceGroundedVerifier {
    pub evidence_store: HashMap<String, Vec<String>>,
}

impl EvidenceGroundedVerifier {
    pub fn new() -> Self {
        Self {
            evidence_store: HashMap::new(),
        }
    }

    /// Add evidence for a response
    pub fn add_evidence(&mut self, response_id: &str, evidence: Vec<String>) {
        self.evidence_store.insert(response_id.to_string(), evidence);
    }

    /// Verify that a response is properly evidence-grounded
    pub fn verify_evidence_grounding(&self, response_id: &str, response: &str) -> Result<(), String> {
        let evidence = self.evidence_store.get(response_id)
            .ok_or_else(|| format!("No evidence found for response {}", response_id))?;

        if evidence.is_empty() {
            return Err(format!("Empty evidence for response {}", response_id));
        }

        // Check that response contains references to evidence
        for ev in evidence {
            if !response.contains(ev) {
                return Err(format!("Response {} does not reference evidence: {}", response_id, ev));
            }
        }

        Ok(())
    }
}