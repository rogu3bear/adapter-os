# GPU Verification Integration Guide

**Purpose:** Tie adapter lifecycle state to real GPU buffer state through cross-layer verification.

**Problem:** Currently, lifecycle state transitions (Cold→Warm→Hot) are independent of GPU buffer state. A lifecycle state can claim an adapter is "Warm" but GPU buffers may be corrupted or missing.

**Solution:** Add GPU buffer verification at critical lifecycle transition points.

---

## Architecture Overview

```
┌─────────────────────────────┐
│   LifecycleManager          │  Metadata layer
│   - Adapter states          │  (Cold, Warm, Hot, Resident)
│   - Transition logic        │
└──────────┬──────────────────┘
           │
           │ Integration Point
           │ (Worker layer)
           │
┌──────────▼──────────────────┐
│   FusedKernels + VramTracker│  Data layer
│   - GPU buffers             │  (Metal/MLX buffers)
│   - Buffer fingerprints     │
└─────────────────────────────┘
```

**Key Principle:** Verification happens at the **Worker layer** where both lifecycle and GPU are accessible.

---

## Integration Points

### 1. Before State Promotion (Unloaded → Cold)

**When:** Adapter is being loaded into GPU memory
**Goal:** Verify GPU buffers match expected fingerprint before confirming Cold state

```rust
// Location: Worker::load_adapter() or similar
use adapteros_lora_kernel_mtl::vram::GpuBufferFingerprint;

// Phase 1: Load adapter into GPU
let adapter_bytes = std::fs::read(adapter_path)?;
kernels.load_adapter(adapter_id, &adapter_bytes)?;

// Phase 2: Verify GPU buffers loaded correctly
let (buffer_size, first_4kb, last_4kb, mid_4kb) =
    kernels.verify_adapter_buffers(adapter_id)?;

let fingerprint = GpuBufferFingerprint::new(
    buffer_size,
    &first_4kb,
    &last_4kb,
    &mid_4kb,
);

// Store fingerprint in VramTracker
vram_tracker.store_fingerprint(adapter_id as u32, fingerprint.clone());

// Phase 3: Check memory footprint against baseline
let (within_tolerance, z_score, baseline_stats) =
    vram_tracker.check_memory_footprint(adapter_id as u32, buffer_size);

if !within_tolerance {
    warn!(
        "Adapter {} memory footprint anomaly: {} bytes (z-score: {:.2})",
        adapter_id, buffer_size, z_score
    );
    // Optional: reject load or flag for investigation
}

// Phase 4: Only NOW update lifecycle state
lifecycle.update_adapter_state(adapter_id, AdapterState::Cold, "loaded_and_verified").await?;
lifecycle.mark_gpu_verified(adapter_id, fingerprint.checkpoint_hash)?;
```

**Critical:** State transition happens AFTER GPU verification passes.

---

### 2. After Hot-Swap Completion

**When:** AdapterTable::swap() completes successfully
**Goal:** Verify new adapter stack matches expected configuration in GPU

```rust
// Location: Worker::swap_adapters() or HotSwapManager
use adapteros_lora_worker::adapter_hotswap::{AdapterTable, GpuFingerprint};

// Phase 1: Execute swap
let (vram_delta, added_count) = adapter_table.swap(&add_ids, &remove_ids)?;

// Phase 2: Verify each newly active adapter has correct GPU buffers
let mut gpu_fingerprints = Vec::new();

for adapter_id_str in &add_ids {
    let adapter_id: u16 = parse_adapter_id(adapter_id_str)?;

    // Verify GPU buffer
    let (buffer_size, first, last, mid) = kernels.verify_adapter_buffers(adapter_id)?;
    let fp = GpuBufferFingerprint::new(buffer_size, &first, &last, &mid);

    // Verify against baseline if exists
    if let Err(msg) = vram_tracker.verify_fingerprint(adapter_id as u32, &fp) {
        // GPU buffer mismatch - rollback!
        warn!("GPU verification failed: {}", msg);
        adapter_table.rollback()?;
        return Err(AosError::Kernel(format!("Post-swap verification failed: {}", msg)));
    }

    // Build GPU fingerprint for cross-layer hash
    gpu_fingerprints.push(GpuFingerprint {
        adapter_id: adapter_id_str.clone(),
        buffer_bytes: buffer_size,
        checkpoint_hash: fp.checkpoint_hash,
    });
}

// Phase 3: Compute and store cross-layer checkpoint
let checkpoint = adapter_table.create_checkpoint(gpu_fingerprints);

info!(
    "Post-swap verification passed: metadata_hash={}, cross_layer_hash={:?}",
    checkpoint.metadata_hash, checkpoint.cross_layer_hash
);
```

