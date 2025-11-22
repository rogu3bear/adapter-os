# Router Migration Guide: compute_weighted_score -> route_with_adapter_info()

## Overview

The router has been refactored to fix per-adapter feature scoring and orthogonality penalties. This guide walks through migrating from the deprecated `route()` method to the new `route_with_adapter_info()` API.

**Status:** Deprecation warnings added in v0.01.1. The old API remains functional but will be removed in v0.02.0.

---

## Problem Statement

The old `route()` method computes a **global feature score** that's identical for all adapters:

```rust
// OLD: Global score (same for all adapters)
let feature_score = self.compute_weighted_score(features);
let mut scores: Vec<(usize, f32)> = priors
    .iter()
    .enumerate()
    .map(|(i, &prior)| {
        let score = prior + feature_score;  // <- Same feature_score for all!
        (i, score)
    })
    .collect();
```

This prevents:
- ✗ Language affinity matching (Python adapters boosted for Python code)
- ✗ Framework specialization (Django adapters boosted for Django patterns)
- ✗ Orthogonality penalties from applying (diversity controls ineffective)
- ✗ Proper stack-based filtering during scoring

**Result:** Adapters with matching metadata are treated identically to mismatched adapters.

---

## Solution: route_with_adapter_info()

The new API accepts adapter metadata and produces **per-adapter feature scores**:

```rust
// NEW: Per-adapter scores
for (i, &prior) in priors.iter().enumerate() {
    let adapter_feature_score = self.compute_adapter_feature_score(features, &adapter_info[i]);
    let orthogonal_penalty = self.compute_adapter_orthogonal_penalty(i);
    let score = prior + adapter_feature_score - orthogonal_penalty;
}
```

**Benefits:**
- ✓ Python adapters get boosted score for Python features
- ✓ Django adapters get boosted for Django patterns
- ✓ Orthogonality penalties actually affect selection
- ✓ Diversity controls work as designed
- ✓ Stack filtering integrated into scoring

---

## Migration Steps

### Step 1: Gather adapter metadata

You need to construct `AdapterInfo` for each adapter:

```rust
use adapteros_lora_router::AdapterInfo;

let adapter_info = vec![
    AdapterInfo {
        id: "python-general".to_string(),
        framework: None,
        languages: vec![0, 1],  // Language indices (0=Python, 1=Rust, etc.)
        tier: "tier_1".to_string(),
    },
    AdapterInfo {
        id: "django-specific".to_string(),
        framework: Some("django".to_string()),
        languages: vec![0],  // Python only
        tier: "persistent".to_string(),
    },
    // ... more adapters
];
```

**Language indices** (standard mapping):
```
0 = Python
1 = Rust
2 = JavaScript/TypeScript
3 = Go
4 = Java
5 = C/C++
6 = C#
7 = Ruby
```

### Step 2: Check your current code

**Pattern 1: Simple route() calls**

OLD:
```rust
let decision = router.route(&features, &priors);
```

NEW:
```rust
let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

**Pattern 2: route_with_code_features() (already correct)**

This method already handles per-adapter scoring internally:
```rust
// Already using per-adapter scoring - no change needed
let decision = router.route_with_code_features(&code_features, &adapter_info);
```

**Pattern 3: route_with_k0_detection() (legacy)**

OLD:
```rust
let decision = router.route_with_k0_detection(&features, &priors);
```

NEW (use route_with_adapter_info and check result):
```rust
let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
if decision.indices.is_empty() {
    // Handle k0 case (no adapters selected)
}
```

---

## Real-world Examples

### Example 1: Inference Pipeline

**Before (inference_pipeline.rs):**
```rust
// In generate_tokens():
let features = self.create_feature_vector(&current_tokens);
let priors = vec![1.0; 8];
let decision = self.router.route(&features, &priors);  // <- Global score!
```

**After:**
```rust
let features = self.create_feature_vector(&current_tokens);
let priors = vec![1.0; 8];

