# Lora-Worker Quick Fix Reference

**70 Errors → 6 Categories → 5 Batches → 2.2 hours**

---

## Quick Win Targets (20 errors / 30 min)

### Batch 1: Imports (13 errors / 10 min)

| File | Line | Fix |
|------|------|-----|
| `src/lib.rs` | 43 | `use tokio::watch;` → `use tokio::sync::watch;` |
| `src/lib.rs` | Top | Add: `use parking_lot::RwLock;` |
| `src/memory.rs` | 85 | `use libc::{...}` → `use nix::libc::{...}` |
| `src/memory.rs` | 86 | `mach::host_info::host_info64` → `mach::host::host_info64` |
| `src/memory.rs` | 87 | `mach::mach_types::mach_port_t` → `mach::port::mach_port_t` |
| `src/memory.rs` | 88 | `mach::vm_statistics64::vm_statistics64_t` → `mach::vm_statistics::vm_statistics64_t` |

### Batch 2: Missing Fields (7 errors / 15 min)

| File | Line | Fix |
|------|------|-----|
| `src/adapter_hotswap.rs` | 156 | Add `active: RwLock::new(HashMap::new()),` before closing `}` |
| `src/adapter_hotswap.rs` | 172 | Add `active: RwLock::new(HashMap::new()),` before closing `}` |
| `src/lib.rs` | 362 | `self.manifest` → `manifest` |
| `src/lib.rs` | 403 | `router,` → `router?,` |

---

## High-Impact Fixes (30 errors / 60 min)

### Batch 3: Type Definitions (10 errors / 30 min)

**Add to `src/memory.rs` (after line 10):**
```rust
#[derive(Debug, Clone)]
pub struct UmaStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub headroom_pct: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressureLevel {
    Low,
    Medium,
    High,
    Critical,
}

fn determine_pressure(stats: &UmaStats, min_headroom: f32) -> MemoryPressureLevel {
    if stats.headroom_pct < min_headroom * 0.5 {
        MemoryPressureLevel::Critical
    } else if stats.headroom_pct < min_headroom * 0.75 {
        MemoryPressureLevel::High
    } else if stats.headroom_pct < min_headroom {
        MemoryPressureLevel::Medium
    } else {
        MemoryPressureLevel::Low
    }
}

async fn emit_telemetry(
    telemetry: &Option<TelemetryWriter>,
    stats: &UmaStats,
    pressure: MemoryPressureLevel,
) {
    if let Some(t) = telemetry {
        t.write_memory_pressure(stats.headroom_pct, pressure).await;
    }
}
```

**Add to `crates/adapteros-core/src/hash.rs` (in impl B3Hash block):**
```rust
/// Zero hash placeholder (32 zero bytes)
pub const fn zero() -> Self {
    Self([0u8; 32])
}
```

**Add to `src/adapter_hotswap.rs` (after imports):**
```rust
pub type StackHandle = Arc<Stack>;
```

### Batch 4: Type Mismatches (15 errors / 25 min)

| File | Line | Change |
|------|------|--------|
| `src/adapter_hotswap.rs` | 34 | `pub generation: u64,` → `pub generation: usize,` |
| `src/lib.rs` | 278 | `telemetry: TelemetryWriter,` → `telemetry: Option<TelemetryWriter>,` |
| `src/lib.rs` | 559, 1124, 1155, 1198 | Keep: `if let Some(t) = &self.telemetry {` (now valid) |
| `src/inference_pipeline.rs` | 233 | `.await??` → `.await?` |
| `src/kvcache.rs` | 353, 358 | `/ std::mem::size_of::<u8>()` → `/ std::mem::size_of::<u8>() as u64` |
| `src/kvcache.rs` | 377, 395 | `contents.as_ptr()` → `contents` |
| `src/kvcache.rs` | 384, 402 | `contents.as_mut_ptr()` → `contents` |

### Batch 5: Remove Duplicates (5 errors / 5 min)

| File | Lines | Action |
|------|-------|--------|
| `src/adapter_hotswap.rs` | 594-607 | **DELETE** entire second `dec_ref()` method |
| `src/memory.rs` | 191-end | **DELETE** second `headroom_pct_macos()` method |

---

## Thread Safety Fixes (20 errors / 40 min)

