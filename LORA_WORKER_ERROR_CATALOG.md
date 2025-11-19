# Lora-Worker Error Catalog (Spreadsheet Format)

**Total Errors:** 70 | **Total Warnings:** 15 | **Files Affected:** 6

---

## Error Breakdown by Category

| Category | Count | % of Total | Complexity | Est. Time | Priority |
|----------|-------|------------|------------|-----------|----------|
| 1. Missing Imports | 13 | 19% | Simple | 10 min | ⭐ CRITICAL |
| 2. Struct Fields | 7 | 10% | Simple | 15 min | ⭐ CRITICAL |
| 3. Mutex Unwrapping | 18 | 26% | Medium | 30 min | HIGH |
| 4. Type Mismatches | 15 | 21% | Medium | 25 min | HIGH |
| 5. Missing Types/Methods | 10 | 14% | Medium-Hard | 45 min | HIGH |
| 6. Duplicate Defs | 7 | 10% | Simple | 15 min | MEDIUM |
| **TOTAL** | **70** | **100%** | — | **2.2 hrs** | — |

---

## Detailed Error List (Sorted by File + Line)

### File: `crates/adapteros-lora-worker/src/lib.rs` (12 errors)

| Line | Error Code | Type | Description | Fix Complexity | Batch |
|------|------------|------|-------------|----------------|-------|
| 43 | E0432 | Import | `tokio::watch` not found | Simple | 1 |
| 270 | E0412 | Import | `RwLock` not found | Simple | 1 |
| 286 | E0433 | Import | `tokio::watch::Sender` not found | Simple | 1 |
| 362 | E0424 | Self | `self.manifest` in static context | Simple | 2 |
| 363 | E0433 | Import | `RwLock::new()` not found | Simple | 1 |
| 394 | E0061 | Sig | Missing 3rd param (telemetry) | Simple | 6 |
| 397 | E0433 | Import | `tokio::watch::channel` not found | Simple | 1 |
| 398 | E0599 | Clone | `HotSwapManager::clone()` missing | Medium | 6 |
| 403 | E0308 | Type | `router` not unwrapped | Simple | 2 |
| 559 | E0308 | Type | `Option` pattern on non-Option | Medium | 4 |
| 1124 | E0308 | Type | `Option` pattern on non-Option | Medium | 4 |
| 1155 | E0308 | Type | `Option` pattern on non-Option | Medium | 4 |
| 1198 | E0308 | Type | `Option` pattern on non-Option | Medium | 4 |
| 1278 | E0616 | Access | `current_stack` private | Medium | 3 |
| 1279 | E0616 | Access | `refcounts` private | Medium | 3 |
| 1282 | E0282 | Type | Type annotation needed | Medium | 3 |

### File: `crates/adapteros-lora-worker/src/adapter_hotswap.rs` (35 errors)

| Line | Error Code | Type | Description | Fix Complexity | Batch |
|------|------------|------|-------------|----------------|-------|
| 34 | — | Type | `generation: u64` (should be usize) | Medium | 4 |
| 156 | E0063 | Field | Missing `active` field | Simple | 2 |
| 172 | E0063 | Field | Missing `active` field | Simple | 2 |
| 207 | E0599 | Mutex | `.lock()` not unwrapped | Medium | Mutex |
| 208 | E0599 | Mutex | `.entry()` on Result | Medium | Mutex |
| 221 | E0308 | Type | u64 → usize mismatch | Medium | 4 |
| 254 | E0308 | Type | u64 → usize in swap | Medium | 4 |
| 280 | E0599 | Mutex | `.lock()` not unwrapped | Medium | Mutex |
| 281 | E0599 | Mutex | `.entry()` on Result | Medium | Mutex |
| 288 | E0308 | Type | u64 → usize mismatch | Medium | 4 |
| 298 | E0308 | Type | u64 → usize mismatch | Medium | 4 |
| 317 | E0308 | Type | u64 → usize in swap | Medium | 4 |
| 320 | E0308 | Type | u64 → usize comparison | Medium | 4 |
| 323 | E0308 | Type | u64 → usize mismatch | Medium | 4 |
| 370 | E0716 | Borrow | Temporary value dropped | Medium | Mutex |
| 495 | — | Dup | First `dec_ref()` (keep) | — | — |
| 497 | E0599 | Mutex | `.get()` on Result | Medium | Mutex |
| 498 | E0282 | Type | Closure type annotation | Medium | Mutex |
| 588 | E0599 | Mutex | `.get()` on Result | Medium | Mutex |
| 589 | E0282 | Type | Closure type annotation | Medium | Mutex |
| 594 | E0592 | Dup | Duplicate `dec_ref()` (delete) | Simple | 5 |
| 596 | E0599 | Mutex | `.get()` on Result | Medium | Mutex |
| 597 | E0282 | Type | Closure type annotation | Medium | Mutex |
| 642 | E0599 | Mutex | `.get()` on Result | Medium | Mutex |
| 643 | E0282 | Type | Closure type annotation | Medium | Mutex |
| 732 | E0412 | Type | `StackHandle` not found | Medium | 3 |
| 746 | — | Clone | Missing `#[derive(Clone)]` | Simple | 6 |
| 1002 | E0599 | Method | `vram_tracker()` missing | Medium | 5 |
| 1020 | E0599 | Method | `B3Hash::zero()` missing | Medium | 3 |
| 1065 | E0599 | Method | `vram_tracker()` missing | Medium | 5 |
| 1103 | E0599 | Method | `B3Hash::zero()` missing | Medium | 3 |

