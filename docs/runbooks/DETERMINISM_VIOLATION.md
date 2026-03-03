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
curl -s http://localhost:18080/v1/diagnostics/determinism-status | jq .

# Run determinism tests
cargo test -p adapteros-lora-router --test determinism
cargo test --test determinism_core_suite

# Verify Q15 denominator (must be 32767.0)
grep -n "32767" crates/adapteros-lora-router/src/quantization.rs

# Check for -ffast-math (must be absent)
./scripts/check_fast_math_flags.sh
```

`/v1/diagnostics/determinism-status` freshness contract:
- `freshness_status=fresh`: latest determinism check is within `stale_after_seconds`.
- `freshness_status=stale`: latest check exists but exceeded `stale_after_seconds`.
- `freshness_status=unknown`: determinism status cannot be trusted (missing check state, invalid timestamp, or query failure).
- `freshness_reason` is machine-readable and should be used by automation for escalation.
- If status is `stale` or `unknown`, run `./aosctl diag run` immediately and investigate persistence/DB health.

---

## Resolution

1. Quarantine affected adapter(s)
2. Preserve forensics: `var/forensics/`
3. Page security team
4. See [DETERMINISM.md](../DETERMINISM.md) for invariants
5. For strict determinism boots, verify MLX build/runtime versions match:
   - Build/runtime mismatch is boot-fatal when `AOS_ENFORCE_MLX_VERSION_MATCH=1`
   - Failure output includes `build_version`, `runtime_version`, and remediation guidance

---

## Determinism Envelope Triage

- `receipt-attested` scope:
  - `*_digest_b3`, token/billing counters, and Q15 decoder/stop fields in receipt digests.
- `unquantized` scope:
  - MLX runtime/kernel implementation details and pre-quantization logits/scores.
- Mitigation rule:
  - Treat any strict-mode MLX runtime/build mismatch as an immediate blocking incident.

---

## Invariants

- Q15 denominator: 32767.0
- Router tie-break: score DESC, stable_id ASC
- Seed: HKDF-SHA256 via `derive_seed`
- No `-ffast-math`
