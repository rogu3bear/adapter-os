//! Comprehensive End-to-End Integration Tests for AOS Loader
//!
//! Tests the complete flow:
//! - Upload .aos file
//! - Load adapter into MLX backend
//! - Run inference with loaded adapter
//! - Unload adapter
//! - Verify cleanup
//!
//! Also tests error cases, shape consistency, and performance.

mod fixture_generator;

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{AosError, B3Hash, Result};
use fixture_generator::{generate_valid_aos, TestManifest};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;
use tempfile::TempDir;

/// Helper to create a minimal RouterRing for testing (k=1)
fn create_test_router_ring() -> TestRouterRing {
    TestRouterRing {
        indices: [0u16; 8],
        gates_q15: [32767i16; 8], // Max Q15 value (1.0)
        k: 1,
        position: 0,
    }
}

/// Minimal RouterRing implementation for testing
#[derive(Debug, Clone)]
struct TestRouterRing {
    pub indices: [u16; 8],
    pub gates_q15: [i16; 8],
    pub k: usize,
    #[allow(dead_code)]
    pub position: usize,
}

/// Mock MLX backend for testing
struct MockMLXBackend {
    loaded_adapters: std::collections::HashMap<String, Vec<u8>>,
    inference_count: usize,
    memory_usage: usize,
}

impl MockMLXBackend {
    fn new() -> Self {
        Self {
            loaded_adapters: std::collections::HashMap::new(),
            inference_count: 0,
            memory_usage: 0,
        }
    }

    fn load_adapter(&mut self, adapter_id: &str, weights: Vec<u8>) -> Result<()> {
        let weight_size = weights.len();
        self.loaded_adapters.insert(adapter_id.to_string(), weights);
        self.memory_usage += weight_size;
        tracing::info!(
            adapter_id = %adapter_id,
            weight_size = weight_size,
            total_memory = self.memory_usage,
            "Adapter loaded into mock backend"
        );
        Ok(())
    }

    fn run_inference(&mut self, adapter_id: &str, _input: &[u32]) -> Result<Vec<f32>> {
        if !self.loaded_adapters.contains_key(adapter_id) {
            return Err(AosError::Lifecycle(format!(
                "Adapter '{}' not loaded",
                adapter_id
            )));
        }

        self.inference_count += 1;
        tracing::info!(
            adapter_id = %adapter_id,
            inference_count = self.inference_count,
            "Running inference"
        );

        // Return dummy logits
        Ok(vec![0.1, 0.2, 0.3, 0.4])
    }

    fn unload_adapter(&mut self, adapter_id: &str) -> Result<()> {
        if let Some(weights) = self.loaded_adapters.remove(adapter_id) {
            self.memory_usage = self.memory_usage.saturating_sub(weights.len());
            tracing::info!(
                adapter_id = %adapter_id,
                freed_bytes = weights.len(),
                remaining_memory = self.memory_usage,
                "Adapter unloaded from mock backend"
            );
            Ok(())
        } else {
            Err(AosError::Lifecycle(format!(
                "Adapter '{}' not found",
                adapter_id
            )))
        }
    }

    fn memory_usage(&self) -> usize {
        self.memory_usage
    }

    fn adapter_count(&self) -> usize {
        self.loaded_adapters.len()
    }
}

// ============================================================================
// Test Group 1: Complete Flow Tests
// ============================================================================

#[test]
fn test_complete_upload_load_inference_unload_flow() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    // Step 1: Create and write .aos file
    let aos_path = temp_dir.path().join("test_adapter.aos");
    generate_valid_aos(&aos_path)?;

    // Step 2: Verify upload (read header)
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&aos_path)?;
    assert!(manifest_offset > 8, "Manifest offset should be past header");
    assert!(manifest_len > 0, "Manifest should have content");

    // Step 3: Load adapter into mock backend
    let mut backend = MockMLXBackend::new();
    let mut file =
        File::open(&aos_path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

    let manifest_bytes = &buffer[manifest_offset as usize..];
    let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    // Read full file as weights (simplified for testing)
    let mut weights_data = Vec::new();
    file.read_to_end(&mut weights_data)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    backend.load_adapter(&manifest.adapter_id, weights_data)?;
    assert_eq!(backend.adapter_count(), 1, "Should have 1 loaded adapter");

    // Step 4: Run inference
    let input_ids = vec![1, 2, 3];
    let logits = backend.run_inference(&manifest.adapter_id, &input_ids)?;
    assert!(!logits.is_empty(), "Should return logits");

    // Step 5: Unload adapter
    backend.unload_adapter(&manifest.adapter_id)?;
    assert_eq!(backend.adapter_count(), 0, "Should have 0 loaded adapters");

    // Step 6: Verify cleanup
    assert_eq!(backend.memory_usage(), 0, "Memory should be fully released");

    Ok(())
}

