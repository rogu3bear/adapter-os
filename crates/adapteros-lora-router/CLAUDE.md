# adapteros-lora-router

K-sparse LoRA adapter routing with deterministic scoring.

## Determinism Invariants

- **Q15 quantization**: Gates quantized via `(gate * 32767.0).round() as i16`
- **Tie-breaking**: score DESC, then stable_id ASC (stable sort)
- **Epsilon**: `1e-9` for floating-point comparison
- **No hashmaps** in hot path - iteration order must be deterministic

## Testing

```bash
cargo test -p adapteros-lora-router --test determinism  # Verify reproducibility
```
