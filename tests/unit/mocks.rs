<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! Deterministic Mocking Utilities
//!
//! This module provides controlled, reproducible test doubles for AdapterOS components.
//! All mocks are designed to produce deterministic outputs for identical inputs,
//! ensuring reliable and repeatable test execution.
//!
//! ## Key Features
//!
//! - **Deterministic Generation**: All mock outputs are derived from input seeds
//! - **Configurable Behavior**: Mocks can be configured for different test scenarios
//! - **Thread Safety**: All mocks are safe to use across test threads
//! - **Performance**: Minimal overhead compared to real implementations
//!
//! ## Usage
//!
//! ```rust
//! use tests_unit::mocks::*;
//!
//! #[test]
//! fn test_with_deterministic_mock() {
//!     let mock = DeterministicRng::from_seed(42);
//!     let value = mock.gen_range(0..100);
//!     assert_eq!(value, 42); // Always the same for seed 42
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use adapteros_core::{B3Hash, derive_seed, derive_seed_indexed};

/// Deterministic random number generator for testing
pub struct DeterministicRng {
    seed: [u8; 32],
    counter: u64,
}

impl DeterministicRng {
    /// Create a new deterministic RNG from a numeric seed
    pub fn from_seed(seed: u64) -> Self {
        let seed_bytes = seed.to_le_bytes();
        let mut full_seed = [0u8; 32];
        full_seed[..8].copy_from_slice(&seed_bytes);
        Self {
            seed: full_seed,
            counter: 0,
        }
    }

    /// Create a new deterministic RNG from a hash seed
    pub fn from_hash_seed(hash: &B3Hash) -> Self {
        Self {
            seed: hash.as_bytes(),
            counter: 0,
        }
    }

    /// Generate a deterministic value in the given range
    pub fn gen_range(&mut self, range: std::ops::Range<i32>) -> i32 {
        self.counter += 1;
        let derived_seed = derive_seed_indexed(&B3Hash::from(self.seed), "rng", self.counter);
        let value = u32::from_le_bytes(derived_seed[..4].try_into().unwrap()) as i32;
        range.start + (value % (range.end - range.start) as u32) as i32
    }

    /// Generate a deterministic boolean value
    pub fn gen_bool(&mut self) -> bool {
        self.gen_range(0..2) == 1
    }

    /// Generate a deterministic float value in [0, 1)
    pub fn gen_float(&mut self) -> f32 {
        self.counter += 1;
        let derived_seed = derive_seed_indexed(&B3Hash::from(self.seed), "float", self.counter);
        let value = u32::from_le_bytes(derived_seed[..4].try_into().unwrap());
        (value as f32) / (u32::MAX as f32)
    }
}

/// Mock telemetry collector for testing
pub struct MockTelemetryCollector {
    events: Arc<Mutex<Vec<TelemetryEvent>>>,
    seed: B3Hash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryEvent {
    pub event_type: String,
<<<<<<< HEAD
    pub kind: Option<String>,
=======
>>>>>>> integration-branch
    pub data: serde_json::Value,
    pub timestamp: u64,
}

impl MockTelemetryCollector {
    /// Create a new mock telemetry collector
    pub fn new(seed: u64) -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            seed: B3Hash::hash(&seed.to_le_bytes()),
        }
    }

    /// Record a telemetry event (deterministic timestamp)
    pub fn record_event(&self, event_type: &str, data: serde_json::Value) {
        let mut events = self.events.lock().unwrap();
        let timestamp = events.len() as u64 * 1000; // Deterministic timestamps
        events.push(TelemetryEvent {
            event_type: event_type.to_string(),
<<<<<<< HEAD
            kind: None,
=======
>>>>>>> integration-branch
            data,
            timestamp,
        });
    }

    /// Get all recorded events
    pub fn get_events(&self) -> Vec<TelemetryEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Clear all events
    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

/// Mock policy engine for testing component behavior
pub struct MockPolicyEngine {
    policies: HashMap<String, serde_json::Value>,
    seed: B3Hash,
}

impl MockPolicyEngine {
    /// Create a new mock policy engine
    pub fn new(seed: u64) -> Self {
        Self {
            policies: HashMap::new(),
            seed: B3Hash::hash(&seed.to_le_bytes()),
        }
    }

    /// Set a policy value
    pub fn set_policy(&mut self, key: &str, value: serde_json::Value) {
        self.policies.insert(key.to_string(), value);
    }

    /// Get a policy value (returns default if not set)
    pub fn get_policy(&self, key: &str) -> Option<&serde_json::Value> {
        self.policies.get(key)
    }

    /// Check if an action is allowed (deterministic based on seed)
    pub fn check_action(&self, action: &str, context: &str) -> bool {
        let combined = format!("{}:{}", action, context);
        let hash = B3Hash::hash(combined.as_bytes());
        let bytes = hash.as_bytes();
        // Use first byte to determine allow/deny (deterministic)
        (bytes[0] % 2) == 0
    }
}

/// Mock adapter registry for testing adapter management
pub struct MockAdapterRegistry {
    adapters: Arc<Mutex<HashMap<String, MockAdapter>>>,
    seed: B3Hash,
}

#[derive(Debug, Clone)]
pub struct MockAdapter {
    pub id: String,
    pub tier: String,
    pub activation_score: f32,
    pub metadata: HashMap<String, String>,
}

