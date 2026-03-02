# Determinism Violation

Replay mismatch or hash divergence. SEV-1.

---

## Symptoms

- Replay produces different output
- Hash verification fails
- Audit chain broken

---

## Diagnosis

```bash
# Check determinism diagnostics
./aosctl diag run
curl -s http://localhost:8080/v1/diagnostics/determinism | jq .

# Run determinism tests
cargo test -p adapteros-lora-router --test determinism
cargo test --test determinism_core_suite

# Verify Q15 denominator (must be 32767.0)
grep -n "32767" crates/adapteros-lora-router/src/quantization.rs

# Check for -ffast-math (must be absent)
./scripts/check_fast_math_flags.sh
```

---

## Resolution

1. Quarantine affected adapter(s)
2. Preserve forensics: `var/forensics/`
3. Page security team
4. See [DETERMINISM.md](../DETERMINISM.md) for invariants

---

## Invariants

- Q15 denominator: 32767.0
- Router tie-break: score DESC, stable_id ASC
- Seed: HKDF-SHA256 via `derive_seed`
- No `-ffast-math`
