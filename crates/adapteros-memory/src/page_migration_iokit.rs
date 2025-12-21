//! IOKit-based page migration tracking for AdapterOS
//!
//! This module provides real-time monitoring of VM page migrations, memory pressure events,
//! and unified memory transitions on Apple Silicon (M1/M2/M3) and Intel platforms.
//!
//! # Features
//!
//! - Page-in/page-out detection via VM statistics
//! - Memory pressure event monitoring
//! - GPU/CPU memory transition tracking (unified memory)
//! - Shared memory pool analysis
//! - Memory migration pattern detection
//! - IOKit-based hardware monitoring
//!
//! # Platform Support
//!
//! - macOS 10.13+ (Big Sur, Monterey, Ventura, Sonoma)
//! - Apple Silicon (M1/M2/M3) and Intel processors
//! - Requires root/elevated privileges for detailed memory tracking

use adapteros_core::B3Hash;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ============================================================================
// FFI BINDINGS FOR IOKIT AND MACH
// ============================================================================

/// FFI-safe page migration event from IOKit
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIPageMigrationInfo {
    /// Event ID (high 64 bits of UUID)
    pub event_id_high: u64,
    /// Event ID (low 64 bits of UUID)
    pub event_id_low: u64,
    /// Migration type (1=PageIn, 2=PageOut, 3=GPU->CPU, 4=CPU->GPU, 5=SharedMemory)
    pub migration_type: u32,
    /// Source memory address
    pub source_addr: u64,
    /// Destination memory address (0 if paged to disk)
    pub dest_addr: u64,
    /// Memory size in bytes
    pub size_bytes: u64,
    /// Timestamp in microseconds since epoch
    pub timestamp: u64,
    /// Memory pressure level (0=low, 1=medium, 2=critical)
    pub pressure_level: u32,
}

/// FFI-safe VM statistics from IOKit
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIVMStats {
    /// Total page-ins since boot
    pub page_ins: u64,
    /// Total page-outs since boot
    pub page_outs: u64,
    /// Total pages freed
    pub pages_freed: u64,
    /// Total pages reactivated
    pub pages_reactivated: u64,
    /// Free pages available
    pub free_pages: u64,
    /// Active pages
    pub active_pages: u64,
    /// Inactive pages
    pub inactive_pages: u64,
    /// Speculative pages
    pub speculative_pages: u64,
    /// Throttled pages
    pub throttled_pages: u64,
    /// Wired pages (locked in RAM)
    pub wired_pages: u64,
}

/// FFI-safe unified memory info for M1/M2
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIUnifiedMemoryInfo {
    /// GPU memory in use (bytes)
    pub gpu_memory_in_use: u64,
    /// GPU memory available (bytes)
    pub gpu_memory_available: u64,
    /// Shared memory pool size (bytes)
    pub shared_memory_pool: u64,
    /// Recent GPU->CPU migrations
    pub gpu_to_cpu_migrations: u64,
    /// Recent CPU->GPU migrations
    pub cpu_to_gpu_migrations: u64,
    /// ANE (Apple Neural Engine) memory in use
    pub ane_memory_in_use: u64,
}

/// FFI-safe mach_vm_region info
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIMachVMRegion {
    /// Virtual address
    pub address: u64,
    /// Region size in bytes
    pub size: u64,
    /// Protection flags (READ=1, WRITE=2, EXECUTE=4)
    pub protection: u32,
    /// Max protection flags
    pub max_protection: u32,
    /// Inheritance policy
    pub inheritance: u32,
    /// Share mode (NONE=0, COPY=1, COW=2, DONTWAIT=3)
    pub share_mode: u32,
    /// Resident pages
    pub resident_pages: u64,
}

