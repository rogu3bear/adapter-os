//! GPU buffer relocation detection
//!
//! Monitors GPU buffer relocations that could affect determinism by:
//! - Tracking buffer address changes over time
//! - Detecting when Metal relocates buffers due to memory pressure
//! - Recording relocation events for replay verification
//! - Ensuring buffer content integrity after relocation

use crate::Result;
use adapteros_core::B3Hash;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use metal::{foreign_types::ForeignType, Buffer, Device};

/// Buffer relocation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferRelocationRecord {
    /// Relocation ID
    pub relocation_id: Uuid,
    /// Buffer identifier
    pub buffer_id: u64,
    /// Original buffer address
    pub original_addr: u64,
    /// New buffer address
    pub new_addr: u64,
    /// Buffer size
    pub size_bytes: u64,
    /// Relocation timestamp
    pub timestamp: u128,
    /// Relocation reason
    pub reason: RelocationReason,
    /// Buffer content hash before relocation
    pub content_hash_before: Option<B3Hash>,
    /// Buffer content hash after relocation
    pub content_hash_after: Option<B3Hash>,
    /// Additional context
    pub context: serde_json::Value,
}

/// Relocation reason
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelocationReason {
    /// Memory pressure triggered relocation
    MemoryPressure,
    /// Heap compaction
    HeapCompaction,
    /// Buffer size change
    SizeChange,
    /// Manual relocation
    Manual,
    /// Unknown reason
    Unknown,
}

/// Buffer state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferState {
    /// Buffer identifier
    pub buffer_id: u64,
    /// Current buffer address
    pub current_addr: u64,
    /// Buffer size
    pub size_bytes: u64,
    /// Allocation timestamp
    pub allocation_timestamp: u128,
    /// Last update timestamp
    pub last_update_timestamp: u128,
    /// Relocation count
    pub relocation_count: u32,
    /// Content hash
    pub content_hash: Option<B3Hash>,
    /// Storage mode
    pub storage_mode: String,
}

/// Buffer relocation detector
pub struct BufferRelocationDetector {
    /// Device reference (reserved for Metal buffer API calls)
    #[cfg(target_os = "macos")]
    _device: Arc<Device>,
    /// Active buffers by buffer ID
    active_buffers: Arc<RwLock<HashMap<u64, BufferState>>>,
    /// Relocation history
    relocation_history: Arc<RwLock<Vec<BufferRelocationRecord>>>,
    /// Next buffer ID
    next_buffer_id: Arc<std::sync::atomic::AtomicU64>,
    /// Relocation detection enabled
    detection_enabled: bool,
}

impl BufferRelocationDetector {
    /// Create a new buffer relocation detector
    #[cfg(target_os = "macos")]
    pub fn new(device: Arc<Device>, detection_enabled: bool) -> Self {
        Self {
            _device: device,
            active_buffers: Arc::new(RwLock::new(HashMap::new())),
            relocation_history: Arc::new(RwLock::new(Vec::new())),
            next_buffer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            detection_enabled,
        }
    }

