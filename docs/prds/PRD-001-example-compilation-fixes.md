# PRD-001: Example Compilation Fixes

**Status**: Draft
**Priority**: P0 (Blocking)
**Estimated Effort**: 1 hour
**Owner**: TBD

---

## 1. Problem Statement

The example files in `examples/` fail to compile due to API signature drift. The `ExampleMetadataV1::new()` constructor signature was updated to require 5 arguments (adding `provenance`), but examples still pass 4 arguments.

This blocks:
- New developer onboarding (examples don't run)
- CI validation of examples (if enabled)
- Documentation accuracy (examples demonstrate incorrect API usage)

---

## 2. Scope

### In Scope
- Fix `examples/create_readme_adapter.rs`
- Fix `examples/train_simple_adapter.rs`
- Verify no other examples have similar issues

### Out of Scope
- Refactoring ExampleMetadataV1 API
- Adding new examples
- Documentation updates beyond code comments

---

## 3. Technical Analysis

### 3.1 Root Cause

The `ExampleMetadataV1::new()` constructor in `crates/adapteros-types/src/training/example.rs` requires 5 parameters:

```rust
pub fn new(
    dataset_id: impl Into<String>,      // Arg 1
    row_id: u64,                         // Arg 2
    source_hash: impl Into<String>,      // Arg 3
    provenance: impl Into<String>,       // Arg 4 (MISSING)
    created_at_unix_ms: u64,             // Arg 5
) -> Self
```

Examples provide only 4 arguments, omitting `provenance`.

### 3.2 Affected Files

| File | Line(s) | Current Call | Issue |
|------|---------|--------------|-------|
| `examples/create_readme_adapter.rs` | 136 | `ExampleMetadataV1::new("README.md", idx as u64 + 1, "readme", 0)` | Missing provenance, wrong type on arg 4 |
| `examples/train_simple_adapter.rs` | 28, 34, 40, 46 | `ExampleMetadataV1::new("demo", N, "demo", created_at)` | Missing provenance |

### 3.3 Correct Pattern

Reference implementation in `crates/adapteros-lora-worker/tests/create_test_adapters.rs:14`:

```rust
let metadata = ExampleMetadataV1::new("test", row_id, "row-hash", "{}", 0);
```

The `provenance` field accepts canonical JSON. For synthetic/test examples, use `"{}"` (empty JSON object).

---

## 4. Implementation Plan

### Phase 1: Fix create_readme_adapter.rs

**File**: `examples/create_readme_adapter.rs`

**Line 136 - Before**:
```rust
ExampleMetadataV1::new("README.md", idx as u64 + 1, "readme", 0),
```

**Line 136 - After**:
```rust
ExampleMetadataV1::new("README.md", idx as u64 + 1, "readme", "{}", 0),
```

### Phase 2: Fix train_simple_adapter.rs

**File**: `examples/train_simple_adapter.rs`

**Lines 28, 34, 40, 46 - Before**:
```rust
ExampleMetadataV1::new("demo", 1, "demo", created_at),
ExampleMetadataV1::new("demo", 2, "demo", created_at),
ExampleMetadataV1::new("demo", 3, "demo", created_at),
ExampleMetadataV1::new("demo", 4, "demo", created_at),
```

**Lines 28, 34, 40, 46 - After**:
```rust
ExampleMetadataV1::new("demo", 1, "demo", "{}", created_at),
ExampleMetadataV1::new("demo", 2, "demo", "{}", created_at),
ExampleMetadataV1::new("demo", 3, "demo", "{}", created_at),
ExampleMetadataV1::new("demo", 4, "demo", "{}", created_at),
```

### Phase 3: Verification

```bash
# Compile all examples
cargo build --examples

# Run each example with --help to verify execution
cargo run --example create_readme_adapter -- --help
cargo run --example train_simple_adapter -- --help
```

---

## 5. Acceptance Criteria

- [ ] `cargo build --examples` completes without errors
- [ ] `cargo run --example create_readme_adapter -- --help` executes successfully
- [ ] `cargo run --example train_simple_adapter -- --help` executes successfully
- [ ] No regressions in `cargo test --workspace`

---

## 6. Testing Strategy

| Test Type | Command | Expected Outcome |
|-----------|---------|------------------|
| Compilation | `cargo build --examples` | Exit 0, no errors |
| Smoke Test | `cargo run --example create_readme_adapter -- --help` | Prints usage |
| Smoke Test | `cargo run --example train_simple_adapter -- --help` | Prints usage |
| Regression | `cargo test --workspace` | All tests pass |

---

## 7. Rollback Plan

If issues arise, revert the single commit containing these changes. No database migrations or configuration changes are involved.

---

## 8. Dependencies

None. This is a self-contained fix with no external dependencies.

---

## 9. Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Incorrect provenance format | Low | Low | Use `"{}"` which is the canonical empty JSON pattern used elsewhere |
| Other examples affected | Low | Low | Phase 3 verification catches any missed files |

---

## 10. Success Metrics

- Examples compile: **Required**
- CI pipeline (if examples are tested): **Green**
- Developer onboarding friction: **Reduced**
