# Routing Decision Training Data

**Purpose:** Train adapters to learn K-sparse router gate distributions and selection patterns

## Overview

This category contains training data for the LoRA router's decision-making process. The router uses Q15-quantized gate scores to select the top-K adapters for each inference step.

## Key Concepts

- **Gate Scores:** Raw floating-point scores computed per adapter
- **Q15 Quantization:** 16-bit fixed-point representation (range: -1.0 to 0.9997)
- **Top-K Selection:** Selecting K adapters with highest gate values
- **Entropy Floor (ε):** Minimum Shannon entropy threshold (default: 0.01)
- **Temperature (τ):** Softmax temperature for gate normalization

## Training Example Schema

```jsonl
{
  "input": {
    "step": 5,
    "token_id": 42,
    "num_adapters": 16,
    "k": 3
  },
  "target": {
    "selected_indices": [0, 4, 7],
    "gate_scores": [0.512, 0.301, 0.187],
    "entropy": 0.75,
    "tau": 0.1
  },
  "metadata": {
    "session_id": "sess-123",
    "quality": 0.95,
    "label": "positive"
  }
}
```

## Quality Criteria

- **Min Examples:** 1000
- **Min Relevance:** 0.90
- **Min Confidence:** 0.95
- **Entropy Range:** 0.01 to 3.0
- **Gate Score Range:** 0.0 to 1.0

## Data Sources

1. **Production Telemetry:** `routing_decisions` table
2. **Router Trace Generation:** `tests/router_trace_generation.rs`
3. **Synthetic Patterns:** K-sparse tie-breaking scenarios
4. **Replay Sessions:** Deterministic router replays

## Example Datasets

- `top_k_selection/` - Basic top-K selection patterns
- `entropy_thresholds/` - Entropy floor enforcement examples
- `tie_breaking/` - Equal gate score resolution
- `temperature_tuning/` - Temperature parameter effects
- `q15_quantization/` - Quantization rounding patterns

## References

- `crates/adapteros-lora-router/` - Router implementation
- `crates/adapteros-db/src/routing_decisions.rs` - Decision storage
- `migrations/0070_routing_decisions.sql` - Schema definition
