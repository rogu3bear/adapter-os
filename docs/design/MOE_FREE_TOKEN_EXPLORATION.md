# MoE "Free Token" Optimization with Static LoRA Adapters

**Status**: Implementation Complete (Phase 1-3)
**Date**: 2025-12-25
**Context**: MoE cold-start penalty (~5s TTFT) vs warm (~250ms)

## Implementation Status

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1: Expert Heat Map Collection | ✅ Complete | `moe_prefix_cache.rs` with `ExpertHeatMap` |
| Phase 2: Expert Pre-Warming | ✅ Complete | Bridge protocol v3 with routing collection |
| Phase 3: Free Token Infrastructure | ✅ Complete | `FreeToken`, `PrecomputedTokens`, manifest support |
| Phase 4: Runtime Learning | 🔲 Future | `ContinuationLearner` not yet implemented |

### Files Implemented

- `crates/adapteros-lora-worker/src/moe_prefix_cache.rs` - MoE-specific cache with free tokens
- `crates/adapteros-lora-worker/src/mlx_subprocess_bridge.rs` - Protocol v3 with routing
- `crates/adapteros-manifest/src/lib.rs` - `FreeTokenHint` in adapter manifest
- `scripts/mlx_bridge_server.py` - MoE detection and routing collection

## Problem Statement

MoE (Mixture of Experts) models like Qwen3-Coder-30B-A3B exhibit significant cold-start latency:

| State | TTFT | Tokens/sec |
|-------|------|------------|
| Cold | ~5000ms | ~2 |
| Warm | ~250ms | ~30 |

This 20x latency penalty occurs because:
1. **Expert loading**: 128 experts, 8 active per token
2. **Routing computation**: Gate network must compute routing for each token
3. **Memory bandwidth**: Expert weights scattered across memory

## Research Hypothesis

**Static LoRA adapters might encode domain-specific routing patterns that can be exploited for "free tokens".**

The intuition:
- A LoRA adapter trained on Python code likely activates a predictable subset of experts
- These routing patterns are stable for the adapter's domain
- We can pre-warm the relevant experts and pre-compute initial routing

## Existing Infrastructure

### PrefixKvCache (`prefix_kv_cache.rs`)

Current capabilities:
- Caches per-layer key/value tensors for static prefixes
- LRU eviction with byte budget
- Single-flight deduplication for concurrent builds
- Cryptographic keying via `prefix_kv_key_b3`

Current `PrefixKvEntry` structure:
```rust
pub struct PrefixKvEntry {
    pub keys: Vec<Vec<f32>>,      // Per-layer keys
    pub values: Vec<Vec<f32>>,    // Per-layer values
    pub tenant_id: String,
    pub prefix_cached_token_count: u32,
    pub kv_bytes: u64,
    // ... LRU tracking
}
```

### MoE LoRA Strategy (`moe.rs`)

Current `RoutingWeightedShared` strategy:
```
expert_out += (Q15_gate / 32767.0) * routing_score[e] * (alpha/rank) * (B @ A) @ x
```

Key insight: Routing scores determine expert activation patterns.

## Proposed Extension: MoEPrefixEntry

Extend the cache to store MoE-specific routing data:

```rust
/// Extended prefix entry for MoE models
pub struct MoEPrefixEntry {
    /// Base KV cache data
    pub kv: PrefixKvEntry,

    /// Per-layer expert routing indices
    /// Dimensions: [num_prefix_tokens][num_layers][num_experts_per_token]
    pub expert_indices: Vec<Vec<Vec<u8>>>,

    /// Aggregated expert heat map (which experts are hot for this prefix)
    /// Used for pre-warming: experts with high counts should be pre-loaded
    pub expert_heat_map: ExpertHeatMap,

    /// Optional: Pre-computed "free tokens"
    /// Tokens that always follow this adapter's prefix
    pub precomputed_continuation: Option<PrecomputedTokens>,
}

/// Expert activation frequency for pre-warming
pub struct ExpertHeatMap {
    /// Per-layer activation counts: expert_id -> activation_count
    pub per_layer: Vec<HashMap<u8, u32>>,

    /// Top-K experts to pre-warm per layer
    pub hot_experts: Vec<Vec<u8>>,

    /// Confidence score (0.0-1.0): how predictable is routing?
    pub routing_stability: f32,
}

/// Pre-computed continuation tokens ("free tokens")
pub struct PrecomputedTokens {
    /// Token IDs that reliably follow this prefix
    pub token_ids: Vec<u32>,

    /// Already-computed logits for these tokens
    pub logits: Vec<Vec<f32>>,

    /// Probability that these tokens will be used (0.0-1.0)
    pub confidence: f32,

    /// Source: how were these tokens determined?
    pub source: FreeTokenSource,
}

#[derive(Debug, Clone)]
pub enum FreeTokenSource {
    /// Derived from adapter training data patterns
    AdapterTrainingData,
    /// Learned from runtime observation
    RuntimeLearned { sample_count: u32 },
    /// Explicitly specified in adapter manifest
    ManifestDeclared,
}
```

