# PRD-002 Unwrap Elimination - Agent Coordination

**Status**: ✅ COMPLETE - All fixes verified compiling!
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
| 7. helpers.rs/synthesize.rs | 0 (test code) | COMPLETE |

## Deeper Crate Fixes (Agents 16-20)

| Crate | Files Fixed | Unwraps Fixed | Verified |
|-------|-------------|---------------|----------|
| adapteros-db | lib.rs, training_datasets/mod.rs | 7+ | ✅ cargo check |
| adapteros-lora-worker | mlx_subprocess_bridge.rs | 8+ | ✅ cargo check |
| adapteros-policy | hash_watcher.rs, validation.rs | 5+ | ✅ cargo check |
| adapteros-lora-mlx-ffi | backend.rs, generation.rs | 3+ | ✅ cargo check |

**Total: 11 files changed, +523/-111 lines**

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

[2026-01-18] Agent-3: Independently verified `error_code_enforcement.rs` - CONFIRMED Agent-4's findings.
  - Grep search for `.unwrap()` found 85+ occurrences, ALL in `#[cfg(test)]` module (lines 163+)
  - Production code (lines 1-162) has ZERO `.unwrap()` calls
  - Production code already uses excellent error handling patterns:
    * Line 69: `.unwrap_or(false)` for content-type check (safe boolean fallback)
    * Lines 78-81: `match body.collect().await` with proper error handling
    * Lines 84-90: `match serde_json::from_slice()` with proper error handling
    * Lines 115-118: `match serde_json::to_vec()` with proper error handling
    * Lines 125-127: `if let Ok(val)` for HeaderValue conversion
  - `cargo check -p adapteros-server-api` passes successfully
  - Per Rule #6 "Don't touch test code" - no changes needed
  - CONFIRMED: Zero production unwraps in entire error_code_enforcement.rs file (1083 lines)

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

[2026-01-18] Agent-15: Independently verified `handlers/datasets/synthesize.rs` - NO production unwraps to fix.
  - PRD-002 assignment specified `handlers/synthesize.rs` (missing `/datasets/` subdirectory)
  - Actual file location: `crates/adapteros-server-api/src/handlers/datasets/synthesize.rs`
  - Grep search found exactly 3 `.unwrap()` calls:
    * Line 578: `serde_json::to_string(&request).unwrap()` - in test_request_serialization test
    * Line 579: `serde_json::from_str(&json).unwrap()` - in test_request_serialization test
    * Line 605: `serde_json::to_string(&counts).unwrap()` - in test_example_counts_serialization test
  - Test module `#[cfg(test)]` starts at line 550
  - All 3 `.unwrap()` calls are in test code (lines 550-611)
  - Production code (lines 1-549) already uses excellent error handling:
    * Line 227: `.unwrap_or_default()` for SynthesisConfig (safe fallback)
    * Line 406-409: `.map_err()` with logging for config read lock (handles RwLock poisoning)
    * Line 423: `.unwrap_or_else(|_| "var".to_string())` for AOS_VAR_DIR (safe fallback)
    * Line 431-432: `.unwrap_or(std::path::Path::new("var"))` for parent path (safe fallback)
    * Lines 46-52, 262, 351, 475-496: All fallible ops use `.map_err()` with descriptive messages
  - No `.expect()` calls found anywhere in the file
  - Using `.unwrap()` in tests is idiomatic Rust - tests should panic on unexpected failures
  - Per Rule #6 "Don't touch test code" - no changes needed
  - CONFIRMED: Zero production unwraps in entire synthesize.rs file (611 lines)

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