// Get adapter metadata (from database or cache)
let adapter_info = self.get_loaded_adapters_info();  // Get framework, languages, tier

let decision = self.router.route_with_adapter_info(&features, &priors, &adapter_info);
```

### Example 2: Worker Inference

**Before (lora-worker/lib.rs):**
```rust
let decision = self.router.route(&features, &priors);
```

**After:**
```rust
// Map loaded adapters to AdapterInfo
let adapter_info: Vec<AdapterInfo> = self.loaded_adapters
    .iter()
    .map(|(id, metadata)| AdapterInfo {
        id: id.clone(),
        framework: metadata.framework.clone(),
        languages: metadata.language_indices.clone(),
        tier: metadata.tier.clone(),
    })
    .collect();

let decision = self.router.route_with_adapter_info(&features, &priors, &adapter_info);
```

### Example 3: ScoringFunction wrapper

**Before (scoring.rs):**
```rust
impl ScoringFunction for MyScorer {
    fn score(&mut self, features: &[f32], priors: &[f32]) -> Decision {
        self.router.route(features, priors)  // <- Global score
    }
}
```

**After:**
```rust
impl ScoringFunction for MyScorer {
    fn score(&mut self, features: &[f32], priors: &[f32]) -> Decision {
        // Assume adapter_info is available (part of scorer state)
        self.router.route_with_adapter_info(features, priors, &self.adapter_info)
    }
}
```

---

## Backward Compatibility

The old `route()` method is maintained for backward compatibility until v0.02.0:

```rust
#[deprecated(since = "0.01.1", note = "Use route_with_adapter_info() for per-adapter scoring")]
pub fn route(&mut self, features: &[f32], priors: &[f32]) -> Decision {
    tracing::warn!("Router::route() is deprecated...");
    // ... old implementation
}
```

**Compilation:**
- `cargo build` - Succeeds with deprecation warnings
- `cargo build --deny warnings` - Fails (forces migration)
- `cargo test` - Works but logs warnings

**Timeline:**
- v0.01.1 (current): Deprecation warnings added
- v0.02.0: `route()` method removed

---

## Per-Adapter Scoring Details

### How It Works

The new API computes separate feature scores for each adapter:

```rust
pub fn compute_adapter_feature_score(
    &self,
    features: &[f32],
    adapter_info: &AdapterInfo,
) -> f32 {
    // Language affinity: boost if adapter supports detected language
    let lang_idx = detect_language_from_features(features);
    if adapter_info.supports_language(lang_idx) {
        score += 2.0 * base_language_score;  // 2x boost!
    }

    // Framework specialization: boost if adapter handles framework
    if let Some(ref framework) = adapter_info.framework {
        if detect_framework(features) == Some(framework) {
            score += 1.5 * base_framework_score;  // 1.5x boost!
        }
    }

    // ... tier-based boosts, orthogonality penalties, etc.
    score
}
```

**Impact on example:**

Given Python code with equal priors `[1.0, 1.0, 1.0]`:

```
Adapter 0 (Python, tier_1):    1.0 + 0.54 (Python match) = 1.54 ✓
Adapter 1 (Rust, tier_0):      1.0 + 0.18 (no match)     = 1.18
Adapter 2 (JS, tier_2):        1.0 + 0.09 (no match)     = 1.09
```

Python adapter ranks first due to language affinity!

### Orthogonality Penalties

If MPLoRA diversity constraints are enabled:

```rust
pub fn set_orthogonal_constraints(&mut self, enabled: true, ...);
```

Penalties are applied **during selection**:

```rust
// Compute penalty based on similarity to recent selections
let orthogonal_penalty = self.compute_adapter_orthogonal_penalty(adapter_idx);
let score = prior + adapter_feature_score - orthogonal_penalty;
```

This ensures adapters that were recently selected are penalized appropriately.

---

## Troubleshooting

### Q: Where do I get adapter metadata?

**A:** Depends on your context:

1. **Database context:** Query from `adapters` table
   ```rust
   let adapter_info: Vec<AdapterInfo> = db.query_adapters()
       .iter()
       .map(|row| AdapterInfo {
           id: row.id.clone(),
           framework: row.framework.clone(),
           languages: row.language_indices.clone(),
           tier: row.tier.clone(),
       })
       .collect();
   ```

2. **Runtime (loaded adapters):** From worker state
   ```rust
   let adapter_info: Vec<AdapterInfo> = self.loaded_adapters.values()
       .map(|metadata| metadata.to_adapter_info())
       .collect();
   ```

3. **Tests:** Construct directly
   ```rust
   let adapter_info = vec![
       AdapterInfo {
           id: "test-adapter".to_string(),
           framework: None,
           languages: vec![0],
           tier: "tier_1".to_string(),
       },
   ];
   ```

### Q: Can I still use route() with empty adapter_info?

**A:** No - the function validates lengths match:
```rust
if priors.len() != adapter_info.len() {
    tracing::warn!("Mismatch, falling back to route()");
    return self.route(features, priors);
}
```

Pass an empty `Vec::new()` only if you have 0 adapters. Otherwise it will warn.

### Q: What if I don't have all adapter metadata?

**A:** Create minimal AdapterInfo objects with defaults:
```rust
AdapterInfo {
    id: adapter_id.clone(),
    framework: None,  // Unknown framework
    languages: vec![],  // Supports all languages (default)
    tier: "tier_1".to_string(),  // Default tier
}
```

The new API degrades gracefully - adapters without metadata just won't get specialized boosts.

### Q: Do I need to update tests?

**A:** Only if they call `router.route()` directly.

Recommended:
```rust
// Test files
let adapter_info = vec![
    AdapterInfo {
        id: "adapter-0".to_string(),
        framework: None,
        languages: vec![0],
        tier: "tier_1".to_string(),
    },
    // ... more adapters
];

