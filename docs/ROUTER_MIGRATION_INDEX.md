# Router Migration Documentation Index

Complete reference for migrating from deprecated `route()` to `route_with_adapter_info()`.

**Status:** v0.01.1 - Deprecation warnings implemented
**Target:** v0.02.0 - Deprecated methods removed
**Effort:** ~40 hours total (4 production files + 41 test files)

---

## Document Map

### 1. Quick Start (You Are Here!)

**For the impatient:** Start with the summary below, then jump to the relevant doc.

### 2. [ROUTER_MIGRATION_SUMMARY.md](../ROUTER_MIGRATION_SUMMARY.md)

**What:** High-level overview of entire migration
**When:** First read - understand the big picture
**Length:** 5 min read
**Includes:**
- Executive summary
- Changes made
- Call site statistics
- Phase-based migration strategy
- Verification checklist

**Key Takeaway:** 46 call sites to migrate over 3 versions

---

### 3. [ROUTER_MIGRATION.md](ROUTER_MIGRATION.md)

**What:** Complete migration guide with detailed rationale
**When:** Before starting migration
**Length:** 15 min read
**Includes:**
- Problem statement with code examples
- Solution architecture
- Step-by-step migration guide (3 patterns)
- Real-world examples from codebase
- Per-adapter scoring details
- Backward compatibility notes
- Performance analysis
- Troubleshooting Q&A
- Related docs

**Key Sections:**
1. **Problem:** Global scores prevent proper adapter selection
2. **Solution:** Per-adapter feature scores with metadata
3. **Migration Steps:** 3 patterns for different contexts
4. **Examples:** Inference pipeline, worker, scoring functions
5. **Details:** How per-adapter scoring works
6. **FAQ:** Answers to common questions

**Key Takeaway:** Why migrate and how to do it step-by-step

---

### 4. [ROUTER_MIGRATION_EXAMPLES.md](ROUTER_MIGRATION_EXAMPLES.md)

**What:** Practical code examples for actual files
**When:** While implementing migration
**Length:** 10 min read
**Includes:**
- Before/after code for 5 key locations
- Implementation patterns
- Minimal migration template
- Suppressing deprecation warnings
- Validation checklist

**Coverage:**
1. `crates/adapteros-lora-worker/src/inference_pipeline.rs:295-330`
2. `crates/adapteros-lora-router/src/scoring.rs:37-53`
3. `crates/adapteros-lora-worker/src/lib.rs:671`
4. `crates/adapteros-lora-worker/src/generation.rs:102-107`
5. Test file patterns

**Key Takeaway:** Copy-paste ready migration patterns

---

### 5. [ROUTER_CALL_SITES.md](ROUTER_CALL_SITES.md)

**What:** Complete inventory of all 46 call sites
**When:** Planning migration scope
**Length:** 10 min read
**Includes:**
- Summary statistics
- Production code (4 sites - HIGH)
- Test code (41 sites - MEDIUM)
- Deprecated methods reference
- Priority matrix
- Verification checklist
- Compilation status tracking

**Call Sites by Priority:**
- HIGH (4): Production inference paths
- MEDIUM (41): Test validation paths
- Total effort: ~40 hours

**Key Takeaway:** Know exactly what needs to be migrated

---

## Quick Migration Path

### For New Code
```rust
// Always use new API for new code
let adapter_info = vec![/* ... */];
let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

### For Existing Code

**Step 1: Understand the issue**
- Read: `ROUTER_MIGRATION_SUMMARY.md` (5 min)
- Understand: Old API uses global scores, new API uses per-adapter

**Step 2: Find your case**
- Check: `ROUTER_MIGRATION_EXAMPLES.md` for your location
- If not listed, check: `ROUTER_CALL_SITES.md` for similar pattern

**Step 3: Implement**
- Copy the pattern from examples
- Construct AdapterInfo from your data
- Replace `router.route()` with `router.route_with_adapter_info()`
- Run tests

**Step 4: Validate**
- `cargo build --workspace`
- `cargo test --workspace`
- No deprecation warnings

---

## Key Facts

### The Problem
```rust
// OLD: Same feature score for ALL adapters
let feature_score = self.compute_weighted_score(features);
let scores: Vec<_> = priors
    .iter()
    .enumerate()
    .map(|(i, &p)| (i, p + feature_score))  // <- Same for all!
    .collect();