### Mutex Unwrapping Pattern

**Apply to ALL `self.refcounts.lock()` calls:**

| File | Lines | Fix Pattern |
|------|-------|-------------|
| `src/adapter_hotswap.rs` | 207, 280, 496, 587, 595 | `let refcounts = self.refcounts.lock();` → `let mut refcounts = self.refcounts.lock().unwrap();` |

**Specific fixes:**

```rust
// Line 208 (.entry on Result)
- refcounts.entry(id.clone())
+ let mut refcounts = self.refcounts.lock().unwrap();
+ refcounts.entry(id.clone())

// Line 497 (.get on Result)
- if let Some(rc) = refcounts.get(name) {
+ let refcounts = self.refcounts.lock().unwrap();
+ if let Some(rc) = refcounts.get(name) {

// Line 642 (type annotation)
- .map_or(false, |rc| rc.load(Ordering::Relaxed) == 0)
+ .map_or(false, |rc: &AtomicUsize| rc.load(Ordering::Relaxed) == 0)

// Line 370 (temporary borrow)
- let mut ids: Vec<_> = self.active.read().keys().collect();
+ let active_guard = self.active.read();
+ let mut ids: Vec<_> = active_guard.keys().collect();
```

---

## Function Signature Fixes (7 errors / 10 min)

| File | Line | Fix |
|------|------|-----|
| `src/lib.rs` | 394 | Add 3rd param: `Some(Arc::new(telemetry.clone()))` |
| `src/inference_pipeline.rs` | 208 | `circuit_breaker,` → `self.circuit_breaker.clone(),` |
| `src/adapter_hotswap.rs` | 746 | Add `#[derive(Clone)]` above struct |
| `src/memory.rs` | 34 | `get_uma_stats()` → `self.get_uma_stats()` |
| `src/memory.rs` | 200 | Wrap in async block or make function async |

---

## Execution Checklist

- [ ] **Batch 1** (10 min): Fix imports in `lib.rs` and `memory.rs`
- [ ] **Batch 2** (15 min): Add missing struct fields, fix `self` references
- [ ] **Batch 3** (30 min): Add type definitions (`UmaStats`, `B3Hash::zero()`, `StackHandle`)
- [ ] **Batch 4** (25 min): Fix all type mismatches (generation, telemetry, pointers)
- [ ] **Batch 5** (5 min): Delete duplicate methods
- [ ] **Mutex fixes** (30 min): Add `.unwrap()` to all mutex locks
- [ ] **Signature fixes** (10 min): Fix function parameters and derives

**Total Time:** 2.2 hours (130 minutes)

---

## Validation Commands

```bash
# After each batch:
cargo check -p adapteros-lora-worker 2>&1 | grep "error\[E"

# Final validation:
cargo build -p adapteros-lora-worker
cargo test -p adapteros-lora-worker
cargo clippy -p adapteros-lora-worker -- -D warnings
```

---

## Error Count by Batch

| Batch | Errors Fixed | Cumulative | % Complete |
|-------|--------------|------------|------------|
| 1 | 13 | 13 | 19% |
| 2 | 7 | 20 | 29% |
| 3 | 10 | 30 | 43% |
| 4 | 15 | 45 | 64% |
| 5 | 5 | 50 | 71% |
| Mutex | 18 | 68 | 97% |
| Sigs | 2 | 70 | 100% |

---

## Files Modified (6 total)

1. `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/lib.rs` - 12 changes
2. `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/adapter_hotswap.rs` - 35 changes
3. `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/memory.rs` - 8 changes
4. `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/kvcache.rs` - 6 changes
5. `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/inference_pipeline.rs` - 2 changes
6. `/Users/star/Dev/aos/crates/adapteros-core/src/hash.rs` - 1 change

**Lines Changed:** ~65 total

---

## Critical Path Items

⭐ **Must fix first:**
1. Imports (blocks compilation start)
2. Missing struct fields (blocks struct creation)
3. Type definitions (blocks type checking)

🔧 **Can parallelize:**
- Mutex unwrapping (independent fixes)
- Type mismatches (isolated changes)

⚠️ **Test after fixing:**
- Memory statistics (macOS-specific)
- Mutex poison recovery
- u64→usize conversions (64-bit assumption)
