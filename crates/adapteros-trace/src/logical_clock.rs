//! Logical clock system for deterministic trace timestamps
//!
//! This module provides deterministic logical timestamps that replace wall-clock
//! timestamps for replay verification. Timestamps are derived from operation data
//! and global state to ensure reproducibility across runs.
//!
//! # Design
//!
//! - **Global Tick Counter**: Atomic counter that advances with each operation
//! - **Operation Tick**: Per-operation counter for ordering within the same global tick
//! - **Token Position**: For inference events, tracks token position in the sequence
//! - **Derivation Hash**: BLAKE3 hash of timestamp derivation data for verification
//!
//! # Citations
//!
//! - Lamport clock pattern: Lamport, "Time, Clocks, and the Ordering of Events in a Distributed System", 1978
//! - Atomic counters: Follows `crates/adapteros-deterministic-exec/src/lib.rs:45` patterns
//! - BLAKE3 derivation: Uses `adapteros_core::B3Hash` for deterministic hashing

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};

/// Logical timestamp for deterministic replay
///
/// This structure replaces wall-clock timestamps with deterministic
/// values derived from operation data and execution order.
///
/// # Example
///
/// ```rust,ignore
/// let clock = LogicalClock::new(global_seed);
/// let timestamp = clock.advance_for_operation("op_1", &inputs)?;
/// assert!(verify_timestamp_derivation(&timestamp, &inputs));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalTimestamp {
    /// Global tick counter for operation ordering
    pub global_tick: u64,
    /// Operation-specific tick counter
    pub op_tick: u64,
    /// Token position for inference events (None for non-inference ops)
    pub token_position: Option<u64>,
    /// BLAKE3 hash of timestamp derivation data
    pub derivation_hash: B3Hash,
}

impl LogicalTimestamp {
    /// Create a new logical timestamp
    pub fn new(
        global_tick: u64,
        op_tick: u64,
        token_position: Option<u64>,
        derivation_hash: B3Hash,
    ) -> Self {
        Self {
            global_tick,
            op_tick,
            token_position,
            derivation_hash,
        }
    }

    /// Compare timestamps for ordering
    pub fn before(&self, other: &LogicalTimestamp) -> bool {
        if self.global_tick != other.global_tick {
            self.global_tick < other.global_tick
        } else {
            self.op_tick < other.op_tick
        }
    }

    /// Check if this timestamp is concurrent with another
    pub fn concurrent_with(&self, other: &LogicalTimestamp) -> bool {
        self.global_tick == other.global_tick && self.op_tick == other.op_tick
    }
}

impl PartialOrd for LogicalTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LogicalTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.global_tick.cmp(&other.global_tick) {
            std::cmp::Ordering::Equal => self.op_tick.cmp(&other.op_tick),
            other => other,
        }
    }
}

/// Logical clock for generating deterministic timestamps
///
/// Thread-safe clock that generates monotonically increasing logical
/// timestamps derived from operation data and global seed.
///
/// # Thread Safety
///
/// Uses `Arc<AtomicU64>` for thread-safe counter management following
/// `crates/adapteros-deterministic-exec/src/lib.rs:83` patterns.
pub struct LogicalClock {
    /// Global tick counter (thread-safe atomic)
    current_tick: Arc<AtomicU64>,
    /// Event counter for operation ordering
    event_counter: Arc<AtomicU64>,
    /// Global seed for deterministic derivation
    global_seed: B3Hash,
}

impl LogicalClock {
    /// Create a new logical clock with the given global seed
    ///
    /// # Arguments
    ///
    /// * `global_seed` - BLAKE3 hash used as the global seed for deterministic derivation
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use adapteros_core::B3Hash;
    /// let global_seed = B3Hash::hash(b"my_seed");
    /// let clock = LogicalClock::new(global_seed);
    /// ```
    pub fn new(global_seed: B3Hash) -> Self {
        Self {
            current_tick: Arc::new(AtomicU64::new(0)),
            event_counter: Arc::new(AtomicU64::new(0)),
            global_seed,
        }
    }

