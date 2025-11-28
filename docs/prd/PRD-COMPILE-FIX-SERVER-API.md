# PRD: Server-API Compilation Fix

**Status:** Ready for Implementation
**Priority:** P0 (Blocking)
**Estimated Scope:** ~10 errors across 5 files

---

## Context

After updating `TrainingJob` type with new provenance fields (`base_model_id`, `collection_id`, `build_id`, `source_documents_json`, `config_hash_b3`) and updating `start_training()` signature to accept 10 arguments (was 8), the `adapteros-server-api` crate has compilation errors that need resolution.

---

## Error Categories

### Category A: Function Signature Mismatch (E0061)
**Count:** 2 errors
**Root Cause:** `start_training()` now takes 10 arguments, callers still pass 8

| File | Line | Fix |
|------|------|-----|
| `handlers/git_repository.rs` | 521 | Add `None, None` for `base_model_id`, `collection_id` |
| `handlers.rs` | 8267 | Add `None, None` for `base_model_id`, `collection_id` |

**Pattern:**
```rust
// Before
.start_training(adapter_name, config, template_id, repo_id, dataset_id, tenant_id, user, role)

// After
.start_training(adapter_name, config, template_id, repo_id, dataset_id, tenant_id, user, role, None, None)
```

---

### Category B: Missing Fields in Struct Initializer (E0063)
**Count:** 1 error
**Root Cause:** `TrainingJobResponse` now has additional fields that must be initialized

| File | Line | Fix |
|------|------|-----|
| `types.rs` | 1632 | Add missing fields with `None` defaults |

**Pattern:**
```rust
TrainingJobResponse {
    // ... existing fields ...
    base_model_id: None,
    collection_id: None,
    build_id: None,
    config_hash_b3: None,
    adapter_id: None,
    weights_hash_b3: None,
}
```

---

### Category C: Type Mismatch on Option Fields (E0308)
**Count:** 2 errors
**Root Cause:** Expecting `Option<String>` but field is `String`

| File | Line | Field | Fix |
|------|------|-------|-----|
| `handlers/adapters.rs` | 2369 | `snapshot.documents_json` | Check actual type, adjust pattern |
| `handlers/adapters.rs` | 2389 | `snapshot.chunking_config_json` | Check actual type, adjust pattern |

**Investigation Required:** Read the `AdapterTrainingSnapshot` struct to determine if fields are `String` or `Option<String>`. Fix the pattern match accordingly:
```rust
// If field is String (not Option):
let docs_json = &snapshot.documents_json;

// If field is Option<String>:
if let Some(ref docs_json) = snapshot.documents_json { ... }
```

---

### Category D: Method Not Found on Non-Option Types (E0599)
**Count:** 3 errors
**Root Cause:** Calling `.unwrap_or()` / `.unwrap_or_else()` on non-Option types

| File | Line | Expression | Fix |
|------|------|------------|-----|
| `handlers/adapters.rs` | 2400 | `adapter.version.unwrap_or_else(...)` | Use value directly or check if Option |
| `handlers/adapters.rs` | 2402 | `adapter.rank.unwrap_or(16)` | Use `adapter.rank` directly (i32) |
| `handlers/adapters.rs` | 2403 | `adapter.alpha.unwrap_or(32.0)` | Use `adapter.alpha` directly (f64) |

**Pattern:**
```rust
// Before (wrong - field is not Option)
rank: adapter.rank.unwrap_or(16),

// After (correct - use directly)
rank: adapter.rank,
```

---

### Category E: Field Does Not Exist (E0609)
**Count:** 1 error
**Root Cause:** `Adapter` struct doesn't have `base_model_id` field

| File | Line | Expression | Fix |
|------|------|------------|-----|
| `handlers/adapters.rs` | 2401 | `adapter.base_model_id.clone()` | Use `None` or add field to Adapter |

**Options:**
1. **Quick fix:** Use `None` if provenance export doesn't need adapter's base_model
2. **Proper fix:** Add `base_model_id` to Adapter struct and migration (if needed)

---

### Category F: Partial Move Error (E0382)
**Count:** 1 error
**Root Cause:** Field moved out of struct, then struct borrowed again

| File | Line | Fix |
|------|------|-----|
| `handlers/system_state.rs` | 305 | Clone the field instead of moving, or restructure |

**Pattern:**
```rust
// Before (moves a.name, then borrows a)
name: a.name,
other: a.other_field, // ERROR: a partially moved

// After (clone to avoid move)
name: a.name.clone(),
other: a.other_field,
```

---

## Implementation Checklist

For each file, the agent should:

1. **Read the file** at the error line with surrounding context
2. **Understand the types** involved (read struct definitions if needed)
3. **Apply the minimal fix** per the patterns above
4. **Verify** with `cargo check -p adapteros-server-api`

### Files to Fix (in order)

| # | File | Errors | Priority |
|---|------|--------|----------|
| 1 | `handlers/adapters.rs` | 5 | High - export handler |
| 2 | `handlers/git_repository.rs` | 1 | High - start_training call |
| 3 | `handlers.rs` | 1 | High - start_training call |
| 4 | `types.rs` | 1 | Medium - response builder |
| 5 | `handlers/system_state.rs` | 1 | Low - unrelated to provenance |

---

## Success Criteria

- [ ] `cargo check -p adapteros-server-api` completes with 0 errors
- [ ] No new warnings introduced
- [ ] All existing tests pass (if any)

---

## Agent Instructions

Execute fixes in parallel where possible:
1. Fix Category A errors (2 files, independent)
2. Fix Category B error (types.rs)
3. Fix Categories C, D, E in handlers/adapters.rs (5 errors, same file)
4. Fix Category F in handlers/system_state.rs

After each file fix, run incremental check:
```bash
cargo check -p adapteros-server-api 2>&1 | grep "^error" | wc -l
```

Target: 0 errors.
