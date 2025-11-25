# De-Slop Remediation Plan

**Purpose:** Comprehensive plan to remove AI-generated code slop from AdapterOS codebase

**Last Updated:** 2025-01-27

**Status:** Planning Phase

---

## Executive Summary

**Total Violations Identified:**
- **High Priority:** 68 instances (48 generic errors + 20 platform patterns)
- **Medium Priority:** 9,151+ instances (mostly false positives from generic variable names)
- **Architectural Violations:** ~35 direct DB access patterns bypassing lifecycle manager
- **Non-Deterministic Patterns:** 14 instances of `tokio::spawn`/`thread::spawn`
- **Logging Violations:** 25 instances of `println!`/`eprintln!`
- **Missing Citations:** Unknown (41 found, many more likely missing)

**Estimated Total Effort:** 120-160 hours (3-4 weeks for 1 engineer, or 1-2 weeks for 2-3 engineers)

**Risk Level:** Medium-High (architectural violations can cause production issues)

---

## Phase 1: Critical Architectural Violations (Week 1)

### 1.1 Lifecycle Manager Bypass Fixes

**Priority:** P0 (Critical - Can cause state inconsistencies)

**Violations Found:** ~35 instances in `crates/adapteros-server-api/src/handlers.rs`

**Pattern:**
```rust
// ❌ WRONG: Direct DB access before lifecycle manager
state.db.update_adapter_state(&adapter_id, "loading", "reason").await?;
if let Some(ref lifecycle) = state.lifecycle_manager {
    // Lifecycle manager used after state already updated
}

// ✅ CORRECT: Lifecycle manager first, then DB update
if let Some(ref lifecycle) = state.lifecycle_manager {
    let mut manager = lifecycle.lock().await;
    manager.record_router_decision(&adapter_id).await?; // Promotes state
    // DB update happens inside lifecycle manager
}
```

**Files to Fix:**
1. `crates/adapteros-server-api/src/handlers.rs` (lines 434-442, 4784-4803, 5012-5041)
2. `crates/adapteros-server-api/src/handlers/domain_adapters.rs` (lines 331-362)
3. `crates/adapteros-server-api/src/handlers/adapters.rs` (lines 85-140)

**Fix Strategy:**
1. Create helper function `load_adapter_via_lifecycle()` that:
   - Checks lifecycle manager availability
   - Calls `LifecycleManager::record_router_decision()` first
   - Handles state promotion automatically
   - Updates DB through lifecycle manager's DB integration
2. Replace all direct `update_adapter_state()` calls with lifecycle manager methods
3. Ensure lifecycle manager is always initialized (remove `Option` wrapper if possible)

**Effort:** 16-20 hours
- 8-10 hours: Create helper functions and refactor lifecycle manager integration
- 4-6 hours: Update all handler functions
- 2-3 hours: Add tests for lifecycle manager integration
- 2 hours: Code review and verification

**Dependencies:**
- Lifecycle manager must have DB integration (already exists)
- Need to verify `LifecycleManager::record_router_decision()` API

**Testing:**
- Unit tests for helper functions
- Integration tests for adapter loading/unloading
- State machine consistency tests

---

### 1.2 Direct SQL Query Replacements

**Priority:** P0 (Critical - Bypasses abstraction layer)

**Violations Found:** ~5 instances in handlers

**Pattern:**
```rust
// ❌ WRONG: Direct SQL bypassing Db trait
sqlx::query("UPDATE adapters SET tier = ? WHERE adapter_id = ?")
    .bind(&new_tier)
    .bind(&adapter_id)
    .execute(state.db.pool())
    .await?;

// ✅ CORRECT: Use Db trait method
state.db.update_adapter_tier(&adapter_id, &new_tier).await?;
```

**Files to Fix:**
1. `crates/adapteros-server-api/src/handlers.rs` (lines 5319-5336)
2. `crates/adapteros-server-api/src/handlers.rs` (lines 8165-8179)

**Fix Strategy:**
1. Add missing methods to `Db` trait:
   - `update_adapter_tier(&self, adapter_id: &str, tier: &str) -> Result<()>`
   - `update_anomaly_status(&self, anomaly_id: &str, status: &str, notes: &str, investigator: &str) -> Result<()>`
