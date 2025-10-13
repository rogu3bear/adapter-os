//! Replay integration for memory watchdog
//!
//! Integrates memory monitoring with the replay system to ensure deterministic
//! memory behavior across runs. Logs memory events to replay bundles and
//! verifies memory consistency during replay.

use crate::{MemoryLayoutHash, MemoryMigrationEvent, MemoryWatchdogError, Result};
use adapteros_core::B3Hash;
use adapteros_telemetry::replay::{ReplayBundle, ReplayEvent};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Memory event types for replay logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryEventType {
    /// Memory allocation event
    Allocation,
    /// Memory deallocation event
    Deallocation,
    /// Page migration event
    PageMigration,
    /// Buffer relocation event
    BufferRelocation,
    /// Memory layout change event
    LayoutChange,
    /// Memory pressure event
    MemoryPressure,
    /// Heap compaction event
    HeapCompaction,
}

/// Memory event for replay logging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEvent {
    /// Event ID
    pub event_id: Uuid,
    /// Event type
    pub event_type: MemoryEventType,
    /// Event timestamp
    pub timestamp: u128,
    /// Event hash
    pub event_hash: B3Hash,
    /// Event payload
    pub payload: serde_json::Value,
    /// Memory layout hash at time of event
    pub layout_hash: Option<MemoryLayoutHash>,
}

/// Replay memory logger
pub struct ReplayMemoryLogger {
    /// Memory events
    memory_events: Arc<RwLock<Vec<MemoryEvent>>>,
    /// Event counter
    event_counter: Arc<std::sync::atomic::AtomicU64>,
    /// Logging enabled
    logging_enabled: bool,
    /// Sampling rate (0.0-1.0)
    sampling_rate: f32,
}

impl ReplayMemoryLogger {
    /// Create a new replay memory logger
    pub fn new(logging_enabled: bool, sampling_rate: f32) -> Self {
        Self {
            memory_events: Arc::new(RwLock::new(Vec::new())),
            event_counter: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            logging_enabled,
            sampling_rate: sampling_rate.clamp(0.0, 1.0),
        }
    }

    /// Log memory allocation event
    pub fn log_allocation(
        &self,
        pointer_addr: u64,
        size_bytes: u64,
        context: String,
        layout_hash: Option<MemoryLayoutHash>,
    ) -> Result<()> {
        if !self.logging_enabled || !self.should_sample() {
            return Ok(());
        }

        let event_id = Uuid::new_v4();
        let timestamp = current_timestamp();
        let event_counter = self
            .event_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let payload = serde_json::json!({
            "pointer_addr": pointer_addr,
            "size_bytes": size_bytes,
            "context": context,
            "event_counter": event_counter,
        });

        let event_hash = self.calculate_event_hash(&payload, timestamp);

        let event = MemoryEvent {
            event_id,
            event_type: MemoryEventType::Allocation,
            timestamp,
            event_hash,
            payload,
            layout_hash,
        };

        {
            let mut events = self.memory_events.write();
            events.push(event);
        }

        debug!(
            "Logged memory allocation event: addr=0x{:x}, size={}, counter={}",
            pointer_addr, size_bytes, event_counter
        );

        Ok(())
    }

    /// Log memory deallocation event
    pub fn log_deallocation(
        &self,
        pointer_addr: u64,
        size_bytes: u64,
        context: String,
        layout_hash: Option<MemoryLayoutHash>,
    ) -> Result<()> {
        if !self.logging_enabled || !self.should_sample() {
            return Ok(());
        }

        let event_id = Uuid::new_v4();
        let timestamp = current_timestamp();
        let event_counter = self
            .event_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let payload = serde_json::json!({
            "pointer_addr": pointer_addr,
            "size_bytes": size_bytes,
            "context": context,
            "event_counter": event_counter,
        });

        let event_hash = self.calculate_event_hash(&payload, timestamp);

        let event = MemoryEvent {
            event_id,
            event_type: MemoryEventType::Deallocation,
            timestamp,
            event_hash,
            payload,
            layout_hash,
        };

        {
            let mut events = self.memory_events.write();
            events.push(event);
        }

        debug!(
            "Logged memory deallocation event: addr=0x{:x}, size={}, counter={}",
            pointer_addr, size_bytes, event_counter
        );

        Ok(())
    }

