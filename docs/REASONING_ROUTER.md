# Reasoning Router Architecture

**Copyright:** (c) 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2026-02-04
**Purpose:** Document the semantic reasoning router for mid-flight adapter swaps

---

## Overview

The **Reasoning Router** is a lightweight semantic routing system that monitors streaming inference output and decides when to swap adapters mid-flight based on thought boundaries. It is **not** a dedicated CoreML prefill step for chain-of-thought processing.

### What It Is

- A **semantic router** for dynamic adapter swaps during streaming inference
- Uses TinyBERT embeddings (optionally ANE-accelerated) to analyze thought segments
- Detects `<thinking>` tokens or newlines as thought boundaries
- Scores potential adapter transitions based on semantic similarity + topology priors
- Emits hot-swap decisions with debounce and shadow-mode support

### What It Is Not

- NOT a dedicated CoreML prefill step
- NOT a separate chain-of-thought reasoning engine
- NOT a replacement for the standard K-sparse LoRA router
- NOT performing additional model inference passes

---

## Architecture

### Components

```
┌─────────────────────────────────────────────────────────────────┐
│                     StreamInspector                              │
│  - Buffers streamed tokens                                       │
│  - Detects thought boundaries (<thinking>, newlines)             │
│  - Triggers embedding + scoring at boundaries                    │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Embedder                                   │
│  ┌──────────────────────┐  ┌──────────────────────────────────┐ │
│  │ Hashed (fallback)    │  │ TinyBERT (primary)               │ │
│  │ - BLAKE3 projection  │  │ - ANE-pinned CoreML model        │ │
│  │ - 32-dim quantized   │  │ - Semantic understanding         │ │
│  │ - No external deps   │  │ - ~128-384 dim output            │ │
│  └──────────────────────┘  └──────────────────────────────────┘ │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                   ReasoningScorer                                │
│  - Adapter cluster centroids (embedded adapter IDs)              │
│  - Topology priors (transition probabilities)                    │
│  - Blended score: 70% semantic + 30% topology                    │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                   HotSwapDecision                                │
│  - Target adapter ID                                             │
│  - Confidence score                                              │
│  - Shadow mode flag (log-only vs. actual swap)                   │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Token Arrival**: Streamed tokens arrive from inference engine
2. **Buffer Accumulation**: Tokens accumulate in rolling buffer (default: 1024 chars)
3. **Boundary Detection**: Inspector checks for `<thinking>` or newline tokens
4. **Embedding**: At boundary, thought segment is embedded via TinyBERT or hash projection
5. **Scoring**: Thought vector compared to adapter cluster centroids
6. **Decision**: If confidence > threshold (default: 0.82) and debounce window clear, emit swap

---

## Configuration

### ReasoningRouterConfig

| Parameter | Default | Description |
|-----------|---------|-------------|
| `confidence_threshold` | 0.82 | Minimum combined score to trigger swap |
| `debounce_tokens` | 50 | Minimum tokens between swaps |
| `shadow_mode` | false | Log decisions without executing swaps |
| `thinking_token` | `<thinking>` | Explicit reasoning boundary marker |
| `analysis_window` | 1024 | Rolling buffer size (characters) |
| `embedder_type` | Hashed | `Hashed` or `TinyBert` |
| `model_path` | (auto) | Path to TinyBERT `.mlpackage` |

### Embedder Selection

```rust
// Default: Hash-based projection (no external dependencies)
let config = ReasoningRouterConfig::default();