2. Implement methods in `adapteros-db` crate
3. Replace direct SQL queries with trait method calls

**Effort:** 8-10 hours
- 2-3 hours: Add methods to Db trait
- 2-3 hours: Implement in adapteros-db
- 2-3 hours: Replace direct queries
- 1-2 hours: Tests

**Dependencies:**
- `adapteros-db` crate must support these operations
- May need migration if schema changes

---

### 1.3 Non-Deterministic Spawn Pattern Fixes

**Priority:** P1 (High - Violates determinism policy)

**Violations Found:** 14 instances

**Pattern:**
```rust
// ❌ WRONG: Non-deterministic spawn
tokio::spawn(async move {
    // Training logic
});

// ✅ CORRECT: Deterministic spawn
use adapteros_deterministic_exec::spawn_deterministic;
spawn_deterministic("training-task".to_string(), async move {
    // Training logic
})?;
```

**Files to Fix:**
1. `crates/adapteros-orchestrator/src/training.rs` (lines 183, 440)
2. `crates/adapteros-server-api/src/handlers/datasets.rs` (line 1698)
3. `crates/adapteros-lora-worker/src/lib.rs` (line 467)
4. Test files (may be acceptable)

**Fix Strategy:**
1. Identify which spawns need determinism:
   - Training operations: YES (deterministic)
   - Background tasks: MAYBE (depends on context)
   - Test code: NO (acceptable)
2. Replace with `spawn_deterministic!` macro
3. Ensure deterministic executor is initialized

**Effort:** 6-8 hours
- 2-3 hours: Identify which spawns need determinism
- 2-3 hours: Replace spawns
- 1-2 hours: Verify deterministic executor initialization
- 1 hour: Tests

**Dependencies:**
- Deterministic executor must be initialized in affected crates
- May need to pass executor context through function parameters

---

## Phase 2: Error Handling Standardization (Week 2)

### 2.1 Generic Error Type Replacements

**Priority:** P1 (High - Inconsistent error handling)

**Violations Found:** 48 instances

**Pattern:**
```rust
// ❌ WRONG: Generic error type
async fn test_function() -> Result<(), Box<dyn std::error::Error>> {
    // ...
}

// ✅ CORRECT: Domain-specific error
async fn test_function() -> Result<(), adapteros_core::AosError> {
    // ...
}
```

**Files to Fix:**
1. Test files: `crates/adapteros-secd/tests/secure_enclave_integration.rs` (4 instances)
2. Example files: `crates/adapteros-lora-kernel-mtl/examples/coreml_inference.rs`
3. FFI code: `crates/adapteros-lora-mlx-ffi/src/lora.rs` (2 instances)
4. CLI code: `crates/adapteros-cli/src/cli_telemetry.rs`, `app.rs` (uses `anyhow::Error` for CLI - may be acceptable)

**Fix Strategy:**
1. **Test code:** Replace with `AosError` or use `#[allow]` if test framework requires generic errors
2. **Example code:** Replace with `AosError` for consistency
3. **FFI code:** Replace with `AosError` or create FFI-specific error type
4. **CLI code:** Evaluate if `anyhow::Error` is acceptable (CLI may need generic errors for user-facing messages)

**Effort:** 12-16 hours
- 4-6 hours: Fix test files
- 2-3 hours: Fix example files
- 3-4 hours: Fix FFI code
- 2-3 hours: Evaluate and fix CLI code
- 1 hour: Tests

**Dependencies:**
- May need to create conversion traits for FFI
- CLI error handling may need special consideration

---

### 2.2 Random Number Generation Fixes

**Priority:** P1 (High - Violates determinism policy)

**Violations Found:** 15 instances in `crates/adapteros-crypto/src/providers/kms.rs`

**Pattern:**
```rust
// ❌ WRONG: Unseeded random number generation
rand::thread_rng().fill_bytes(&mut private);

// ✅ CORRECT: HKDF-seeded randomness
use adapteros_core::{derive_seed, B3Hash};
let seed = derive_seed(&base_hash, "kms-key-generation");
let mut rng = adapteros_crypto::seeded_rng(&seed);
rng.fill_bytes(&mut private);
```

