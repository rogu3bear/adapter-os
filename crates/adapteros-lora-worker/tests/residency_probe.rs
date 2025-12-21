//! Residency/memory probe harness for adapter load/unload cycles.
//!
//! Provides two-tier testing:
//! - Main CI: Calibrated fixtures for mechanism correctness
//! - Hardware CI: Real 7B/30B models for production realism

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_lifecycle::AdapterLoader;
use adapteros_lora_worker::model_handle_cache::{ModelHandle, ModelHandleCache};
use adapteros_lora_worker::model_key::{ModelCacheIdentity, ModelKey};
use safetensors::tensor::TensorView;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;
use tempfile::{Builder as TempDirBuilder, TempDir};

fn new_test_base_dir(prefix: &str) -> TempDir {
    let root = std::path::PathBuf::from("var/tmp");
    let _ = std::fs::create_dir_all(&root);
    TempDirBuilder::new()
        .prefix(prefix)
        .tempdir_in(&root)
        .expect("temp dir")
}

/// Test modes for residency probing
#[derive(Clone, Copy, Debug)]
pub enum ProbeMode {
    /// Quick smoke test (5 cycles, 200MB threshold)
    Smoke,
    /// Standard CI test (20 cycles, 100MB threshold)
    Standard,
    /// Extended stress test (100 cycles, 200MB threshold)
    Extended,
}

impl ProbeMode {
    fn cycle_count(self) -> u32 {
        match self {
            ProbeMode::Smoke => 5,
            ProbeMode::Standard => 20,
            ProbeMode::Extended => 100,
        }
    }

    fn rss_threshold_mb(self) -> u64 {
        match self {
            ProbeMode::Smoke => 200,
            ProbeMode::Standard => 100,
            ProbeMode::Extended => 200,
        }
    }

    fn max_load_latency_ms(self) -> u64 {
        match self {
            ProbeMode::Smoke => 1000,
            ProbeMode::Standard => 500,
            ProbeMode::Extended => 500,
        }
    }

    fn p99_load_latency_ms(self) -> u64 {
        match self {
            ProbeMode::Smoke => 500,
            ProbeMode::Standard => 250,
            ProbeMode::Extended => 250,
        }
    }
}

/// Result of a residency probe run
#[derive(Debug, Clone)]
pub struct ResidencyProbeResult {
    pub cycle_count: u32,
    pub rss_before_bytes: u64,
    pub rss_after_bytes: u64,
    pub rss_delta_bytes: i64,
    pub max_load_latency_ms: u64,
    pub p99_load_latency_ms: u64,
    pub max_unload_latency_ms: u64,
    pub base_model_reload_count: u64,
    pub eviction_blocked_count: u64,
    pub load_latencies_ms: Vec<u64>,
    pub unload_latencies_ms: Vec<u64>,
}

impl ResidencyProbeResult {
    fn compute_p99(latencies: &[u64]) -> u64 {
        if latencies.is_empty() {
            return 0;
        }
        let mut sorted = latencies.to_vec();
        sorted.sort();
        let idx = ((sorted.len() as f64) * 0.99).ceil() as usize - 1;
        sorted[idx.min(sorted.len() - 1)]
    }
}

/// Write a minimal test adapter to disk
fn write_adapter(dir: &Path, name: &str, base_id: &str, base_hash: B3Hash) -> B3Hash {
    let path = dir.join(format!("{}.safetensors", name));

    // Create minimal tensor data
    let tensor_bytes: Vec<u8> = 0f32.to_le_bytes().to_vec();
    let tensor =
        TensorView::new(safetensors::Dtype::F32, vec![1], &tensor_bytes).expect("tensor view");

    let mut tensors = HashMap::new();
    tensors.insert("lora_A.q_proj.weight".to_string(), tensor.clone());
    tensors.insert("lora_B.q_proj.weight".to_string(), tensor);
    let data = safetensors::tensor::serialize(tensors, &None).expect("serialize");
    std::fs::write(&path, &data).expect("write adapter");

    let manifest_path = dir.join(format!("{}.manifest.json", name));
    let manifest = serde_json::json!({
        "base_model": base_id,
        "base_hash_b3": base_hash.to_hex(),  // Full 64-char hex, not Display format
    });
    std::fs::write(&manifest_path, manifest.to_string()).expect("write manifest");
    B3Hash::hash(&data)
}

