# LoRA Routing Patterns

## Core Module: `crates/adapteros-lora-router/`

### Key Files
- `router.rs` - Main `Router` struct and routing logic
- `quantization.rs` - Q15 gate quantization
- `scoring.rs` - Scoring functions (`ScoringFunction` trait)
- `types.rs` - Core types (`Decision`, `DecisionCandidate`, `FeatureVector`)
- `policy_mask.rs` - Policy-based adapter filtering
- `layer_routing.rs` - Per-layer routing for deep models

---

## K-Sparse Routing Algorithm

### Entry Point
`Router::route()` and variants (`route_with_adapter_info`, `route_with_backend_context`)

### Flow
1. Score adapters via `compute_weighted_score()` using `RouterWeights`
2. Apply policy mask filtering via `PolicyMask::build()`
3. Quantize scores to Q15 via `quantize_gate()`
4. Sort deterministically via `sort_candidates_by_quantized_gate()`
5. Select top-K candidates
6. Return `Decision` with `indices`, `gates_q15`, `entropy`

### Tie-Breaking Rule (CRITICAL)
```rust
// router.rs:1265-1273 - sort_candidates_by_quantized_gate()
candidates.sort_by(|a, b| {
    b.gate_q15
        .cmp(&a.gate_q15)              // Score DESC
        .then_with(|| b.raw_score.total_cmp(&a.raw_score))
        .then_with(|| a.adapter_idx.cmp(&b.adapter_idx))  // Index ASC for ties
});
```

---

## Q15 Quantization

### Constants (`quantization.rs:3-21`)
```rust
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;  // NOT 32768 - critical for determinism
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;
pub const Q15_FORMAT_NAME: &str = "Q15";
```

### Why 32767?
- i16 max is 32767, using 32768 overflows
- Exact representation of 1.0 when gate=32767
- Consistent round-trips: f32→Q15→f32

### Quantize Function (`quantization.rs:169-173`)
```rust
pub(crate) fn quantize_gate(gate: f32) -> i16 {
    let scaled = (gate * ROUTER_GATE_Q15_DENOM).round() as i32;
    scaled.clamp(0, ROUTER_GATE_Q15_MAX as i32) as i16
}
```

### `GateQuantFormat` Struct
- `q15()` - Factory for standard Q15 format
- `encode()` / `decode()` - Convert between f32 and i16
- `validate()` - Verify format is Q15 with correct denominator

---

## Scoring System

### `ScoringFunction` Trait (`scoring.rs`)
- `name() -> &str`
- `score(features, adapters, config) -> Vec<f32>`

### Built-in Scorers
| Scorer | Purpose |
|--------|---------|
| `WeightedScorer` | Default weighted feature scoring |
| `EntropyFloorScorer` | Entropy-based soft gating |
| `AdapterAwareScorer` | Language/framework affinity |

### `RouterWeights` Fields (`types.rs`)
- `language_weight`, `framework_weight`
- `symbol_hits_weight`, `path_tokens_weight`
- `prompt_verb_weight`, `orthogonal_weight`
- `diversity_weight`, `similarity_penalty`

---

## Feature Vector

### Standard Length: 16 elements (`FEATURE_VECTOR_STANDARD_LEN`)
### Extended Length: 24 elements (`FEATURE_VECTOR_EXTENDED_LEN`)

### Key Accessors (`FeatureVector` impl)
- `language()` - Detected programming language
- `framework()`, `framework_strength()` - Framework detection
- `symbol_hits()` - Symbol match count
- `path_tokens()` - Path token matches
- `prompt_verb()`, `prompt_verb_strength()` - Intent detection
- `attn_entropy()` - Attention entropy (extended only)

---

## Adapter Tiers

### `AdapterTier` Enum
- `Tier0` - Highest priority (boost: `TIER_0_BOOST`)
- `Tier1` - Medium priority
- `Tier2` - Standard priority

### `LoraTier` Enum
- `Max` - Full LoRA capacity (boost: `LORA_TIER_MAX_BOOST`)
- `Standard` - Default tier
- `Micro` - Minimal capacity

---

## Policy Mask System (`policy_mask.rs`)

### `PolicyMask` Struct
- `allowed: Vec<bool>` - Per-adapter allow flags
- `digest: [u8; 32]` - BLAKE3 hash for audit
- `overrides_applied: usize`

### `PolicyOverrideFlags`
- `allow_list: Option<Vec<AdapterId>>`
- `deny_list: Option<Vec<AdapterId>>`
- `trust_state: TrustState`

---

## Layer Routing (`layer_routing.rs`)

For per-layer adapter routing in deep transformer models.

### `LayerType` Enum
- Attention, MLP, CrossAttention, etc.
- `is_routable()` - Whether layer can have per-layer adapters

### `LayerDecision` Struct
- `indices`, `gates_q15` - Same as top-level
- `is_active()`, `dominant_adapter()` - Layer state queries

### `LayerRoutingChain`
- Tracks decisions across all layers
- `compute_chain_hash()` - BLAKE3 hash of full chain

---

## Determinism Config

### `RouterDeterminismConfig` (`types.rs`)
- `ieee754_deterministic: bool` - Enforce IEEE 754 compliance
- `enable_decision_hashing: bool` - Compute decision hashes

### `DecisionHash` Struct
- `input_hash`, `output_hash`, `reasoning_hash`
- `combined_hash` - Final deterministic hash
- `tau`, `eps`, `k` - Router parameters at decision time

---

## Constants (`constants.rs`)

| Constant | Value | Purpose |
|----------|-------|---------|
| `TIER_0_BOOST` | - | Highest tier boost |
| `TIER_1_BOOST` | - | Medium tier boost |
| `TIER_2_BOOST` | - | Standard tier boost |
| `LANGUAGE_AFFINITY_MULTIPLIER` | - | Language match bonus |
| `FRAMEWORK_SPECIALIZATION_MULTIPLIER` | - | Framework match bonus |
| `TIE_BREAK_RELATIVE_EPSILON` | - | Tie-breaking threshold |

---

## Common Patterns

### 1. Creating a Router
```rust
let router = Router::new_with_weights(weights, k, tau, eps);
router.set_active_stack("my_stack", &adapter_ids);
```

### 2. Making Routing Decisions
```rust
let decision = router.route_with_adapter_info(&features, &adapters)?;
// decision.indices - selected adapter indices
// decision.gates_q15 - Q15-quantized weights
// decision.entropy - routing entropy
```

### 3. Applying Policy Mask
```rust
let mask = PolicyMask::build(&adapters, &overrides);
let filtered = filter_decision_by_policy(decision, &mask);
```