**Files to Fix:**
1. `crates/adapteros-crypto/src/providers/kms.rs` (13 instances)

**Fix Strategy:**
1. Create `seeded_rng()` helper function in `adapteros-crypto`
2. Replace all `rand::thread_rng()` calls with seeded RNG
3. Ensure seed derivation uses HKDF with proper domain separation

**Effort:** 8-10 hours
- 2-3 hours: Create seeded RNG helper
- 3-4 hours: Replace all instances
- 2-3 hours: Verify seed derivation and tests

**Dependencies:**
- HKDF implementation must be available
- Need to determine seed source for KMS operations

---

## Phase 3: Logging Standardization (Week 2-3)

### 3.1 println! Replacement

**Priority:** P2 (Medium - Violates logging policy)

**Violations Found:** 25 instances

**Pattern:**
```rust
// ❌ WRONG: println! in production code
println!("Adapter loaded: {}", adapter_id);

// ✅ CORRECT: Structured logging
tracing::info!(adapter_id = %adapter_id, "Adapter loaded");
```

**Files to Fix:**
1. **CLI output (acceptable):**
   - `crates/adapteros-cli/src/commands/infer.rs:70` - User output
   - `crates/adapteros-cli/src/main.rs:1177` - Error messages to stderr
2. **Debug statements (should remove or use tracing::debug!):**
   - `crates/adapteros-lora-router/src/lib.rs:1325` - Debug output
3. **Deprecation warnings (acceptable):**
   - `crates/adapteros-cli/src/main.rs:1510+` - Deprecation warnings

**Fix Strategy:**
1. **CLI output:** Keep `println!`/`eprintln!` for user-facing output (acceptable)
2. **Debug statements:** Replace with `tracing::debug!` or remove
3. **Deprecation warnings:** Keep `eprintln!` for CLI warnings (acceptable)

**Effort:** 4-6 hours
- 1-2 hours: Identify which are violations vs. acceptable
- 2-3 hours: Replace debug statements
- 1 hour: Tests

**Dependencies:**
- None

---

## Phase 4: Code Quality Improvements (Week 3)

### 4.1 Citation Addition

**Priority:** P2 (Medium - Documentation/compliance)

**Violations Found:** Unknown (41 citations found, many more likely missing)

**Pattern:**
```rust
// ❌ MISSING: Citation for extracted code
pub async fn check_alert_exists(...) -> Result<bool> {
    // Extracted from handlers.rs
}

// ✅ CORRECT: Citation included
/// 【2025-01-27†refactor(server)†extract-alert-dedup】
pub async fn check_alert_exists(...) -> Result<bool> {
    // Extracted from handlers.rs
}
```

**Fix Strategy:**
1. Identify all extracted functions/services
2. Add citations using format: `【YYYY-MM-DD†category†identifier】`
3. Create citation registry if needed

**Effort:** 8-12 hours
- 4-6 hours: Identify missing citations
- 2-4 hours: Add citations
- 2 hours: Verify format

**Dependencies:**
- Need to understand extraction history
- May need to consult git history

---

### 4.2 Generic Variable Name Improvements

**Priority:** P3 (Low - Many false positives)

**Violations Found:** 9,151 instances (mostly false positives)

**Fix Strategy:**
1. Focus on high-impact areas only:
   - Handler functions
   - Service modules
   - Public APIs
2. Ignore test code and internal helpers
3. Use clippy lints to catch new violations

**Effort:** 16-24 hours (optional, low priority)
- Focus on critical paths only
- Use automated refactoring tools where possible

---

## Phase 5: Prevention Mechanisms (Week 3-4)

### 5.1 Enhanced Detection Tools

**Priority:** P1 (High - Prevents future slop)

**Current Tools:**
- `ai_slop_detector.sh` - Pattern-based detection
- `jscpd` - Duplication detection
- `cargo clippy` - Rust linting