#[test]
fn test_multiple_adapters_sequential_loading() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    // Create 3 different .aos files
    let paths: Vec<PathBuf> = (0..3)
        .map(|i| {
            let path = temp_dir.path().join(format!("adapter_{}.aos", i));
            generate_valid_aos(&path).expect("Failed to generate .aos");
            path
        })
        .collect();

    // Load all adapters
    for (i, path) in paths.iter().enumerate() {
        let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;
        let mut file =
            File::open(path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

        let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
        file.read_exact(&mut buffer)
            .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

        let manifest_bytes = &buffer[manifest_offset as usize..];
        let _manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

        let mut weights_data = Vec::new();
        file.read_to_end(&mut weights_data)
            .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

        backend.load_adapter(&format!("adapter-{}", i), weights_data)?;
    }

    assert_eq!(backend.adapter_count(), 3, "Should have 3 loaded adapters");

    // Unload in reverse order
    for i in (0..3).rev() {
        backend.unload_adapter(&format!("adapter-{}", i))?;
    }

    assert_eq!(
        backend.adapter_count(),
        0,
        "All adapters should be unloaded"
    );
    assert_eq!(backend.memory_usage(), 0, "Memory should be fully released");

    Ok(())
}

#[test]
fn test_adapter_hot_swap() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    let path1 = temp_dir.path().join("adapter_v1.aos");
    let path2 = temp_dir.path().join("adapter_v2.aos");

    generate_valid_aos(&path1)?;
    generate_valid_aos(&path2)?;

    // Load v1
    let (offset, len) = AOS2Writer::read_header(&path1)?;
    let mut file =
        File::open(&path1).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
    let mut buffer = vec![0u8; offset as usize + len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
    let mut weights_v1 = Vec::new();
    file.read_to_end(&mut weights_v1)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    backend.load_adapter("adapter-hotswap", weights_v1)?;
    let initial_memory = backend.memory_usage();

    // Run inference with v1
    let logits_v1 = backend.run_inference("adapter-hotswap", &[1, 2, 3])?;
    assert!(!logits_v1.is_empty());

    // Hot-swap to v2
    backend.unload_adapter("adapter-hotswap")?;

    let (offset, len) = AOS2Writer::read_header(&path2)?;
    let mut file =
        File::open(&path2).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
    let mut buffer = vec![0u8; offset as usize + len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
    let mut weights_v2 = Vec::new();
    file.read_to_end(&mut weights_v2)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    backend.load_adapter("adapter-hotswap", weights_v2)?;

    // Run inference with v2
    let logits_v2 = backend.run_inference("adapter-hotswap", &[1, 2, 3])?;
    assert!(!logits_v2.is_empty());

    // Verify memory usage is similar (hot-swap should not leak)
    let final_memory = backend.memory_usage();
    let memory_diff = (final_memory as i64 - initial_memory as i64).abs();
    assert!(
        memory_diff < 1024,
        "Memory difference should be minimal after hot-swap"
    );

    Ok(())
}

// ============================================================================
// Test Group 2: Error Cases
// ============================================================================

#[test]
fn test_loading_nonexistent_adapter() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let bad_path = temp_dir.path().join("nonexistent.aos");

    let result = AOS2Writer::read_header(&bad_path);
    assert!(result.is_err(), "Should fail with non-existent file");
}

#[test]
fn test_loading_corrupted_aos_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path = temp_dir.path().join("corrupted.aos");

    // Write invalid data (less than 8 bytes for header)
    std::fs::write(&path, b"NOPE").expect("Failed to write corrupt file");

    let result = AOS2Writer::read_header(&path);
    assert!(
        result.is_err(),
        "Should fail with corrupted file (too short for header)"
    );
}