    /// Create a logical clock starting from a specific tick
    ///
    /// Useful for resuming from a checkpoint or replay scenario.
    pub fn new_from_tick(global_seed: B3Hash, start_tick: u64) -> Self {
        Self {
            current_tick: Arc::new(AtomicU64::new(start_tick)),
            event_counter: Arc::new(AtomicU64::new(0)),
            global_seed,
        }
    }

    /// Get the current global tick without advancing
    pub fn current_tick(&self) -> u64 {
        self.current_tick.load(Ordering::SeqCst)
    }

    /// Get the current event counter without advancing
    pub fn current_event(&self) -> u64 {
        self.event_counter.load(Ordering::SeqCst)
    }

    /// Advance the clock and generate a timestamp for an operation
    ///
    /// Derives a deterministic timestamp from:
    /// - Global seed
    /// - Current tick and event counters
    /// - Operation ID and type
    /// - Input data (hashed for determinism)
    ///
    /// # Arguments
    ///
    /// * `op_id` - Unique operation identifier
    /// * `event_type` - Type of event (e.g., "inference.start", "kernel.execute")
    /// * `inputs` - Input data for the operation
    ///
    /// # Returns
    ///
    /// A `LogicalTimestamp` with cryptographically derived hash for verification.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut inputs = HashMap::new();
    /// inputs.insert("token_id".to_string(), json!(42));
    /// let timestamp = clock.advance_for_operation("op_1", "inference.token", &inputs)?;
    /// ```
    pub fn advance_for_operation(
        &self,
        op_id: &str,
        event_type: &str,
        inputs: &HashMap<String, serde_json::Value>,
    ) -> Result<LogicalTimestamp> {
        // Atomically advance counters (SeqCst for deterministic ordering)
        let global_tick = self.current_tick.fetch_add(1, Ordering::SeqCst);
        let op_tick = self.event_counter.fetch_add(1, Ordering::SeqCst);

        // Extract token position from inputs if this is an inference event
        let token_position = self.extract_token_position(event_type, inputs)?;

        // Derive timestamp hash from operation data and global state
        let derivation_hash = self.derive_timestamp_hash(
            global_tick,
            op_tick,
            op_id,
            event_type,
            inputs,
            token_position,
        )?;

        Ok(LogicalTimestamp::new(
            global_tick,
            op_tick,
            token_position,
            derivation_hash,
        ))
    }

    /// Derive BLAKE3 hash for timestamp verification
    ///
    /// Hash includes:
    /// - Global seed (for uniqueness per execution)
    /// - Tick counters (for ordering)
    /// - Operation identifiers (for event type verification)
    /// - Input data (for deterministic reproducibility)
    ///
    /// This follows the pattern in `crates/adapteros-trace/src/schema.rs:135` for
    /// canonical JSON serialization and BLAKE3 hashing.
    fn derive_timestamp_hash(
        &self,
        global_tick: u64,
        op_tick: u64,
        op_id: &str,
        event_type: &str,
        inputs: &HashMap<String, serde_json::Value>,
        token_position: Option<u64>,
    ) -> Result<B3Hash> {
        let mut hasher = blake3::Hasher::new();

        // Hash global seed for uniqueness
        hasher.update(self.global_seed.as_bytes());

        // Hash tick counters for ordering
        hasher.update(&global_tick.to_le_bytes());
        hasher.update(&op_tick.to_le_bytes());

        // Hash operation identifiers
        hasher.update(op_id.as_bytes());
        hasher.update(event_type.as_bytes());

        // Hash token position if present
        if let Some(pos) = token_position {
            hasher.update(&pos.to_le_bytes());
        }

        // Hash input keys and values in sorted order for determinism
        let mut sorted_keys: Vec<_> = inputs.keys().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            hasher.update(key.as_bytes());
            if let Some(value) = inputs.get(key) {
                // Use canonical JSON serialization for deterministic hashing
                let canonical_bytes = serde_jcs::to_vec(value).map_err(|e| {
                    AosError::Parse(format!(
                        "Failed to serialize input value for key '{}': {}",
                        key, e
                    ))
                })?;
                hasher.update(&canonical_bytes);
            }
        }

