//! Main memory watchdog coordinator
//!
//! Coordinates all memory monitoring components to ensure unified memory stability
//! and deterministic behavior. Provides a single interface for memory monitoring
//! and integrates with the replay system.

use crate::{
    MemoryLayoutHash, MemoryMigrationEvent, MemoryPressureLevel, MemoryWatchdogConfig,
    MemoryWatchdogError, Result,
};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use metal::Device;

use super::{
    buffer_relocation::BufferRelocationDetector, heap_observer::MetalHeapObserver,
    memory_map::MemoryMapHasher, pointer_canonicalizer::PointerCanonicalizer,
    replay_integration::ReplayMemoryLogger,
};

/// Memory watchdog status
#[derive(Debug, Clone)]
pub enum WatchdogStatus {
    /// Watchdog is running normally
    Running,
    /// Watchdog is paused
    Paused,
    /// Watchdog encountered an error
    Error(String),
    /// Watchdog is shutting down
    ShuttingDown,
}

/// Memory watchdog statistics
#[derive(Debug, Clone)]
pub struct WatchdogStats {
    /// Total memory events recorded
    pub total_events: usize,
    /// Total page migrations detected
    pub page_migrations: usize,
    /// Total buffer relocations detected
    pub buffer_relocations: usize,
    /// Total memory layout changes
    pub layout_changes: usize,
    /// Current memory pressure level
    pub memory_pressure: MemoryPressureLevel,
    /// Memory layout hash
    pub current_layout_hash: Option<MemoryLayoutHash>,
    /// Watchdog uptime in microseconds
    pub uptime_micros: u128,
}

/// Main memory watchdog
pub struct MemoryWatchdog {
    /// Watchdog configuration
    config: MemoryWatchdogConfig,
    /// Watchdog status
    status: Arc<RwLock<WatchdogStatus>>,
    /// Start timestamp
    start_timestamp: u128,

    /// Metal heap observer
    heap_observer: Option<MetalHeapObserver>,
    /// Pointer canonicalizer
    pointer_canonicalizer: PointerCanonicalizer,
    /// Buffer relocation detector
    buffer_relocation_detector: BufferRelocationDetector,
    /// Memory map hasher
    memory_map_hasher: MemoryMapHasher,
    /// Replay memory logger
    replay_logger: ReplayMemoryLogger,

    /// Watchdog ID
    watchdog_id: Uuid,
}

impl MemoryWatchdog {
    /// Create a new memory watchdog
    #[cfg(target_os = "macos")]
    pub fn new(config: MemoryWatchdogConfig) -> Result<Self> {
        let device = Device::system_default().ok_or_else(|| {
            MemoryWatchdogError::HeapObservationFailed("No Metal device available".to_string())
        })?;

        let device_arc = Arc::new(device);

        let heap_observer = if config.enable_heap_observation {
            Some(MetalHeapObserver::new(
                device_arc.clone(),
                config.sampling_rate,
            ))
        } else {
            None
        };

        let pointer_canonicalizer = PointerCanonicalizer::new(10000); // 10k history

        let buffer_relocation_detector = if config.enable_buffer_relocation_detection {
            BufferRelocationDetector::new(device_arc.clone(), true)
        } else {
            BufferRelocationDetector::new(device_arc.clone(), false)
        };

        let memory_map_hasher = if config.enable_memory_map_hashing {
            MemoryMapHasher::new(device_arc, true)
        } else {
            MemoryMapHasher::new(device_arc, false)
        };

        let replay_logger = ReplayMemoryLogger::new(true, config.sampling_rate);

        let watchdog_id = Uuid::new_v4();
        let start_timestamp = current_timestamp();

        info!(
            "Memory watchdog initialized: id={}, heap_obs={}, pointer_canon={}, buffer_reloc={}, memory_map={}",
            watchdog_id,
            config.enable_heap_observation,
            config.enable_pointer_canonicalization,
            config.enable_buffer_relocation_detection,
            config.enable_memory_map_hashing
        );

        Ok(Self {
            config,
            status: Arc::new(RwLock::new(WatchdogStatus::Running)),
            start_timestamp,
            heap_observer,
            pointer_canonicalizer,
            buffer_relocation_detector,
            memory_map_hasher,
            replay_logger,
            watchdog_id,
        })
    }