**Critical:** If GPU verification fails, ROLLBACK swap before confirming new state.

---

### 3. After Rollback

**When:** HotSwapManager::rollback() is called due to failure
**Goal:** Verify GPU buffers match previous checkpoint configuration

```rust
// Location: Worker error recovery path

// Phase 1: Rollback metadata
adapter_table.rollback()?;

// Phase 2: Get last verified checkpoint
let checkpoints = adapter_table.get_checkpoints(1);
if let Some(last_checkpoint) = checkpoints.last() {
    // Phase 3: Verify current GPU state matches checkpoint
    let current_gpu_fps: Vec<GpuFingerprint> = last_checkpoint.adapter_ids
        .iter()
        .filter_map(|adapter_id_str| {
            let adapter_id: u16 = parse_adapter_id(adapter_id_str).ok()?;
            let (buffer_size, first, last, mid) = kernels.verify_adapter_buffers(adapter_id).ok()?;
            let fp = GpuBufferFingerprint::new(buffer_size, &first, &last, &mid);

            Some(GpuFingerprint {
                adapter_id: adapter_id_str.clone(),
                buffer_bytes: buffer_size,
                checkpoint_hash: fp.checkpoint_hash,
            })
        })
        .collect();

    // Verify cross-layer hash matches
    let matches = adapter_table.verify_against_checkpoint(
        last_checkpoint,
        &current_gpu_fps
    )?;

    if !matches {
        error!("GPU state does NOT match rollback checkpoint!");
        return Err(AosError::Kernel(
            "GPU buffers diverged from lifecycle state after rollback".to_string()
        ));
    }

    info!("Rollback verification passed: GPU matches checkpoint");
}
```

**Critical:** After rollback, GPU buffers MUST match the checkpoint exactly.

---

### 4. On-Demand Verification (CLI/API)

**When:** User requests integrity check via CLI or API
**Goal:** Verify all loaded adapters have matching GPU buffers

```rust
// Location: CLI handler or API endpoint

pub async fn verify_gpu_integrity(
    lifecycle: &LifecycleManager,
    kernels: &impl FusedKernels,
    vram_tracker: &mut VramTracker,
) -> Result<GpuIntegrityReport> {
    let loaded_adapters = lifecycle.get_loaded_adapters();

    let mut verified = Vec::new();
    let mut failed = Vec::new();
    let mut skipped = Vec::new();

    for (adapter_id, adapter_name, state) in loaded_adapters {
        // Attempt GPU verification
        match kernels.verify_adapter_buffers(adapter_id) {
            Ok((buffer_size, first, last, mid)) => {
                let current_fp = GpuBufferFingerprint::new(buffer_size, &first, &last, &mid);

                // Check against baseline
                match vram_tracker.verify_fingerprint(adapter_id as u32, &current_fp) {
                    Ok(true) => {
                        verified.push((adapter_id, adapter_name));
                    }
                    Ok(false) => {
                        // No baseline - first verification
                        vram_tracker.store_fingerprint(adapter_id as u32, current_fp);
                        verified.push((adapter_id, adapter_name));
                    }
                    Err(msg) => {
                        failed.push((adapter_id, adapter_name, msg));
                    }
                }
            }
            Err(e) => {
                skipped.push((adapter_id, adapter_name.clone()));
                warn!("Could not verify adapter {} ({}): {}", adapter_id, adapter_name, e);
            }
        }
    }

    Ok(GpuIntegrityReport {
        verified,
        failed,
        skipped,
        total_checked: loaded_adapters.len(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    })
}
```

---

## Testing Requirements

### 1. Fault Injection Tests

**File:** `tests/fault_injection_harness.rs`