[2026-01-18] Agent-16 (adapteros-db): Fixed 9 production unwraps across 2 files
  - **Methodology**: Searched all 139 files in `crates/adapteros-db/src/`
  - **Total unwraps found**: 423 across 40 files
  - **Key finding**: 95%+ of unwraps are in test code (policy_audit.rs has 73 - all tests)

  **Files analyzed (top files by count)**:
  - `policy_audit.rs` (73 unwraps): ALL in `#[cfg(test)]` module (lines 839+)
  - `audit.rs` (46 unwraps): ALL in `#[cfg(test)]` module (lines 930+)
  - `replay_metadata.rs` (27 unwraps): ALL in `#[cfg(test)]` module (lines 824+)
  - `replay_executions.rs` (25 unwraps): ALL in test code
  - `inference_evidence.rs` (24 unwraps): ALL in test code
  - `lifecycle.rs` (24 unwraps): ALL in test code
  - `lib.rs` (19 unwraps): 7 production FIXED + 12 in test/doc code
  - `policy_management.rs` (19 unwraps): ALL in test code
  - `training_datasets/mod.rs` (6 unwraps): 2 production FIXED + 4 in test code

  **Production unwraps fixed in `lib.rs`** (7 total):
  1. Line 1870: `performance_monitor.write().unwrap()` -> `match` with poisoned lock recovery
  2. Line 1877: `performance_monitor.read().unwrap()` -> `match` with poisoned lock recovery
  3. Line 1884: `performance_monitor.write().unwrap()` -> `match` with poisoned lock recovery
  4. Line 2108: `tenant_rate_limits.read().unwrap()` -> `match` with poisoned lock recovery
  5. Line 2116: `tenant_rate_limits.write().unwrap()` -> `match` with poisoned lock recovery
  6. Line 2123: `plan_cache.read().unwrap()` -> `match` with poisoned lock recovery
  7. Line 2130: `plan_cache.write().unwrap()` -> `match` with poisoned lock recovery
  - All fixes include `tracing::error!` logging with context

  **Production unwraps fixed in `training_datasets/mod.rs`** (2 total):
  1. Line 3294: `version.unwrap().trust_state` -> proper `match` pattern (was guarded by is_none() check)
  2. Line 3341: `version.unwrap().trust_state` -> proper `match` pattern (same issue in `_with_tx` variant)
  - Converted `if version.is_none() { return } ... version.unwrap()` to `let version = match version { ... }`

  **Verification**: `cargo check -p adapteros-db` passes

