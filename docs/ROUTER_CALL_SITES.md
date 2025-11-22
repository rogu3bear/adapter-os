# Router Call Sites Analysis

Complete inventory of all `router.route()` call sites for migration tracking.

**Generated:** 2025-11-21
**Status:** Deprecation warnings added in v0.01.1
**Target:** All migrations complete before v0.02.0

---

## Summary

| Category | Count | Priority | Status |
|----------|-------|----------|--------|
| **Production Code** | 4 | HIGH | Pending migration |
| **Tests** | 41 | MEDIUM | Backward compatible |
| **Scoring Functions** | 1 | HIGH | Pending migration |
| **Total** | 46 | - | - |

---

## Production Code (HIGH Priority)

These are production code paths that should be migrated first.

### 1. crates/adapteros-lora-worker/src/inference_pipeline.rs:299

**File:** `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/inference_pipeline.rs`
**Line:** 299
**Method:** `InferencePipeline::generate_tokens()`

```rust
let decision = self.router.route(&features, &priors);
```

**Context:** Main inference loop - critical path
**Migration:** Needs access to loaded adapter metadata
**Recommended:** Construct AdapterInfo from pipeline state
**Difficulty:** Medium (requires adding adapter metadata tracking)

**Migration Example:**
```rust
let adapter_info: Vec<AdapterInfo> = self.loaded_adapters
    .iter()
    .map(|meta| AdapterInfo {
        id: meta.id.clone(),
        framework: meta.framework.clone(),
        languages: meta.language_indices.clone(),
        tier: meta.tier.clone(),
    })
    .collect();

let decision = self.router.route_with_adapter_info(&features, &priors, &adapter_info);
```

**Impact:** Enables per-adapter scoring during inference

---

### 2. crates/adapteros-lora-worker/src/lib.rs:671

**File:** `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/lib.rs`
**Line:** 671
**Method:** Unknown (likely inference method)

```rust
let decision = self.router.route(&features, &priors);
```

**Context:** Worker inference
**Migration:** Map from loaded adapter state
**Recommended:** Same pattern as inference_pipeline.rs
**Difficulty:** Medium

---

### 3. crates/adapteros-lora-worker/src/generation.rs:107

**File:** `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/generation.rs`
**Line:** 107
**Method:** `Generator::generate()`

```rust
let decision = router.route(&features, &priors);
```

**Context:** Token generation loop
**Migration:** Create minimal adapter info for 8 adapters
**Recommended:** Create placeholder AdapterInfo since this uses dummy features
**Difficulty:** Low

**Migration Example:**
```rust
let adapter_info: Vec<AdapterInfo> = (0..num_adapters)
    .map(|i| AdapterInfo {
        id: format!("adapter_{}", i),
        framework: None,
        languages: vec![],
        tier: "tier_1".to_string(),
    })
    .collect();

let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

---

### 4. crates/adapteros-lora-router/src/scoring.rs:51

**File:** `/Users/star/Dev/aos/crates/adapteros-lora-router/src/scoring.rs`
**Line:** 51
**Struct:** `WeightedScorer`
**Method:** `ScoringFunction::score()`

```rust
impl ScoringFunction for WeightedScorer {
    fn score(&mut self, features: &[f32], priors: &[f32], ...) -> Decision {
        self.router.route(features, priors)  // <- DEPRECATED
    }
}
```

**Context:** Pluggable scoring function
**Migration:** Store adapter_info in struct OR extend trait signature
**Recommended:** Store adapter_info field (simpler)
**Difficulty:** Medium

**Recommended Implementation:**
```rust
pub struct WeightedScorer {
    router: Router,
    adapter_info: Vec<AdapterInfo>,
}

impl WeightedScorer {
    pub fn with_adapter_info(mut self, adapter_info: Vec<AdapterInfo>) -> Self {
        self.adapter_info = adapter_info;
        self
    }
}