## "Free Token" Acquisition Strategies

### Strategy 1: Adapter Manifest Declaration (IMPLEMENTED)

Adapter authors declare common continuations in their manifest:

```yaml
# adapter.yaml
adapters:
  - id: python-docstring-helper
    hash: "..."
    tier: persistent
    rank: 16
    alpha: 32.0
    target_modules: ["q_proj", "v_proj"]
    # MoE optimization hints
    free_tokens:
      - trigger: "def "
        tokens: ["\n", "    ", '"""']
        confidence: 0.85
        max_temperature: 0.3
      - trigger: "class "
        tokens: [":", "\n", "    "]
        confidence: 0.80
    hot_experts:
      0: [5, 12, 23]    # Layer 0: experts 5, 12, 23 are hot
      1: [8, 10, 15]    # Layer 1: experts 8, 10, 15 are hot
```

**Rust API Usage:**

```rust
use adapteros_manifest::{Adapter, FreeTokenHint};

// Access free tokens from manifest
if let Some(hints) = &adapter.free_tokens {
    for hint in hints {
        if prompt.ends_with(&hint.trigger) && temperature <= hint.max_temperature {
            // Deliver free tokens immediately
            for token in &hint.tokens {
                on_token(StreamingToken {
                    token: token.clone(),
                    is_free: true,
                    ..
                });
            }
        }
    }
}
```

### Strategy 2: Training Data Analysis

During adapter creation, analyze training data for patterns:

```rust
/// Analyze adapter training data for common continuations
pub fn analyze_continuation_patterns(
    training_examples: &[TrainingExample],
    min_confidence: f32,
) -> Vec<ContinuationPattern> {
    // 1. Tokenize all examples
    // 2. Build prefix trie
    // 3. Identify high-frequency continuations
    // 4. Filter by confidence threshold
}
```

### Strategy 3: Runtime Learning

Track actual continuations and learn patterns:

```rust
/// Runtime pattern learner
pub struct ContinuationLearner {
    /// Observed continuations per adapter
    observations: HashMap<AdapterId, PrefixTrie>,

    /// Minimum samples before considering pattern stable
    min_samples: u32,

    /// Minimum frequency for pattern to be considered
    min_frequency: f32,
}

impl ContinuationLearner {
    pub fn observe(&mut self, adapter_id: AdapterId, prompt: &[u32], completion: &[u32]) {
        // Update trie with observation
    }

    pub fn get_predictions(&self, adapter_id: &AdapterId, prompt: &[u32]) -> Option<&[u32]> {
        // Return high-confidence continuations
    }
}
```

## Expert Pre-Warming Protocol

```rust
/// Pre-warm experts based on adapter routing patterns
pub async fn prewarm_experts(
    model: &MoEModel,
    adapter: &AdapterId,
    heat_map: &ExpertHeatMap,
) -> Result<()> {
    // 1. Identify hot experts from heat map
    let hot_experts: Vec<(LayerIdx, ExpertIdx)> = heat_map
        .hot_experts
        .iter()
        .enumerate()
        .flat_map(|(layer, experts)| {
            experts.iter().map(move |e| (layer, *e))
        })
        .collect();

    // 2. Pre-load expert weights into GPU memory
    for (layer, expert) in hot_experts {
        model.prefetch_expert(layer, expert).await?;
    }

    // 3. Optionally: pre-compute routing for known prefix
    if let Some(prefix_tokens) = adapter_static_prefix(adapter) {
        let routing = model.compute_routing(prefix_tokens)?;
        cache_routing(adapter, routing);
    }

    Ok(())
}
```

## Integration with Streaming

The streaming infrastructure we just built enables incremental free token delivery:

```rust
impl MlxSubprocessBridge {
    pub fn generate_with_free_tokens<F>(
        &self,
        adapter_id: &AdapterId,
        prompt: &str,
        max_tokens: usize,
        mut on_token: F,
    ) -> Result<StreamingResult>
    where
        F: FnMut(StreamingToken) -> bool,
    {
        // 1. Check for precomputed free tokens
        if let Some(free) = self.free_token_cache.get(adapter_id, prompt) {
            // Deliver free tokens immediately (no model call)
            for (i, token) in free.tokens.iter().enumerate() {
                let should_continue = on_token(StreamingToken {
                    token: token.text.clone(),
                    index: i,
                    token_id: Some(token.id),
                    is_free: true,  // New field: indicates pre-computed
                });
                if !should_continue {
                    return Ok(StreamingResult { /* ... */ });
                }
            }

            // 2. Continue with normal generation from where free tokens ended
            let remaining = max_tokens.saturating_sub(free.tokens.len());
            if remaining > 0 {
                self.generate_stream_continuation(
                    prompt,
                    &free.tokens,
                    remaining,
                    on_token,
                )
            }
        } else {
            // No free tokens, normal generation
            self.generate_stream(prompt, max_tokens, on_token)
        }
    }
}
```

## Metrics and Validation

```rust
/// Metrics for free token optimization
pub struct FreeTokenMetrics {
    /// Total free tokens delivered
    pub tokens_delivered: u64,

    /// Free tokens that matched actual model output (validation)
    pub tokens_validated: u64,

    /// Free tokens rejected (model would have produced different)
    pub tokens_rejected: u64,

    /// Latency saved (estimated)
    pub latency_saved_ms: f64,

    /// Per-adapter accuracy
    pub per_adapter_accuracy: HashMap<AdapterId, f32>,
}
```

## Risk Analysis

### Risk 1: Prediction Mismatch

Free tokens may not match what the model would actually produce.

**Mitigation**:
- Track validation rate per adapter
- Disable free tokens if accuracy drops below threshold (e.g., 90%)
- Always validate first few free tokens against actual model output

### Risk 2: Temperature Sensitivity

Higher temperatures increase output variance, reducing prediction accuracy.

**Mitigation**:
- Only enable free tokens for temperature < 0.3
- Reduce free token count proportionally to temperature

### Risk 3: Context Sensitivity

Free tokens may be valid for some contexts but not others.

**Mitigation**:
- Include recent context hash in cache key
- Limit free tokens to first 1-3 tokens
- Use confidence thresholds

## Implementation Phases

### Phase 1: Expert Heat Map Collection (Research)
- Instrument MoE routing to collect expert activation patterns
- Build per-adapter heat maps during inference
- Analyze routing stability across requests

### Phase 2: Expert Pre-Warming (MVP)
- Implement `prewarm_experts()` based on heat maps
- Add adapter manifest support for explicit expert hints
- Measure cold-start improvement

### Phase 3: Free Token Infrastructure (Experimental)
- Add `precomputed_continuation` field to cache
- Implement manifest-based free token declaration
- Add streaming integration with `is_free` flag

### Phase 4: Runtime Learning (Future)
- Implement `ContinuationLearner` for pattern detection
- Add validation and accuracy tracking
- Auto-disable for low-accuracy adapters

## Questions for Further Research

1. **Routing Stability**: How stable are expert routing patterns for a given adapter? Does the same adapter always activate similar experts?

2. **Cross-Adapter Interference**: If we pre-warm experts for adapter A, does it hurt adapter B?

3. **Memory Trade-offs**: How much GPU memory is needed for pre-warming vs. benefit gained?

4. **Token Horizon**: How many free tokens can we reliably predict? 1? 3? More?

5. **Temperature Correlation**: At what temperature does prediction accuracy fall off?

## Related Work

- **Speculative Decoding**: Uses draft model to propose tokens, main model validates
- **Medusa Heads**: Multiple prediction heads for parallel token generation
- **Prompt Caching**: Anthropic's approach to caching prompt prefixes (KV only)

The "free token" concept combines aspects of speculative decoding (token prediction) with prompt caching (reuse of computation) in an MoE-specific way.

---

## Related Documentation

- [**TOKEN_CACHING_ECONOMICS.md**](../TOKEN_CACHING_ECONOMICS.md) — Economics of prefix caching and "free tokens"
- [**CRYPTO_RECEIPTS.md**](../CRYPTO_RECEIPTS.md) — Cryptographic receipt structure

---

## Next Steps

1. **Instrument routing**: Add telemetry to collect expert activation patterns
2. **Build heat maps**: Aggregate patterns per adapter over multiple requests
3. **Measure stability**: Quantify routing predictability
4. **Prototype pre-warming**: Implement basic expert prefetch
5. **Evaluate**: Measure TTFT improvement with pre-warming