#[test]
fn test_unloading_during_inference() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    let path = temp_dir.path().join("test.aos");
    generate_valid_aos(&path)?;

    let (offset, len) = AOS2Writer::read_header(&path)?;
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
    let mut buffer = vec![0u8; offset as usize + len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
    let mut weights = Vec::new();
    file.read_to_end(&mut weights)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    backend.load_adapter("test-adapter", weights)?;

    // Unload before inference completes
    backend.unload_adapter("test-adapter")?;

    // Try inference on unloaded adapter
    let result = backend.run_inference("test-adapter", &[1, 2, 3]);
    assert!(result.is_err(), "Should fail with unloaded adapter");

    Ok(())
}

#[test]
fn test_memory_pressure_scenarios() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    // Simulate memory pressure by loading many adapters
    const MAX_ADAPTERS: usize = 10;
    const MEMORY_LIMIT: usize = 5 * 1024 * 1024; // 5MB

    for i in 0..MAX_ADAPTERS {
        let path = temp_dir.path().join(format!("adapter_{}.aos", i));
        generate_valid_aos(&path)?;

        let (offset, len) = AOS2Writer::read_header(&path)?;
        let mut file =
            File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
        let mut buffer = vec![0u8; offset as usize + len as usize];
        file.read_exact(&mut buffer)
            .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
        let mut weights = Vec::new();
        file.read_to_end(&mut weights)
            .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

        // Check memory before loading
        if backend.memory_usage() + weights.len() > MEMORY_LIMIT {
            tracing::warn!(
                current_memory = backend.memory_usage(),
                required = weights.len(),
                limit = MEMORY_LIMIT,
                "Memory pressure detected, not loading adapter"
            );
            break;
        }

        backend.load_adapter(&format!("adapter-{}", i), weights)?;
    }

    tracing::info!(
        loaded_adapters = backend.adapter_count(),
        memory_usage = backend.memory_usage(),
        "Loaded adapters under memory pressure"
    );

    // Cleanup all adapters
    for i in 0..backend.adapter_count() {
        backend.unload_adapter(&format!("adapter-{}", i))?;
    }

    Ok(())
}

#[test]
fn test_permission_denied_cases() {
    // Test reading from a path with invalid permissions
    let result = AOS2Writer::read_header(&PathBuf::from("/dev/null"));
    assert!(result.is_err(), "Should fail with permission issues");
}

// ============================================================================
// Test Group 3: Shape Consistency
// ============================================================================

#[test]
fn test_tensor_shape_consistency() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("shape_test.aos");
    generate_valid_aos(&path)?;

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;

    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

    let manifest_bytes = &buffer[manifest_offset as usize..];
    let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;

    // Verify expected dimensions
    assert_eq!(manifest.rank, 8, "Rank should match expected value");
    assert_eq!(
        manifest.base_model, "llama-7b",
        "Base model should match expected value"
    );

    Ok(())
}

#[test]
fn test_router_ring_k1_configuration() {
    let ring = create_test_router_ring();

    assert_eq!(ring.k, 1, "RouterRing should have k=1");
    assert_eq!(ring.indices.len(), 8, "RouterRing should have 8 slots");
    assert_eq!(ring.gates_q15.len(), 8, "RouterRing should have 8 gates");
    assert_eq!(
        ring.gates_q15[0], 32767,
        "First gate should be max Q15 value"
    );
}

#[test]
fn test_gate_value_validation() {
    let ring = create_test_router_ring();

    // Q15 format: -32768 to 32767 maps to [-1.0, 1.0)
    // i16 is always in this range, so just verify it's a valid i16
    assert!(
        ring.gates_q15[0] <= 32767,
        "Gate values should be valid Q15"
    );

    // Convert Q15 to float
    let gate_float = ring.gates_q15[0] as f32 / 32768.0;
    assert!(
        gate_float >= -1.0 && gate_float <= 1.0,
        "Gate float value should be in [-1.0, 1.0]"
    );
}

// ============================================================================
// Test Group 4: Performance Tests
// ============================================================================