    /// Log page migration event
    pub fn log_page_migration(
        &self,
        migration_event: &MemoryMigrationEvent,
        layout_hash: Option<MemoryLayoutHash>,
    ) -> Result<()> {
        if !self.logging_enabled || !self.should_sample() {
            return Ok(());
        }

        let event_id = Uuid::new_v4();
        let timestamp = current_timestamp();
        let event_counter = self
            .event_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let payload = serde_json::json!({
            "migration_id": migration_event.event_id,
            "migration_type": migration_event.migration_type,
            "source_addr": migration_event.source_addr,
            "dest_addr": migration_event.dest_addr,
            "size_bytes": migration_event.size_bytes,
            "context": migration_event.context,
            "event_counter": event_counter,
        });

        let event_hash = self.calculate_event_hash(&payload, timestamp);

        let event = MemoryEvent {
            event_id,
            event_type: MemoryEventType::PageMigration,
            timestamp,
            event_hash,
            payload,
            layout_hash,
        };

        {
            let mut events = self.memory_events.write();
            events.push(event);
        }

        info!(
            "Logged page migration event: type={:?}, size={}, counter={}",
            migration_event.migration_type, migration_event.size_bytes, event_counter
        );

        Ok(())
    }

    /// Log buffer relocation event
    pub fn log_buffer_relocation(
        &self,
        buffer_id: u64,
        original_addr: u64,
        new_addr: u64,
        size_bytes: u64,
        reason: String,
        layout_hash: Option<MemoryLayoutHash>,
    ) -> Result<()> {
        if !self.logging_enabled || !self.should_sample() {
            return Ok(());
        }

        let event_id = Uuid::new_v4();
        let timestamp = current_timestamp();
        let event_counter = self
            .event_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let payload = serde_json::json!({
            "buffer_id": buffer_id,
            "original_addr": original_addr,
            "new_addr": new_addr,
            "size_bytes": size_bytes,
            "reason": reason,
            "event_counter": event_counter,
        });

        let event_hash = self.calculate_event_hash(&payload, timestamp);

        let event = MemoryEvent {
            event_id,
            event_type: MemoryEventType::BufferRelocation,
            timestamp,
            event_hash,
            payload,
            layout_hash,
        };

        {
            let mut events = self.memory_events.write();
            events.push(event);
        }

        info!(
            "Logged buffer relocation event: buffer_id={}, 0x{:x} -> 0x{:x}, counter={}",
            buffer_id, original_addr, new_addr, event_counter
        );

        Ok(())
    }

    /// Log memory layout change event
    pub fn log_layout_change(
        &self,
        old_layout_hash: &MemoryLayoutHash,
        new_layout_hash: &MemoryLayoutHash,
        change_reason: String,
    ) -> Result<()> {
        if !self.logging_enabled || !self.should_sample() {
            return Ok(());
        }

        let event_id = Uuid::new_v4();
        let timestamp = current_timestamp();
        let event_counter = self
            .event_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let payload = serde_json::json!({
            "old_layout_hash": old_layout_hash.layout_hash,
            "new_layout_hash": new_layout_hash.layout_hash,
            "change_reason": change_reason,
            "event_counter": event_counter,
        });

        let event_hash = self.calculate_event_hash(&payload, timestamp);

        let event = MemoryEvent {
            event_id,
            event_type: MemoryEventType::LayoutChange,
            timestamp,
            event_hash,
            payload,
            layout_hash: Some(new_layout_hash.clone()),
        };

        {
            let mut events = self.memory_events.write();
            events.push(event);
        }

        info!(
            "Logged memory layout change event: reason={}, counter={}",
            change_reason, event_counter
        );

        Ok(())
    }

    /// Log memory pressure event
    pub fn log_memory_pressure(
        &self,
        pressure_level: f32,
        total_memory: u64,
        used_memory: u64,
        context: String,
    ) -> Result<()> {
        if !self.logging_enabled || !self.should_sample() {
            return Ok(());
        }

        let event_id = Uuid::new_v4();
        let timestamp = current_timestamp();
        let event_counter = self
            .event_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let payload = serde_json::json!({
            "pressure_level": pressure_level,
            "total_memory": total_memory,
            "used_memory": used_memory,
            "context": context,
            "event_counter": event_counter,
        });

        let event_hash = self.calculate_event_hash(&payload, timestamp);

        let event = MemoryEvent {
            event_id,
            event_type: MemoryEventType::MemoryPressure,
            timestamp,
            event_hash,
            payload,
            layout_hash: None,
        };

        {
            let mut events = self.memory_events.write();
            events.push(event);
        }

        warn!(
            "Logged memory pressure event: level={:.2}, used={}/{} ({}%), counter={}",
            pressure_level,
            used_memory,
            total_memory,
            (used_memory as f32 / total_memory as f32) * 100.0,
            event_counter
        );

        Ok(())
    }

