# Router Migration Implementation Examples

Practical code examples for migrating from `route()` to `route_with_adapter_info()`.

---

## Location: crates/adapteros-lora-worker/src/inference_pipeline.rs

### Current Code (Lines 295-330)

```rust
// 5. Router decision: select K adapters
let features = self.create_feature_vector(&current_tokens);
let priors = vec![1.0; 8]; // Uniform priors for all adapters
let decision = self.router.route(&features, &priors);  // <- DEPRECATED

// Emit router decision telemetry
let router_event = RouterDecisionEvent {
    step,
    input_token_id,
    candidate_adapters: decision
        .candidates
        .iter()
        .map(|c| RouterCandidate {
            adapter_idx: c.adapter_idx,
            raw_score: c.raw_score,
            gate_q15: c.gate_q15,
        })
        .collect(),
    entropy: decision.entropy,
    tau: self.router.tau(),
    entropy_floor: self.router.eps(),
    stack_hash: self.router.stack_hash(),
    stack_id: request.stack_id.clone(),
    stack_version: request.stack_version,
};
```

### Migration Path

**Step 1:** Add adapter metadata tracking to `InferencePipeline` struct:

```rust
pub struct InferencePipeline {
    // ... existing fields ...
    /// Metadata for loaded adapters
    loaded_adapters: Vec<AdapterMetadata>,
}

#[derive(Clone)]
struct AdapterMetadata {
    id: String,
    framework: Option<String>,
    language_indices: Vec<usize>,
    tier: String,
}
```

**Step 2:** Build AdapterInfo vector before routing:

```rust
// 5. Router decision: select K adapters
let features = self.create_feature_vector(&current_tokens);
let priors = vec![1.0; 8]; // Uniform priors for all adapters

// Convert loaded adapters to AdapterInfo
use adapteros_lora_router::AdapterInfo;
let adapter_info: Vec<AdapterInfo> = self.loaded_adapters
    .iter()
    .map(|meta| AdapterInfo {
        id: meta.id.clone(),
        framework: meta.framework.clone(),
        languages: meta.language_indices.clone(),
        tier: meta.tier.clone(),
    })
    .collect();

// Use the new API
let decision = self.router.route_with_adapter_info(&features, &priors, &adapter_info);

// Rest of the code remains identical
let router_event = RouterDecisionEvent { /* ... */ };
```

---

## Location: crates/adapteros-lora-router/src/scoring.rs

### Current WeightedScorer (Lines 37-53)

```rust
impl ScoringFunction for WeightedScorer {
    fn name(&self) -> &str {
        "weighted"
    }

    fn score(
        &mut self,
        features: &[f32],
        priors: &[f32],
        _k: usize,
        _tau: f32,
        _eps: f32,
    ) -> Decision {
        // Use the existing router logic
        self.router.route(features, priors)  // <- DEPRECATED
    }
}
```

### Updated Implementation

The `ScoringFunction` trait needs to be extended to support adapter metadata:

**Option A: Add adapter_info parameter to trait**

```rust
pub trait ScoringFunction: Send + Sync {
    fn name(&self) -> &str;

    /// Score adapters with per-adapter feature scoring
    /// # Arguments
    /// * `features` - Feature vector
    /// * `priors` - Prior scores for each adapter
    /// * `adapter_info` - Metadata for each adapter
    /// * `k` - Number of adapters to select
    /// * `tau` - Temperature
    /// * `eps` - Entropy floor
    fn score_with_adapters(
        &mut self,
        features: &[f32],
        priors: &[f32],
        adapter_info: &[crate::AdapterInfo],
        k: usize,
        tau: f32,
        eps: f32,
    ) -> Decision;

    // Backward-compatible default (deprecated)
    fn score(&mut self, features: &[f32], priors: &[f32], k: usize, tau: f32, eps: f32) -> Decision {
        // Create minimal adapter info
        let adapter_info = vec![
            crate::AdapterInfo {
                id: format!("adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "tier_1".to_string(),
            };
            priors.len()
        ];
        self.score_with_adapters(features, priors, &adapter_info, k, tau, eps)
    }
}
```

**Option B: Store adapter_info in WeightedScorer** (Simpler)

```rust
pub struct WeightedScorer {
    router: Router,
    adapter_info: Vec<crate::AdapterInfo>,  // <- Add this field
}

impl WeightedScorer {
    pub fn new(router: Router) -> Self {
        Self {
            router,
            adapter_info: Vec::new(),
        }
    }

    pub fn with_adapter_info(mut self, adapter_info: Vec<crate::AdapterInfo>) -> Self {
        self.adapter_info = adapter_info;
        self
    }
}

impl ScoringFunction for WeightedScorer {
    fn name(&self) -> &str {
        "weighted"
    }

    fn score(
        &mut self,
        features: &[f32],
        priors: &[f32],
        _k: usize,
        _tau: f32,
        _eps: f32,
    ) -> Decision {
        if self.adapter_info.is_empty() {
            // Fallback to old API if no adapter info
            #[allow(deprecated)]
            return self.router.route(features, priors);
        }

        // Use new API with per-adapter scoring
        self.router.route_with_adapter_info(features, priors, &self.adapter_info)
    }
}
```