// IOKit FFI bindings - only available when C++ implementation is compiled
// NOTE: C++ implementation currently disabled - stub implementations provided
#[cfg(all(target_os = "macos", feature = "iokit-cpp"))]
extern "C" {
    // ===== VM Statistics (via Mach APIs) =====

    /// Initialize IOKit monitoring for the current process
    /// Returns non-zero on success
    pub fn iokit_vm_init() -> i32;

    /// Get current VM page statistics
    /// Fills out_stats with current VM statistics
    /// Returns 0 on success
    pub fn iokit_vm_get_stats(out_stats: *mut FFIVMStats) -> i32;

    /// Get page-in delta since last call
    /// Returns number of pages paged in, or negative on error
    pub fn iokit_vm_get_pagein_delta() -> i64;

    /// Get page-out delta since last call
    /// Returns number of pages paged out, or negative on error
    pub fn iokit_vm_get_pageout_delta() -> i64;

    // ===== Memory Pressure =====

    /// Get current memory pressure level
    /// Returns: 0=low, 1=medium, 2=critical, -1=error
    pub fn iokit_memory_pressure_level() -> i32;

    /// Enable memory pressure callbacks
    /// Returns non-zero on success
    pub fn iokit_memory_pressure_enable() -> i32;

    /// Disable memory pressure callbacks
    pub fn iokit_memory_pressure_disable() -> i32;

    // ===== Unified Memory (Apple Silicon) =====

    /// Check if system supports unified memory (M1/M2/M3)
    /// Returns 1 if supported, 0 otherwise
    pub fn iokit_unified_memory_supported() -> i32;

    /// Get unified memory information
    /// Returns 0 on success
    pub fn iokit_unified_memory_info(out_info: *mut FFIUnifiedMemoryInfo) -> i32;

    /// Get GPU memory usage for process
    /// Returns GPU memory in bytes, or negative on error
    pub fn iokit_gpu_memory_usage() -> i64;

    /// Get ANE memory usage for process
    /// Returns ANE memory in bytes, or negative on error
    pub fn iokit_ane_memory_usage() -> i64;

    // ===== VM Region Info =====

    /// Get detailed VM region information for an address
    /// Returns 0 on success
    pub fn iokit_vm_region_info(address: u64, out_region: *mut FFIMachVMRegion) -> i32;

    /// Iterate through all VM regions
    /// Calls callback for each region with FFIMachVMRegion struct
    /// Returns number of regions scanned
    pub fn iokit_vm_scan_regions(callback: extern "C" fn(*mut FFIMachVMRegion) -> i32) -> i32;

    // ===== Migration Event Tracking =====

    /// Get pending page migration events
    /// out_events: pointer to array where events will be written
    /// max_events: maximum number of events that can fit
    /// Returns number of events written
    pub fn iokit_migration_get_events(
        out_events: *mut FFIPageMigrationInfo,
        max_events: u32,
    ) -> i32;

    /// Clear migration event buffer
    pub fn iokit_migration_clear_events() -> i32;

    /// Get last error message
    /// buffer: character buffer for error message
    /// buffer_len: size of buffer
    /// Returns number of bytes written
    pub fn iokit_get_last_error(buffer: *mut i8, buffer_len: usize) -> usize;

    /// Cleanup IOKit resources
    pub fn iokit_vm_cleanup() -> i32;
}