### File: `crates/adapteros-lora-worker/src/memory.rs` (8 errors)

| Line | Error Code | Type | Description | Fix Complexity | Batch |
|------|------------|------|-------------|----------------|-------|
| 34 | E0425 | Method | `get_uma_stats()` not in scope | Medium | 6 |
| 35 | E0425 | Method | `determine_pressure()` not in scope | Medium | 3 |
| 37 | E0425 | Method | `emit_telemetry()` not in scope | Medium | 3 |
| 84 | — | Dup | First `headroom_pct_macos()` (keep) | — | — |
| 85 | E0432 | Import | `libc` not found | Simple | 1 |
| 86 | E0432 | Import | `mach::host_info` not found | Simple | 1 |
| 87 | E0603 | Import | `mach_port_t` private | Simple | 1 |
| 88 | E0432 | Import | `mach::vm_statistics64` not found | Simple | 1 |
| 90 | E0425 | Method | `mach_host_self()` not found | Simple | 1 |
| 93 | E0412 | Type | `mach_msg_type_number_t` not found | Simple | 1 |
| 108 | E0425 | Value | `vm_kernel_page_size` not found | Simple | 1 |
| 174 | E0412 | Type | `UmaStats` not defined | Medium | 3 |
| 183 | E0422 | Type | `UmaStats` not found | Medium | 3 |
| 191 | E0592 | Dup | Duplicate `headroom_pct_macos()` (delete) | Simple | 5 |
| 200 | E0728 | Async | `await` in sync function | Medium | 6 |
| 204 | E0599 | Method | `parse_vm_stat()` missing | Medium | 5 |

### File: `crates/adapteros-lora-worker/src/kvcache.rs` (6 errors)

| Line | Error Code | Type | Description | Fix Complexity | Batch |
|------|------------|------|-------------|----------------|-------|
| 353 | E0308 | Type | u64 / usize mismatch | Simple | 4 |
| 353 | E0277 | Type | Cannot divide u64 by usize | Simple | 4 |
| 358 | E0308 | Type | u64 / usize mismatch | Simple | 4 |
| 358 | E0277 | Type | Cannot divide u64 by usize | Simple | 4 |
| 377 | E0599 | Method | `as_ptr()` on raw pointer | Simple | 4 |
| 384 | E0599 | Method | `as_mut_ptr()` on raw pointer | Simple | 4 |
| 395 | E0599 | Method | `as_ptr()` on raw pointer | Simple | 4 |
| 402 | E0599 | Method | `as_mut_ptr()` on raw pointer | Simple | 4 |

### File: `crates/adapteros-lora-worker/src/inference_pipeline.rs` (2 errors)

| Line | Error Code | Type | Description | Fix Complexity | Batch |
|------|------------|------|-------------|----------------|-------|
| 208 | E0425 | Scope | `circuit_breaker` not in scope | Simple | 6 |
| 217 | E0308 | Type | Double `??` operator | Simple | 4 |

### File: `crates/adapteros-core/src/hash.rs` (1 addition)

| Line | Error Code | Type | Description | Fix Complexity | Batch |
|------|------------|------|-------------|----------------|-------|
| — | — | Add | Need `B3Hash::zero()` method | Simple | 3 |

---

## Error Code Frequency

| Error Code | Count | Description | Common Fix |
|------------|-------|-------------|------------|
| E0432 | 6 | Unresolved import | Fix import path |
| E0599 | 16 | Method not found | Unwrap Result, add method |
| E0308 | 11 | Type mismatch | Cast, unwrap, or change type |
| E0412 | 4 | Type not found | Define type or import |
| E0282 | 5 | Type annotation needed | Add explicit type |
| E0592 | 2 | Duplicate definition | Delete one |
| E0063 | 2 | Missing field | Add field to initializer |
| E0425 | 4 | Value/function not found | Import or change scope |
| E0433 | 3 | Failed resolve | Fix import path |
| E0424 | 1 | Invalid self | Change to parameter |
| E0061 | 1 | Arg count mismatch | Add missing argument |
| E0616 | 2 | Private field | Use accessor or make public |
| E0603 | 1 | Private import | Import from correct path |
| E0422 | 1 | Struct not found | Define struct |
| E0716 | 1 | Borrow lifetime | Extend guard lifetime |
| E0277 | 2 | Trait not implemented | Cast operands |
| E0728 | 1 | Await in sync | Make function async |
| **TOTAL** | **63** | (7 duplicates) | — |