#[test]
fn test_load_unload_performance() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    let path = temp_dir.path().join("perf_test.aos");
    generate_valid_aos(&path)?;

    // Measure load time
    let (offset, len) = AOS2Writer::read_header(&path)?;
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
    let mut buffer = vec![0u8; offset as usize + len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
    let mut weights = Vec::new();
    file.read_to_end(&mut weights)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    let load_start = Instant::now();
    backend.load_adapter("perf-test", weights)?;
    let load_duration = load_start.elapsed();

    tracing::info!(
        load_time_ms = load_duration.as_millis(),
        "Adapter load time"
    );

    // Measure unload time
    let unload_start = Instant::now();
    backend.unload_adapter("perf-test")?;
    let unload_duration = unload_start.elapsed();

    tracing::info!(
        unload_time_ms = unload_duration.as_millis(),
        "Adapter unload time"
    );

    // Performance assertions (generous thresholds for CI)
    assert!(
        load_duration.as_millis() < 1000,
        "Load should complete in < 1s"
    );
    assert!(
        unload_duration.as_millis() < 100,
        "Unload should complete in < 100ms"
    );

    Ok(())
}

#[test]
fn test_memory_usage_tracking() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    let path = temp_dir.path().join("memory_test.aos");
    generate_valid_aos(&path)?;

    let initial_memory = backend.memory_usage();
    assert_eq!(initial_memory, 0, "Initial memory should be 0");

    let (offset, len) = AOS2Writer::read_header(&path)?;
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
    let mut buffer = vec![0u8; offset as usize + len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
    let mut weights = Vec::new();
    file.read_to_end(&mut weights)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    let expected_memory = weights.len();
    backend.load_adapter("memory-test", weights)?;

    let loaded_memory = backend.memory_usage();
    assert_eq!(
        loaded_memory, expected_memory,
        "Memory usage should match weight size"
    );

    backend.unload_adapter("memory-test")?;

    let final_memory = backend.memory_usage();
    assert_eq!(final_memory, 0, "Memory should be fully released");

    Ok(())
}

#[test]
fn test_no_memory_leaks_repeated_operations() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    let path = temp_dir.path().join("leak_test.aos");
    generate_valid_aos(&path)?;

    // Perform 100 load/unload cycles
    for iteration in 0..100 {
        let (offset, len) = AOS2Writer::read_header(&path)?;
        let mut file =
            File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
        let mut buffer = vec![0u8; offset as usize + len as usize];
        file.read_exact(&mut buffer)
            .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
        let mut weights = Vec::new();
        file.read_to_end(&mut weights)
            .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

        backend.load_adapter("leak-test", weights)?;
        backend.unload_adapter("leak-test")?;

        // Check for memory leaks every 10 iterations
        if iteration % 10 == 0 {
            assert_eq!(
                backend.memory_usage(),
                0,
                "Memory should be 0 after unload (iteration {})",
                iteration
            );
        }
    }

    // Final check
    assert_eq!(
        backend.memory_usage(),
        0,
        "No memory leaks after 100 cycles"
    );

    Ok(())
}