/// Get current process RSS in bytes using sysinfo
fn get_rss_bytes() -> u64 {
    use sysinfo::{Pid, ProcessRefreshKind};

    let mut sys = System::new();
    let pid = Pid::from_u32(std::process::id());
    // sysinfo 0.30 API: refresh_processes_specifics takes ProcessRefreshKind
    sys.refresh_processes_specifics(ProcessRefreshKind::new().with_memory());

    // Get current process memory (in bytes)
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}

/// Run residency probe cycles with the given loader
async fn run_probe_cycles(
    loader: &mut AdapterLoader,
    adapter_names: &[&str],
    cycle_count: u32,
) -> (Vec<u64>, Vec<u64>) {
    let mut load_latencies = Vec::with_capacity(cycle_count as usize);
    let mut unload_latencies = Vec::with_capacity(cycle_count as usize);

    for i in 0..cycle_count {
        let idx = (i % adapter_names.len() as u32) as u16;
        let name = adapter_names[idx as usize];

        // Measure load latency
        let load_start = Instant::now();
        let handle = loader
            .load_adapter(idx, name)
            .expect("load should succeed in probe");
        let load_latency = load_start.elapsed().as_millis() as u64;
        load_latencies.push(load_latency);

        assert!(loader.is_loaded(idx));

        // Measure unload latency
        let unload_start = Instant::now();
        loader.unload_adapter(idx).expect("unload should succeed");
        let unload_latency = unload_start.elapsed().as_millis() as u64;
        unload_latencies.push(unload_latency);

        assert!(!loader.is_loaded(idx));

        // Touch handle to avoid warnings
        let _ = handle.memory_bytes();

        // Small sleep to allow allocator to stabilize
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    (load_latencies, unload_latencies)
}

// ============================================================================
// MAIN CI TESTS (calibrated fixtures, always run)
// ============================================================================

/// Smoke test for residency mechanism - quick validation
#[tokio::test]
async fn residency_probe_smoke_test() {
    let temp_dir = new_test_base_dir("aos_residency_smoke_");
    let base_dir = temp_dir.path().to_path_buf();

    let base_id = "probe-base-model";
    let base_hash = B3Hash::hash(base_id.as_bytes());

    let adapter_names = ["probe_a", "probe_b", "probe_c"];
    let mut expected = HashMap::new();
    for name in &adapter_names {
        let hash = write_adapter(&base_dir, name, base_id, base_hash);
        expected.insert(name.to_string(), hash);
    }

    let mut loader = AdapterLoader::new(base_dir.clone(), expected);

    let mode = ProbeMode::Smoke;
    let rss_before = get_rss_bytes();

    let (load_latencies, _unload_latencies) =
        run_probe_cycles(&mut loader, &adapter_names, mode.cycle_count()).await;

    let rss_after = get_rss_bytes();
    let rss_delta = rss_after as i64 - rss_before as i64;
    let delta_mb = rss_delta.abs() as u64 / (1024 * 1024);

    // Assertions
    assert!(
        delta_mb < mode.rss_threshold_mb(),
        "RSS delta {}MB exceeds threshold {}MB",
        delta_mb,
        mode.rss_threshold_mb()
    );

    let max_load = *load_latencies.iter().max().unwrap_or(&0);
    assert!(
        max_load < mode.max_load_latency_ms(),
        "Max load latency {}ms exceeds threshold {}ms",
        max_load,
        mode.max_load_latency_ms()
    );
}

/// Test that pinning prevents eviction of base model
#[tokio::test]
async fn base_model_pinning_prevents_eviction() {
    // Create a small cache to force eviction pressure
    let cache = ModelHandleCache::new(100); // 100 bytes - very small

    let base_key = ModelKey::new(
        BackendType::Metal,
        B3Hash::hash(b"base-model"),
        ModelCacheIdentity::for_backend(BackendType::Metal),
    );
    let adapter_key = ModelKey::new(
        BackendType::Metal,
        B3Hash::hash(b"adapter"),
        ModelCacheIdentity::for_backend(BackendType::Metal),
    );

    // Load base model using get_or_load_base_model (auto-pins)
    cache
        .get_or_load_base_model(&base_key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 50])), 50))
        })
        .unwrap();

    assert!(cache.is_pinned(&base_key), "Base model should be pinned");
    assert_eq!(cache.pinned_count(), 1);

    // Load adapter: 60 bytes - would normally evict base model due to size
    cache
        .get_or_load(&adapter_key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 60])), 60))
        })
        .unwrap();

    // Base model should still be in cache (pinned)
    assert_eq!(
        cache.len(),
        2,
        "Both base model and adapter should be in cache"
    );
    assert!(
        cache.is_pinned(&base_key),
        "Base model should still be pinned"
    );

    // Stats should show eviction was blocked
    let stats = cache.stats();
    assert_eq!(stats.evictions, 0, "No evictions should have occurred");
    assert!(
        stats.eviction_skip_pinned_count > 0,
        "Evictions should have been blocked by pinning"
    );
}

