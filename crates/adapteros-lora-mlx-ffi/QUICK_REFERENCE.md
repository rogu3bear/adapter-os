# Sampling Strategy - Quick Reference Card

## Strategy Selection

| Temperature | Strategy | Mode | Use Case |
|-------------|----------|------|----------|
| `0.0` | Greedy | Deterministic | Testing, reproducibility |
| `0.0 < T < 1.0` | Stochastic | Focused | Formal writing, logic |
| `1.0` | Stochastic | Baseline | Default, general purpose |
| `1.0 < T < 2.0` | Stochastic | Creative | Creative writing, brainstorm |
| `T >= 2.0` | Stochastic | Very random | Exploration, experimentation |

## Configuration Examples

### Greedy (Deterministic)
```rust
GenerationConfig {
    temperature: 0.0,
    ..Default::default()
}
```
Output: Always identical, 100% reproducible

### Default (Stochastic)
```rust
GenerationConfig::default()  // temperature: 1.0
```
Output: Reproducible with same seed, naturally varied

### Top-K Sampling
```rust
GenerationConfig {
    temperature: 0.8,
    top_k: Some(40),
    ..Default::default()
}
```
Output: Safe, prevents rare tokens

### Top-P Nucleus Sampling
```rust
GenerationConfig {
    temperature: 0.8,
    top_p: Some(0.95),
    ..Default::default()
}
```
Output: Adaptive to context

### Production (Recommended)
```rust
GenerationConfig {
    temperature: 0.8,
    top_k: Some(50),
    top_p: Some(0.95),
    repetition_penalty: 1.1,
    ..Default::default()
}
```
Output: High quality, diverse, safe

## Pipeline Visualization

```
Input Tokens → Model Forward Pass → Logits
                                      ↓
                          Repetition Penalty (if > 1.0)
                                      ↓
                          Temperature Scaling
                                      ↓
                            Softmax (log-stable)
                                      ↓
                        Top-K Filtering (if set)
                                      ↓
                        Top-P Filtering (if set)
                                      ↓
                          Strategy Decision
                            /            \
                        Greedy          Stochastic
                      (T = 0.0)         (T > 0.0)
                        ↓                  ↓
                      Argmax            Random Sample
                        ↓                  ↓
                      Token ID ←──────────┘
```

## Code Patterns

### Usage Pattern
```rust
use adapteros_lora_mlx_ffi::{MLXFFIModel, GenerationConfig, MLXGenerator};

let config = GenerationConfig {
    temperature: 0.7,
    top_p: Some(0.95),
    max_tokens: 100,
    ..Default::default()
};

let mut gen = MLXGenerator::new(model.model_hash, config);
let output = gen.generate(&model, input_tokens)?;
```

### Checking Strategy
```rust
let strategy = config.sampling_strategy();
match strategy {
    SamplingStrategy::Greedy => println!("Deterministic mode"),
    SamplingStrategy::Stochastic => println!("Random mode"),
}
```

### Testing Strategy
```rust
let greedy = GenerationConfig {
    temperature: 0.0,  // Automatic greedy
    ..Default::default()
};

let stochastic = GenerationConfig {
    temperature: 0.5,  // Automatic stochastic
    ..Default::default()
};

assert_eq!(greedy.sampling_strategy(), SamplingStrategy::Greedy);
assert_eq!(stochastic.sampling_strategy(), SamplingStrategy::Stochastic);
```

## Temperature Guide

```
    0.0 ┌─── Greedy (deterministic)
        │
    0.5 ├─── Cold (focused, formal)
        │
    1.0 ├─── Baseline (balanced)
        │
    1.5 ├─── Warm (creative)
        │
    2.0 └─── Hot (very random)
```

## Functions Summary

| Function | Input | Output | Notes |
|----------|-------|--------|-------|
| `sample_greedy()` | Probabilities | Token ID | Deterministic argmax |
| `sample_from_distribution()` | Probabilities | Token ID | HKDF-seeded random |
| `apply_top_k()` | Probabilities, K | Filtered probs | Keep top K only |
| `apply_top_p()` | Probabilities, P | Filtered probs | Cumsum threshold |
| `softmax()` | Logits | Probabilities | Normalization |
| `apply_temperature()` | Logits, T | Scaled logits | Divide by T |

## Determinism Guarantee

### Greedy (temperature = 0.0)
```
Same config + Same model + Same tokens → Same output (100% guaranteed)
```

### Stochastic (temperature > 0.0)
```
Same config + Same model + Same tokens + Same HKDF seed → Same output (guaranteed)
```

## Performance

| Mode | Time | Speedup | Use |
|------|------|---------|-----|
| Greedy only | Fast | 1x baseline | Testing |
| Stochastic only | Baseline | 1x | Production |
| + Top-K | +5-10% | 0.9-0.95x | Safe generation |
| + Top-P | +5-10% | 0.9-0.95x | Adaptive |
| + Both | +10-15% | 0.85-0.9x | High quality |

## Common Mistakes to Avoid

### ❌ Wrong: Ignoring temperature
```rust
let config = GenerationConfig::default();
// Will be stochastic (temperature: 1.0)
```

### ✓ Right: Explicit temperature
```rust
let config = GenerationConfig {
    temperature: 0.0,  // Explicitly greedy
    ..Default::default()
};
```

### ❌ Wrong: Top-K without temperature
```rust
let config = GenerationConfig {
    top_k: Some(40),
    // temperature defaults to 1.0 (stochastic)
    ..Default::default()
};
```

### ✓ Right: Combined settings
```rust
let config = GenerationConfig {
    temperature: 0.8,
    top_k: Some(40),
    top_p: Some(0.95),
    ..Default::default()
};
```

## Testing Checklist

- [ ] Test greedy mode: `temperature: 0.0`
- [ ] Test stochastic mode: `temperature: 0.7`
- [ ] Test top-K filtering: `top_k: Some(40)`
- [ ] Test top-P filtering: `top_p: Some(0.95)`
- [ ] Test combined filters
- [ ] Test reproducibility with same seed
- [ ] Test different models produce different outputs
- [ ] Test edge cases (empty probs, uniform dist)

## Integration Checklist

- [ ] Import: `use adapteros_lora_mlx_ffi::SamplingStrategy;`
- [ ] Create config with desired temperature
- [ ] Create generator: `MLXGenerator::new(model_hash, config)`
- [ ] Generate: `gen.generate(&model, tokens)?`
- [ ] Check results: reproducible if same seed

## Related Documentation

- **Full Architecture:** `SAMPLING_STRATEGY.md`
- **Implementation Details:** `SAMPLING_IMPLEMENTATION_SUMMARY.md`
- **Complete Summary:** `STOCHASTIC_SAMPLING_FINAL_SUMMARY.md`
- **Code Reference:** `src/generation.rs` (Lines 16-887)

## File Locations

- Main code: `/Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi/src/generation.rs`
- Exports: `/Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi/src/lib.rs:26`
- Tests: `src/generation.rs` Lines 564-887

---

**Last Updated:** 2025-11-19
**Status:** Production Ready ✓