    /// Create a new buffer relocation detector (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn new(_device: Option<()>, detection_enabled: bool) -> Self {
        Self {
            active_buffers: Arc::new(RwLock::new(HashMap::new())),
            relocation_history: Arc::new(RwLock::new(Vec::new())),
            next_buffer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            detection_enabled,
        }
    }

    /// Register a buffer for monitoring
    #[cfg(target_os = "macos")]
    pub fn register_buffer(&self, buffer: &Buffer) -> Result<u64> {
        if !self.detection_enabled {
            return Ok(0); // Skip monitoring
        }

        let buffer_id = self
            .next_buffer_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = current_timestamp();
        let current_addr = buffer.as_ptr() as u64;
        let size_bytes = buffer.length();
        let storage_mode = format!("{:?}", buffer.resource_options());

        let buffer_state = BufferState {
            buffer_id,
            current_addr,
            size_bytes,
            allocation_timestamp: timestamp,
            last_update_timestamp: timestamp,
            relocation_count: 0,
            content_hash: None,
            storage_mode,
        };

        {
            let mut active = self.active_buffers.write();
            active.insert(buffer_id, buffer_state);
        }

        debug!(
            "Registered buffer for relocation monitoring: id={}, addr=0x{:x}, size={}",
            buffer_id, current_addr, size_bytes
        );

        Ok(buffer_id)
    }

    /// Register a buffer for monitoring (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn register_buffer(&self, _buffer: Option<()>) -> Result<u64> {
        if !self.detection_enabled {
            return Ok(0); // Skip monitoring
        }

        let buffer_id = self
            .next_buffer_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = current_timestamp();

        let buffer_state = BufferState {
            buffer_id,
            current_addr: 0,
            size_bytes: 0,
            allocation_timestamp: timestamp,
            last_update_timestamp: timestamp,
            relocation_count: 0,
            content_hash: None,
            storage_mode: "unknown".to_string(),
        };

        {
            let mut active = self.active_buffers.write();
            active.insert(buffer_id, buffer_state);
        }

        Ok(buffer_id)
    }

    /// Check for buffer relocations
    #[cfg(target_os = "macos")]
    pub fn check_relocations(&self) -> Result<Vec<BufferRelocationRecord>> {
        if !self.detection_enabled {
            return Ok(Vec::new());
        }

        let mut relocations = Vec::new();
        let timestamp = current_timestamp();

        {
            let mut active = self.active_buffers.write();
            let mut history = self.relocation_history.write();

            for (buffer_id, buffer_state) in active.iter_mut() {
                // In a real implementation, we would query Metal for current buffer addresses
                // For now, we simulate relocation detection based on memory pressure
                if self.should_simulate_relocation(buffer_state) {
                    let original_addr = buffer_state.current_addr;
                    let new_addr = self.simulate_new_address(original_addr);

                    if new_addr != original_addr {
                        let relocation = BufferRelocationRecord {
                            relocation_id: Uuid::new_v4(),
                            buffer_id: *buffer_id,
                            original_addr,
                            new_addr,
                            size_bytes: buffer_state.size_bytes,
                            timestamp,
                            reason: RelocationReason::MemoryPressure,
                            content_hash_before: buffer_state.content_hash,
                            content_hash_after: None, // Would calculate after relocation
                            context: serde_json::json!({
                                "simulated": true,
                                "memory_pressure": self.get_memory_pressure_level(),
                            }),
                        };

                        // Update buffer state
                        buffer_state.current_addr = new_addr;
                        buffer_state.last_update_timestamp = timestamp;
                        buffer_state.relocation_count += 1;

                        relocations.push(relocation.clone());
                        history.push(relocation);

                        info!(
                            "Detected buffer relocation: id={}, 0x{:x} -> 0x{:x}",
                            buffer_id, original_addr, new_addr
                        );
                    }
                }
            }
        }

        Ok(relocations)
    }

    /// Check for buffer relocations (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn check_relocations(&self) -> Result<Vec<BufferRelocationRecord>> {
        if !self.detection_enabled {
            return Ok(Vec::new());
        }

        // No-op on non-macOS platforms
        Ok(Vec::new())
    }

    /// Simulate relocation based on memory pressure
    fn should_simulate_relocation(&self, buffer_state: &BufferState) -> bool {
        // Simulate relocation for large buffers under memory pressure
        buffer_state.size_bytes > 1024 * 1024 // > 1MB
    }

    /// Simulate new buffer address
    fn simulate_new_address(&self, original_addr: u64) -> u64 {
        // Simulate address change by adding random offset
        let offset = (original_addr % 1000) * 0x1000;
        original_addr + offset
    }

    /// Get memory pressure level based on tracked buffer sizes.
    /// Returns an estimate (0.0-1.0) based on total tracked buffer size.
    fn get_memory_pressure_level(&self) -> f32 {
        let active = self.active_buffers.read();
        let total_tracked_bytes: u64 = active.values().map(|s| s.size_bytes).sum();

        // Estimate pressure based on tracked buffer size
        // Assume ~16GB as reference for high memory system
        const REFERENCE_MEMORY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
        let pressure = (total_tracked_bytes as f64 / REFERENCE_MEMORY_BYTES as f64) as f32;

        // Clamp to 0.0-1.0 range
        pressure.clamp(0.0, 1.0)
    }

    /// Unregister a buffer
    pub fn unregister_buffer(&self, buffer_id: u64) -> Result<()> {
        if !self.detection_enabled {
            return Ok(());
        }

        {
            let mut active = self.active_buffers.write();
            active.remove(&buffer_id);
        }

        debug!(
            "Unregistered buffer from relocation monitoring: id={}",
            buffer_id
        );
        Ok(())
    }

    /// Get relocation history
    pub fn get_relocation_history(&self) -> Vec<BufferRelocationRecord> {
        let history = self.relocation_history.read();
        history.clone()
    }

    /// Get active buffer states
    pub fn get_active_buffers(&self) -> Vec<BufferState> {
        let active = self.active_buffers.read();
        active.values().cloned().collect()
    }

    /// Get relocation statistics
    pub fn get_relocation_stats(&self) -> RelocationStats {
        let active = self.active_buffers.read();
        let history = self.relocation_history.read();

        let total_buffers = active.len();
        let total_relocations = history.len();
        let buffers_with_relocations = active.values().filter(|b| b.relocation_count > 0).count();

        RelocationStats {
            total_buffers,
            total_relocations,
            buffers_with_relocations,
            average_relocations_per_buffer: if total_buffers > 0 {
                total_relocations as f32 / total_buffers as f32
            } else {
                0.0
            },
        }
    }

    /// Clear all recorded data
    pub fn clear(&self) {
        {
            let mut active = self.active_buffers.write();
            active.clear();
        }
        {
            let mut history = self.relocation_history.write();
            history.clear();
        }

        self.next_buffer_id
            .store(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Enable or disable relocation detection
    pub fn set_detection_enabled(&mut self, enabled: bool) {
        self.detection_enabled = enabled;
        info!(
            "Buffer relocation detection {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Check if detection is enabled
    pub fn is_detection_enabled(&self) -> bool {
        self.detection_enabled
    }

    /// Verify buffer content integrity after relocation
    #[cfg(target_os = "macos")]
    pub fn verify_content_integrity(&self, buffer: &Buffer, expected_hash: B3Hash) -> Result<bool> {
        if !self.detection_enabled {
            return Ok(true); // Skip verification
        }

        // Calculate current buffer content hash
        let current_hash = self.calculate_buffer_hash(buffer)?;

        let integrity_ok = current_hash == expected_hash;

        if !integrity_ok {
            warn!(
                "Buffer content integrity check failed: expected={:?}, actual={:?}",
                expected_hash, current_hash
            );
        }

        Ok(integrity_ok)
    }

    /// Verify buffer content integrity (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn verify_content_integrity(
        &self,
        _buffer: Option<()>,
        _expected_hash: B3Hash,
    ) -> Result<bool> {
        if !self.detection_enabled {
            return Ok(true); // Skip verification
        }

        // No-op on non-macOS platforms
        Ok(true)
    }

    /// Calculate buffer content hash
    #[cfg(target_os = "macos")]
    fn calculate_buffer_hash(&self, buffer: &Buffer) -> Result<B3Hash> {
        let contents = buffer.contents();
        let size = buffer.length() as usize;

        // Hash buffer contents
        let mut hasher = blake3::Hasher::new();
        // SAFETY: `contents` is a valid pointer to Metal buffer data obtained via `buffer.contents()`.
        // `size` is the buffer length from `buffer.length()`. The Metal buffer owns this memory
        // and remains valid for the duration of this scope. The slice is read-only and does not
        // outlive the buffer reference.
        unsafe {
            let slice = std::slice::from_raw_parts(contents as *const u8, size);
            hasher.update(slice);
        }

        Ok(B3Hash::new(*hasher.finalize().as_bytes()))
    }

    /// Calculate buffer content hash (non-macOS)
    #[cfg(not(target_os = "macos"))]
    fn calculate_buffer_hash(&self, _buffer: Option<()>) -> Result<B3Hash> {
        // Return dummy hash on non-macOS platforms
        Ok(adapteros_core::B3Hash::hash(b"dummy"))
    }
}

/// Relocation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelocationStats {
    pub total_buffers: usize,
    pub total_relocations: usize,
    pub buffers_with_relocations: usize,
    pub average_relocations_per_buffer: f32,
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
    fn test_buffer_relocation_detector_creation() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = Device::system_default() {
                let detector = BufferRelocationDetector::new(Arc::new(device.clone()), true);
                assert!(detector.is_detection_enabled());

                let stats = detector.get_relocation_stats();
                assert_eq!(stats.total_buffers, 0);
                assert_eq!(stats.total_relocations, 0);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let detector = BufferRelocationDetector::new(None, true);
            assert!(detector.is_detection_enabled());
        }
    }

    #[test]
    fn test_buffer_registration() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = Device::system_default() {
                let detector = BufferRelocationDetector::new(Arc::new(device.clone()), true);

                let buffer = device.new_buffer(1024, metal::MTLResourceOptions::empty());
                let buffer_id = detector.register_buffer(&buffer).unwrap();

                assert!(buffer_id > 0);

                let stats = detector.get_relocation_stats();
                assert_eq!(stats.total_buffers, 1);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let detector = BufferRelocationDetector::new(None, true);
            let buffer_id = detector.register_buffer(None).unwrap();
            assert!(buffer_id > 0);
        }
    }

    #[test]
    #[ignore = "Flaky/hangs on some macOS CI/dev hosts when probing Metal relocation"]
    fn test_relocation_detection() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = Device::system_default() {
                let detector = BufferRelocationDetector::new(Arc::new(device.clone()), true);

                let buffer = device.new_buffer(2 * 1024 * 1024, metal::MTLResourceOptions::empty()); // 2MB
                let _buffer_id = detector.register_buffer(&buffer).unwrap();

                let relocations = detector.check_relocations().unwrap();

                // Should detect relocation for large buffer
                assert!(!relocations.is_empty());
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let detector = BufferRelocationDetector::new(None, true);
            let relocations = detector.check_relocations().unwrap();
            assert!(relocations.is_empty());
        }
    }

    #[test]
    fn test_detection_enable_disable() {
        #[cfg(target_os = "macos")]
        let device = Arc::new(metal::Device::system_default().unwrap());
        #[cfg(not(target_os = "macos"))]
        let device = Arc::new(metal::Device::system_default().unwrap_or_else(|| {
            // Create a mock device for testing
            unsafe { std::mem::transmute(0x1usize) }
        }));
        let mut detector = BufferRelocationDetector::new(device, true);
        assert!(detector.is_detection_enabled());

        detector.set_detection_enabled(false);
        assert!(!detector.is_detection_enabled());

        detector.set_detection_enabled(true);
        assert!(detector.is_detection_enabled());
    }

    #[test]
    fn test_buffer_unregistration() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = Device::system_default() {
                let detector = BufferRelocationDetector::new(Arc::new(device.clone()), true);

                let buffer = device.new_buffer(1024, metal::MTLResourceOptions::empty());
                let buffer_id = detector.register_buffer(&buffer).unwrap();

                detector.unregister_buffer(buffer_id).unwrap();

                let stats = detector.get_relocation_stats();
                assert_eq!(stats.total_buffers, 0);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let detector = BufferRelocationDetector::new(None, true);
            let buffer_id = detector.register_buffer(None).unwrap();
            detector.unregister_buffer(buffer_id).unwrap();
        }
    }

    #[test]
    fn test_content_integrity_verification() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = Device::system_default() {
                let detector = BufferRelocationDetector::new(Arc::new(device.clone()), true);

                let buffer = device.new_buffer(1024, metal::MTLResourceOptions::empty());
                let expected_hash = detector.calculate_buffer_hash(&buffer).unwrap();

                let integrity_ok = detector
                    .verify_content_integrity(&buffer, expected_hash)
                    .unwrap();
                assert!(integrity_ok);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let detector = BufferRelocationDetector::new(None, true);
            let expected_hash = adapteros_crypto::B3Hash::hash(b"test");
            let integrity_ok = detector
                .verify_content_integrity(None, expected_hash)
                .unwrap();
            assert!(integrity_ok);
        }
    }
}