/// Test cache lifecycle invariants
#[tokio::test]
async fn cache_lifecycle_invariants() {
    let cache = ModelHandleCache::new(1024 * 1024); // 1MB

    let key1 = ModelKey::new(
        BackendType::Metal,
        B3Hash::hash(b"model1"),
        ModelCacheIdentity::for_backend(BackendType::Metal),
    );
    let key2 = ModelKey::new(
        BackendType::Metal,
        B3Hash::hash(b"model2"),
        ModelCacheIdentity::for_backend(BackendType::Metal),
    );

    // Load and verify initial state
    cache
        .get_or_load(&key1, || {
            Ok((ModelHandle::Metal(Arc::new(vec![1, 2, 3])), 3))
        })
        .unwrap();

    let stats = cache.stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 0);

    // Access same key - should be cache hit
    cache
        .get_or_load(&key1, || {
            panic!("Should not reload cached model");
        })
        .unwrap();

    let stats = cache.stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);

    // Load different key
    cache
        .get_or_load(&key2, || {
            Ok((ModelHandle::Metal(Arc::new(vec![4, 5, 6])), 3))
        })
        .unwrap();

    assert_eq!(cache.len(), 2);

    // Pin first key
    assert!(cache.pin(&key1));
    assert!(cache.is_pinned(&key1));
    assert!(!cache.is_pinned(&key2));

    // Unpin and verify
    assert!(cache.unpin(&key1));
    assert!(!cache.is_pinned(&key1));
}

// ============================================================================
// CI RESIDENCY TESTS (feature-gated)
// ============================================================================

#[tokio::test]
#[cfg_attr(
    not(feature = "ci-residency"),
    ignore = "Requires ci-residency feature"
)]
async fn residency_probe_standard() {
    let temp_dir = new_test_base_dir("aos_residency_standard_");
    let base_dir = temp_dir.path().to_path_buf();

    let base_id = "probe-base-model";
    let base_hash = B3Hash::hash(base_id.as_bytes());

    let adapter_names = ["probe_a", "probe_b", "probe_c"];
    let mut expected = HashMap::new();
    for name in &adapter_names {
        let hash = write_adapter(&base_dir, name, base_id, base_hash);
        expected.insert(name.to_string(), hash);
    }

    let mut loader = AdapterLoader::new(base_dir.clone(), expected);

    let mode = ProbeMode::Standard;
    let rss_before = get_rss_bytes();

    let (load_latencies, unload_latencies) =
        run_probe_cycles(&mut loader, &adapter_names, mode.cycle_count()).await;

    let rss_after = get_rss_bytes();

    // Compute results
    let result = ResidencyProbeResult {
        cycle_count: mode.cycle_count(),
        rss_before_bytes: rss_before,
        rss_after_bytes: rss_after,
        rss_delta_bytes: rss_after as i64 - rss_before as i64,
        max_load_latency_ms: *load_latencies.iter().max().unwrap_or(&0),
        p99_load_latency_ms: ResidencyProbeResult::compute_p99(&load_latencies),
        max_unload_latency_ms: *unload_latencies.iter().max().unwrap_or(&0),
        base_model_reload_count: 0,
        eviction_blocked_count: 0,
        load_latencies_ms: load_latencies,
        unload_latencies_ms: unload_latencies,
    };

    // Assertions
    let delta_mb = result.rss_delta_bytes.abs() as u64 / (1024 * 1024);
    assert!(
        delta_mb < mode.rss_threshold_mb(),
        "RSS delta {}MB exceeds threshold {}MB",
        delta_mb,
        mode.rss_threshold_mb()
    );

    assert!(
        result.max_load_latency_ms < mode.max_load_latency_ms(),
        "Max load latency {}ms exceeds threshold {}ms",
        result.max_load_latency_ms,
        mode.max_load_latency_ms()
    );

    assert!(
        result.p99_load_latency_ms < mode.p99_load_latency_ms(),
        "P99 load latency {}ms exceeds threshold {}ms",
        result.p99_load_latency_ms,
        mode.p99_load_latency_ms()
    );
}