---

## Warnings (15 total)

| Warning Code | Count | Description | Action |
|--------------|-------|-------------|--------|
| unused_imports | 13 | Unused use statements | Run `cargo fix` |
| unexpected_cfgs | 1 | `feature = "test-utils"` unknown | Add to Cargo.toml or remove |
| dead_code | 1 | Unused fields in validators | Suppress or remove |
| **TOTAL** | **15** | — | Auto-fixable |

---

## Fix Dependencies (Blocking Graph)

```
Batch 1 (Imports)
   ↓
Batch 2 (Struct Fields)
   ↓
Batch 3 (Type Definitions) ←─── CRITICAL PATH
   ↓
Batch 4 (Type Mismatches) ←──┐
   ↓                         │
Mutex Fixes ─────────────────┘
   ↓
Batch 5 (Duplicates)
   ↓
Batch 6 (Signatures)
   ↓
✅ Compilation Success
```

---

## Risk Matrix

| Fix Category | Risk Level | Mitigation |
|--------------|------------|------------|
| Imports | Low | Mechanical change |
| Struct Fields | Low | Compiler enforced |
| Type Definitions | Medium | Unit test `UmaStats` |
| Mutex Unwrapping | **High** | Add poison recovery |
| Type Casts | Medium | Validate 64-bit assumption |
| Duplicates | Low | Dead code removal |
| Signatures | Medium | Integration test hot-swap |

---

## Complexity Distribution

| Complexity | Error Count | % of Total | Time Estimate |
|------------|-------------|------------|---------------|
| Simple | 27 | 39% | 40 min |
| Medium | 38 | 54% | 80 min |
| Hard | 5 | 7% | 10 min |
| **TOTAL** | **70** | **100%** | **130 min** |

---

## Files Sorted by Change Density

| File | Errors | Lines | Density | Priority |
|------|--------|-------|---------|----------|
| `adapter_hotswap.rs` | 35 | ~1100 | 3.2% | HIGH |
| `lib.rs` | 12 | ~1400 | 0.9% | HIGH |
| `memory.rs` | 8 | ~210 | 3.8% | MEDIUM |
| `kvcache.rs` | 6 | ~420 | 1.4% | MEDIUM |
| `inference_pipeline.rs` | 2 | ~250 | 0.8% | LOW |
| `hash.rs` | 1 | ~150 | 0.7% | LOW |

**Total Lines Changed:** ~65 / ~3530 = **1.8% of lora-worker codebase**

---

## Batch Execution Plan (Optimized)

| Batch | Time | Errors | Cumulative | Build Status |
|-------|------|--------|------------|--------------|
| 1 | 10 min | 13 | 13 (19%) | ❌ Still failing (57 errors) |
| 2 | 15 min | 7 | 20 (29%) | ❌ Still failing (50 errors) |
| 3 | 30 min | 10 | 30 (43%) | ⚠️ Partial (40 errors) |
| 4 | 25 min | 15 | 45 (64%) | ⚠️ Partial (25 errors) |
| 5 | 5 min | 5 | 50 (71%) | ⚠️ Partial (20 errors) |
| Mutex | 30 min | 18 | 68 (97%) | ⚠️ Near success (2 errors) |
| 6 | 10 min | 2 | 70 (100%) | ✅ **SUCCESS** |

---

## Success Metrics

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| Compilation Errors | 70 | 0 | 🔴 |
| Warnings | 15 | <10 | 🟡 |
| Build Time | N/A | <60s | ⚪ |
| Test Pass Rate | 0% | 100% | 🔴 |
| Downstream Blocks | 1 (server-api) | 0 | 🔴 |

---

## Next Steps Checklist

- [ ] Review fix plan with lead developer
- [ ] Create feature branch: `fix/lora-worker-compilation`
- [ ] Execute batches 1-6 sequentially
- [ ] Run `cargo test -p adapteros-lora-worker` after each batch
- [ ] Validate with `cargo build -p adapteros-server-api`
- [ ] Update CHANGELOG.md with fix summary
- [ ] Create PR with detailed commit messages

---

**Document Generated:** 2025-11-19
**Agent:** Claude Code Agent 18
**Command:** `cargo build -p adapteros-lora-worker`
**Output:** `/Users/star/Dev/aos/lora_worker_errors.txt`