// Production: TinyBERT on ANE
let config = ReasoningRouterConfig {
    embedder_type: EmbedderType::TinyBert,
    model_path: Some("/var/models/tiny-bert-4bit-ane.mlpackage".into()),
    ..Default::default()
};
```

---

## Scoring Algorithm

### Combined Score Formula

```
combined_score = (semantic_weight * semantic_similarity) + (topology_weight * topology_prior)
```

Default weights: 70% semantic, 30% topology

### Semantic Similarity

Cosine similarity between thought embedding and adapter cluster centroid:

```rust
semantic = cosine_similarity(thought_vector, adapter_centroid)
```

### Topology Prior

Historical transition probability between adapter clusters:

```rust
topology = transitions.get((from_cluster, to_cluster)).unwrap_or(default_prob)
```

Default probability: 0.5 (neutral)

### Transition Acceptance

A transition is accepted when:
1. `combined_score >= confidence_threshold`
2. `tokens_since_last_swap >= debounce_tokens`
3. Target adapter differs from current adapter

### Confidence Threshold Math

Understanding the relationship between threshold and minimum semantic similarity:

```
combined_score = (semantic_weight × semantic) + (topology_weight × topology_prior)
```

Solving for minimum semantic similarity at the threshold boundary:

```
min_semantic = (threshold - topology_weight × topology_prior) / semantic_weight
```

With default weights (70% semantic, 30% topology) and default topology prior (0.5):

| Threshold | Min Semantic Similarity Required |
|-----------|----------------------------------|
| 0.82      | 0.957 (very strict)              |
| 0.75      | 0.857                            |
| 0.70      | 0.786                            |
| 0.65      | 0.714                            |
| 0.60      | 0.643                            |

**Note:** The default threshold (0.82) requires near-perfect semantic alignment to trigger. Consider lowering to 0.70-0.75 for more responsive routing, or use shadow mode to observe behavior before tuning.

---

## UI Integration

### "Reasoning Mode" Toggle

The UI's "Reasoning Mode" toggle in the chat interface enables this system. When enabled:

1. Inference requests include `reasoning_mode: true`
2. Backend preference is set to CoreML for ANE acceleration
3. The reasoning router monitors the inference stream
4. Mid-flight adapter swaps occur based on thought boundaries

**Important clarification:** The "Reasoning Mode" toggle does **not** enable a dedicated prefill step or chain-of-thought processing. It enables the semantic routing system that can swap adapters during streaming output based on detected thought patterns.

### Tooltip Text

Current (accurate):
> "Enable reasoning mode (mid-stream swaps to specialist adapters; routes to CoreML backend)"

---

## Relationship to CoreML Backend

### What CoreML Provides

The CoreML backend preference when reasoning mode is enabled serves two purposes:

1. **ANE-accelerated TinyBERT**: The reasoning router's embedder runs on ANE for fast semantic analysis
2. **Primary inference backend**: Main inference uses CoreML for ANE determinism

### What CoreML Does NOT Provide

- No separate "reasoning prefill" pass
- No dedicated chain-of-thought processing pipeline
- No additional inference calls for reasoning

The CoreML backend selection is about hardware acceleration for the semantic embedder and primary inference, not about enabling fundamentally different reasoning behavior.

---

## Implementation Reference

### Key Files

| File | Purpose |
|------|---------|
| `crates/adapteros-lora-worker/src/reasoning_router.rs` | Core reasoning router implementation |
| `crates/adapteros-lora-worker/src/ane_embedder.rs` | TinyBERT ANE embedder |
| `crates/adapteros-ui/src/signals/chat.rs` | UI state including reasoning mode |
| `crates/adapteros-ui/src/components/chat_dock.rs` | Reasoning mode toggle UI |

### Key Types

```rust
// Configuration
pub struct ReasoningRouterConfig { ... }
pub enum EmbedderType { Hashed, TinyBert }

// Scoring
pub struct ReasoningScorer { clusters, topology, weights }
pub struct TransitionScore { target, confidence, semantic, topology }

// Output
pub struct ThoughtTransition { from, to, thought, confidence, token_index }
pub struct HotSwapDecision { transition, shadow_mode }

// Stream Processing
pub struct StreamInspector { buffer, scorer, embedder, config, current_cluster }
```

---

## Comparison: Reasoning Router vs. K-Sparse Router

| Aspect | K-Sparse Router | Reasoning Router |
|--------|-----------------|------------------|
| **Timing** | Before inference | During streaming inference |
| **Input** | User prompt | Generated token stream |
| **Output** | Initial adapter selection | Mid-flight adapter swap decisions |
| **Latency** | Blocking (pre-inference) | Non-blocking (monitors stream) |
| **Adapters** | Selects top-K adapters | Swaps single active adapter |
| **Purpose** | Initial routing | Dynamic re-routing based on output |

The two systems are complementary:
- K-sparse router selects the initial adapter set
- Reasoning router can swap within that set (or beyond) based on generated content

---

## Shadow Mode

Shadow mode allows observing reasoning decisions without executing swaps:

```rust
let config = ReasoningRouterConfig {
    shadow_mode: true,
    ..Default::default()
};
```

In shadow mode:
- All scoring and decision logic runs normally
- Decisions are logged but not executed
- Current cluster is not updated
- Useful for tuning thresholds and observing behavior

---

## Performance Considerations

### Embedding Cost

| Embedder | Latency | Memory | Quality |
|----------|---------|--------|---------|
| Hashed | ~50μs | ~4KB | Low (hash projection) |
| TinyBERT (ANE) | ~2ms | ~10MB | High (semantic understanding) |

### When to Use Each

- **Hashed**: Development, testing, or when TinyBERT unavailable
- **TinyBERT**: Production deployments on Apple Silicon with ANE

### Fallback Behavior

If TinyBERT loading fails (model not found, CoreML unavailable), the system automatically falls back to the hashed embedder with a warning log.

---

## See Also

- [BACKEND_ARCHITECTURE.md](BACKEND_ARCHITECTURE.md) - Multi-backend architecture
- [COREML_BACKEND.md](COREML_BACKEND.md) - CoreML ANE acceleration details
- [GLOSSARY.md](GLOSSARY.md) - Term definitions including Router, K-sparse
- [docs/ui/data-flow.md](ui/data-flow.md) - UI data flow including chat state

---

**Signed:** Documentation Team
**Date:** 2026-02-04
**Status:** Architectural Reference