impl MockAdapterRegistry {
    /// Create a new mock adapter registry
    pub fn new(seed: u64) -> Self {
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
            seed: B3Hash::hash(&seed.to_le_bytes()),
        }
    }

    /// Register a mock adapter
    pub fn register_adapter(&self, adapter: MockAdapter) {
        self.adapters.lock().unwrap().insert(adapter.id.clone(), adapter);
    }

    /// Get an adapter by ID
    pub fn get_adapter(&self, id: &str) -> Option<MockAdapter> {
        self.adapters.lock().unwrap().get(id).cloned()
    }

    /// List all adapters
    pub fn list_adapters(&self) -> Vec<MockAdapter> {
        self.adapters.lock().unwrap().values().cloned().collect()
    }

    /// Get top K adapters by activation score (deterministic ordering)
    pub fn get_top_adapters(&self, k: usize) -> Vec<MockAdapter> {
        let mut adapters = self.list_adapters();
        adapters.sort_by(|a, b| b.activation_score.partial_cmp(&a.activation_score).unwrap());
        adapters.into_iter().take(k).collect()
    }
}

/// Mock evidence collector for testing evidence-grounded responses
pub struct MockEvidenceCollector {
    evidence: Arc<Mutex<Vec<EvidenceItem>>>,
    seed: B3Hash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceItem {
    pub source: String,
    pub content: String,
    pub confidence: f32,
    pub span: Option<(usize, usize)>,
}

impl MockEvidenceCollector {
    /// Create a new mock evidence collector
    pub fn new(seed: u64) -> Self {
        Self {
            evidence: Arc::new(Mutex::new(Vec::new())),
            seed: B3Hash::hash(&seed.to_le_bytes()),
        }
    }

    /// Add evidence item
    pub fn add_evidence(&self, item: EvidenceItem) {
        self.evidence.lock().unwrap().push(item);
    }

    /// Get all evidence items
    pub fn get_evidence(&self) -> Vec<EvidenceItem> {
        self.evidence.lock().unwrap().clone()
    }

    /// Get evidence for a specific source
    pub fn get_evidence_for_source(&self, source: &str) -> Vec<EvidenceItem> {
        self.evidence.lock().unwrap()
            .iter()
            .filter(|e| e.source == source)
            .cloned()
            .collect()
    }

    /// Calculate total confidence score
    pub fn total_confidence(&self) -> f32 {
        self.evidence.lock().unwrap()
            .iter()
            .map(|e| e.confidence)
            .sum()
    }
}

/// Utility for creating deterministic test data
pub struct TestDataGenerator {
    seed: B3Hash,
    counter: u64,
}

impl TestDataGenerator {
    /// Create a new test data generator
    pub fn new(seed: u64) -> Self {
        Self {
            seed: B3Hash::hash(&seed.to_le_bytes()),
            counter: 0,
        }
    }

    /// Generate a deterministic string of given length
    pub fn gen_string(&mut self, length: usize) -> String {
        self.counter += 1;
        let derived_seed = derive_seed_indexed(&self.seed, "string", self.counter);
        let mut result = String::with_capacity(length);

        for i in 0..length {
            let byte = derived_seed[i % 32];
            result.push((b'a' + (byte % 26)) as char);
        }

        result
    }

    /// Generate a deterministic vector of floats
    pub fn gen_floats(&mut self, count: usize, range: std::ops::Range<f32>) -> Vec<f32> {
        (0..count).map(|_| {
            self.counter += 1;
            let derived_seed = derive_seed_indexed(&self.seed, "float", self.counter);
            let value = u32::from_le_bytes(derived_seed[..4].try_into().unwrap()) as f32 / u32::MAX as f32;
            range.start + value * (range.end - range.start)
        }).collect()
    }

    /// Generate deterministic JSON data
    pub fn gen_json(&mut self) -> serde_json::Value {
        let string_val = self.gen_string(10);
        let float_val = self.gen_floats(1, 0.0..1.0)[0];
        serde_json::json!({
            "id": string_val,
            "score": float_val,
            "timestamp": self.counter
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_rng_consistency() {
        let mut rng1 = DeterministicRng::from_seed(42);
        let mut rng2 = DeterministicRng::from_seed(42);

        for _ in 0..100 {
            assert_eq!(rng1.gen_range(0..100), rng2.gen_range(0..100));
        }
    }

    #[test]
    fn test_mock_telemetry_deterministic() {
        let collector1 = MockTelemetryCollector::new(123);
        let collector2 = MockTelemetryCollector::new(123);

        let data = serde_json::json!({"test": "value"});

        collector1.record_event("test_event", data.clone());
        collector2.record_event("test_event", data);

        let events1 = collector1.get_events();
        let events2 = collector2.get_events();

        assert_eq!(events1.len(), events2.len());
        assert_eq!(events1[0], events2[0]);
    }

    #[test]
    fn test_policy_engine_deterministic() {
        let engine1 = MockPolicyEngine::new(456);
        let engine2 = MockPolicyEngine::new(456);

        assert_eq!(
            engine1.check_action("read", "file.txt"),
            engine2.check_action("read", "file.txt")
        );
    }

    #[test]
    fn test_test_data_generator_consistency() {
        let mut gen1 = TestDataGenerator::new(789);
        let mut gen2 = TestDataGenerator::new(789);

        assert_eq!(gen1.gen_string(20), gen2.gen_string(20));
        assert_eq!(gen1.gen_floats(5, 0.0..1.0), gen2.gen_floats(5, 0.0..1.0));
    }
}</code>