        Ok(B3Hash::new(*hasher.finalize().as_bytes()))
    }

    /// Extract token position from inputs for inference events
    ///
    /// Looks for token position indicators in common input field names:
    /// - "token_id"
    /// - "token_position"
    /// - "position"
    fn extract_token_position(
        &self,
        event_type: &str,
        inputs: &HashMap<String, serde_json::Value>,
    ) -> Result<Option<u64>> {
        // Only extract for inference-related events
        if !event_type.starts_with("inference.") {
            return Ok(None);
        }

        // Try common field names
        for field_name in &["token_id", "token_position", "position"] {
            if let Some(value) = inputs.get(*field_name) {
                if let Some(num) = value.as_u64() {
                    return Ok(Some(num));
                }
                if let Some(num) = value.as_i64() {
                    if num >= 0 {
                        return Ok(Some(num as u64));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Verify that a timestamp was correctly derived
    ///
    /// Re-derives the timestamp hash from the given data and compares
    /// with the recorded hash in the timestamp.
    ///
    /// # Returns
    ///
    /// `true` if the timestamp is valid, `false` otherwise.
    pub fn verify_timestamp(
        &self,
        timestamp: &LogicalTimestamp,
        op_id: &str,
        event_type: &str,
        inputs: &HashMap<String, serde_json::Value>,
    ) -> Result<bool> {
        let expected_hash = self.derive_timestamp_hash(
            timestamp.global_tick,
            timestamp.op_tick,
            op_id,
            event_type,
            inputs,
            timestamp.token_position,
        )?;

        Ok(timestamp.derivation_hash == expected_hash)
    }

    /// Reset the clock to a specific state
    ///
    /// Used for testing and replay scenarios.
    #[cfg(test)]
    pub fn reset_to(&self, global_tick: u64, event_counter: u64) {
        self.current_tick.store(global_tick, Ordering::SeqCst);
        self.event_counter.store(event_counter, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_logical_timestamp_ordering() {
        let ts1 = LogicalTimestamp::new(1, 0, None, B3Hash::hash(b"test1"));
        let ts2 = LogicalTimestamp::new(2, 0, None, B3Hash::hash(b"test2"));
        let ts3 = LogicalTimestamp::new(2, 1, None, B3Hash::hash(b"test3"));

        assert!(ts1.before(&ts2));
        assert!(ts2.before(&ts3));
        assert!(!ts2.before(&ts1));
    }

    #[test]
    fn test_logical_clock_creation() {
        let global_seed = B3Hash::hash(b"test_seed");
        let clock = LogicalClock::new(global_seed);

        assert_eq!(clock.current_tick(), 0);
        assert_eq!(clock.current_event(), 0);
    }

    #[test]
    fn test_timestamp_generation() {
        let global_seed = B3Hash::hash(b"test_seed");
        let clock = LogicalClock::new(global_seed);

        let mut inputs = HashMap::new();
        inputs.insert("token_id".to_string(), json!(42));

        let ts1 = clock
            .advance_for_operation("op_1", "inference.token", &inputs)
            .unwrap();
        let ts2 = clock
            .advance_for_operation("op_2", "inference.token", &inputs)
            .unwrap();

        assert_eq!(ts1.global_tick, 0);
        assert_eq!(ts1.op_tick, 0);
        assert_eq!(ts1.token_position, Some(42));

        assert_eq!(ts2.global_tick, 1);
        assert_eq!(ts2.op_tick, 1);

        assert!(ts1.before(&ts2));
    }

    #[test]
    fn test_timestamp_derivation_determinism() {
        let global_seed = B3Hash::hash(b"test_seed");
        let clock1 = LogicalClock::new(global_seed);
        let clock2 = LogicalClock::new(global_seed);

        let mut inputs = HashMap::new();
        inputs.insert("key1".to_string(), json!("value1"));
        inputs.insert("key2".to_string(), json!(42));

        let ts1 = clock1
            .advance_for_operation("op_1", "kernel.execute", &inputs)
            .unwrap();
        let ts2 = clock2
            .advance_for_operation("op_1", "kernel.execute", &inputs)
            .unwrap();

        // Same seed and inputs should produce same derivation hash
        assert_eq!(ts1.derivation_hash, ts2.derivation_hash);
    }

    #[test]
    fn test_timestamp_verification() {
        let global_seed = B3Hash::hash(b"test_seed");
        let clock = LogicalClock::new(global_seed);

        let mut inputs = HashMap::new();
        inputs.insert("token_id".to_string(), json!(100));

        let timestamp = clock
            .advance_for_operation("op_test", "inference.token", &inputs)
            .unwrap();

        // Verify with correct data
        assert!(clock
            .verify_timestamp(&timestamp, "op_test", "inference.token", &inputs)
            .unwrap());

        // Verify fails with wrong op_id
        assert!(!clock
            .verify_timestamp(&timestamp, "op_wrong", "inference.token", &inputs)
            .unwrap());

        // Verify fails with wrong inputs
        let mut wrong_inputs = HashMap::new();
        wrong_inputs.insert("token_id".to_string(), json!(999));
        assert!(!clock
            .verify_timestamp(&timestamp, "op_test", "inference.token", &wrong_inputs)
            .unwrap());
    }

    #[test]
    fn test_token_position_extraction() {
        let global_seed = B3Hash::hash(b"test_seed");
        let clock = LogicalClock::new(global_seed);

        // Test with token_id
        let mut inputs1 = HashMap::new();
        inputs1.insert("token_id".to_string(), json!(42));
        let ts1 = clock
            .advance_for_operation("op1", "inference.token", &inputs1)
            .unwrap();
        assert_eq!(ts1.token_position, Some(42));

        // Test with token_position
        let mut inputs2 = HashMap::new();
        inputs2.insert("token_position".to_string(), json!(100));
        let ts2 = clock
            .advance_for_operation("op2", "inference.token", &inputs2)
            .unwrap();
        assert_eq!(ts2.token_position, Some(100));

        // Test non-inference event
        let mut inputs3 = HashMap::new();
        inputs3.insert("token_id".to_string(), json!(50));
        let ts3 = clock
            .advance_for_operation("op3", "kernel.execute", &inputs3)
            .unwrap();
        assert_eq!(ts3.token_position, None);
    }

    #[test]
    fn test_concurrent_timestamps() {
        let ts1 = LogicalTimestamp::new(5, 10, None, B3Hash::hash(b"test1"));
        let ts2 = LogicalTimestamp::new(5, 10, None, B3Hash::hash(b"test2"));
        let ts3 = LogicalTimestamp::new(5, 11, None, B3Hash::hash(b"test3"));

        assert!(ts1.concurrent_with(&ts2));
        assert!(!ts1.concurrent_with(&ts3));
    }

    #[test]
    fn test_clock_reset() {
        let global_seed = B3Hash::hash(b"test_seed");
        let clock = LogicalClock::new(global_seed);

        let inputs = HashMap::new();
        clock
            .advance_for_operation("op1", "test.event", &inputs)
            .unwrap();
        clock
            .advance_for_operation("op2", "test.event", &inputs)
            .unwrap();

        assert_eq!(clock.current_tick(), 2);
        assert_eq!(clock.current_event(), 2);

        clock.reset_to(0, 0);

        assert_eq!(clock.current_tick(), 0);
        assert_eq!(clock.current_event(), 0);
    }

    #[test]
    fn test_clock_from_tick() {
        let global_seed = B3Hash::hash(b"test_seed");
        let clock = LogicalClock::new_from_tick(global_seed, 100);

        assert_eq!(clock.current_tick(), 100);

        let inputs = HashMap::new();
        let ts = clock
            .advance_for_operation("op1", "test.event", &inputs)
            .unwrap();

        assert_eq!(ts.global_tick, 100);
    }
}
