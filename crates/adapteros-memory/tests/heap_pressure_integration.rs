//! Integration tests: Metal heap observer with memory pressure manager
//!
//! Tests the interaction between heap observation and memory pressure management.
//! Simulates realistic memory pressure scenarios and validates correct behavior.

use adapteros_core::B3Hash;
use adapteros_memory::{HeapAllocation, HeapObserverMemoryStats, HeapState};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// MOCK MEMORY PRESSURE MANAGER FOR TESTING
// ============================================================================

/// Simulated memory pressure manager that works with heap observer
struct MockMemoryPressureManager {
    /// Target memory utilization threshold (e.g., 0.85 = 85%)
    target_utilization: f32,
    /// Total system memory budget
    total_memory_budget: u64,
    /// Current memory usage
    current_usage: u64,
    /// List of "adapters" with their memory usage
    adapter_allocations: HashMap<String, u64>,
    /// Eviction history for testing
    evicted_adapters: Vec<String>,
}

impl MockMemoryPressureManager {
    fn new(total_memory_budget: u64, target_utilization: f32) -> Self {
        Self {
            target_utilization,
            total_memory_budget,
            current_usage: 0,
            adapter_allocations: HashMap::new(),
            evicted_adapters: Vec::new(),
        }
    }

    fn allocate_adapter(&mut self, adapter_id: String, size: u64) -> Result<(), String> {
        if self.current_usage + size > self.total_memory_budget {
            return Err("Insufficient memory".to_string());
        }

        self.adapter_allocations.insert(adapter_id, size);
        self.current_usage += size;
        Ok(())
    }

    fn deallocate_adapter(&mut self, adapter_id: &str) -> Result<u64, String> {
        if let Some(size) = self.adapter_allocations.remove(adapter_id) {
            self.current_usage -= size;
            Ok(size)
        } else {
            Err(format!("Adapter {} not found", adapter_id))
        }
    }

    fn get_memory_pressure(&self) -> f32 {
        if self.total_memory_budget == 0 {
            0.0
        } else {
            self.current_usage as f32 / self.total_memory_budget as f32
        }
    }

    fn check_memory_pressure(&mut self) -> Result<(), String> {
        let pressure = self.get_memory_pressure();

        if pressure > self.target_utilization {
            // Evict largest adapter
            if let Some((adapter_id, _)) = self
                .adapter_allocations
                .iter()
                .max_by_key(|(_, &size)| size)
            {
                let adapter_id = adapter_id.clone();
                self.deallocate_adapter(&adapter_id)?;
                self.evicted_adapters.push(adapter_id);
            }
            return Err(format!(
                "Memory pressure {:.1}% exceeded threshold",
                pressure * 100.0
            ));
        }

        Ok(())
    }

    fn get_utilization_pct(&self) -> f32 {
        self.get_memory_pressure() * 100.0
    }

    fn get_available_memory(&self) -> u64 {
        self.total_memory_budget.saturating_sub(self.current_usage)
    }

    fn get_eviction_history(&self) -> Vec<String> {
        self.evicted_adapters.clone()
    }
}

// ============================================================================
// MOCK HEAP OBSERVER FOR TESTING
// ============================================================================

struct MockHeapObserver {
    allocations: HashMap<u64, HeapAllocation>,
    heap_states: HashMap<u64, HeapState>,
    next_buffer_id: u64,
}

impl MockHeapObserver {
    fn new() -> Self {
        Self {
            allocations: HashMap::new(),
            heap_states: HashMap::new(),
            next_buffer_id: 1,
        }
    }

    fn record_allocation(&mut self, heap_id: u64, size: u64, offset: u64) -> u64 {
        let buffer_id = self.next_buffer_id;
        self.next_buffer_id += 1;

        let allocation = HeapAllocation {
            allocation_id: Uuid::new_v4(),
            heap_id,
            buffer_id,
            size_bytes: size,
            offset_bytes: offset,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros(),
            memory_addr: Some(0x1000 + offset),
            storage_mode: "shared".to_string(),
        };

        self.allocations.insert(buffer_id, allocation);
        buffer_id
    }

    fn remove_allocation(&mut self, buffer_id: u64) -> Option<u64> {
        self.allocations.remove(&buffer_id).map(|a| a.size_bytes)
    }