```

### The Solution
```rust
// NEW: Different score for each adapter
for (i, &prior) in priors.iter().enumerate() {
    let feature_score = self.compute_adapter_feature_score(features, &adapter_info[i]);
    let score = prior + feature_score;  // <- Different for each adapter!
}
```

### The Benefit
With Python code and equal priors `[1.0, 1.0, 1.0]`:
```
Old: Adapter 0: 1.54  |  Adapter 1: 1.54  |  Adapter 2: 1.54  (all identical!)
New: Adapter 0: 1.54  |  Adapter 1: 1.18  |  Adapter 2: 1.09  (Python adapter wins!)
                 ✓ Python adapter          ✗ No specialization
```

---

## Migration by Role

### For Inference Engineers
1. Read: `ROUTER_MIGRATION.md` (architecture section)
2. Examples: `ROUTER_MIGRATION_EXAMPLES.md` (inference_pipeline.rs)
3. Migrate: `crates/adapteros-lora-worker/src/inference_pipeline.rs`

### For Test Engineers
1. Read: `ROUTER_MIGRATION.md` (test patterns section)
2. Examples: `ROUTER_MIGRATION_EXAMPLES.md` (test patterns)
3. Catalog: `ROUTER_CALL_SITES.md` (test files)
4. Template: `ROUTER_MIGRATION_EXAMPLES.md` (minimal template)

### For Routing Engineers
1. Read: `ROUTER_MIGRATION_SUMMARY.md` (entire doc)
2. Deep dive: `ROUTER_MIGRATION.md` (full details)
3. Implement: `ROUTER_MIGRATION_EXAMPLES.md` (implementation patterns)
4. Verify: `ROUTER_CALL_SITES.md` (all affected sites)

### For Platform Engineers
1. Read: `ROUTER_MIGRATION_SUMMARY.md` (timeline)
2. Plan: `ROUTER_CALL_SITES.md` (effort estimation)
3. Track: Verification checklist in summary doc
4. Monitor: Deprecation warnings during transition

---

## Documentation Statistics

| Document | Size | Read Time | Purpose |
|----------|------|-----------|---------|
| ROUTER_MIGRATION_INDEX.md | This doc | 5 min | Navigation guide |
| ROUTER_MIGRATION_SUMMARY.md | 12 KB | 8 min | Executive overview |
| ROUTER_MIGRATION.md | 11 KB | 15 min | Complete guide |
| ROUTER_MIGRATION_EXAMPLES.md | 9 KB | 10 min | Code examples |
| ROUTER_CALL_SITES.md | 10 KB | 10 min | Call site inventory |
| **Total** | **52 KB** | **48 min** | **Complete reference** |

---

## Deprecation Timeline

### v0.01.1 (Current)
- Deprecation warnings added ✓
- Migration guides created ✓
- Call sites documented ✓
- Backward compatible ✓

**Action:** Start planning migration

### v0.01.2 (1 week)
- Migrate production code (4 files)
- All inference paths updated
- Integration tests pass

**Action:** Production migration sprint

### v0.01.3 (2 weeks)
- Migrate test code (41 files)
- Full test suite updated
- No deprecation warnings

**Action:** Test migration sprint

### v0.02.0 (1 month)
- Remove `route()` method
- Remove `route_with_k0_detection()` method
- Version bump

**Action:** Final cleanup

---

## Implementation Patterns

### Pattern 1: Direct Adapter Metadata

When you have adapter metadata available:

```rust
use adapteros_lora_router::AdapterInfo;

let adapter_info: Vec<AdapterInfo> = adapters
    .iter()
    .map(|adapter| AdapterInfo {
        id: adapter.id.clone(),
        framework: adapter.framework.clone(),
        languages: adapter.language_indices.clone(),
        tier: adapter.tier.clone(),
    })
    .collect();

let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

### Pattern 2: Minimal Adapter Info

When you don't have all metadata:

```rust
use adapteros_lora_router::AdapterInfo;

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

### Pattern 3: Struct Field Storage

When called multiple times:

```rust
pub struct MyRouter {
    router: Router,
    adapter_info: Vec<AdapterInfo>,
}