    /// Convert memory events to replay events
    pub fn to_replay_events(&self) -> Vec<ReplayEvent> {
        let events = self.memory_events.read();

        events
            .iter()
            .map(|event| ReplayEvent {
                event_type: format!(
                    "memory.{}",
                    format!("{:?}", event.event_type).to_lowercase()
                ),
                timestamp: event.timestamp,
                event_hash: event.event_hash,
                payload: event.payload.clone(),
            })
            .collect()
    }

    /// Create replay bundle from memory events
    pub fn create_replay_bundle(
        &self,
        cpid: String,
        plan_id: String,
        seed_global: B3Hash,
    ) -> ReplayBundle {
        let replay_events = self.to_replay_events();

        ReplayBundle {
            cpid,
            plan_id,
            seed_global,
            events: replay_events,
            rng_checkpoints: Vec::new(),
        }
    }

    /// Verify memory consistency during replay
    pub fn verify_replay_consistency(&self, expected_events: &[MemoryEvent]) -> Result<()> {
        let actual_events = {
            let events = self.memory_events.read();
            events.clone()
        };

        if actual_events.len() != expected_events.len() {
            return Err(MemoryWatchdogError::MemoryLayoutMismatch {
                expected: format!("{} events", expected_events.len()),
                actual: format!("{} events", actual_events.len()),
            });
        }

        for (i, (actual, expected)) in actual_events.iter().zip(expected_events.iter()).enumerate()
        {
            if actual.event_hash != expected.event_hash {
                return Err(MemoryWatchdogError::MemoryLayoutMismatch {
                    expected: format!("event {} hash {:?}", i, expected.event_hash),
                    actual: format!("event {} hash {:?}", i, actual.event_hash),
                });
            }
        }

        Ok(())
    }

    /// Calculate event hash
    fn calculate_event_hash(&self, payload: &serde_json::Value, timestamp: u128) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash payload deterministically
        if let Ok(payload_bytes) = serde_json::to_vec(payload) {
            hasher.update(&payload_bytes);
        }

        // Hash timestamp
        hasher.update(&timestamp.to_le_bytes());

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Check if we should sample this event
    fn should_sample(&self) -> bool {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        current_timestamp().hash(&mut hasher);
        let hash = hasher.finish();

        // Use hash to determine sampling
        (hash as f32 / u64::MAX as f32) < self.sampling_rate
    }

    /// Get memory events
    pub fn get_memory_events(&self) -> Vec<MemoryEvent> {
        let events = self.memory_events.read();
        events.clone()
    }

    /// Get event count
    pub fn get_event_count(&self) -> usize {
        let events = self.memory_events.read();
        events.len()
    }

    /// Clear all recorded events
    pub fn clear(&self) {
        {
            let mut events = self.memory_events.write();
            events.clear();
        }

        self.event_counter
            .store(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Enable or disable logging
    pub fn set_logging_enabled(&mut self, enabled: bool) {
        self.logging_enabled = enabled;
        info!(
            "Memory replay logging {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Check if logging is enabled
    pub fn is_logging_enabled(&self) -> bool {
        self.logging_enabled
    }

    /// Set sampling rate
    pub fn set_sampling_rate(&mut self, rate: f32) {
        self.sampling_rate = rate.clamp(0.0, 1.0);
        info!(
            "Memory event sampling rate set to {:.2}",
            self.sampling_rate
        );
    }

    /// Get sampling rate
    pub fn get_sampling_rate(&self) -> f32 {
        self.sampling_rate
    }
}

/// Get current timestamp in microseconds
fn current_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_memory_logger_creation() {
        let logger = ReplayMemoryLogger::new(true, 1.0);
        assert!(logger.is_logging_enabled());
        assert_eq!(logger.get_sampling_rate(), 1.0);
        assert_eq!(logger.get_event_count(), 0);
    }

    #[test]
    fn test_memory_allocation_logging() {
        let logger = ReplayMemoryLogger::new(true, 1.0);

        logger
            .log_allocation(0x1000, 1024, "test allocation".to_string(), None)
            .unwrap();

        assert_eq!(logger.get_event_count(), 1);

        let events = logger.get_memory_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].event_type, MemoryEventType::Allocation));
    }

