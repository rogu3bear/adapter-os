# Reasoning Router Architecture

## Overview

The **Reasoning Router** is a semantic routing system for mid-flight adapter swaps during streaming inference. It is **NOT** a dedicated CoreML prefill step or chain-of-thought engine.

**Key distinction:** The K-sparse router selects adapters *before* inference; the reasoning router can swap adapters *during* streaming output based on thought boundaries.

## Module Location

`crates/adapteros-lora-worker/src/reasoning_router.rs`

## What It Does

1. **Monitors token stream** via `StreamInspector`
2. **Detects thought boundaries** (`<thinking>` tokens, newlines)
3. **Embeds thought segments** using TinyBERT (ANE) or hash projection (fallback)
4. **Scores adapter transitions** combining:
   - 70% semantic similarity (cosine between thought vector and adapter centroid)
   - 30% topology prior (historical transition probability)
5. **Emits hot-swap decisions** when confidence > threshold (0.82) and debounce satisfied

## What It Does NOT Do

- NOT a dedicated CoreML prefill step
- NOT a separate chain-of-thought reasoning engine
- NOT a replacement for K-sparse router (it complements it)
- NOT performing additional model inference passes

## Key Components

### StreamInspector
- Buffers streamed tokens (rolling 1024-char window)
- Calls `is_boundary_token()` on each token
- On boundary: embeds accumulated text, scores transition, optionally emits swap

### Embedder (Enum)
- `Hashed(FastEmbedder)` - BLAKE3 projection, 32-dim, no deps
- `TinyBert(TinyBertEmbedder)` - ANE-pinned CoreML model, semantic understanding

### ReasoningScorer
- `clusters: HashMap<String, Vec<f32>>` - Adapter ID embeddings
- `topology: TopologyPrior` - Transition probability matrix
- `semantic_weight: 0.7, topology_weight: 0.3`

### HotSwapDecision
- `transition: ThoughtTransition` - From/to adapters, confidence, thought text
- `shadow_mode: bool` - Log-only vs. actual swap

## Configuration

```rust
ReasoningRouterConfig {
    confidence_threshold: 0.82,
    debounce_tokens: 50,
    shadow_mode: false,
    thinking_token: "<thinking>".to_string(),
    analysis_window: 1024,
    embedder_type: EmbedderType::Hashed, // or TinyBert
    model_path: None, // Auto-detects var/models/tiny-bert-4bit-ane.mlpackage
}
```

## UI "Reasoning Mode" Toggle

When enabled in the chat UI:
1. Sets `reasoning_mode: true` on inference request
2. Sets `backend: "coreml"` for ANE-accelerated embedder
3. Enables semantic routing during generation

**Important:** The toggle does NOT enable a dedicated prefill step. It enables the semantic router that can swap adapters mid-stream.

## ANE Embedder

`crates/adapteros-lora-worker/src/ane_embedder.rs`

- Loads TinyBERT `.mlpackage` pinned to `CpuAndNeuralEngine`
- Falls back to hash projection if CoreML unavailable
- Memory tracked via `ffi::record_model_load()`

## Documentation

Full documentation: `docs/REASONING_ROUTER.md`
Glossary entry: `docs/GLOSSARY.md` (under "Reasoning Router")