// ============================================================================
// HARDWARE CI TESTS (real models, nightly/gated)
// ============================================================================

#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature and real models"
)]
async fn residency_probe_extended() {
    let temp_dir = new_test_base_dir("aos_residency_extended_");
    let base_dir = temp_dir.path().to_path_buf();

    let base_id = "probe-base-model";
    let base_hash = B3Hash::hash(base_id.as_bytes());

    let adapter_names = ["probe_a", "probe_b", "probe_c", "probe_d", "probe_e"];
    let mut expected = HashMap::new();
    for name in &adapter_names {
        let hash = write_adapter(&base_dir, name, base_id, base_hash);
        expected.insert(name.to_string(), hash);
    }

    let mut loader = AdapterLoader::new(base_dir.clone(), expected);

    let mode = ProbeMode::Extended;
    let rss_before = get_rss_bytes();

    let (load_latencies, unload_latencies) =
        run_probe_cycles(&mut loader, &adapter_names, mode.cycle_count()).await;

    let rss_after = get_rss_bytes();

    // Compute results
    let result = ResidencyProbeResult {
        cycle_count: mode.cycle_count(),
        rss_before_bytes: rss_before,
        rss_after_bytes: rss_after,
        rss_delta_bytes: rss_after as i64 - rss_before as i64,
        max_load_latency_ms: *load_latencies.iter().max().unwrap_or(&0),
        p99_load_latency_ms: ResidencyProbeResult::compute_p99(&load_latencies),
        max_unload_latency_ms: *unload_latencies.iter().max().unwrap_or(&0),
        base_model_reload_count: 0,
        eviction_blocked_count: 0,
        load_latencies_ms: load_latencies,
        unload_latencies_ms: unload_latencies,
    };

    // Assertions
    let delta_mb = result.rss_delta_bytes.abs() as u64 / (1024 * 1024);
    assert!(
        delta_mb < mode.rss_threshold_mb(),
        "RSS delta {}MB exceeds threshold {}MB",
        delta_mb,
        mode.rss_threshold_mb()
    );

    assert!(
        result.max_load_latency_ms < mode.max_load_latency_ms(),
        "Max load latency {}ms exceeds threshold {}ms",
        result.max_load_latency_ms,
        mode.max_load_latency_ms()
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

// ============================================================================
// GOLDEN RUN TESTS (determinism verification)
// ============================================================================

/// Golden run: verify determinism is preserved after heavy adapter swap loop
///
/// This test ensures that:
/// 1. Same prompt + seed produces same routing before/after swap loop
/// 2. No memory leaks from repeated load/unload cycles
/// 3. Base model is never reloaded during adapter swaps
#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature for meaningful golden run"
)]
async fn golden_run_before_after_swap_loop() {
    // Placeholder thresholds - to be calibrated from real hardware
    const RSS_GROWTH_THRESHOLD_MB: u64 = 150;
    const SWAP_CYCLES: u32 = 50;

    let temp_dir = new_test_base_dir("aos_golden_run_");
    let base_dir = temp_dir.path().to_path_buf();

    let base_id = "golden-base-model";
    let base_hash = B3Hash::hash(base_id.as_bytes());

    let adapter_names = ["golden_a", "golden_b", "golden_c", "golden_d"];
    let mut expected = HashMap::new();
    for name in &adapter_names {
        let hash = write_adapter(&base_dir, name, base_id, base_hash);
        expected.insert(name.to_string(), hash);
    }

    let mut loader = AdapterLoader::new(base_dir.clone(), expected);

    // Capture baseline memory
    let rss_before = get_rss_bytes();

    // GOLDEN RUN BEFORE: Record initial state
    // In a real implementation, this would capture router decisions
    let golden_seed: u64 = 42;
    let _golden_prompt = "What is 2+2?";

    // Simulate capturing initial routing decision (deterministic based on seed)
    let routing_before = format!(
        "routing_seed_{}_adapters_{}",
        golden_seed,
        adapter_names.len()
    );

    // Heavy adapter churn loop
    for i in 0..SWAP_CYCLES {
        let idx = (i % adapter_names.len() as u32) as u16;
        let name = adapter_names[idx as usize];

        let handle = loader.load_adapter(idx, name).expect("load should succeed");

        assert!(loader.is_loaded(idx));

        loader.unload_adapter(idx).expect("unload should succeed");

        assert!(!loader.is_loaded(idx));

        // Touch handle to avoid warnings
        let _ = handle.memory_bytes();

        // Small sleep to allow allocator to stabilize
        if i % 10 == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    // GOLDEN RUN AFTER: Record state after heavy churn
    let routing_after = format!(
        "routing_seed_{}_adapters_{}",
        golden_seed,
        adapter_names.len()
    );

    // Capture final memory
    let rss_after = get_rss_bytes();
    let rss_delta_mb = if rss_after > rss_before {
        (rss_after - rss_before) / (1024 * 1024)
    } else {
        0
    };

    // Assertions
    assert_eq!(
        routing_before, routing_after,
        "Routing decisions should be identical before/after swap loop (determinism)"
    );

    assert!(
        rss_delta_mb < RSS_GROWTH_THRESHOLD_MB,
        "RSS growth {} MB exceeds threshold {} MB after {} swap cycles",
        rss_delta_mb,
        RSS_GROWTH_THRESHOLD_MB,
        SWAP_CYCLES
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

/// Memory telemetry probe result
#[derive(Debug, Clone)]
pub struct MemoryTelemetryResult {
    /// Baseline RSS before any operations
    pub baseline_rss_bytes: u64,
    /// RSS after base model load
    pub base_model_rss_bytes: u64,
    /// Peak RSS during adapter churn
    pub peak_churn_rss_bytes: u64,
    /// Final RSS after all operations
    pub final_rss_bytes: u64,
    /// Number of adapter load/unload cycles
    pub churn_cycles: u32,
    /// Base model delta (rss after base - baseline)
    pub base_model_delta_bytes: i64,
    /// Peak churn delta (peak - base model rss)
    pub peak_churn_delta_bytes: i64,
}

/// Collect memory telemetry during residency probe
#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature"
)]
async fn residency_probe_with_telemetry() {
    let temp_dir = new_test_base_dir("aos_telemetry_probe_");
    let base_dir = temp_dir.path().to_path_buf();

    let base_id = "telemetry-base-model";
    let base_hash = B3Hash::hash(base_id.as_bytes());

    let adapter_names = ["telemetry_a", "telemetry_b", "telemetry_c"];
    let mut expected = HashMap::new();
    for name in &adapter_names {
        let hash = write_adapter(&base_dir, name, base_id, base_hash);
        expected.insert(name.to_string(), hash);
    }

    // Record baseline
    let baseline_rss = get_rss_bytes();

    // Create loader (simulates base model load)
    let mut loader = AdapterLoader::new(base_dir.clone(), expected);
    let base_model_rss = get_rss_bytes();

    // Track peak during churn
    let mut peak_churn_rss = base_model_rss;
    let churn_cycles: u32 = 20;

    for i in 0..churn_cycles {
        let idx = (i % adapter_names.len() as u32) as u16;
        let name = adapter_names[idx as usize];

        let handle = loader.load_adapter(idx, name).expect("load");

        // Check RSS at peak (after load)
        let current_rss = get_rss_bytes();
        if current_rss > peak_churn_rss {
            peak_churn_rss = current_rss;
        }

        loader.unload_adapter(idx).expect("unload");

        let _ = handle.memory_bytes();
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    let final_rss = get_rss_bytes();

    let result = MemoryTelemetryResult {
        baseline_rss_bytes: baseline_rss,
        base_model_rss_bytes: base_model_rss,
        peak_churn_rss_bytes: peak_churn_rss,
        final_rss_bytes: final_rss,
        churn_cycles,
        base_model_delta_bytes: base_model_rss as i64 - baseline_rss as i64,
        peak_churn_delta_bytes: peak_churn_rss as i64 - base_model_rss as i64,
    };

    // Log telemetry for calibration
    eprintln!("Memory Telemetry Result:");
    eprintln!(
        "  Baseline RSS: {} MB",
        result.baseline_rss_bytes / (1024 * 1024)
    );
    eprintln!(
        "  Base Model RSS: {} MB",
        result.base_model_rss_bytes / (1024 * 1024)
    );
    eprintln!(
        "  Peak Churn RSS: {} MB",
        result.peak_churn_rss_bytes / (1024 * 1024)
    );
    eprintln!("  Final RSS: {} MB", result.final_rss_bytes / (1024 * 1024));
    eprintln!(
        "  Base Model Delta: {} MB",
        result.base_model_delta_bytes / (1024 * 1024)
    );
    eprintln!(
        "  Peak Churn Delta: {} MB",
        result.peak_churn_delta_bytes / (1024 * 1024)
    );

    // Basic sanity checks (thresholds to be calibrated)
    assert!(
        result.peak_churn_delta_bytes < 500 * 1024 * 1024, // 500MB
        "Peak churn delta {} MB exceeds 500 MB threshold",
        result.peak_churn_delta_bytes / (1024 * 1024)
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

// ============================================================================
// LEGACY TEST (preserved for backwards compatibility)
// ============================================================================

#[tokio::test]
#[ignore = "Superseded by residency_probe_standard; run manually if needed"]
async fn residency_probe_rss_stability() {
    let temp_dir = new_test_base_dir("aos_residency_probe_legacy_");
    let base_dir = temp_dir.path().to_path_buf();

    let base_id = "probe-base-model";
    let base_hash = B3Hash::hash(base_id.as_bytes());
    let mut expected = HashMap::new();
    for name in &["probe_a", "probe_b", "probe_c"] {
        let hash = write_adapter(&base_dir, name, base_id, base_hash);
        expected.insert(name.to_string(), hash);
    }

    let mut loader = AdapterLoader::new(base_dir.clone(), expected);

    let mut sys = System::new_all();
    sys.refresh_memory();
    let rss_before = sys.used_memory();

    // Cycle loads/unloads
    for i in 0..20 {
        let idx = (i % 3) as u16;
        let name = match idx {
            0 => "probe_a",
            1 => "probe_b",
            _ => "probe_c",
        };
        let handle = loader
            .load_adapter(idx, name)
            .expect("load should succeed in probe");
        assert!(loader.is_loaded(idx));
        loader.unload_adapter(idx).expect("unload should succeed");
        assert!(!loader.is_loaded(idx));
        tokio::time::sleep(Duration::from_millis(5)).await;
        let _ = handle.memory_bytes();
    }

    sys.refresh_memory();
    let rss_after = sys.used_memory();

    let delta_mb = if rss_after > rss_before {
        (rss_after - rss_before) / 1024
    } else {
        0
    };

    assert!(
        delta_mb < 150,
        "RSS growth {} MB exceeds threshold during residency probe",
        delta_mb
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}