    fn get_memory_stats(&self) -> HeapObserverMemoryStats {
        let total_allocated: u64 = self.allocations.values().map(|a| a.size_bytes).sum();
        let total_heap_size: u64 = self.heap_states.values().map(|h| h.total_size).sum();
        let total_heap_used: u64 = self.heap_states.values().map(|h| h.used_size).sum();

        HeapObserverMemoryStats {
            total_allocated,
            total_heap_size,
            total_heap_used,
            allocation_count: self.allocations.len(),
            heap_count: self.heap_states.len(),
            migration_event_count: 0,
        }
    }

    fn get_heap_utilization(&self) -> f32 {
        let stats = self.get_memory_stats();
        if stats.total_heap_size == 0 {
            0.0
        } else {
            (stats.total_heap_used as f32 / stats.total_heap_size as f32) * 100.0
        }
    }
}

// ============================================================================
// INTEGRATION TESTS: BASIC PRESSURE MANAGEMENT
// ============================================================================

#[test]
fn test_pressure_manager_basic_allocation() {
    let mut manager = MockMemoryPressureManager::new(
        10 * 1024 * 1024, // 10 MB budget
        0.85,             // 85% target
    );

    // Should succeed
    assert!(manager
        .allocate_adapter("adapter_a".to_string(), 2 * 1024 * 1024)
        .is_ok());
    assert_eq!(manager.get_utilization_pct(), 20.0);
}

#[test]
fn test_pressure_manager_exceeds_budget() {
    let mut manager = MockMemoryPressureManager::new(
        10 * 1024 * 1024, // 10 MB budget
        0.85,
    );

    // Allocate 9 MB
    assert!(manager
        .allocate_adapter("adapter_a".to_string(), 9 * 1024 * 1024)
        .is_ok());

    // Try to allocate 2 MB more (should fail)
    assert!(manager
        .allocate_adapter("adapter_b".to_string(), 2 * 1024 * 1024)
        .is_err());
}

#[test]
fn test_pressure_manager_deallocation() {
    let mut manager = MockMemoryPressureManager::new(10 * 1024 * 1024, 0.85);

    manager
        .allocate_adapter("adapter_a".to_string(), 5 * 1024 * 1024)
        .ok();
    assert_eq!(manager.get_utilization_pct(), 50.0);

    manager.deallocate_adapter("adapter_a").ok();
    assert_eq!(manager.get_utilization_pct(), 0.0);
}

#[test]
fn test_pressure_manager_threshold_detection() {
    let mut manager = MockMemoryPressureManager::new(10 * 1024 * 1024, 0.85);

    // Allocate to 80% (below threshold)
    manager
        .allocate_adapter("adapter_a".to_string(), 8 * 1024 * 1024)
        .ok();
    assert_eq!(manager.get_utilization_pct(), 80.0);
    assert!(manager.check_memory_pressure().is_ok());

    // Allocate to 90% (above threshold)
    manager
        .allocate_adapter("adapter_b".to_string(), 1 * 1024 * 1024)
        .ok();
    assert_eq!(manager.get_utilization_pct(), 90.0);
    assert!(manager.check_memory_pressure().is_err());
}

#[test]
fn test_pressure_manager_eviction() {
    let mut manager = MockMemoryPressureManager::new(10 * 1024 * 1024, 0.85);

    // Load three adapters
    manager
        .allocate_adapter("adapter_a".to_string(), 3 * 1024 * 1024)
        .ok();
    manager
        .allocate_adapter("adapter_b".to_string(), 3 * 1024 * 1024)
        .ok();
    manager
        .allocate_adapter("adapter_c".to_string(), 3 * 1024 * 1024)
        .ok();

    assert_eq!(manager.get_utilization_pct(), 90.0);

    // Trigger eviction of largest adapter
    let _ = manager.check_memory_pressure();
    assert_eq!(manager.get_eviction_history().len(), 1);
    assert!(manager
        .get_eviction_history()
        .contains(&"adapter_c".to_string()));
}

// ============================================================================
// INTEGRATION TESTS: HEAP OBSERVER WITH PRESSURE MANAGER
// ============================================================================

#[test]
fn test_heap_observer_with_pressure_manager() {
    let mut observer = MockHeapObserver::new();
    let mut manager = MockMemoryPressureManager::new(
        10 * 1024 * 1024, // 10 MB budget
        0.85,
    );

    // Simulate loading adapter with heap observation
    observer.record_allocation(1, 2 * 1024 * 1024, 0);
    manager
        .allocate_adapter("adapter_a".to_string(), 2 * 1024 * 1024)
        .ok();

    observer.record_allocation(1, 3 * 1024 * 1024, 2 * 1024 * 1024);
    manager
        .allocate_adapter("adapter_b".to_string(), 3 * 1024 * 1024)
        .ok();

    // Verify both tracking systems agree
    let stats = observer.get_memory_stats();
    let manager_usage = manager.total_memory_budget - manager.get_available_memory();

    assert_eq!(stats.total_allocated, 5 * 1024 * 1024);
    assert_eq!(manager_usage, 5 * 1024 * 1024);
}

