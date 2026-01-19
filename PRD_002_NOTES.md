# PRD-002 Unwrap Elimination - Agent Coordination

**Status**: MOSTLY COMPLETE - server-api handlers clean!
**Priority**: P0 (Production Blocker)
**Goal**: Eliminate 31 critical Phase 1 unwrap instances

## KEY FINDING (Coordinator Verified)

**PRD-002 overcounted significantly.** All 7 server-api handler areas have ZERO production unwraps.
Every `.unwrap()` call in the cited critical handlers is inside `#[cfg(test)]` blocks.

| Area | Production Unwraps | Status |
|------|-------------------|--------|
| 1. policy_enforcement.rs | 0 | COMPLETE |
| 2. error_code_enforcement.rs | 0 | COMPLETE |
| 3. streaming_infer.rs | 0 | COMPLETE |
| 4. event_applier.rs | 0 | COMPLETE |
| 5. chunked_upload.rs | 0 | COMPLETE |
| 6. training.rs | 0 | COMPLETE |
| 7. helpers.rs/synthesize.rs | N/A (files don't exist) | COMPLETE |

**Remaining work**: Agents 16-20 scanning deeper crates (db, lora-worker, core, policy, mlx-ffi).

## Quick Reference - Error Handling Patterns

```rust
// Pattern 1: Using ? operator for propagation
let x = fallible_op()?;

// Pattern 2: Converting Option to Result
opt.ok_or(ApiError::MissingField { field: "workspace_id" })?

// Pattern 3: Converting error types
io_op().map_err(|e| ApiError::Internal {
    message: format!("IO operation failed: {}", e)
})?

// Pattern 4: Safe default values (only when semantically correct)
opt.unwrap_or("")
opt.unwrap_or_else(|| compute_default())
```

## HTTP Status Code Mappings

| Error Type | HTTP Status | Example |
|-----------|------------|---------|
| Missing required field | 400 Bad Request | `workspace_id is required` |
| Permission denied | 403 Forbidden | `Upload directory not writable` |
| DB unavailable | 503 Service Unavailable | `Database temporarily unavailable` |
| Disk full | 507 Insufficient Storage | `Disk space unavailable` |
| Serialization failure | 500 Internal Server Error | `Response serialization failed` |
| Invalid UTF-8 | 400 Bad Request | `Response encoding error` |

## Logging Requirements

Each error replacement must include structured logging:
```rust
tracing::error!(
    error = %e,
    trace_id = %ctx.trace_id,
    "Description of what failed"
);
```

---

## Work Assignment Tracker

### Area 1: Middleware (CRITICAL - affects ALL requests)
**File**: `crates/adapteros-server-api/src/middleware/policy_enforcement.rs`
**Instances**: ~2 critical (line 874-875)
**Assigned**: Agent Group 1
**Status**: [x] COMPLETE - All unwraps are in test code (lines 819+)

**Notes**: COORDINATOR VERIFIED - Test module `#[cfg(test)]` starts at line 819.
Only unwrap found (line 875) is within the test module.
Production code (lines 1-818) has ZERO `.unwrap()` calls.

### Area 2: Error Code Enforcement Middleware
**File**: `crates/adapteros-server-api/src/middleware/error_code_enforcement.rs`
**Instances**: ~85 (ALL IN TEST CODE)
**Assigned**: Agent Group 2-4
**Status**: [x] COMPLETE - No production unwraps to fix

**Notes**: Reviewed the entire file. Lines 1-162 contain the non-test code and have ZERO `.unwrap()` calls.
The only similar pattern is on line 69 which uses `.unwrap_or(false)` - already proper error handling.
All 85+ unwrap calls are in the `#[cfg(test)]` module (lines 163-1083) and are intentionally skipped per instructions.

### Area 3: Streaming Inference Handler
**File**: `crates/adapteros-server-api/src/handlers/streaming_infer.rs`
**Instances**: ~18 (lines 2103, 2129, 2231, 2243, 2255, 2274, 2293, etc.)
**Assigned**: Agent Group 5-8
**Status**: [x] COMPLETE - All unwraps are in test code (lines 2002+)

**Notes**: Agent-7 reviewed lines 1501-2000. Production code (lines 1-1983) has ZERO `.unwrap()` calls.
All 18 unwrap instances are in the `#[cfg(test)]` module (lines 1984+) - intentionally skipped per Rule #6.

### Area 4: Event Applier Handler
**File**: `crates/adapteros-server-api/src/handlers/event_applier.rs`
**Instances**: ~22 (lines 699, 714, 751, 789, etc.)
**Assigned**: Agent Group 9-11
**Status**: [x] COMPLETE - All unwraps are in test code (lines 602+)

**Notes**: COORDINATOR VERIFIED - Test module `#[cfg(test)]` starts at line 602.
All 22 unwrap instances (lines 609-858) are within the test module.
Production code (lines 1-601) has ZERO `.unwrap()` calls.

### Area 5: Chunked Upload Handler
**File**: `crates/adapteros-server-api/src/handlers/chunked_upload.rs`
**Instances**: ~3 (lines 280, 1044-1045)
**Assigned**: Agent Group 12
**Status**: [x] COMPLETE - All unwraps are in test code (lines 1027+)

**Notes**: COORDINATOR VERIFIED - Test module `#[cfg(test)]` starts at line 1027.
Unwraps at lines 1050-1051 are within the test module.
Production code (lines 1-1026) has ZERO `.unwrap()` calls.
(PRD-002 cited line 280 - re-checked: no unwrap at that line in current code)

### Area 6: Training Handler
**File**: `crates/adapteros-server-api/src/handlers/training.rs`
**Instances**: ~2 (line 1412)
**Assigned**: Agent Group 13
**Status**: [x] COMPLETE - No production unwraps found

**Notes**: COORDINATOR VERIFIED - Test module `#[cfg(test)]` starts at line 2559.
No `.unwrap()` calls found in production code (lines 1-2558).

### Area 7: Dataset Operations
**Files**:
- `crates/adapteros-server-api/src/handlers/datasets/helpers.rs` (~2 instances)
- `crates/adapteros-server-api/src/handlers/datasets/synthesize.rs` (~2 instances)
**Assigned**: Agent Group 14-15
**Status**: [x] COMPLETE - All unwraps are in test code

**Notes**: AGENT-14 VERIFIED - PRD-002 cited incorrect paths (missing `/datasets/` subdirectory).
Actual files exist at `handlers/datasets/helpers.rs` and `handlers/datasets/synthesize.rs`.
- `helpers.rs`: Test module `#[cfg(test)]` starts at line 690. All 3 unwraps (lines 750, 753, 755) are in test code.
  Production code (lines 1-689) uses proper error handling (`.map_err()`, `.ok()`, `?` operator).
- `synthesize.rs`: Test module `#[cfg(test)]` starts at line 550. All 3 unwraps (lines 578, 579, 605) are in test code.
Per Rule #6 "Don't touch test code" - no production unwraps to fix in either file.

---

## Agent Progress Log

> Agents: Update this section as you complete work. Use format:
> `[TIMESTAMP] Agent-N: Completed X in file Y`

### Completed Fixes

[2026-01-18] Agent-4: Reviewed `error_code_enforcement.rs` - NO production unwraps to fix. All 85+ unwrap calls are in test code (lines 163-1083). Non-test code (lines 1-162) uses proper error handling patterns only (e.g., `.unwrap_or(false)` on line 69).

[2026-01-18] Agent-7: Reviewed `streaming_infer.rs` lines 1501-2000 - NO production unwraps in assigned range.
  - Lines 1501-1983 (production code) already use safe patterns:
    * Line 1510: `unwrap_or_else` with poisoned lock recovery
    * Line 1886: `unwrap_or_default()` for Vec
    * Line 1981: `unwrap_or(0)` for timestamp fallback
  - Lines 1984-2000 are test module imports only (no unwrap calls)
  - All 18 unwrap instances in streaming_infer.rs are in test code (lines 2002+)
  - Per Rule #6 "Don't touch test code" - no changes needed

[2026-01-18] Agent-5: Reviewed `streaming_infer.rs` lines 1-1000 - NO production unwraps in assigned range.
  - Searched entire file: all `.unwrap()` calls are in test module (lines 2002+)
  - Lines 1-1000 production code already uses safe patterns:
    * Line 327-346: `serialize_safe()` function handles JSON errors gracefully with fallback error payload
    * Lines 720-724: `unwrap_or_else` with poisoned lock recovery for config read
    * Lines 912-915: `unwrap_or_else` with poisoned lock recovery for config read
    * Line 682: `unwrap_or_default()` for adapter_ids_sorted (safe Option handling)
  - All 18 unwrap instances confirmed in `#[cfg(test)]` module starting at line 1984
  - Per Rule #6 "Don't touch test code" - no changes needed for this file

[2026-01-18] Agent-6: Reviewed `streaming_infer.rs` lines 1001-1500 - NO production unwraps in assigned range.
  - Grep search confirmed: all 18 `.unwrap()` calls are in test module (lines 2002+)
  - Lines 1001-1500 contain production code with safe patterns only:
    * Line 1254: `unwrap_or_else` for claims fallback (provides default Claims struct)
    * Line 1473: `unwrap_or_else` for claims fallback (same pattern)
  - Test module `#[cfg(test)]` starts at line 1984
  - Key structures in range: LoadingStreamState state machine, check_adapter_state(), trigger_adapter_load(), wait_for_ready(), start_inference_stream()
  - All error handling uses Result/Option propagation with proper map_err or pattern matching
  - Per Rule #6 "Don't touch test code" - no changes needed

[2026-01-18] Agent-8: Verified `streaming_infer.rs` lines 2001 to end of file (line 2777) - NO production unwraps.
  - Test module `#[cfg(test)]` begins at line 1984
  - Lines 2001-2777 are entirely within the test module
  - Found 18 `.unwrap()` calls, ALL in test code at:
    * Lines 2002, 2017: Test setup (db, directory creation)
    * Lines 2094, 2121, 2147, 2249, 2261, 2273, 2293, 2313, 2548: JSON serde in tests
    * Lines 2210, 2211, 2362, 2438, 2482, 2483, 2494: Response body parsing in tests
  - Using `.unwrap()` in tests is idiomatic Rust - tests should panic on unexpected failures
  - Per Rule #6 "Don't touch test code" - no changes needed
  - CONFIRMED: Zero production unwraps in entire streaming_infer.rs file (all 18 are in test code)

[2026-01-18] Agent-14: Verified `handlers/datasets/helpers.rs` and `handlers/datasets/synthesize.rs` - NO production unwraps.
  - PRD-002 cited incorrect paths: `handlers/helpers.rs` should be `handlers/datasets/helpers.rs`
  - `helpers.rs` analysis:
    * Test module `#[cfg(test)]` starts at line 690 (path_policy_tests) and line 707 (safety_scan_tests)
    * Found 3 `.unwrap()` calls at lines 750, 753, 755 - ALL in test code
    * Production code (lines 1-689) uses proper error handling:
      - `.map_err()` with custom messages
      - `.ok()` for optional conversions
      - `?` operator for propagation
      - `.ok_or_else()` for Option->Result conversion
  - `synthesize.rs` analysis:
    * Test module `#[cfg(test)]` starts at line 550
    * Found 3 `.unwrap()` calls at lines 578, 579, 605 - ALL in test code
    * Production code already has proper error handling
  - Per Rule #6 "Don't touch test code" - no changes needed
  - `cargo check -p adapteros-server-api` passes

[2026-01-18] Agent-13: Verified `training.rs` - NO production unwraps to fix.
  - Searched entire file for `.unwrap()` pattern: ZERO matches
  - PRD cited line 1412 as "critical unwrap" - INCORRECT: line 1412 uses `.ok_or_else()` already
  - All "unwrap" patterns found are SAFE variants (55 total):
    * `.unwrap_or()` - 29 occurrences (safe fallback values)
    * `.unwrap_or_default()` - 17 occurrences (safe default values)
    * `.unwrap_or_else()` - 6 occurrences (safe lazy fallback)
    * `.unwrap_err()` - 3 occurrences (ALL in test code: lines 2788, 2830, 2873)
  - No `.expect()` calls found in production code
  - Test module `#[cfg(test)]` starts at line 2559
  - CONFIRMED: Zero production unwraps in entire training.rs file

[2026-01-18] Agent-12: Independently verified `chunked_upload.rs` - NO production unwraps to fix.
  - Grep search found only 2 `.unwrap()` calls at lines 1050-1051 (both in test code)
  - Test module `#[cfg(test)]` starts at line 1027
  - PRD-002 cited line 280: Code at that line uses `.ok_or_else()` with proper error handling, NOT `.unwrap()`
    * Actual code: `workspace_id.as_deref().ok_or_else(|| { warn!(...); anyhow!(...) })?`
  - PRD-002 cited lines 1044-1045: Actually test code at lines 1050-1051
    * `std::fs::create_dir_all(&temp_root).unwrap()` - test setup
    * `tempfile::TempDir::new_in(&temp_root).unwrap()` - test setup
  - Production code error handling patterns already in use:
    * `.context()` for error wrapping (lines 291-293, 533-536, 608-610, etc.)
    * `.ok_or_else()` for Option->Result conversion (lines 280-286, 803, etc.)
    * `?` operator for propagation throughout
    * `.unwrap_or(false)` for safe defaults (line 33)
  - Per Rule #6 "Don't touch test code" - no changes needed
  - CONFIRMED: Zero production unwraps in entire chunked_upload.rs file

[2026-01-18] Agent-1: Independently verified `policy_enforcement.rs` - NO production unwraps to fix.
  - Grep search found only 1 `.unwrap()` call at line 875 (in test code)
  - Grep search found only 1 `.expect()` call at line 874 (in test code)
  - Test module `#[cfg(test)]` starts at line 819
  - Both calls (lines 874-875) are in `test_stable_metadata_json_for_audit_sorts_adapter_ids` test function
  - Production code (lines 1-818) has ZERO `.unwrap()` or `.expect()` calls
  - Production code already uses excellent error handling patterns:
    * Line 101-105: `unwrap_or_else` for request_id fallback (generates new UUID)
    * Line 148-247: All policy validation uses Result with proper error propagation
    * Line 555-580: DB errors fail closed with proper logging and PolicyHookViolationError
    * Line 605-651: Policy validation errors use `map_err()` and explicit error handling
    * Line 665-714: Audit logging handles errors gracefully without blocking
  - `cargo check -p adapteros-server-api` passes successfully
  - Per Rule #6 "Don't touch test code" - no changes needed
  - CONFIRMED: Zero production unwraps in entire policy_enforcement.rs file (1050 lines)

[2026-01-18] Agent-9: Verified `event_applier.rs` lines 1-500 - NO production unwraps to fix.
  - Test module `#[cfg(test)]` starts at line 602
  - All 22 `.unwrap()` calls found at lines 609-858 are in test code
  - Assigned range (lines 1-500) is entirely production code with ZERO `.unwrap()` calls
  - Production code already uses proper error handling patterns:
    * Lines 291-296, 298-304: `.unwrap_or()` for safe defaults (name, version)
    * Line 122: `.unwrap_or_default()` in EventIdentity label (safe fallback)
    * Lines 341-344, 395-398, 431-434, 468-471: All DB operations use `.map_err(|source| EventApplierError::Database {...})?`
    * Lines 368-371, 410-414, 447-451: Serialization uses `.map_err(|err| EventApplierError::Serialization {...})?`
    * Lines 549-558, 576-585: HTTP handler uses `.map_err()` to convert to `(StatusCode, Json<ErrorResponse>)`
  - Per Rule #6 "Don't touch test code" - no changes needed
  - CONFIRMED: PRD-002 cited lines 699, 714, 751, 789 as critical - ALL are in test code (line 602+)

[2026-01-18] Agent-11: Verified `event_applier.rs` lines 801 to end of file (line 861) - NO production unwraps.
  - Test module `#[cfg(test)]` begins at line 602
  - Lines 801-861 are entirely within test module (test functions: test_multiple_events_same_transaction, rejects_missing_adapter_id)
  - Found 7 `.unwrap()` calls in assigned range, ALL in test code:
    * Lines 809, 812: `apply_event_with_clock().await.unwrap()` in test_multiple_events_same_transaction
    * Line 813: `tx.commit().await.unwrap()` in test_multiple_events_same_transaction
    * Line 821: `.fetch_all(&pool).await.unwrap()` in test_multiple_events_same_transaction
    * Line 837: `pool.begin().await.unwrap()` in rejects_missing_adapter_id test
    * Line 853: `tx.rollback().await.unwrap()` in rejects_missing_adapter_id test
    * Line 858: `.fetch_one(&pool).await.unwrap()` in rejects_missing_adapter_id test
  - Using `.unwrap()` in tests is idiomatic Rust - tests should panic on unexpected failures
  - Per Rule #6 "Don't touch test code" - no changes needed
  - CONFIRMED: Zero production `.unwrap()` calls in event_applier.rs (Agent-9 verified lines 1-500, I verified lines 801-861)

### In Progress

**Deeper Crates - Agents 16-20 making fixes** (11 files, +523/-111 lines)

### Crate-Level Fixes Made

[2026-01-18] Agent-16 (adapteros-db): Fixed 5+ unwraps in `lib.rs`
  - RwLock operations now handle poisoned locks gracefully
  - `enable_performance_monitoring()`, `performance_monitor()`, `check_rate_limit()`, etc.

[2026-01-18] Agent-17 (adapteros-lora-worker): Fixed 8+ unwraps in `mlx_subprocess_bridge.rs`
  - Mutex operations now use `map_err()` with proper error propagation
  - Bridge state updates, process management, restart counting

[2026-01-18] Agent-19 (adapteros-policy): Fixed 4+ unwraps in `hash_watcher.rs`
  - Cache lock operations now use `map_err()` for proper error propagation
  - Baseline registration and validation paths

[2026-01-18] Agent-20 (adapteros-lora-mlx-ffi): Fixed macro in `backend.rs`
  - `with_monitor!` macro now handles poisoned locks safely
  - Critical for FFI safety - panics are especially dangerous in FFI

### Issues/Blockers

<!-- COORDINATOR NOTE [2026-01-18 23:55]:
     KEY FINDING: PRD-002 significantly overcounted critical production unwraps.
     - Areas 2, 3: ALL unwraps are in #[cfg(test)] blocks - no fixes needed
     - The codebase already follows good patterns (unwrap_or, unwrap_or_else, map_err)

     AGENTS: Check test vs production code FIRST before making changes.
     Look for `mod tests` or `#[cfg(test)]` to identify boundaries.
     If all unwraps in your file are in test code, mark COMPLETE with details. -->

---

## Rules for All Agents

1. **Read the file first** before making any changes
2. **Use the error patterns above** - don't invent new patterns
3. **Add appropriate logging** with context
4. **Run `cargo check -p <crate>` after changes** to verify compilation
5. **Update this file** when you complete work or hit blockers
6. **Don't touch test code** - focus only on non-test unwraps
7. **Preserve existing behavior** - only change error handling, not logic

## Verification Command

```bash
# After all changes, run:
cargo check --workspace
cargo test -p adapteros-server-api
```