---

## Location: crates/adapteros-lora-worker/src/lib.rs

### Current Code (Around Line 671)

```rust
// In some inference method
let decision = self.router.route(&features, &priors);  // <- DEPRECATED
```

### Migration (If adapter data is available)

```rust
// Build adapter info from loaded adapters
let adapter_info: Vec<AdapterInfo> = self.adapters
    .iter()
    .map(|(_, adapter)| AdapterInfo {
        id: adapter.id.clone(),
        framework: adapter.metadata.framework.clone(),
        languages: adapter.metadata.language_indices.clone(),
        tier: adapter.metadata.tier.clone(),
    })
    .collect();

let decision = self.router.route_with_adapter_info(&features, &priors, &adapter_info);
```

---

## Location: crates/adapteros-lora-worker/src/generation.rs

### Current Code (Lines 102-107)

```rust
// Get router decision
// For now, use dummy features and uniform priors sized to adapter_count
let num_adapters = 8; // Default adapter count for dummy routing
let features = vec![0.0f32; 16]; // Dummy features
let priors = vec![1.0f32 / num_adapters as f32; num_adapters]; // Uniform priors
let decision = router.route(&features, &priors);  // <- DEPRECATED
```

### Migration

```rust
// Get router decision with adapter info
let num_adapters = 8;
let features = vec![0.0f32; 16];
let priors = vec![1.0f32 / num_adapters as f32; num_adapters];

// Create minimal adapter info (required for new API)
use adapteros_lora_router::AdapterInfo;
let adapter_info: Vec<AdapterInfo> = (0..num_adapters)
    .map(|i| AdapterInfo {
        id: format!("adapter_{}", i),
        framework: None,
        languages: vec![],  // All languages
        tier: "tier_1".to_string(),
    })
    .collect();

let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

---

## Location: Test Files

### Pattern: Tests in crates/adapteros-lora-router/tests/

**Current (determinism.rs line 29):**

```rust
let decision1 = router.route(&[], &priors);
let decision2 = router.route(&[], &priors);
```

**Migration (create adapter info):**

```rust
use adapteros_lora_router::AdapterInfo;

let adapter_info = vec![
    AdapterInfo {
        id: "test-1".to_string(),
        framework: None,
        languages: vec![0],
        tier: "tier_1".to_string(),
    },
    AdapterInfo {
        id: "test-2".to_string(),
        framework: None,
        languages: vec![1],
        tier: "tier_1".to_string(),
    },
];

let decision1 = router.route_with_adapter_info(&[], &priors, &adapter_info);
let decision2 = router.route_with_adapter_info(&[], &priors, &adapter_info);
```

---

## Minimal Migration Template

For quick migrations, use this template:

```rust
use adapteros_lora_router::AdapterInfo;

// Build adapter info (replace with real metadata if available)
let adapter_info: Vec<AdapterInfo> = (0..priors.len())
    .map(|i| AdapterInfo {
        id: format!("adapter_{}", i),
        framework: None,
        languages: vec![],
        tier: "tier_1".to_string(),
    })
    .collect();

// OLD:
// let decision = router.route(&features, &priors);

// NEW:
let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
```

---

## Suppressing Deprecation Warnings

For code that can't be migrated immediately:

```rust
#[allow(deprecated)]
{
    let decision = router.route(&features, &priors);
    // ... use decision
}
```

Or at module level:

```rust
#![allow(deprecated)]
```

---

## Validation Checklist

After migration:

- [ ] Code compiles with `cargo build`
- [ ] Deprecation warnings are resolved or suppressed
- [ ] `cargo test --workspace` passes
- [ ] Router decisions produce expected adapter selections
- [ ] Adapter with matching metadata scores higher than mismatched
- [ ] Orthogonality penalties properly penalize recent adapters
- [ ] No performance regression (benchmark if performance-critical)

---

## Related Files

Key files for understanding the migration:

1. **Router API:** `/crates/adapteros-lora-router/src/lib.rs:693`
   - `route_with_adapter_info()` implementation
   - `compute_adapter_feature_score()` logic

2. **Migration Guide:** `/docs/ROUTER_MIGRATION.md`
   - Complete migration guide with rationale

3. **Tests:** `/tests/router_per_adapter_scoring.rs`
   - Real examples of `route_with_adapter_info()` usage
   - Per-adapter scoring validation

4. **Integration:** `/crates/adapteros-lora-worker/src/inference_pipeline.rs:299`
   - Actual integration point for inference