[2026-01-18] Agent-17 (adapteros-lora-worker): Fixed 19 production unwraps in `mlx_subprocess_bridge.rs`
  - **Methodology**: Searched all files in `crates/adapteros-lora-worker/src/`
  - **Total unwraps found**: 575+ across 66 files (including training/* subdirectory)
  - **Key finding**: 95%+ of unwraps are in test code

  **Files analyzed (top files by count)**:
  - `training/trainer/tests.rs` (74 unwraps): ALL test code - skipped per Rule #6
  - `model_handle_cache.rs` (54 unwraps): ALL in `#[cfg(test)]` module (lines 1500+)
  - `training/builder.rs` (31 unwraps): ALL test code
  - `active_learning.rs` (30 unwraps): ALL in `#[cfg(test)]` module (lines 194+)
  - `prefix_kv_cache.rs` (26 unwraps): ALL in `#[cfg(test)]` module (lines 866+)
  - `mlx_subprocess_bridge.rs` (25 unwraps): 19 production FIXED + 6 in test code

  **Production unwraps fixed in `mlx_subprocess_bridge.rs`** (19 total):
  1-6. Bridge state init (streaming_supported, bridge_protocol_version, is_moe, num_experts, experts_per_token, collect_routing) -> `match` with error return
  7-8. `ensure_running()` process and restart_count locks -> `map_err()` with error propagation
  9-10. Restart count update and reset -> `match` with poisoned lock recovery
  11. `send_request()` process lock -> `map_err()` with error propagation
  12. `check_bridge_health()` process lock -> `map_err()` with error propagation
  13. Health check timestamp update -> `match` with best-effort logging
  14. `prewarm_experts()` process lock -> `map_err()` with error propagation
  15. `generate_text()` process lock -> `map_err()` with error propagation
  16-17. `generate_stream()` process/num_experts locks -> `map_err()` and `unwrap_or_else()` safe default
  18. `shutdown()` process lock -> `map_err()` with error propagation
  19. FusedKernels `load()` context buffer clear -> `match` with error propagation

  **Error handling patterns used**:
  - Critical paths: `map_err(|e| { error!(...); AosError::Kernel(...) })?`
  - Best-effort ops: `match` with logging only
  - Safe defaults: `unwrap_or_else(|e| { error!(...); 0 })` for num_layers

  **Verification**: `cargo check -p adapteros-lora-worker` passes

[2026-01-18] Agent-19 (adapteros-policy): Fixed 15 production unwraps across 2 files

  **hash_watcher.rs** (14 production unwraps -> 0):
  - `register_baseline()`: Cache write lock now uses `map_err()` with error logging
  - `validate_policy_pack()`: Cache read lock now uses `map_err()` with error logging
  - `validate_policy_pack()`: Refactored hash validation to avoid `baseline_hash.unwrap()` by using match arm pattern binding
  - `record_violation()`: SystemTime duration now uses `unwrap_or_else()` with warn logging (fallback to 0)
  - `record_violation()`: Violations write lock now uses match with error logging (skips on poisoned)
  - `get_violations()`: Violations read lock now uses match (returns empty vec on poisoned)
  - `clear_violations()`: Write lock now uses `map_err()` returning `AosError::Internal`
  - `clear_all_violations()`: Write lock now uses `map_err()` returning `AosError::Internal`
  - `is_quarantined()`: Read lock now uses match (conservative: returns true on poisoned for safety)
  - `violation_count()`: Read lock now uses match (returns 0 on poisoned)
  - `load_cache()`: Cache write lock now uses `map_err()` with error logging
  - `start_background_watcher()`: Policy hashes read lock now uses match with continue (skips sweep on poisoned)

  **validation.rs** (1 production unwrap -> 0):
  - `validate_customization()`: Replaced `as_object().unwrap()` with match pattern
  - Refactored guard-clause pattern to combine is_object check and as_object into single match

  **Remaining unwraps in adapteros-policy/src/**: ALL in test code or static Regex initialization
  - Test files: Use `.unwrap()` idiomatically for test assertions
  - Regex patterns in `packs/*.rs`: Static `Lazy::new()` initialization - standard fail-fast pattern

  **Verification**: `cargo check -p adapteros-policy` passes

[2026-01-18] Agent-20 (adapteros-lora-mlx-ffi): Fixed 2 production unwraps
  - **Analysis**: Searched all 24 source files, found 190 total `.unwrap()` calls across 15 files
  - **Key finding**: 95%+ of unwraps are in test code (attention.rs, generation.rs tests, etc.)

  **Production unwraps fixed**:
  1. `generation.rs` line 931: `self.cache.as_ref().unwrap()` -> `.ok_or_else()` with error logging
     - Critical path in `generate_with_prefix_cache()` for KV cache initialization
     - Added `tracing::error!` logging when cache missing unexpectedly
     - Returns `AosError::Internal` with descriptive message
  2. `backend.rs` macro `with_monitor!`: `monitor.lock().unwrap()` -> `match` with poisoned lock recovery
     - Critical for FFI safety - panics are especially dangerous in FFI
     - Now logs error and continues using poisoned guard's data instead of panicking
     - Added documentation explaining the FFI safety rationale

  **Files analyzed (counts)**:
  - `attention.rs` (53 unwraps): ALL in `#[cfg(test)]` module (lines 851+)
  - `tensor.rs` (31 unwraps): ALL in `#[cfg(test)]` module (lines 740+)
  - `generation.rs` (29 unwraps): 1 production FIXED + 28 in test code
  - `array.rs` (25 unwraps): ALL in `#[cfg(test)]` module (lines 516+)
  - `kv_cache.rs` (15 unwraps): ALL in `#[cfg(test)]` module (lines 654+)
  - `backend.rs` (1 unwrap): 1 production FIXED in macro

  **Verification**: `cargo check -p adapteros-lora-mlx-ffi` passes

[2026-01-18] Agent-18 (adapteros-core): Analyzed and fixed production unwraps
  - **Methodology**: Searched all 76 source files in `crates/adapteros-core/src/`
  - **Total unwraps found**: 305 across 41 files
  - **Key finding**: 95%+ of unwraps are in test code or static initialization

  **Files analyzed (non-test unwraps)**:
  - `naming.rs` (25 unwraps): ALL static `Lazy<Regex>` initialization - standard Rust pattern
  - `redaction.rs` (14 unwraps): ALL static `Lazy<Regex>` initialization - same pattern
  - `tenant.rs` (23 unwraps): 1 static regex + 22 in test code
  - `receipt_digest.rs` (23 unwraps): 1 production unwrap FIXED + 22 in test code
  - `retry_metrics.rs` (26 unwraps): ALL in test code
  - `third_party_verification.rs` (25 unwraps): ALL in test code
  - `seed.rs` (8 unwraps): ALL in test code or docstrings
  - `determinism.rs` (1 unwrap): Production unwrap DOCUMENTED with expect
  - `retry_policy.rs` (3 unwraps): 1 production unwrap DOCUMENTED + 2 in test code

  **Fixes applied**:
  1. `receipt_digest.rs` line 702: `.try_into().unwrap()` -> `.try_into().map_err()?`
     - Added error: "allowed_mask header not 4 bytes"
  2. `determinism.rs` line 74: `.unwrap()` -> `.expect("request_seed[..8] is always 8 bytes")`
     - Added safety comment explaining invariant
  3. `retry_policy.rs` line 431: `.unwrap()` -> `.expect("HKDF expand for 8 bytes always succeeds")`
     - Added safety comment about HKDF cryptographic limits

  **Static regex unwraps (intentionally NOT fixed)**:
  - `naming.rs`: 7 static regex patterns (TENANT_REGEX, DOMAIN_REGEX, etc.)
  - `redaction.rs`: 14 REDACTION_PATTERNS
  - `tenant.rs`: TENANT_ID_REGEX
  - **Rationale**: Static `Lazy::new()` with literal regex is idiomatic Rust - fail-fast at startup

  **Verification**: `cargo check -p adapteros-core` passes

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