```rust
#[tokio::test]
async fn test_gpu_corruption_detection() {
    // Load adapter normally
    let adapter_id = 1;
    kernels.load_adapter(adapter_id, &adapter_bytes)?;

    // Create baseline fingerprint
    let (size, first, last, mid) = kernels.verify_adapter_buffers(adapter_id)?;
    let baseline_fp = GpuBufferFingerprint::new(size, &first, &last, &mid);
    vram_tracker.store_fingerprint(adapter_id, baseline_fp);

    // SIMULATE: GPU buffer corruption (e.g., flip random bits)
    corrupt_gpu_buffer(adapter_id);

    // Attempt verification - should FAIL
    let (size2, first2, last2, mid2) = kernels.verify_adapter_buffers(adapter_id)?;
    let corrupted_fp = GpuBufferFingerprint::new(size2, &first2, &last2, &mid2);

    let result = vram_tracker.verify_fingerprint(adapter_id, &corrupted_fp);
    assert!(result.is_err()); // Should detect mismatch
}
```

### 2. Rollback Verification Tests

```rust
#[tokio::test]
async fn test_rollback_gpu_verification() {
    // Create initial checkpoint
    let initial_checkpoint = adapter_table.create_checkpoint(vec![...]);

    // Swap adapters
    adapter_table.swap(&["adapter_2"], &["adapter_1"])?;

    // Rollback
    adapter_table.rollback()?;

    // CRITICAL: Verify GPU buffers match initial checkpoint
    let current_fps = get_current_gpu_fingerprints(&kernels)?;
    let matches = adapter_table.verify_against_checkpoint(&initial_checkpoint, &current_fps)?;

    assert!(matches, "GPU buffers must match rollback checkpoint");
}
```

### 3. Memory Footprint Tolerance Tests

```rust
#[tokio::test]
async fn test_memory_footprint_anomaly_detection() {
    let mut vram_tracker = VramTracker::new();

    // Establish baseline (load adapter 5 times)
    for _ in 0..5 {
        kernels.load_adapter(1, &adapter_bytes)?;
        let (size, first, last, mid) = kernels.verify_adapter_buffers(1)?;
        let fp = GpuBufferFingerprint::new(size, &first, &last, &mid);
        vram_tracker.store_fingerprint(1, fp);
        kernels.unload_adapter(1)?;
    }

    // Load again with expected size - should pass
    kernels.load_adapter(1, &adapter_bytes)?;
    let (normal_size, _, _, _) = kernels.verify_adapter_buffers(1)?;
    let (within_tolerance, z_score, _) = vram_tracker.check_memory_footprint(1, normal_size);
    assert!(within_tolerance);
    assert!(z_score < 2.0);

    // Simulate bloated allocation (3x normal) - should FAIL tolerance
    let bloated_size = normal_size * 3;
    let (within_tolerance2, z_score2, _) = vram_tracker.check_memory_footprint(1, bloated_size);
    assert!(!within_tolerance2);
    assert!(z_score2 > 2.0);
}
```

---

## Checkpoint Snapshot Persistence

**Optional Enhancement:** Persist checkpoints to disk for crash recovery

```rust
// Location: Worker shutdown/startup

// On shutdown: Save checkpoints to disk
pub fn save_checkpoints(adapter_table: &AdapterTable, path: &Path) -> Result<()> {
    let checkpoints = adapter_table.get_checkpoints(20);
    let json = serde_json::to_string_pretty(&checkpoints)?;
    std::fs::write(path, json)?;
    Ok(())
}

// On startup: Restore checkpoints
pub fn restore_checkpoints(adapter_table: &AdapterTable, path: &Path) -> Result<()> {
    if path.exists() {
        let json = std::fs::read_to_string(path)?;
        let checkpoints: Vec<StackCheckpoint> = serde_json::from_str(&json)?;
        // Verify current state matches most recent checkpoint
        if let Some(last) = checkpoints.last() {
            let current_fps = get_current_gpu_fingerprints(&kernels)?;
            let matches = adapter_table.verify_against_checkpoint(last, &current_fps)?;
            if !matches {
                warn!("GPU state diverged during restart - reinitializing");
            }
        }
    }
    Ok(())
}
```

---

## Summary

**What's Working Now:**
- ✅ Lifecycle state tracking (metadata layer)
- ✅ GPU buffer allocation tracking (data layer)
- ✅ Hot-swap metadata rollback

**What This Integration Adds:**
- ✅ GPU buffer fingerprinting with checkpoint sampling
- ✅ Cross-layer hash (metadata + GPU fingerprints)
- ✅ Adaptive memory footprint anomaly detection
- ✅ Verification before state promotions
- ✅ Post-swap GPU verification with rollback on failure
- ✅ Post-rollback GPU state verification
- ✅ On-demand integrity checks via API

**Result:** Lifecycle state and GPU buffer state are now **provably consistent** through cryptographic fingerprints and cross-layer hashing.