#[test]
fn test_heap_observer_memory_pressure_correlation() {
    let mut observer = MockHeapObserver::new();
    let mut manager = MockMemoryPressureManager::new(20 * 1024 * 1024, 0.85);

    // Setup heap state
    observer.heap_states.insert(
        1,
        HeapState {
            heap_id: 1,
            total_size: 20 * 1024 * 1024,
            used_size: 0,
            allocation_count: 0,
            heap_hash: B3Hash::hash(b"test"),
            allocation_order_hash: B3Hash::hash(b"test"),
        },
    );

    // Load adapters with increasing memory usage
    for i in 0..5 {
        let size = 2 * 1024 * 1024;
        let adapter_id = format!("adapter_{}", i);
        let offset = (i as u64) * size;

        observer.record_allocation(1, size, offset);
        manager.allocate_adapter(adapter_id, size).ok();

        let pressure = manager.get_memory_pressure();
        let heap_util = observer.get_heap_utilization();

        println!(
            "Step {}: Manager pressure {:.1}%, Heap utilization {:.1}%",
            i,
            pressure * 100.0,
            heap_util
        );

        // Pressure should increase with each allocation
        assert!(pressure > (i as f32 - 1.0) / 10.0);
    }

    // Verify final state
    let final_pressure = manager.get_memory_pressure();
    assert!(final_pressure >= 0.4 && final_pressure <= 0.6); // ~50%
}

// ============================================================================
// INTEGRATION TESTS: EVICTION AND CLEANUP
// ============================================================================

#[test]
fn test_eviction_clears_heap_observer() {
    let mut observer = MockHeapObserver::new();
    let mut manager = MockMemoryPressureManager::new(10 * 1024 * 1024, 0.85);

    // Allocate adapter_a
    let _buffer_id_a = observer.record_allocation(1, 5 * 1024 * 1024, 0);
    manager
        .allocate_adapter("adapter_a".to_string(), 5 * 1024 * 1024)
        .ok();

    // Allocate adapter_b (triggers pressure)
    let buffer_id_b = observer.record_allocation(1, 5 * 1024 * 1024, 5 * 1024 * 1024);
    manager
        .allocate_adapter("adapter_b".to_string(), 5 * 1024 * 1024)
        .ok();

    // Verify both allocations tracked
    let stats = observer.get_memory_stats();
    assert_eq!(stats.allocation_count, 2);

    // Evict adapter_b
    observer.remove_allocation(buffer_id_b);
    manager.deallocate_adapter("adapter_b").ok();

    // Verify only adapter_a remains
    let stats = observer.get_memory_stats();
    assert_eq!(stats.allocation_count, 1);
    assert_eq!(stats.total_allocated, 5 * 1024 * 1024);
}

#[test]
fn test_pressure_recovery_scenario() {
    let mut observer = MockHeapObserver::new();
    let mut manager = MockMemoryPressureManager::new(10 * 1024 * 1024, 0.85);

    // Phase 1: Normal operation (50%)
    observer.record_allocation(1, 5 * 1024 * 1024, 0);
    manager
        .allocate_adapter("adapter_a".to_string(), 5 * 1024 * 1024)
        .ok();
    assert!(manager.check_memory_pressure().is_ok());

    // Phase 2: High pressure (90%)
    observer.record_allocation(1, 5 * 1024 * 1024, 5 * 1024 * 1024);
    manager
        .allocate_adapter("adapter_b".to_string(), 5 * 1024 * 1024)
        .ok();
    assert!(manager.check_memory_pressure().is_err());

    // Phase 3: Recovery (50%)
    observer.remove_allocation(2);
    manager.deallocate_adapter("adapter_b").ok();
    assert!(manager.check_memory_pressure().is_ok());

    // Verify final state
    let stats = observer.get_memory_stats();
    assert_eq!(stats.allocation_count, 1);
    assert_eq!(manager.get_utilization_pct(), 50.0);
}

// ============================================================================
// INTEGRATION TESTS: MULTI-ADAPTER SCENARIOS
// ============================================================================