impl MyRouter {
    fn route(&mut self, features: &[f32], priors: &[f32]) -> Decision {
        self.router.route_with_adapter_info(features, priors, &self.adapter_info)
    }
}
```

---

## Validation Checklist

Use this checklist to track your migration:

### Pre-Migration
- [ ] Read `ROUTER_MIGRATION_SUMMARY.md`
- [ ] Identify your location in codebase
- [ ] Find pattern in `ROUTER_MIGRATION_EXAMPLES.md`
- [ ] Understand per-adapter scoring (read ROUTER_MIGRATION.md)

### During Migration
- [ ] Gather or create adapter metadata
- [ ] Construct AdapterInfo vector
- [ ] Update router call
- [ ] Build: `cargo build --lib`
- [ ] Test locally: `cargo test`

### Post-Migration
- [ ] No deprecation warnings: `cargo build --deny warnings`
- [ ] All tests pass: `cargo test --workspace`
- [ ] Functionality unchanged (compare old vs new routing)
- [ ] Performance acceptable (< 1µs for 8 adapters)

---

## Common Patterns & Examples

### Search Your Location

**Find your file:** `ROUTER_CALL_SITES.md`
- Production code: Pages 2-3
- Test code: Page 4
- Look for your filename

**Get example:** `ROUTER_MIGRATION_EXAMPLES.md`
- Examples for 5 key locations
- Test file template
- Minimal migration pattern

**Copy pattern:**
- Adapt to your context
- Use template from examples
- Validate with checklist

---

## Performance Impact

**Benchmark (8 adapters):**
- Old `route()`: 0.5 µs
- New `route_with_adapter_info()`: 0.6 µs
- Overhead: ~20% (negligible)

**Total routing:** < 1 µs vs inference (ms) - no regression

---

## Support & Resources

### If You're Stuck

1. **Can't find your location?**
   - Check: `ROUTER_CALL_SITES.md` for complete inventory
   - Similar pattern likely exists

2. **Don't have adapter metadata?**
   - Create minimal AdapterInfo (see Pattern 2 above)
   - Read: `ROUTER_MIGRATION.md` troubleshooting section

3. **Migration patterns unclear?**
   - Copy from: `ROUTER_MIGRATION_EXAMPLES.md`
   - Read: Full examples for 5 key locations

4. **Still unclear?**
   - Read: `ROUTER_MIGRATION.md` (detailed guide)
   - Check: Tests in `tests/router_per_adapter_scoring.rs`

### API Reference

**New method signature:**
```rust
pub fn route_with_adapter_info(
    &mut self,
    features: &[f32],
    priors: &[f32],
    adapter_info: &[AdapterInfo],
) -> Decision
```

**AdapterInfo fields:**
```rust
pub struct AdapterInfo {
    pub id: String,
    pub framework: Option<String>,
    pub languages: Vec<usize>,
    pub tier: String,
}
```

**Language indices:**
```
0 = Python, 1 = Rust, 2 = JavaScript, 3 = Go,
4 = Java, 5 = C/C++, 6 = C#, 7 = Ruby
```

---

## Document Usage Guide

### Quick Reference
- **Start here:** This page
- **5-minute overview:** `ROUTER_MIGRATION_SUMMARY.md`
- **Looking for code:** `ROUTER_MIGRATION_EXAMPLES.md`
- **Tracking scope:** `ROUTER_CALL_SITES.md`

### Deep Understanding
- **Complete guide:** `ROUTER_MIGRATION.md`
- **Architecture:** Read architecture patterns section
- **Per-adapter scoring:** Read scoring details section
- **Troubleshooting:** Read FAQ section

### Implementation
- **Code examples:** `ROUTER_MIGRATION_EXAMPLES.md`
- **Test reference:** `tests/router_per_adapter_scoring.rs`
- **Call sites:** `ROUTER_CALL_SITES.md` (find your location)
- **Validation:** Use checklists in summary doc

---

## Next Steps

1. **Today:** Read `ROUTER_MIGRATION_SUMMARY.md` (5 min)
2. **Tomorrow:** Find your location in `ROUTER_CALL_SITES.md`
3. **This week:** Copy pattern from `ROUTER_MIGRATION_EXAMPLES.md`
4. **Next week:** Implement and validate
5. **Before v0.02.0:** Ensure all tests pass with no deprecation warnings

---

## Version History

| Date | Status | Changes |
|------|--------|---------|
| 2025-11-21 | v1.0 | Initial migration guide |
| TBD | v0.01.2 | Production code migrations |
| TBD | v0.01.3 | Test migrations |
| TBD | v0.02.0 | Deprecated methods removed |

---

## Contacts & Questions

For questions about this migration:
1. Check FAQ in `ROUTER_MIGRATION.md`
2. Review examples in `ROUTER_MIGRATION_EXAMPLES.md`
3. Check call site details in `ROUTER_CALL_SITES.md`
4. Read full guide in `ROUTER_MIGRATION.md`

---

**Ready to migrate?** Start with [ROUTER_MIGRATION_SUMMARY.md](../ROUTER_MIGRATION_SUMMARY.md)
