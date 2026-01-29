# Determinism Invariants

## Core Module: `crates/adapteros-core/src/seed.rs`

### Purpose
All randomness in AdapterOS derives from a global seed using HKDF-SHA256, ensuring deterministic replay.

---

## HKDF Seed Derivation

### Constants
```rust
// seed.rs:97-106
pub const HKDF_ALGORITHM_VERSION: u32 = 2;
pub const HKDF_OUTPUT_LENGTH: usize = 32;  // Matches ChaCha20Rng seed size
```

### Core Function: `derive_seed()` (seed.rs:967-1032)
```rust
pub fn derive_seed(global: &B3Hash, label: &str) -> [u8; HKDF_OUTPUT_LENGTH]
```
- Uses HKDF-SHA256 (ONLY supported KDF - never substitute)
- `global` = BLAKE3 hash of global seed
- `label` = context-specific string (e.g., "router", "inference")
- Returns 32 bytes for ChaCha20Rng

### Seed Override
If `DeterminismConfig.fixed_seed` is set, it overrides the global seed for testing.

---

## SeedMode Enum (seed.rs:852-863)

| Mode | Behavior |
|------|----------|
| `Strict` | Requires manifest hash; fails if missing |
| `BestEffort` | Uses manifest when present; fallback otherwise |
| `NonDeterministic` | Dev-only random seed (non-replayable) |

### Default
`SeedMode::default()` returns `BestEffort` (see test `seed_mode_default_matches_central_default`).

---

## DeterminismConfig (seed.rs:462-504)

### Fields
| Field | Type | Purpose |
|-------|------|---------|
| `fixed_seed` | `Option<u64>` | Override all RNG with this seed |
| `fixed_timestamp` | `Option<DateTime<Utc>>` | Override all time operations |
| `stable_ordering` | `bool` | Force sorted/stable iteration |
| `strict_mode` | `bool` | Enable strict determinism validation |
| `trace_seeds` | `bool` | Log seed derivation details |

### Factory Methods
- `DeterminismConfig::fully_deterministic()` - All flags enabled
- `DeterminismConfig::for_replay()` - Replay-safe configuration
- `DeterminismConfig::builder()` - Builder pattern

### Global Access
```rust
get_determinism_config() -> DeterminismConfig
set_determinism_config(config)
with_determinism_config(config, closure)  // Scoped override
```

---

## Canonical Invariants (`crates/adapteros-core/src/invariants.rs`)

### Q15 Gate Denominator
```rust
// invariants.rs
pub const Q15_GATE_DENOMINATOR: f32 = 32767.0;
```
**CRITICAL**: DO NOT CHANGE TO 32768 - breaks replay compatibility.

### Q15 Encode/Decode
```rust
pub fn encode_q15_gate(gate: f32) -> i16
pub fn decode_q15_gate(gate_q15: i16) -> f32
```

### Canonical Score Comparator (invariants.rs:135-156)
```rust
pub fn canonical_score_comparator(a: &(usize, f32), b: &(usize, f32)) -> Ordering
```
- Primary: **Score DESC** (higher scores first)
- Tie-break: **Index ASC** (lower indices first)
- Uses `total_cmp()` for IEEE 754 total ordering (handles NaN deterministically)

### Canonical Adapter Sort
```rust
pub fn canonical_adapter_sort(adapters: &mut [(usize, f32)])
pub fn validate_canonical_sort(adapters: &[(usize, f32)]) -> bool
```

---

## TypedSeed (seed.rs)

### Struct Fields
- `version: u32` - HKDF algorithm version
- `bytes: [u8; 32]` - Derived seed bytes
- `checksum: [u8; 4]` - First 4 bytes of BLAKE3 hash for validation

### Validation
```rust
typed_seed.validate() -> Result<(), SeedError>
typed_seed.validate_with_config(config) -> Result<(), SeedError>  // Strict mode
typed_seed.validate_checksum() -> bool
```

---

## SeedLineage (seed.rs)

Tracks the provenance of a seed for audit.

### Fields
- `root_seed_digest: [u8; 32]` - BLAKE3 hash of root seed
- `seed_mode: SeedMode`
- `has_manifest_binding: bool`
- `hkdf_version: u32`

### Methods
```rust
SeedLineage::from_typed_seed(seed, mode, has_manifest)
SeedLineage::verify_seed(raw_seed) -> bool
SeedLineage::verify_typed_seed(typed_seed) -> bool
```

---

## Worker Determinism (`crates/adapteros-lora-worker/src/determinism.rs`)

### Guards
```rust
init_determinism_guards()      // Enable guards
determinism_guards_enabled() -> bool
determinism_violation_count() -> usize
is_strict_mode() -> bool
```

### Strict Mode Guard
In strict mode, violations are logged and counted. Used for audit and replay verification.

---

## What Breaks Determinism

| Don't Do | Why | Alternative |
|----------|-----|-------------|
| Use `rand::random()` | Unseeded RNG | `get_deterministic_rng()` |
| Use `Instant::now()` | Non-deterministic time | `get_deterministic_timestamp()` |
| Use `HashMap` iteration order | Non-deterministic | Use `BTreeMap` or `maybe_stable_sort()` |
| Use `-ffast-math` | Non-IEEE 754 | Ensure IEEE 754 compliance |
| Change Q15 denominator | Breaks replay | Always use 32767.0 |
| Use `f32::partial_cmp` for sorting | NaN non-determinism | Use `f32::total_cmp()` |

---

## Debug Determinism

### Environment Variable
```bash
AOS_DEBUG_DETERMINISM=1
```
Logs detailed seed derivation including:
- Label, global seed prefix, checksum
- Fixed seed overrides
- Thread-local config changes

### Check Function
```rust
determinism_debug_enabled() -> bool
```

---

## Testing Determinism

### Test Fixtures
```rust
// Fully deterministic config
let config = DeterminismConfig::builder()
    .fixed_seed(42)
    .fixed_timestamp(some_datetime)
    .stable_ordering(true)
    .strict_mode(true)
    .build();
```

### Guard Pattern
```rust
let _guard = DeterminismConfigGuard::new(config);
// ... operations use this config ...
// Config restored on drop
```

### Key Tests
- `crates/adapteros-core/src/seed.rs` - `test_hkdf_golden_vector_stability`
- `tests/determinism_core_suite.rs` - Cross-system determinism verification
- `crates/adapteros-lora-router/tests/determinism.rs` - Router determinism

---

## Invariant Checklist

When modifying determinism-critical code:

1. [ ] Derive all RNG from `derive_seed()` or `get_deterministic_rng()`
2. [ ] Use `get_deterministic_timestamp()` for time-based operations
3. [ ] Use `canonical_score_comparator()` for adapter sorting
4. [ ] Use `encode_q15_gate()` / `decode_q15_gate()` for gate conversion
5. [ ] Never use `-ffast-math` compiler flags
6. [ ] Run `cargo test --test determinism_core_suite` after changes
7. [ ] Set `AOS_DEBUG_DETERMINISM=1` to trace seed operations