#[test]
fn test_multi_adapter_load_balancing() {
    let mut observer = MockHeapObserver::new();
    let mut manager = MockMemoryPressureManager::new(
        20 * 1024 * 1024, // 20 MB budget
        0.85,
    );

    // Setup heap state
    observer.heap_states.insert(
        1,
        HeapState {
            heap_id: 1,
            total_size: 20 * 1024 * 1024,
            used_size: 0,
            allocation_count: 0,
            heap_hash: B3Hash::hash(b"test"),
            allocation_order_hash: B3Hash::hash(b"test"),
        },
    );

    // Load multiple adapters with different sizes
    let adapters = vec![
        ("adapter_large", 8 * 1024 * 1024),
        ("adapter_medium", 4 * 1024 * 1024),
        ("adapter_small", 2 * 1024 * 1024),
    ];

    let mut offset = 0u64;
    let mut buffer_ids = Vec::new();

    for (adapter_id, size) in adapters {
        let buffer_id = observer.record_allocation(1, size, offset);
        buffer_ids.push(buffer_id);
        manager.allocate_adapter(adapter_id.to_string(), size).ok();
        offset += size;

        let pressure = manager.get_memory_pressure();
        println!(
            "{}: {} MB allocated, pressure {:.1}%",
            adapter_id,
            size / (1024 * 1024),
            pressure * 100.0
        );
    }

    // Verify all adapters loaded
    let stats = observer.get_memory_stats();
    assert_eq!(stats.allocation_count, 3);
    assert_eq!(stats.total_allocated, 14 * 1024 * 1024);
    assert_eq!(manager.get_utilization_pct(), 70.0);
}

#[test]
fn test_selective_eviction_strategy() {
    let mut observer = MockHeapObserver::new();
    let mut manager = MockMemoryPressureManager::new(10 * 1024 * 1024, 0.85);

    // Load adapters in order of size (for deterministic eviction)
    observer.record_allocation(1, 2 * 1024 * 1024, 0); // adapter_a
    manager
        .allocate_adapter("adapter_a".to_string(), 2 * 1024 * 1024)
        .ok();

    observer.record_allocation(1, 3 * 1024 * 1024, 2 * 1024 * 1024); // adapter_b
    manager
        .allocate_adapter("adapter_b".to_string(), 3 * 1024 * 1024)
        .ok();

    observer.record_allocation(1, 4 * 1024 * 1024, 5 * 1024 * 1024); // adapter_c
    manager
        .allocate_adapter("adapter_c".to_string(), 4 * 1024 * 1024)
        .ok();

    // Trigger eviction (should evict largest: adapter_c)
    let _ = manager.check_memory_pressure();

    let history = manager.get_eviction_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0], "adapter_c");
    assert_eq!(manager.get_utilization_pct(), 50.0);
}

// ============================================================================
// INTEGRATION TESTS: MEMORY FRAGMENTATION WITH PRESSURE
// ============================================================================

#[test]
fn test_fragmentation_under_memory_pressure() {
    let mut observer = MockHeapObserver::new();

    // Create fragmented allocation pattern
    observer.record_allocation(1, 1024, 0); // 0-1024
    observer.record_allocation(1, 1024, 2048); // Gap: 1024-2048
    observer.record_allocation(1, 1024, 3072); // Gap: 3072-4096
    observer.record_allocation(1, 1024, 5120); // Multiple gaps

    observer.heap_states.insert(
        1,
        HeapState {
            heap_id: 1,
            total_size: 8192,
            used_size: 4096,
            allocation_count: 4,
            heap_hash: B3Hash::hash(b"fragmented"),
            allocation_order_hash: B3Hash::hash(b"order"),
        },
    );

    let stats = observer.get_memory_stats();
    let utilization = (stats.total_heap_used as f32 / stats.total_heap_size as f32) * 100.0;

    println!("Fragmented heap: {:.1}% utilization", utilization);
    assert!(utilization > 0.0);
    assert!(utilization < 100.0);
}

// ============================================================================
// DOCUMENTATION
// ============================================================================
//
// # Integration Test Documentation
//
// ## Running Tests
// ```bash
// cargo test --test heap_pressure_integration -- --nocapture
// ```
//
// ## Test Categories
//
// ### Memory Pressure Manager Tests
// - Basic allocation and deallocation
// - Budget enforcement
// - Threshold detection
// - Eviction strategy
//
// ### Heap Observer Integration
// - Allocation tracking
// - Memory pressure correlation
// - Eviction cleanup
// - Pressure recovery
//
// ### Multi-Adapter Scenarios
// - Load balancing
// - Selective eviction
// - Fragmentation tracking
//
// ## Performance Goals
//
// - All integration tests complete in < 100ms
// - Memory tracking within 1KB of actual usage
// - Eviction selection deterministic and correct
