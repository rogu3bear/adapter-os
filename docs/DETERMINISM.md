# DETERMINISM

Reproducible inference. Source: `adapteros-core/seed.rs`, `adapteros-lora-router/quantization.rs`.

---

## Seed Derivation

From `adapteros-core/src/seed.rs`:

```mermaid
flowchart TB
    subgraph Input["Inputs"]
        MH["Manifest Hash (B3Hash)<br/>or fallback hash"]
    end

    subgraph Derive["derive_seed()"]
        HKDF["HKDF-SHA256<br/>label = domain"]
    end

    subgraph Output["Derived Seeds"]
        R["router<br/>Tie-breaking in K-sparse"]
        S["sampling<br/>Temperature, top-p"]
        D["dropout<br/>Training mask"]
    end

    MH --> HKDF
    HKDF --> R
    HKDF --> S
    HKDF --> D
```

**Function:** `adapteros_core::seed::derive_seed(hash: &B3Hash, label: &str) -> [u8; 32]`

**Labels:** `SeedLabel::Router`, `SeedLabel::Sampling`, `SeedLabel::Dropout` (see `derive_seed_typed`).

---

## Router Determinism

K-sparse router tie-breaking. Source: `adapteros-lora-router`.

```mermaid
flowchart LR
    subgraph Input["Adapter Scores"]
        A["Adapter A: 0.85"]
        B["Adapter B: 0.85"]
        C["Adapter C: 0.80"]
    end

    subgraph Sort["sort_by"]
        S1["score DESC"]
        S2["stable_id ASC"]
    end

    subgraph Output["Order"]
        O["B before A (stable_id)<br/>then C"]
    end

    A --> S1
    B --> S1
    C --> S1
    S1 --> S2 --> O
```

**Invariant:** Tie-break must be `(score DESC, stable_id ASC)`. No `sort_unstable_by` without tie-breaker.

---

## Q15 Quantization

Gate values quantized for deterministic routing. Source: `adapteros-lora-router/src/quantization.rs`.

| Rule | Value | Rationale |
|------|-------|-----------|
| Q15 denominator | **32767.0** | Must be 32767, NOT 32768 |
| Quantization | `(gate * 32767.0).round() as i16` | Fixed-point representation |

**Verification:** `grep -n "32767" crates/adapteros-lora-router/src/quantization.rs`

---

## Modes

From `adapteros_core::SeedMode`:

| Mode | Behavior | Use Case |
|------|----------|----------|
| Strict | Requires manifest hash; fails if missing | Production inference |
| BestEffort | Uses manifest when present; fallback hash | Dev/testing |
| NonDeterministic | Random seed (non-replayable) | Benchmarking only |

**Config:** `[general] determinism_mode = "besteffort"` in cp.toml.

---

## DeterminismConfig

For replay and testing. Source: `adapteros_core::seed::DeterminismConfig`.

```rust
// Fixed seed and timestamp for replay
DeterminismConfig::builder()
    .fixed_seed(12345)
    .fixed_timestamp(...)
    .stable_ordering(true)
    .build();
```

---

## Verification

```bash
cargo test --test determinism_core_suite
cargo test -p adapteros-lora-router --test determinism
cargo test -p adapteros-server-api --test replay_determinism_tests
./scripts/check_fast_math_flags.sh
```

Set `AOS_DEBUG_DETERMINISM=1` for seed logging.
