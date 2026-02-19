# adapterOS Determinism

Deterministic execution and reproducibility in adapterOS.

---

## Guarantees

- **Bit-exact reproducibility** — Same inputs + same state → same outputs (per backend)
- **Deterministic routing** — K-sparse adapter selection with fixed tie-breaking (score DESC, stable_id ASC)
- **Seed isolation** — HKDF domain separation; no RNG reuse across requests
- **Replay support** — Past inferences reconstructible with evidence

---

## Key Mechanisms

### Seed Derivation

- Global seed from BLAKE3 of manifest/config
- HKDF-SHA256 for domain separation
- Canonical reference: `crates/adapteros-core/src/seed.rs`

### Router Determinism

- Q15 quantization denominator: 32767.0
- Tie-break: score DESC, stable_id ASC
- Canonical reference: `crates/adapteros-lora-router/src/quantization.rs`

### No Fast-Math

- No `-ffast-math` compiler flags (CI enforces)

---

## Verification

```bash
cargo test --test determinism_core_suite
cargo test -p adapteros-lora-router --test determinism
cargo test -p adapteros-server-api --test replay_determinism_tests
```

Set `AOS_DEBUG_DETERMINISM=1` to log seed inputs and router details.

---

## See Also

- [docs/DETERMINISM.md](docs/DETERMINISM.md) — Full determinism documentation
- [docs/API_REFERENCE.md](docs/API_REFERENCE.md) — Replay endpoint (`/v1/adapteros/replay`)