    #[test]
    fn test_memory_deallocation_logging() {
        let logger = ReplayMemoryLogger::new(true, 1.0);

        logger
            .log_deallocation(0x1000, 1024, "test deallocation".to_string(), None)
            .unwrap();

        assert_eq!(logger.get_event_count(), 1);

        let events = logger.get_memory_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            MemoryEventType::Deallocation
        ));
    }

    #[test]
    fn test_page_migration_logging() {
        let logger = ReplayMemoryLogger::new(true, 1.0);

        let migration_event = MemoryMigrationEvent {
            event_id: Uuid::new_v4(),
            migration_type: crate::MigrationType::PageOut,
            source_addr: Some(0x1000),
            dest_addr: None,
            size_bytes: 4096,
            timestamp: current_timestamp(),
            context: serde_json::json!({"reason": "memory_pressure"}),
        };

        logger.log_page_migration(&migration_event, None).unwrap();

        assert_eq!(logger.get_event_count(), 1);

        let events = logger.get_memory_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            MemoryEventType::PageMigration
        ));
    }

    #[test]
    fn test_buffer_relocation_logging() {
        let logger = ReplayMemoryLogger::new(true, 1.0);

        logger
            .log_buffer_relocation(1, 0x1000, 0x2000, 1024, "memory_pressure".to_string(), None)
            .unwrap();

        assert_eq!(logger.get_event_count(), 1);

        let events = logger.get_memory_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            MemoryEventType::BufferRelocation
        ));
    }

    #[test]
    fn test_memory_pressure_logging() {
        let logger = ReplayMemoryLogger::new(true, 1.0);

        logger
            .log_memory_pressure(
                0.85,
                1024 * 1024 * 1024, // 1GB
                850 * 1024 * 1024,  // 850MB
                "high_usage".to_string(),
            )
            .unwrap();

        assert_eq!(logger.get_event_count(), 1);

        let events = logger.get_memory_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            MemoryEventType::MemoryPressure
        ));
    }

    #[test]
    fn test_replay_bundle_creation() {
        let logger = ReplayMemoryLogger::new(true, 1.0);

        // Log some events
        logger
            .log_allocation(0x1000, 1024, "test".to_string(), None)
            .unwrap();
        logger
            .log_deallocation(0x1000, 1024, "test".to_string(), None)
            .unwrap();

        let bundle = logger.create_replay_bundle(
            "test-cpid".to_string(),
            "test-plan".to_string(),
            B3Hash::hash(b"test-seed"),
        );

        assert_eq!(bundle.cpid, "test-cpid");
        assert_eq!(bundle.plan_id, "test-plan");
        assert_eq!(bundle.events.len(), 2);
    }

    #[test]
    fn test_replay_consistency_verification() {
        let logger = ReplayMemoryLogger::new(true, 1.0);

        // Log some events
        logger
            .log_allocation(0x1000, 1024, "test".to_string(), None)
            .unwrap();
        logger
            .log_deallocation(0x1000, 1024, "test".to_string(), None)
            .unwrap();

        let expected_events = logger.get_memory_events();

        // Verify consistency
        logger.verify_replay_consistency(&expected_events).unwrap();

        // Clear and verify inconsistency
        logger.clear();
        let result = logger.verify_replay_consistency(&expected_events);
        assert!(result.is_err());
    }

    #[test]
    fn test_logging_enable_disable() {
        let mut logger = ReplayMemoryLogger::new(true, 1.0);
        assert!(logger.is_logging_enabled());

        logger.set_logging_enabled(false);
        assert!(!logger.is_logging_enabled());

        // Logging should be skipped when disabled
        logger
            .log_allocation(0x1000, 1024, "test".to_string(), None)
            .unwrap();
        assert_eq!(logger.get_event_count(), 0);

        logger.set_logging_enabled(true);
        assert!(logger.is_logging_enabled());
    }

    #[test]
    fn test_sampling_rate() {
        let mut logger = ReplayMemoryLogger::new(true, 0.5);
        assert_eq!(logger.get_sampling_rate(), 0.5);

        logger.set_sampling_rate(0.8);
        assert_eq!(logger.get_sampling_rate(), 0.8);

        // Test clamping
        logger.set_sampling_rate(1.5);
        assert_eq!(logger.get_sampling_rate(), 1.0);

        logger.set_sampling_rate(-0.5);
        assert_eq!(logger.get_sampling_rate(), 0.0);
    }
}