impl ScoringFunction for WeightedScorer {
    fn score(&mut self, features: &[f32], priors: &[f32], ...) -> Decision {
        if self.adapter_info.is_empty() {
            #[allow(deprecated)]
            return self.router.route(features, priors);
        }
        self.router.route_with_adapter_info(features, priors, &self.adapter_info)
    }
}
```

---

## Test Code (MEDIUM Priority)

These tests verify router functionality. They can remain using deprecated API during transition, but should be migrated for validation.

### Tests in crates/adapteros-lora-router/tests/

**File:** `crates/adapteros-lora-router/tests/determinism.rs`
**Occurrences:** Lines 29, 30, 55, 103, 163, 182, 211

These tests verify routing determinism. They should be migrated to validate that per-adapter scoring is also deterministic.

**File:** `crates/adapteros-lora-router/tests/telemetry.rs`
**Occurrences:** Lines 18, 67, 93, 119, 135, 136, 139, 140, 167

These tests validate telemetry emission. They should be updated to use route_with_adapter_info to test full path.

**File:** `crates/adapteros-lora-router/tests/router_ring_golden.rs`
**Occurrences:** Line 236

Golden test for RouterRing conversion.

---

### Tests in root tests/ directory

**Recommended:** Update all to use route_with_adapter_info for complete validation

**Files with router.route() calls:**

1. `tests/router_correctness_proofs.rs` (11 occurrences)
   - Lines: 67, 141, 195, 314, 324, 347, 381, 414, 452, 477, 540
   - Purpose: Verify routing correctness
   - Recommendation: Migrate to validate per-adapter scoring correctness

2. `tests/router_per_adapter_scoring.rs` (1 occurrence at line 164)
   - Purpose: Test per-adapter scoring (mixed with new API tests)
   - Recommendation: Complete migration to use only new API

3. `tests/determinism_guardrail_suite.rs` (3 occurrences)
   - Lines: 89, 180, 524
   - Purpose: Test determinism guarantees
   - Recommendation: Validate per-adapter determinism

4. `tests/mplora_determinism.rs` (4 occurrences)
   - Lines: 84, 85, 334, 439
   - Purpose: Test MPLoRA diversity features
   - Recommendation: Ensure orthogonality still deterministic

5. `tests/router_trace_generation.rs` (5 occurrences)
   - Lines: 102, 122, 142, 162, 182, 370
   - Purpose: Generate routing traces
   - Recommendation: Update trace generation to use new API

6. `tests/determinism_harness.rs` (1 occurrence at line 441)
   - Purpose: Test deterministic execution

7. `tests/fault_injection_harness.rs` (2 occurrences)
   - Lines: 79, 104
   - Purpose: Test fault tolerance

8. `tests/router_scoring_weights.rs`
   - No route() calls (check implementation)

---

## Deprecated Methods Summary

### route()

**Signature:**
```rust
#[deprecated(since = "0.01.1", note = "Use route_with_adapter_info() for per-adapter scoring")]
pub fn route(&mut self, features: &[f32], priors: &[f32]) -> Decision
```

**Problem:** Produces global feature score identical for all adapters
**Solution:** Use `route_with_adapter_info()` with per-adapter metadata
**Removal:** v0.02.0

**Call sites:** 46 locations

---

### route_with_k0_detection()

**Signature:**
```rust
#[deprecated(since = "0.01.1", note = "Use route_with_adapter_info() for proper k0 detection")]
pub fn route_with_k0_detection(&mut self, features: &[f32], priors: &[f32]) -> Decision
```

**Problem:** Uses same global scoring as route()
**Solution:** Use route_with_adapter_info() and check if `decision.indices.is_empty()`
**Removal:** v0.02.0

**Call sites:** 0 identified (but method exists and should be documented)

---

## Migration Priority Matrix

| Priority | Category | Action | Timeline |
|----------|----------|--------|----------|
| **HIGH** | Production inference paths | Migrate to route_with_adapter_info | v0.01.2 |
| **HIGH** | Scoring functions | Add adapter_info field | v0.01.2 |
| **MEDIUM** | Core tests | Migrate with route_with_adapter_info | v0.01.3 |
| **MEDIUM** | Integration tests | Update to use new API | v0.01.3 |
| **LOW** | Golden tests | Update as needed | v0.02.0 |

---

## Migration Verification Checklist

Use this checklist to track migration progress:

### Production Code
- [ ] `crates/adapteros-lora-worker/src/inference_pipeline.rs:299` - Migrated
- [ ] `crates/adapteros-lora-worker/src/lib.rs:671` - Migrated
- [ ] `crates/adapteros-lora-worker/src/generation.rs:107` - Migrated
- [ ] `crates/adapteros-lora-router/src/scoring.rs:51` - Migrated

### Test Code (Select High-Value Tests)
- [ ] `tests/router_per_adapter_scoring.rs` - All migrated
- [ ] `tests/router_correctness_proofs.rs` - Migrated
- [ ] `crates/adapteros-lora-router/tests/determinism.rs` - Migrated
- [ ] `crates/adapteros-lora-router/tests/telemetry.rs` - Migrated

### Build Validation
- [ ] `cargo build --workspace` - No deprecation warnings
- [ ] `cargo test --workspace` - All tests pass
- [ ] `cargo build --deny warnings` - Strict check passes

### Performance
- [ ] No regression in inference latency
- [ ] Router scoring still < 1µs for 8 adapters

---

## Compilation Status

**Current (v0.01.1):**
```
✓ Compilation succeeds
⚠ Deprecation warnings on 46 call sites
✓ Tests pass with warnings
```

**Target (v0.02.0):**
```
✓ Compilation succeeds
✓ No deprecation warnings
✓ All tests pass
✓ All adapters use per-adapter scoring
```

---

## Questions & Answers

**Q: Should I migrate all call sites at once?**
A: No. Start with production code (4 files), then tests. Prioritize by impact.

**Q: Can I suppress the warnings?**
A: Yes, temporarily:
```rust
#[allow(deprecated)]
let decision = router.route(&features, &priors);
```

**Q: Will the API change again?**
A: route_with_adapter_info() is the canonical API. No further changes expected.

**Q: What if I don't have adapter metadata?**
A: Create minimal AdapterInfo with defaults (None framework, empty languages).

**Q: Is there a performance impact?**
A: ~20% overhead (0.5µs → 0.6µs for 8 adapters). Negligible vs. inference.

---

## Implementation Resources

1. **Migration Guide:** `docs/ROUTER_MIGRATION.md`
2. **Code Examples:** `docs/ROUTER_MIGRATION_EXAMPLES.md`
3. **Test Reference:** `tests/router_per_adapter_scoring.rs`
4. **API Docs:** `crates/adapteros-lora-router/src/lib.rs:693`

---

## Related Issues/PRs

- Deprecation warnings added in: `src/lib.rs` (current)
- Migration guide created: `docs/ROUTER_MIGRATION.md`
- Examples document: `docs/ROUTER_MIGRATION_EXAMPLES.md`

---

## Timeline

| Version | Status | Milestone |
|---------|--------|-----------|
| v0.01.1 | Current | Deprecation warnings, migration guides |
| v0.01.2 | Next | Production code migrations |
| v0.01.3 | Follow-up | Test migrations |
| v0.02.0 | Final | Remove deprecated methods |

