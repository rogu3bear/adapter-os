//! Unified Memory Watchdog for AdapterOS
//!
//! This crate provides comprehensive memory monitoring and stability guarantees:
//! - Metal heap observer that records page migrations
//! - Canonicalize pointer reuse and detect GPU buffer relocation
//! - Hash memory map per run and verify identical layout
//! - Integration with replay logs for determinism verification
//!
//! # Core Components
//!
//! 1. **MemoryWatchdog** - Main watchdog that coordinates all monitoring
//! 2. **MetalHeapObserver** - Monitors Metal heap allocations and page migrations
//! 3. **PointerCanonicalizer** - Canonicalizes pointer reuse patterns
//! 4. **BufferRelocationDetector** - Detects GPU buffer relocations
//! 5. **MemoryMapHasher** - Hashes memory layouts for determinism verification
//!
//! # Determinism Guarantees
//!
//! - Identical memory layouts produce identical hashes
//! - Pointer reuse patterns are canonicalized across runs
//! - Buffer relocations are detected and logged
//! - Page migrations are tracked and recorded

pub mod buffer_pool;
pub mod buffer_relocation;
pub mod heap_observer;
pub mod memory_map;
pub mod optimization;
pub mod pointer_canonicalizer;
pub mod pressure_manager;
pub mod replay_integration;
pub mod telemetry;
pub mod unified_interface;
pub mod unified_memory;
pub mod unified_tracker;
pub mod watchdog;

pub use buffer_pool::{BufferPool, BufferPoolConfig, BufferPoolStats, TensorFormat};
pub use buffer_relocation::BufferRelocationDetector;
pub use heap_observer::MetalHeapObserver;
pub use memory_map::MemoryMapHasher;
pub use optimization::{MemoryOptimizationPlan, MemoryOptimizer, MemoryPressureReport};
pub use pointer_canonicalizer::PointerCanonicalizer;
pub use pressure_manager::{EvictedAdapter, MemoryPressureManager, MemoryStats};
pub use replay_integration::ReplayMemoryLogger;
pub use telemetry::{MemoryTelemetryWriter, TelemetryEventSink};
pub use unified_interface::{
    AdapterCategory, AdapterMemoryInfo, AdapterState, CleanupOperation, MemoryCleanupReport,
    MemoryManager, MemoryPressureLevel, MemoryUsageStats,
    UnifiedMemoryManager as UnifiedMemoryManagerInterface,
};
pub use unified_memory::{
    AllocationRequest, MemoryBlock, MemoryStats as UnifiedMemoryStats, MemoryType,
    UnifiedMemoryManager,
};
pub use unified_tracker::{
    BackendType, EvictionStrategy, GpuBufferFingerprint, MemoryLimits, MemoryPressure,
    PressureLevel, UnifiedMemoryTracker,
};
pub use watchdog::MemoryWatchdog;

/// Error types for memory watchdog operations
#[derive(thiserror::Error, Debug)]
pub enum MemoryWatchdogError {
    #[error("Metal heap observation failed: {0}")]
    HeapObservationFailed(String),

    #[error("Pointer canonicalization failed: {0}")]
    PointerCanonicalizationFailed(String),

    #[error("Buffer relocation detection failed: {0}")]
    BufferRelocationFailed(String),

    #[error("Memory map hashing failed: {0}")]
    MemoryMapHashingFailed(String),

    #[error("Replay integration failed: {0}")]
    ReplayIntegrationFailed(String),

    #[error("Memory layout mismatch: expected {expected}, got {actual}")]
    MemoryLayoutMismatch { expected: String, actual: String },

    #[error("Page migration detected: {details}")]
    PageMigrationDetected { details: String },

    #[error("Buffer relocation detected: {details}")]
    BufferRelocationDetected { details: String },
}

/// Result type for memory watchdog operations
pub type Result<T> = std::result::Result<T, MemoryWatchdogError>;

/// Memory layout hash for determinism verification
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MemoryLayoutHash {
    /// Hash of the complete memory layout
    pub layout_hash: adapteros_core::B3Hash,
    /// Hash of pointer reuse patterns
    pub pointer_pattern_hash: adapteros_core::B3Hash,
    /// Hash of buffer allocation order
    pub allocation_order_hash: adapteros_core::B3Hash,
    /// Timestamp when layout was captured
    pub timestamp: u128,
}

/// Memory migration event
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryMigrationEvent {
    /// Event ID
    pub event_id: uuid::Uuid,
    /// Type of migration (page_out, page_in, buffer_relocate)
    pub migration_type: MigrationType,
    /// Source memory address (if applicable)
    pub source_addr: Option<u64>,
    /// Destination memory address (if applicable)
    pub dest_addr: Option<u64>,
    /// Size of migrated memory
    pub size_bytes: u64,
    /// Timestamp of migration
    pub timestamp: u128,
    /// Additional context
    pub context: serde_json::Value,
}

/// Types of memory migrations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MigrationType {
    /// Page was moved out of physical memory
    PageOut,
    /// Page was moved into physical memory
    PageIn,
    /// GPU buffer was relocated
    BufferRelocate,
    /// Heap compaction occurred
    HeapCompaction,
    /// Memory pressure triggered eviction
    PressureEviction,
}

/// Configuration for memory watchdog
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryWatchdogConfig {
    /// Enable Metal heap observation
    pub enable_heap_observation: bool,
    /// Enable pointer canonicalization
    pub enable_pointer_canonicalization: bool,
    /// Enable buffer relocation detection
    pub enable_buffer_relocation_detection: bool,
    /// Enable memory map hashing
    pub enable_memory_map_hashing: bool,
    /// Sampling rate for memory events (0.0-1.0)
    pub sampling_rate: f32,
    /// Memory pressure threshold for warnings
    pub pressure_warning_threshold: f32,
    /// Memory pressure threshold for critical alerts
    pub pressure_critical_threshold: f32,
}

impl Default for MemoryWatchdogConfig {
    fn default() -> Self {
        Self {
            enable_heap_observation: true,
            enable_pointer_canonicalization: true,
            enable_buffer_relocation_detection: true,
            enable_memory_map_hashing: true,
            sampling_rate: 1.0,                // Sample all events by default
            pressure_warning_threshold: 0.85,  // 85% memory usage
            pressure_critical_threshold: 0.95, // 95% memory usage
        }
    }
}
