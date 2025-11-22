# Fixing Integration Test Compilation

**Current Status:** 58 compilation errors in `adapteros-server-api` library
**Blocking:** Integration tests cannot compile until library compiles
**Documentation tests:** Already working (no library dependency)

## Error Analysis

### Error 1: Missing `schema_version` Field (10 occurrences)

**Symptom:**
```
error[E0063]: missing field `schema_version` in initializer of `ErrorResponse`
  --> crates/adapteros-server-api/src/errors.rs:180
    |
180 |         adapteros_api_types::ErrorResponse {
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ missing `schema_version`
```

**Root Cause:** `ErrorResponse` struct definition in `adapteros-api-types` now requires a `schema_version` field, but all initializers in `errors.rs` don't provide it.

**Affected Files:**
```
crates/adapteros-server-api/src/errors.rs
  - Lines: 180, 188, 189, 195, 196, 197, 211, 213, 220, 221
  - Total: 10 occurrences
```

**Fix Instructions:**

Step 1: Check the current `ErrorResponse` structure:
```bash
grep -A 10 "pub struct ErrorResponse" crates/adapteros-api-types/src/lib.rs
```

Expected output should show `schema_version` field (likely `String` type).

Step 2: Open `errors.rs` and find all `ErrorResponse` initializations:
```bash
grep -n "ErrorResponse {" crates/adapteros-server-api/src/errors.rs
```

Step 3: For each occurrence, add the `schema_version` field. Example:

**Before:**
```rust
adapteros_api_types::ErrorResponse {
    error: error_msg,
    code: error_code.to_string(),
    details: details_map,
}
```

**After:**
```rust
adapteros_api_types::ErrorResponse {
    error: error_msg,
    code: error_code.to_string(),
    details: details_map,
    schema_version: "1.0".to_string(),  // Add this line
}
```

Step 4: Compile and verify:
```bash
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0063\]"
# Should now show 0 occurrences of this error
```

### Error 2: Unresolved Import `OperationProgressEvent`

**Symptom:**
```
error[E0432]: unresolved import `crate::types::OperationProgressEvent`
  --> crates/adapteros-server-api/src/operation_tracker.rs:11
    |
11 | use crate::types::OperationProgressEvent;
    |     ^^^^^^^^^^^ could not find `OperationProgressEvent` in this crate
```

**Root Cause:** Type is being imported but either:
1. Not defined in `types.rs`, or
2. Not exported in `mod.rs`

**Fix Instructions:**

Step 1: Check if type exists:
```bash
grep -r "OperationProgressEvent" crates/adapteros-server-api/src/
```

Step 2a: If type doesn't exist, define it in `types.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationProgressEvent {
    pub operation_id: String,
    pub progress_pct: f32,
    pub status: String,
    pub timestamp: String,
}
```

Step 2b: If type exists, check it's exported in `types/mod.rs`:
```bash
grep "pub use\|pub struct\|pub enum" crates/adapteros-server-api/src/types.rs | grep OperationProgressEvent
```

Step 3: If type exists but not exported, add to `types/mod.rs`:
```rust
pub mod operation_progress {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct OperationProgressEvent {
        pub operation_id: String,
        pub progress_pct: f32,
        pub status: String,
        pub timestamp: String,
    }
}

pub use operation_progress::OperationProgressEvent;
```

Step 4: Verify the import now works:
```bash
cargo check -p adapteros-server-api 2>&1 | grep "OperationProgressEvent"
```

### Error 3: Failed Type Resolution in Handler Modules

**Symptom:**
```
error[E0432]: failed to resolve: use of unresolved module or crate
error[E0433]: failed to resolve: use of unresolved module
error[E0412]: cannot find type in scope
```

**Root Cause:** Handler modules not properly exported or types not in scope

**Fix Instructions:**

Step 1: Find unresolved types:
```bash
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0432\]\|error\[E0412\]" | head -20
```

Step 2: For each error, check if handler module is exported:
```bash
# Example: If error mentions "handlers::streaming"
grep "pub mod streaming" crates/adapteros-server-api/src/handlers/mod.rs
# If not found, add it
```

Step 3: Update `handlers/mod.rs` to export all modules:
```rust
pub mod adapters;
pub mod auth_enhanced;
pub mod auth_logout;
pub mod auth_me;
pub mod batch;
pub mod dashboard;
pub mod datasets;
pub mod health;
pub mod infer;
pub mod meta;
pub mod models;
pub mod notifications;
pub mod ready;
pub mod replay;
pub mod streaming_infer;
pub mod telemetry;
pub mod training;
pub mod workspaces;
// ... etc - add any missing modules
```

Step 4: For each unresolved type, check its definition:
```bash
# Example: if error is about "SomeType"
grep -r "pub struct SomeType\|pub enum SomeType" crates/adapteros-server-api/src/
```

Step 5: Add missing type exports to `types.rs` if needed:
```rust
pub mod common_types {
    pub use crate::handlers::some_module::SomeType;
}
pub use common_types::SomeType;
```

### Error 4: Trait Bound and Type Mismatch Errors

**Symptom:**
```
error[E0277]: the trait bound `X: Y` is not satisfied
error[E0308]: mismatched types
```

**Root Cause:** Usually indicates missing trait implementations or wrong return types

**Fix Instructions:**

Step 1: Get the full error message:
```bash
cargo check -p adapteros-server-api 2>&1 | grep -A 5 "error\[E0277\]\|error\[E0308\]" | head -30
```

Step 2: Read the error carefully. It usually shows:
- What trait is missing
- What type doesn't implement it
- Where to add the implementation

Example error:
```
error[E0277]: `X` doesn't implement `Display`
  --> file.rs:10
   |