let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

---

## Deprecation Timeline

| Version | Status | Notes |
|---------|--------|-------|
| v0.01.1 | Deprecation warnings | Old API still works, logs warnings |
| v0.02.0 | Removal | `route()` and `route_with_k0_detection()` removed |

**Action items:**
- [ ] Update all production code to use `route_with_adapter_info()`
- [ ] Update integration tests to use new API
- [ ] Run `cargo build --deny warnings` to catch any remaining calls
- [ ] Test with both APIs side-by-side before v0.02.0 cutover

---

## Performance Impact

The new API has **negligible performance overhead**:

| Operation | Time |
|-----------|------|
| Old `route()` | 0.5 µs (8 adapters) |
| New `route_with_adapter_info()` | 0.6 µs (8 adapters) |
| Overhead | ~20% (in per-adapter feature scoring) |

The additional per-adapter scoring logic is O(k) where k ≤ 8, so it's negligible vs. network/inference costs.

---

## Related Documentation

- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md#k-sparse-routing) - Router architecture
- [MULTI_ADAPTER_ROUTING.md](MULTI_ADAPTER_ROUTING.md) - Multi-adapter selection details
- [docs/RBAC.md](RBAC.md) - Role-based access for adapter metadata
- `crates/adapteros-lora-router/src/lib.rs:693` - `route_with_adapter_info()` implementation

---

## Implementation Checklist

When migrating a module:

- [ ] Identify all `router.route()` calls
- [ ] Determine how to access adapter metadata
- [ ] Construct `AdapterInfo` vector
- [ ] Replace `router.route()` with `router.route_with_adapter_info()`
- [ ] Add logging/tracing for debugging
- [ ] Run tests with both APIs (capture deprecation warnings)
- [ ] Verify adapter selection behavior improves
- [ ] Remove old code once confident

---

## Questions?

See:
1. Test examples: `tests/router_per_adapter_scoring.rs`
2. Integration: `crates/adapteros-lora-worker/src/lib.rs:671`
3. Code search: `grep -r "route_with_adapter_info" crates/ tests/`