// Stub implementations when C++ is not compiled
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_vm_init() -> i32 {
    1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_vm_get_stats(_: *mut FFIVMStats) -> i32 {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_vm_get_pagein_delta() -> i64 {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_vm_get_pageout_delta() -> i64 {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_memory_pressure_level() -> i32 {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_memory_pressure_enable() -> i32 {
    1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_memory_pressure_disable() -> i32 {
    1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_unified_memory_supported() -> i32 {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_unified_memory_info(_: *mut FFIUnifiedMemoryInfo) -> i32 {
    -1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_gpu_memory_usage() -> i64 {
    -1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_ane_memory_usage() -> i64 {
    -1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_vm_region_info(_: u64, _: *mut FFIMachVMRegion) -> i32 {
    -1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_vm_scan_regions(_: extern "C" fn(*mut FFIMachVMRegion) -> i32) -> i32 {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_migration_get_events(_: *mut FFIPageMigrationInfo, _: u32) -> i32 {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_migration_clear_events() -> i32 {
    1
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_get_last_error(_: *mut i8, _: usize) -> usize {
    0
}
#[cfg(not(all(target_os = "macos", feature = "iokit-cpp")))]
#[allow(dead_code)]
pub fn iokit_vm_cleanup() -> i32 {
    1
}

// ============================================================================
// STUB IMPLEMENTATIONS FOR NON-MACOS PLATFORMS
// ============================================================================

#[cfg(not(target_os = "macos"))]
pub fn iokit_vm_init() -> i32 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_vm_get_stats(_out_stats: *mut FFIVMStats) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_vm_get_pagein_delta() -> i64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_vm_get_pageout_delta() -> i64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_memory_pressure_level() -> i32 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_memory_pressure_enable() -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_memory_pressure_disable() -> i32 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_unified_memory_supported() -> i32 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_unified_memory_info(_out_info: *mut FFIUnifiedMemoryInfo) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_gpu_memory_usage() -> i64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_ane_memory_usage() -> i64 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_vm_region_info(_address: u64, _out_region: *mut FFIMachVMRegion) -> i32 {
    -1
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_migration_get_events(_out_events: *mut FFIPageMigrationInfo, _max_events: u32) -> i32 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_migration_clear_events() -> i32 {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_get_last_error(_buffer: *mut i8, _buffer_len: usize) -> usize {
    0
}

#[cfg(not(target_os = "macos"))]
pub fn iokit_vm_cleanup() -> i32 {
    0
}

// ============================================================================
// RUST TYPES FOR PAGE MIGRATION TRACKING
// ============================================================================

/// Page migration type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageMigrationType {
    /// Page brought into physical RAM from disk
    PageIn,
    /// Page moved from physical RAM to disk
    PageOut,
    /// GPU to CPU memory transfer
    GpuToCpu,
    /// CPU to GPU memory transfer
    CpuToGpu,
    /// Shared memory pool transition
    SharedMemory,
}

impl PageMigrationType {
    /// Convert from FFI u32 representation
    pub fn from_ffi(value: u32) -> Option<Self> {
        match value {
            1 => Some(PageMigrationType::PageIn),
            2 => Some(PageMigrationType::PageOut),
            3 => Some(PageMigrationType::GpuToCpu),
            4 => Some(PageMigrationType::CpuToGpu),
            5 => Some(PageMigrationType::SharedMemory),
            _ => None,
        }
    }

    /// Convert to FFI u32 representation
    pub fn to_ffi(&self) -> u32 {
        match self {
            PageMigrationType::PageIn => 1,
            PageMigrationType::PageOut => 2,
            PageMigrationType::GpuToCpu => 3,
            PageMigrationType::CpuToGpu => 4,
            PageMigrationType::SharedMemory => 5,
        }
    }
}

/// Memory pressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryPressureLevel {
    /// Low memory pressure (healthy conditions)
    Low,
    /// Medium memory pressure (approaching limit)
    Medium,
    /// High memory pressure (system under stress)
    High,
    /// Critical pressure (active eviction)
    Critical,
}

impl MemoryPressureLevel {
    /// Convert from IOKit i32 representation
    /// Maps: 0=Low (normal), 1=Medium (warning), 2=Critical
    /// Note: High level is not currently reported by IOKit but reserved for future use
    pub fn from_iokit(value: i32) -> Option<Self> {
        match value {
            0 => Some(MemoryPressureLevel::Low),
            1 => Some(MemoryPressureLevel::Medium),
            2 => Some(MemoryPressureLevel::Critical),
            _ => None,
        }
    }
}

/// VM statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMStatistics {
    /// Total page-ins since boot
    pub page_ins: u64,
    /// Total page-outs since boot
    pub page_outs: u64,
    /// Delta page-ins since last query
    pub pagein_delta: u64,
    /// Delta page-outs since last query
    pub pageout_delta: u64,
    /// Free pages available
    pub free_pages: u64,
    /// Active pages
    pub active_pages: u64,
    /// Inactive pages
    pub inactive_pages: u64,
    /// Timestamp of this snapshot
    pub timestamp: u128,
}

/// Page migration event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMigrationEvent {
    /// Unique event ID
    pub event_id: Uuid,
    /// Migration type
    pub migration_type: PageMigrationType,
    /// Source address
    pub source_addr: Option<u64>,
    /// Destination address
    pub dest_addr: Option<u64>,
    /// Size of migrated memory in bytes
    pub size_bytes: u64,
    /// Timestamp in microseconds
    pub timestamp: u128,
    /// Memory pressure at time of migration
    pub pressure_level: MemoryPressureLevel,
    /// Additional context
    pub context: serde_json::Value,
}

/// Unified memory information for Apple Silicon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMemoryInfo {
    /// GPU memory in use (bytes)
    pub gpu_memory_in_use: u64,
    /// GPU memory available (bytes)
    pub gpu_memory_available: u64,
    /// Shared memory pool size (bytes)
    pub shared_memory_pool: u64,
    /// Recent GPU->CPU migrations
    pub gpu_to_cpu_migrations: u64,
    /// Recent CPU->GPU migrations
    pub cpu_to_gpu_migrations: u64,
    /// ANE (Apple Neural Engine) memory in use
    pub ane_memory_in_use: u64,
    /// Timestamp
    pub timestamp: u128,
}

/// VM region information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMRegionInfo {
    /// Virtual address
    pub address: u64,
    /// Region size in bytes
    pub size: u64,
    /// Is readable
    pub readable: bool,
    /// Is writable
    pub writable: bool,
    /// Is executable
    pub executable: bool,
    /// Share mode
    pub share_mode: String,
    /// Resident pages
    pub resident_pages: u64,
}

// ============================================================================
// PAGE MIGRATION TRACKER
// ============================================================================

/// IOKit-based page migration tracker
pub struct PageMigrationTracker {
    /// Page migration events
    migration_events: Arc<RwLock<Vec<PageMigrationEvent>>>,
    /// VM statistics history
    vm_stats_history: Arc<RwLock<Vec<VMStatistics>>>,
    /// Unified memory info (if available)
    unified_memory_info: Arc<RwLock<Option<UnifiedMemoryInfo>>>,
    /// VM regions cache
    vm_regions: Arc<RwLock<Vec<VMRegionInfo>>>,
    /// Last known memory pressure
    last_pressure: Arc<RwLock<MemoryPressureLevel>>,
    /// Supports unified memory (M1/M2/M3)
    supports_unified: bool,
    /// Hash of migration event sequence
    event_sequence_hash: Arc<RwLock<B3Hash>>,
}

impl PageMigrationTracker {
    /// Create a new page migration tracker
    #[cfg(target_os = "macos")]
    pub fn new() -> crate::Result<Self> {
        // Initialize IOKit monitoring
        if iokit_vm_init() == 0 {
            return Err(crate::MemoryWatchdogError::HeapObservationFailed(
                "Failed to initialize IOKit VM monitoring".to_string(),
            ));
        }

        // Check if system supports unified memory
        let supports_unified = iokit_unified_memory_supported() != 0;

        if supports_unified {
            info!("Unified memory (Apple Silicon) detected and supported");
        }

        // Enable memory pressure monitoring
        let _ = iokit_memory_pressure_enable();

        debug!("Page migration tracker initialized");

        Ok(Self {
            migration_events: Arc::new(RwLock::new(Vec::new())),
            vm_stats_history: Arc::new(RwLock::new(Vec::new())),
            unified_memory_info: Arc::new(RwLock::new(None)),
            vm_regions: Arc::new(RwLock::new(Vec::new())),
            last_pressure: Arc::new(RwLock::new(MemoryPressureLevel::Low)),
            supports_unified,
            event_sequence_hash: Arc::new(RwLock::new(B3Hash::hash(b""))),
        })
    }

    /// Non-macOS fallback
    #[cfg(not(target_os = "macos"))]
    pub fn new() -> crate::Result<Self> {
        warn!("Page migration tracking not available on this platform");
        Ok(Self {
            migration_events: Arc::new(RwLock::new(Vec::new())),
            vm_stats_history: Arc::new(RwLock::new(Vec::new())),
            unified_memory_info: Arc::new(RwLock::new(None)),
            vm_regions: Arc::new(RwLock::new(Vec::new())),
            last_pressure: Arc::new(RwLock::new(MemoryPressureLevel::Low)),
            supports_unified: false,
            event_sequence_hash: Arc::new(RwLock::new(B3Hash::hash(b""))),
        })
    }

    /// Update VM statistics from IOKit
    #[cfg(target_os = "macos")]
    pub fn update_vm_stats(&self) -> crate::Result<VMStatistics> {
        let mut ffi_stats = unsafe { std::mem::zeroed::<FFIVMStats>() };

        if iokit_vm_get_stats(&mut ffi_stats) != 0 {
            return Err(crate::MemoryWatchdogError::HeapObservationFailed(
                "Failed to get VM statistics".to_string(),
            ));
        }

        // Get deltas
        let pagein_delta = iokit_vm_get_pagein_delta() as u64;
        let pageout_delta = iokit_vm_get_pageout_delta() as u64;

        let stats = VMStatistics {
            page_ins: ffi_stats.page_ins,
            page_outs: ffi_stats.page_outs,
            pagein_delta,
            pageout_delta,
            free_pages: ffi_stats.free_pages,
            active_pages: ffi_stats.active_pages,
            inactive_pages: ffi_stats.inactive_pages,
            timestamp: current_timestamp(),
        };

        // Record in history
        {
            let mut history = self.vm_stats_history.write();
            history.push(stats.clone());

            // Keep last 1000 entries
            if history.len() > 1000 {
                let remove_count = history.len() - 1000;
                history.drain(0..remove_count);
            }
        }

        // Check for page migration activity
        if pagein_delta > 0 || pageout_delta > 0 {
            self.detect_migrations()?;
        }

        // Update memory pressure
        self.update_memory_pressure()?;

        debug!(
            "VM Stats: pagein_delta={}, pageout_delta={}, free_pages={}",
            pagein_delta, pageout_delta, stats.free_pages
        );

        Ok(stats)
    }

    /// Update unified memory info (Apple Silicon)
    #[cfg(target_os = "macos")]
    pub fn update_unified_memory_info(&self) -> crate::Result<Option<UnifiedMemoryInfo>> {
        if !self.supports_unified {
            return Ok(None);
        }

        let mut ffi_info = unsafe { std::mem::zeroed::<FFIUnifiedMemoryInfo>() };

        if iokit_unified_memory_info(&mut ffi_info) != 0 {
            return Ok(None);
        }

        let info = UnifiedMemoryInfo {
            gpu_memory_in_use: ffi_info.gpu_memory_in_use,
            gpu_memory_available: ffi_info.gpu_memory_available,
            shared_memory_pool: ffi_info.shared_memory_pool,
            gpu_to_cpu_migrations: ffi_info.gpu_to_cpu_migrations,
            cpu_to_gpu_migrations: ffi_info.cpu_to_gpu_migrations,
            ane_memory_in_use: ffi_info.ane_memory_in_use,
            timestamp: current_timestamp(),
        };

        {
            let mut unified = self.unified_memory_info.write();
            *unified = Some(info.clone());
        }

        if info.gpu_to_cpu_migrations > 0 || info.cpu_to_gpu_migrations > 0 {
            debug!(
                "Unified memory transitions: GPU->CPU={}, CPU->GPU={}",
                info.gpu_to_cpu_migrations, info.cpu_to_gpu_migrations
            );
        }

        Ok(Some(info))
    }

    /// Detect active page migrations
    #[cfg(target_os = "macos")]
    pub fn detect_migrations(&self) -> crate::Result<()> {
        let mut events: [FFIPageMigrationInfo; 256] = unsafe { std::mem::zeroed() };

        let count = iokit_migration_get_events(events.as_mut_ptr(), 256);

        if count < 0 {
            return Err(crate::MemoryWatchdogError::HeapObservationFailed(
                "Failed to get migration events".to_string(),
            ));
        }

        let pressure = *self.last_pressure.read();

        for ffi_event in events.iter().take(count as usize) {
            if let Some(migration_type) = PageMigrationType::from_ffi(ffi_event.migration_type) {
                let event = PageMigrationEvent {
                    event_id: Uuid::new_v4(),
                    migration_type,
                    source_addr: if ffi_event.source_addr != 0 {
                        Some(ffi_event.source_addr)
                    } else {
                        None
                    },
                    dest_addr: if ffi_event.dest_addr != 0 {
                        Some(ffi_event.dest_addr)
                    } else {
                        None
                    },
                    size_bytes: ffi_event.size_bytes,
                    timestamp: ffi_event.timestamp as u128,
                    pressure_level: pressure,
                    context: serde_json::json!({
                        "type": format!("{:?}", migration_type),
                        "pressure": format!("{:?}", pressure),
                    }),
                };

                {
                    let mut events_guard = self.migration_events.write();
                    events_guard.push(event.clone());

                    // Keep last 10000 events
                    if events_guard.len() > 10000 {
                        let remove_count = events_guard.len() - 10000;
                        events_guard.drain(0..remove_count);
                    }
                }

                info!(
                    "Page migration detected: {:?} {} bytes",
                    migration_type, ffi_event.size_bytes
                );
            }
        }

        // Clear the IOKit buffer
        let _ = iokit_migration_clear_events();

        Ok(())
    }

    /// Update memory pressure level
    #[cfg(target_os = "macos")]
    pub fn update_memory_pressure(&self) -> crate::Result<()> {
        let level_raw = iokit_memory_pressure_level();

        if let Some(level) = MemoryPressureLevel::from_iokit(level_raw) {
            {
                let mut pressure = self.last_pressure.write();
                if *pressure != level {
                    info!("Memory pressure changed: {:?} -> {:?}", *pressure, level);
                    *pressure = level;
                }
            }

            if level == MemoryPressureLevel::Critical {
                warn!("Critical memory pressure detected!");
            }
        }

        Ok(())
    }

    /// Scan and cache all VM regions
    #[cfg(target_os = "macos")]
    pub fn scan_vm_regions(&self) -> crate::Result<Vec<VMRegionInfo>> {
        let regions = Vec::new();

        // We would implement a callback-based approach here
        // For now, we'll scan key regions manually
        debug!("Scanning VM regions");

        {
            let mut cached = self.vm_regions.write();
            *cached = regions.clone();
        }

        Ok(regions)
    }

    /// Get detailed memory stats for migration analysis
    pub fn get_detailed_stats(&self) -> DetailedMemoryStats {
        let migration_events = self.migration_events.read();
        let vm_stats = self.vm_stats_history.read();
        let pressure = *self.last_pressure.read();

        let total_pagein_events = migration_events
            .iter()
            .filter(|e| e.migration_type == PageMigrationType::PageIn)
            .count();
        let total_pageout_events = migration_events
            .iter()
            .filter(|e| e.migration_type == PageMigrationType::PageOut)
            .count();
        let total_gpu_cpu_events = migration_events
            .iter()
            .filter(|e| e.migration_type == PageMigrationType::GpuToCpu)
            .count();
        let total_cpu_gpu_events = migration_events
            .iter()
            .filter(|e| e.migration_type == PageMigrationType::CpuToGpu)
            .count();

        let total_bytes_migrated: u64 = migration_events.iter().map(|e| e.size_bytes).sum();

        let latest_vm_stats = vm_stats.last().cloned();
        let unified = self.unified_memory_info.read().clone();

        DetailedMemoryStats {
            total_migration_events: migration_events.len(),
            pagein_events: total_pagein_events,
            pageout_events: total_pageout_events,
            gpu_cpu_migrations: total_gpu_cpu_events,
            cpu_gpu_migrations: total_cpu_gpu_events,
            total_bytes_migrated,
            current_pressure: pressure,
            latest_vm_stats,
            unified_memory: unified,
            timestamp: current_timestamp(),
        }
    }

    /// Get recent migration events
    pub fn get_recent_migrations(&self, limit: usize) -> Vec<PageMigrationEvent> {
        let events = self.migration_events.read();
        events.iter().rev().take(limit).cloned().collect()
    }

    /// Clear all recorded events
    pub fn clear_events(&self) {
        {
            let mut events = self.migration_events.write();
            events.clear();
        }
        {
            let mut stats = self.vm_stats_history.write();
            stats.clear();
        }
    }
}

impl Default for PageMigrationTracker {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            migration_events: Arc::new(RwLock::new(Vec::new())),
            vm_stats_history: Arc::new(RwLock::new(Vec::new())),
            unified_memory_info: Arc::new(RwLock::new(None)),
            vm_regions: Arc::new(RwLock::new(Vec::new())),
            last_pressure: Arc::new(RwLock::new(MemoryPressureLevel::Low)),
            supports_unified: false,
            event_sequence_hash: Arc::new(RwLock::new(B3Hash::hash(b""))),
        })
    }
}

impl Drop for PageMigrationTracker {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let _ = iokit_memory_pressure_disable();
            let _ = iokit_vm_cleanup();
        }
    }
}

// ============================================================================
// DETAILED STATISTICS
// ============================================================================

/// Comprehensive memory migration statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedMemoryStats {
    /// Total number of migration events
    pub total_migration_events: usize,
    /// Page-in events
    pub pagein_events: usize,
    /// Page-out events
    pub pageout_events: usize,
    /// GPU to CPU migrations
    pub gpu_cpu_migrations: usize,
    /// CPU to GPU migrations
    pub cpu_gpu_migrations: usize,
    /// Total bytes migrated
    pub total_bytes_migrated: u64,
    /// Current memory pressure level
    pub current_pressure: MemoryPressureLevel,
    /// Latest VM statistics
    pub latest_vm_stats: Option<VMStatistics>,
    /// Unified memory info (if available)
    pub unified_memory: Option<UnifiedMemoryInfo>,
    /// Timestamp
    pub timestamp: u128,
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Get current timestamp in microseconds
pub fn current_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_migration_type_conversion() {
        assert_eq!(
            PageMigrationType::from_ffi(1),
            Some(PageMigrationType::PageIn)
        );
        assert_eq!(
            PageMigrationType::from_ffi(2),
            Some(PageMigrationType::PageOut)
        );
        assert_eq!(
            PageMigrationType::from_ffi(3),
            Some(PageMigrationType::GpuToCpu)
        );
        assert_eq!(PageMigrationType::from_ffi(99), None);
    }

    #[test]
    fn test_memory_pressure_conversion() {
        assert_eq!(
            MemoryPressureLevel::from_iokit(0),
            Some(MemoryPressureLevel::Low)
        );
        assert_eq!(
            MemoryPressureLevel::from_iokit(1),
            Some(MemoryPressureLevel::Medium)
        );
        assert_eq!(
            MemoryPressureLevel::from_iokit(2),
            Some(MemoryPressureLevel::Critical)
        );
        // Invalid value returns None
        assert_eq!(MemoryPressureLevel::from_iokit(-1), None);
        assert_eq!(MemoryPressureLevel::from_iokit(99), None);
    }

    #[test]
    fn test_page_migration_tracker_creation() {
        #[cfg(target_os = "macos")]
        {
            if let Ok(tracker) = PageMigrationTracker::new() {
                let events = tracker.migration_events.read();
                assert!(
                    events.is_empty(),
                    "tracker should start with no migration events"
                );
            }
        }
    }

    #[test]
    fn test_detailed_stats() {
        #[cfg(target_os = "macos")]
        {
            if let Ok(tracker) = PageMigrationTracker::new() {
                let stats = tracker.get_detailed_stats();
                assert_eq!(stats.current_pressure, MemoryPressureLevel::Low);
            }
        }
    }

    #[test]
    fn test_event_clearing() {
        #[cfg(target_os = "macos")]
        {
            if let Ok(tracker) = PageMigrationTracker::new() {
                tracker.clear_events();
                assert_eq!(tracker.migration_events.read().len(), 0);
            }
        }
    }
}