**Improvements Needed:**
1. **AST-based architectural checks:**
   - Detect lifecycle manager bypass
   - Detect direct SQL queries
   - Detect non-deterministic spawns in deterministic contexts

2. **Pre-commit hooks:**
   - Citation format validation
   - Architectural pattern checks
   - Error type validation

3. **CI integration:**
   - Make duplication checks blocking
   - Add architectural violation detection
   - Citation validation

**Effort:** 20-30 hours
- 8-10 hours: Create AST-based linter rules
- 4-6 hours: Pre-commit hooks
- 4-6 hours: CI integration
- 4-8 hours: Documentation and training

**Dependencies:**
- Need Rust AST parsing (syn crate)
- Need to define architectural rules clearly

---

### 5.2 Documentation and Training

**Priority:** P2 (Medium - Prevents future slop)

**Content Needed:**
1. **Architectural patterns guide:**
   - When to use lifecycle manager
   - When to use deterministic execution
   - Error handling patterns
   - Logging patterns

2. **Code review checklist:**
   - Lifecycle manager usage
   - Deterministic execution
   - Error handling
   - Citations

3. **Examples:**
   - Correct patterns
   - Common mistakes
   - How to fix violations

**Effort:** 8-12 hours
- 4-6 hours: Write documentation
- 2-3 hours: Create examples
- 2-3 hours: Review and refine

---

## Implementation Timeline

### Week 1: Critical Fixes
- **Days 1-2:** Lifecycle manager bypass fixes
- **Days 3-4:** Direct SQL query replacements
- **Day 5:** Non-deterministic spawn fixes

### Week 2: Error Handling
- **Days 1-2:** Generic error type replacements
- **Days 3-4:** Random number generation fixes
- **Day 5:** Logging standardization

### Week 3: Quality Improvements
- **Days 1-2:** Citation addition
- **Days 3-4:** Prevention mechanisms (detection tools)
- **Day 5:** Documentation

### Week 4: Testing and Verification
- **Days 1-2:** Integration testing
- **Days 3-4:** Performance verification
- **Day 5:** Code review and final fixes

---

## Risk Mitigation

### Risks:
1. **Breaking changes:** Fixes may break existing functionality
2. **Performance impact:** Lifecycle manager may add overhead
3. **Test failures:** Changes may require test updates
4. **Merge conflicts:** Large refactoring may conflict with other work

### Mitigation:
1. **Incremental fixes:** Fix one pattern at a time
2. **Comprehensive testing:** Test each fix before moving on
3. **Feature flags:** Use feature flags for risky changes
4. **Code review:** All fixes require code review
5. **Rollback plan:** Keep fixes in separate branches until verified

---

## Success Metrics

### Quantitative:
- **0** lifecycle manager bypasses
- **0** direct SQL queries in handlers
- **<5** non-deterministic spawns (only in acceptable contexts)
- **<10** generic error types (only in acceptable contexts)
- **100%** citation coverage for extracted code

### Qualitative:
- Code follows architectural patterns consistently
- New code follows established patterns
- Code review catches violations before merge
- Documentation is clear and comprehensive

---

## Dependencies and Prerequisites

### Required:
1. **Lifecycle manager API documentation:** Need to understand correct usage
2. **Deterministic executor initialization:** Need to verify initialization points
3. **Db trait methods:** Need to add missing methods
4. **Test infrastructure:** Need tests for all fixes

### Nice to Have:
1. **Automated refactoring tools:** Would speed up fixes
2. **Architectural linting rules:** Would prevent future violations
3. **Code review automation:** Would catch violations earlier

---

## References

- [AI_SLOP_DETECTION_CHALLENGES.md](AI_SLOP_DETECTION_CHALLENGES.md) - Analysis of detection challenges
- [CLAUDE.md](../CLAUDE.md) - Development standards
- [DEPRECATED_PATTERNS.md](DEPRECATED_PATTERNS.md) - Known anti-patterns
- [ai_slop_detector.sh](../ai_slop_detector.sh) - Detection script
- [crates/adapteros-lora-lifecycle/src/lib.rs](../../crates/adapteros-lora-lifecycle/src/lib.rs) - Lifecycle manager implementation