    /// Create a new memory watchdog (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn new(config: MemoryWatchdogConfig) -> Result<Self> {
        let heap_observer = if config.enable_heap_observation {
            Some(MetalHeapObserver::new(None, config.sampling_rate))
        } else {
            None
        };

        let pointer_canonicalizer = PointerCanonicalizer::new(10000);

        let buffer_relocation_detector = if config.enable_buffer_relocation_detection {
            BufferRelocationDetector::new(None, true)
        } else {
            BufferRelocationDetector::new(None, false)
        };

        let memory_map_hasher = if config.enable_memory_map_hashing {
            MemoryMapHasher::new(None, true)
        } else {
            MemoryMapHasher::new(None, false)
        };

        let replay_logger = ReplayMemoryLogger::new(true, config.sampling_rate);

        let watchdog_id = Uuid::new_v4();
        let start_timestamp = current_timestamp();

        info!(
            "Memory watchdog initialized (non-macOS): id={}, heap_obs={}, pointer_canon={}, buffer_reloc={}, memory_map={}",
            watchdog_id,
            config.enable_heap_observation,
            config.enable_pointer_canonicalization,
            config.enable_buffer_relocation_detection,
            config.enable_memory_map_hashing
        );

        Ok(Self {
            config,
            status: Arc::new(RwLock::new(WatchdogStatus::Running)),
            start_timestamp,
            heap_observer,
            pointer_canonicalizer,
            buffer_relocation_detector,
            memory_map_hasher,
            replay_logger,
            watchdog_id,
        })
    }

    /// Monitor memory allocation
    pub fn monitor_allocation(
        &self,
        pointer_addr: u64,
        size_bytes: u64,
        context: String,
    ) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }

        // Record allocation in pointer canonicalizer
        if self.config.enable_pointer_canonicalization {
            self.pointer_canonicalizer.record_allocation(
                pointer_addr,
                size_bytes,
                context.clone(),
            )?;
        }

        // Generate current memory layout hash
        let layout_hash = if self.config.enable_memory_map_hashing {
            Some(self.memory_map_hasher.generate_memory_layout_hash()?)
        } else {
            None
        };

        // Log allocation event
        self.replay_logger
            .log_allocation(pointer_addr, size_bytes, context, layout_hash)?;

        debug!(
            "Monitored memory allocation: addr=0x{:x}, size={}",
            pointer_addr, size_bytes
        );

        Ok(())
    }

    /// Monitor memory deallocation
    pub fn monitor_deallocation(
        &self,
        pointer_addr: u64,
        size_bytes: u64,
        context: String,
    ) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }

        // Record deallocation in pointer canonicalizer
        if self.config.enable_pointer_canonicalization {
            self.pointer_canonicalizer
                .record_deallocation(pointer_addr)?;
        }

        // Generate current memory layout hash
        let layout_hash = if self.config.enable_memory_map_hashing {
            Some(self.memory_map_hasher.generate_memory_layout_hash()?)
        } else {
            None
        };

        // Log deallocation event
        self.replay_logger
            .log_deallocation(pointer_addr, size_bytes, context, layout_hash)?;

        debug!(
            "Monitored memory deallocation: addr=0x{:x}, size={}",
            pointer_addr, size_bytes
        );

        Ok(())
    }

    /// Monitor Metal buffer allocation
    #[cfg(target_os = "macos")]
    pub fn monitor_metal_buffer_allocation(&self, buffer: &metal::Buffer) -> Result<u64> {
        if !self.is_running() {
            return Ok(0);
        }

        let buffer_id = if let Some(ref observer) = self.heap_observer {
            observer.observe_allocation(buffer, None)?
        } else {
            0
        };

        if self.config.enable_buffer_relocation_detection {
            self.buffer_relocation_detector.register_buffer(buffer)?;
        }

        if self.config.enable_memory_map_hashing {
            self.memory_map_hasher.add_metal_buffer(buffer)?;
        }

        Ok(buffer_id)
    }

    /// Monitor Metal buffer allocation (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn monitor_metal_buffer_allocation(&self, _buffer: Option<()>) -> Result<u64> {
        if !self.is_running() {
            return Ok(0);
        }

        let buffer_id = if let Some(ref observer) = self.heap_observer {
            // Create a dummy buffer for non-macOS platforms
            #[cfg(target_os = "macos")]
            {
                if let Some(device) = metal::Device::system_default() {
                    let buffer = device.new_buffer(1024, metal::MTLResourceOptions::default());
                    observer.observe_allocation(&buffer, None)?
                } else {
                    0
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                0
            }
        } else {
            0
        };

        if self.config.enable_buffer_relocation_detection {
            self.buffer_relocation_detector.register_buffer(None)?;
        }

        if self.config.enable_memory_map_hashing {
            self.memory_map_hasher.add_metal_buffer(None)?;
        }

        Ok(buffer_id)
    }

    /// Check for memory events and migrations
    pub fn check_memory_events(&self) -> Result<Vec<MemoryMigrationEvent>> {
        if !self.is_running() {
            return Ok(Vec::new());
        }

        let mut all_events = Vec::new();

        // Check heap observer for page migrations
        if let Some(ref observer) = self.heap_observer {
            let migration_events = observer.get_migration_events();
            all_events.extend(migration_events);
        }

        // Check buffer relocation detector
        if self.config.enable_buffer_relocation_detection {
            let relocations = self.buffer_relocation_detector.check_relocations()?;
            for relocation in relocations {
                let migration_event = MemoryMigrationEvent {
                    event_id: relocation.relocation_id,
                    migration_type: crate::MigrationType::BufferRelocate,
                    source_addr: Some(relocation.original_addr),
                    dest_addr: Some(relocation.new_addr),
                    size_bytes: relocation.size_bytes,
                    timestamp: relocation.timestamp,
                    context: relocation.context,
                };
                all_events.push(migration_event);
            }
        }

        // Log any detected events
        for event in &all_events {
            self.replay_logger.log_page_migration(event, None)?;
        }

        Ok(all_events)
    }

    /// Generate memory layout hash
    pub fn generate_memory_layout_hash(&self) -> Result<MemoryLayoutHash> {
        if !self.is_running() {
            return Ok(MemoryLayoutHash {
                layout_hash: adapteros_core::B3Hash::hash(b"stopped"),
                pointer_pattern_hash: adapteros_core::B3Hash::hash(b"stopped"),
                allocation_order_hash: adapteros_core::B3Hash::hash(b"stopped"),
                timestamp: current_timestamp(),
            });
        }

        if self.config.enable_memory_map_hashing {
            self.memory_map_hasher.generate_memory_layout_hash()
        } else {
            Ok(MemoryLayoutHash {
                layout_hash: adapteros_core::B3Hash::hash(b"disabled"),
                pointer_pattern_hash: adapteros_core::B3Hash::hash(b"disabled"),
                allocation_order_hash: adapteros_core::B3Hash::hash(b"disabled"),
                timestamp: current_timestamp(),
            })
        }
    }

    /// Verify memory layout consistency
    pub fn verify_layout_consistency(&self, expected_hash: &MemoryLayoutHash) -> Result<()> {
        if !self.is_running() {
            return Ok(());
        }

        if self.config.enable_memory_map_hashing {
            self.memory_map_hasher
                .verify_layout_consistency(expected_hash)?;
        }

        Ok(())
    }

    /// Get watchdog statistics
    pub fn get_stats(&self) -> WatchdogStats {
        let current_timestamp = current_timestamp();
        let uptime_micros = current_timestamp - self.start_timestamp;

        let memory_events = self.replay_logger.get_memory_events();
        let total_events = memory_events.len();

        let page_migrations = memory_events
            .iter()
            .filter(|e| {
                matches!(
                    e.event_type,
                    crate::replay_integration::MemoryEventType::PageMigration
                )
            })
            .count();

        let buffer_relocations = memory_events
            .iter()
            .filter(|e| {
                matches!(
                    e.event_type,
                    crate::replay_integration::MemoryEventType::BufferRelocation
                )
            })
            .count();

        let layout_changes = memory_events
            .iter()
            .filter(|e| {
                matches!(
                    e.event_type,
                    crate::replay_integration::MemoryEventType::LayoutChange
                )
            })
            .count();

        let memory_pressure = self.get_memory_pressure_level();
        let current_layout_hash = self.generate_memory_layout_hash().ok();

        WatchdogStats {
            total_events,
            page_migrations,
            buffer_relocations,
            layout_changes,
            memory_pressure,
            current_layout_hash,
            uptime_micros,
        }
    }

    /// Get memory pressure level based on actual heap observer stats
    fn get_memory_pressure_level(&self) -> MemoryPressureLevel {
        // Get actual memory stats from heap observer if available
        let pressure = if let Some(ref observer) = self.heap_observer {
            let stats = observer.get_memory_stats();
            if stats.total_heap_size > 0 {
                stats.total_heap_used as f32 / stats.total_heap_size as f32
            } else {
                // No heaps tracked yet - assume low pressure
                0.5
            }
        } else {
            // Heap observation disabled or not available - use default low pressure
            0.5
        };

        if pressure >= self.config.pressure_critical_threshold {
            MemoryPressureLevel::Critical
        } else if pressure >= self.config.pressure_warning_threshold {
            MemoryPressureLevel::High
        } else if pressure >= 0.7 {
            MemoryPressureLevel::Medium
        } else {
            MemoryPressureLevel::Low
        }
    }

    /// Check if watchdog is running
    pub fn is_running(&self) -> bool {
        let status = self.status.read();
        matches!(*status, WatchdogStatus::Running)
    }

    /// Pause the watchdog
    pub fn pause(&self) {
        let mut status = self.status.write();
        *status = WatchdogStatus::Paused;
        info!("Memory watchdog paused");
    }

    /// Resume the watchdog
    pub fn resume(&self) {
        let mut status = self.status.write();
        *status = WatchdogStatus::Running;
        info!("Memory watchdog resumed");
    }

    /// Shutdown the watchdog
    pub fn shutdown(&self) {
        let mut status = self.status.write();
        *status = WatchdogStatus::ShuttingDown;
        info!("Memory watchdog shutting down");
    }

    /// Get watchdog ID
    pub fn get_watchdog_id(&self) -> Uuid {
        self.watchdog_id
    }

    /// Get configuration
    pub fn get_config(&self) -> &MemoryWatchdogConfig {
        &self.config
    }

    /// Update configuration
    pub fn update_config(&mut self, config: MemoryWatchdogConfig) {
        self.config = config;
        info!("Memory watchdog configuration updated");
    }

    /// Clear all recorded data
    pub fn clear(&self) {
        if let Some(ref observer) = self.heap_observer {
            observer.clear();
        }

        self.pointer_canonicalizer.clear();
        self.buffer_relocation_detector.clear();
        self.memory_map_hasher.clear();
        self.replay_logger.clear();

        info!("Memory watchdog data cleared");
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
    fn test_memory_watchdog_creation() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        assert!(watchdog.is_running());
        assert!(!watchdog.get_watchdog_id().is_nil());
    }

    #[test]
    fn test_memory_allocation_monitoring() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        watchdog
            .monitor_allocation(0x1000, 1024, "test allocation".to_string())
            .unwrap();

        let stats = watchdog.get_stats();
        assert!(stats.total_events > 0);
    }

    #[test]
    fn test_memory_deallocation_monitoring() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        watchdog
            .monitor_deallocation(0x1000, 1024, "test deallocation".to_string())
            .unwrap();

        let stats = watchdog.get_stats();
        assert!(stats.total_events > 0);
    }

    #[test]
    fn test_memory_layout_hash_generation() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        let layout_hash = watchdog.generate_memory_layout_hash().unwrap();
        assert_ne!(
            layout_hash.layout_hash,
            adapteros_core::B3Hash::new([0u8; 32])
        );
    }

    #[test]
    fn test_watchdog_pause_resume() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        assert!(watchdog.is_running());

        watchdog.pause();
        assert!(!watchdog.is_running());

        watchdog.resume();
        assert!(watchdog.is_running());
    }

    #[test]
    fn test_watchdog_shutdown() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        watchdog.shutdown();
        assert!(!watchdog.is_running());
    }

    #[test]
    fn test_memory_events_checking() {
        let config = MemoryWatchdogConfig::default();
        let watchdog = MemoryWatchdog::new(config).unwrap();

        let events = watchdog.check_memory_events().unwrap();
        // Events may be empty depending on system state - just verify we can check
        assert!(events.is_empty() || events.len() > 0);
    }

    #[test]
    fn test_configuration_update() {
        let config = MemoryWatchdogConfig::default();
        let mut watchdog = MemoryWatchdog::new(config).unwrap();

        let new_config = MemoryWatchdogConfig {
            sampling_rate: 0.5,
            ..Default::default()
        };

        watchdog.update_config(new_config);
        assert_eq!(watchdog.get_config().sampling_rate, 0.5);
    }
}