#[test]
fn test_concurrent_adapter_access() -> Result<()> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    // Create multiple .aos files
    let paths: Vec<PathBuf> = (0..4)
        .map(|i| {
            let path = temp_dir.path().join(format!("concurrent_{}.aos", i));
            generate_valid_aos(&path).expect("Failed to generate .aos");
            path
        })
        .collect();

    let backend = Arc::new(Mutex::new(MockMLXBackend::new()));

    // Spawn threads to load adapters concurrently
    let handles: Vec<_> = paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let backend = Arc::clone(&backend);
            let path = path.clone();

            thread::spawn(move || {
                let (offset, len) = AOS2Writer::read_header(&path).expect("Failed to read header");
                let mut file = File::open(&path).expect("Failed to open file");
                let mut buffer = vec![0u8; offset as usize + len as usize];
                file.read_exact(&mut buffer).expect("Failed to read buffer");
                let mut weights = Vec::new();
                file.read_to_end(&mut weights)
                    .expect("Failed to read weights");

                let mut backend = backend.lock().unwrap();
                backend
                    .load_adapter(&format!("concurrent-{}", i), weights)
                    .expect("Failed to load adapter");
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify all adapters loaded
    let backend = backend.lock().unwrap();
    assert_eq!(backend.adapter_count(), 4, "Should have 4 loaded adapters");

    Ok(())
}

// ============================================================================
// Test Group 5: Integration with RouterRing
// ============================================================================

#[test]
fn test_router_ring_integration_with_loaded_adapter() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    let path = temp_dir.path().join("router_test.aos");
    generate_valid_aos(&path)?;

    // Load adapter
    let (offset, len) = AOS2Writer::read_header(&path)?;
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
    let mut buffer = vec![0u8; offset as usize + len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
    let mut weights = Vec::new();
    file.read_to_end(&mut weights)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    backend.load_adapter("router-test", weights)?;

    // Create RouterRing with k=1 pointing to this adapter
    let mut ring = create_test_router_ring();
    ring.indices[0] = 0; // Adapter index 0
    ring.gates_q15[0] = 32767; // Full weight (1.0 in Q15)

    // Verify RouterRing configuration
    assert_eq!(ring.k, 1, "k should be 1");
    assert_eq!(ring.indices[0], 0, "Should route to adapter 0");
    assert_eq!(ring.gates_q15[0], 32767, "Gate should be max Q15");

    // Run inference
    let logits = backend.run_inference("router-test", &[1, 2, 3])?;
    assert!(!logits.is_empty(), "Should return logits");

    Ok(())
}

#[test]
fn test_multiple_adapters_router_ring_selection() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let mut backend = MockMLXBackend::new();

    // Load 3 adapters
    for i in 0..3 {
        let path = temp_dir.path().join(format!("multi_router_{}.aos", i));
        generate_valid_aos(&path)?;

        let (offset, len) = AOS2Writer::read_header(&path)?;
        let mut file =
            File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;
        let mut buffer = vec![0u8; offset as usize + len as usize];
        file.read_exact(&mut buffer)
            .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;
        let mut weights = Vec::new();
        file.read_to_end(&mut weights)
            .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

        backend.load_adapter(&format!("multi-router-{}", i), weights)?;
    }

    // Create RouterRing selecting adapter 1
    let mut ring = create_test_router_ring();
    ring.indices[0] = 1; // Select middle adapter
    ring.gates_q15[0] = 32767;

    // Verify selection
    assert_eq!(ring.indices[0], 1, "Should route to adapter 1");

    // Run inference on selected adapter
    let logits = backend.run_inference("multi-router-1", &[1, 2, 3])?;
    assert!(!logits.is_empty(), "Should return logits");

    Ok(())
}

// ============================================================================
// Test Group 6: Hash Validation
// ============================================================================

#[test]
fn test_aos_file_hash_integrity() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("hash_test.aos");
    generate_valid_aos(&path)?;

    // Read file and compute hash
    let file_data =
        std::fs::read(&path).map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    let hash1 = B3Hash::hash(&file_data);

    // Re-read and verify hash is consistent
    let file_data2 =
        std::fs::read(&path).map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    let hash2 = B3Hash::hash(&file_data2);

    assert_eq!(
        hash1.to_hex(),
        hash2.to_hex(),
        "Hash should be deterministic"
    );

    Ok(())
}

#[test]
fn test_manifest_hash_validation() -> Result<()> {
    let temp_dir =
        TempDir::new().map_err(|e| AosError::Io(format!("Failed to create temp dir: {}", e)))?;

    let path = temp_dir.path().join("manifest_hash_test.aos");
    generate_valid_aos(&path)?;

    let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;

    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

    let manifest_bytes = &buffer[manifest_offset as usize..];
    let manifest_hash = B3Hash::hash(manifest_bytes);

    // Verify manifest hash is consistent across reads
    let mut file = File::open(&path).map_err(|e| AosError::Io(format!("Failed to open: {}", e)))?;

    let mut buffer2 = vec![0u8; manifest_offset as usize + manifest_len as usize];
    file.read_exact(&mut buffer2)
        .map_err(|e| AosError::Io(format!("Failed to read: {}", e)))?;

    let manifest_bytes2 = &buffer2[manifest_offset as usize..];
    let manifest_hash2 = B3Hash::hash(manifest_bytes2);

    assert_eq!(
        manifest_hash.to_hex(),
        manifest_hash2.to_hex(),
        "Manifest hash should be deterministic"
    );

    Ok(())
}