10 |     println!("{}", value_of_type_x);
   |                    ^^^^^^^^^^^^^^^ `X` cannot be formatted
```

Fix: Add `Display` implementation or use `Debug` instead:
```rust
// Either add implementation
impl Display for X {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "...")
    }
}

// Or use Debug
println!("{:?}", value_of_type_x);
```

## Systematic Fix Approach

### Phase 1: Fix Struct Field Errors (20 minutes)
```bash
# 1. Check all ErrorResponse errors
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0063\]" | wc -l
# Should show ~10 errors

# 2. Add schema_version to all ErrorResponse initializers
# (Use instructions above)

# 3. Verify fixed
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0063\]"
# Should show 0 errors
```

### Phase 2: Fix Import/Module Errors (30 minutes)
```bash
# 1. Find all unresolved imports
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0432\]"

# 2. For each error, determine if:
#    a) Type needs to be defined
#    b) Type needs to be exported
#    c) Module needs to be exported

# 3. Make fixes and recheck
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0432\]"
```

### Phase 3: Fix Type Mismatches (30 minutes)
```bash
# 1. Find all trait bound errors
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0277\]"

# 2. For each error, determine:
#    a) What trait is missing
#    b) Whether to implement it or change usage

# 3. Make fixes and recheck
cargo check -p adapteros-server-api 2>&1 | grep "error\[E0277\]"
```

### Phase 4: Verify Complete Compilation
```bash
# Full check
cargo check -p adapteros-server-api

# Run tests compilation
cargo test -p adapteros-server-api --no-run

# Count remaining errors
cargo check -p adapteros-server-api 2>&1 | grep "^error" | wc -l
# Should show 0
```

## Verification Checklist

After fixing, verify:
- [ ] `cargo check -p adapteros-server-api` completes with 0 errors
- [ ] `cargo test -p adapteros-server-api --no-run` succeeds
- [ ] `cargo test -p adapteros-server-api --test api_consistency_tests` passes
- [ ] `cargo test -p adapteros-server-api --test database_validation_tests` passes
- [ ] `cargo test -p adapteros-server-api --test security_validation_tests` passes
- [ ] `cargo test -p adapteros-server-api --test type_validation_tests` passes

## Common Fixes Quick Reference

### Fix: Missing `schema_version`
```bash
# Find all instances
grep -n "ErrorResponse {" crates/adapteros-server-api/src/errors.rs

# Add schema_version: "1.0".to_string(), to each struct initializer
```

### Fix: Unresolved Import
```bash
# Check if type exists
grep -r "pub struct TypeName" crates/adapteros-server-api/src/

# If found, check if exported
grep "pub use.*TypeName" crates/adapteros-server-api/src/mod.rs

# If not exported, add export or update import path
```

### Fix: Unresolved Module
```bash
# Check if handler module exists
ls -la crates/adapteros-server-api/src/handlers/module_name.rs

# If exists, add to handlers/mod.rs:
# pub mod module_name;

# If doesn't exist, create it or fix import path
```

## Testing After Fixes

### Compile Tests Only (No Execution)
```bash
cargo test -p adapteros-server-api --no-run
```

### Run Documentation Tests
```bash
cargo test -p adapteros-server-api --test api_consistency_tests
cargo test -p adapteros-server-api --test database_validation_tests
cargo test -p adapteros-server-api --test security_validation_tests
cargo test -p adapteros-server-api --test type_validation_tests
```

### Run Integration Tests (After Fixing Ignores)
```bash
cargo test -p adapteros-server-api --test auth_middleware_test
cargo test -p adapteros-server-api --test telemetry
```

## Help Resources

If stuck on a specific error:

1. **Get full error context:**
   ```bash
   cargo check -p adapteros-server-api 2>&1 | grep -A 10 "error\[EXXX\]"
   ```

2. **Check Rust error explanations:**
   ```bash
   rustc --explain E0432  # Example for E0432
   ```

3. **Look for similar patterns:**
   ```bash
   grep -r "ErrorResponse {" crates/ | head -5
   ```

4. **Check related files for working examples:**
   ```bash
   # If fixing a handler module error:
   grep -l "pub mod" crates/adapteros-server-api/src/handlers/*.rs
   ```

## Expected Timeline

| Phase | Task | Time | Status |
|-------|------|------|--------|
| 1 | Fix ErrorResponse fields | 20 min | Ready to implement |
| 2 | Fix imports/modules | 30 min | Ready to implement |
| 3 | Fix type mismatches | 30 min | Ready to implement |
| 4 | Verify compilation | 10 min | Ready to test |
| **Total** | **All fixes** | **90 min** | **On track** |

After fixes complete:
- Documentation tests: 5 min to run
- Integration tests setup: 10 min to update ignores
- Full test suite: 15 min to run (once ignores updated)
