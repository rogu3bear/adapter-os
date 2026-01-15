# adapterOS Determinism Invariants

> Complete list of determinism rules and invariants for reproducible inference.

## Critical Rules Summary

| # | Rule | Source | Failure Mode |
|---|------|--------|--------------|
| 1 | HKDF-SHA256 only for seed derivation | `seed.rs:825-826` | Replay incompatibility |
| 2 | Q15 denominator = 32767.0 (never 32768) | `quantization.rs:22,28-35` | Compile panic |
| 3 | Same inputs → same seed | `seed.rs:65,1182-1230` | Test failure |
| 4 | Score DESC, index ASC sorting | `router.rs:sort_by()` | Non-deterministic routing |
| 5 | HKDF output length = 32 bytes | `seed.rs:109-113` | ChaCha20Rng incompatibility |
| 6 | TypedSeed version matching | `seed.rs:262-267` | DeterminismViolation |
| 7 | TypedSeed checksum validation | `seed.rs:202-212` | Corruption undetected |
| 8 | Full entropy isolation | `seed.rs:1056-1078` | Context bleed-through |
| 9 | IEEE 754 total_cmp() for sorting | `router.rs` | NaN ordering variance |
| 10 | Kahan summation for softmax | `router.rs` | Rounding drift |

---

## 1. Seed Derivation Invariants

**File:** `crates/adapteros-core/src/seed.rs`

### HKDF-SHA256 Algorithm (MANDATORY)
```rust
// Only HKDF-SHA256 must be used
Hkdf::<Sha256>::new(Some(salt), ikm)
```
- **Rule:** All seed derivation uses HKDF-SHA256 exclusively
- **Consequence:** Substituting other KDFs breaks replay
- **Location:** Lines 68, 825-826, 848

### HKDF Output Length (FIXED)
```rust
const HKDF_OUTPUT_LENGTH: usize = 32;
```
- **Rule:** All HKDF-derived seeds are exactly 32 bytes
- **Compatible with:** ChaCha20Rng
- **Location:** Lines 109-113

### Algorithm Version Tracking
```rust
const HKDF_ALGORITHM_VERSION: u32 = 2;
```
- **Rule:** Version incremented if algorithm changes
- **Validation:** `TypedSeed::validate()` checks version
- **Location:** Lines 98-107, 232-246

### Global Seed (BLAKE3)
- **Rule:** Global seed = BLAKE3 hash of manifest/request
- **Implementation:** `B3Hash::hash()`
- **Location:** Lines 11-13, 832-883

### Label Uniqueness
- **Rule:** Different labels produce cryptographically distinct seeds
- **Purpose:** Domain separation prevents seed reuse
- **Location:** Lines 66, 848-883

### Determinism Guarantee
```rust
derive_seed(hash_A, "router") == derive_seed(hash_A, "router")
// Always true, guaranteed across platforms
```
- **Tests:** Lines 1182-1230

---

## 2. Seed Mode Invariants

**File:** `crates/adapteros-core/src/seed.rs` (Lines 727-1039)

| Mode | Behavior | Use Case |
|------|----------|----------|
| `Strict` | Requires manifest hash; fails if missing | Production |
| `BestEffort` | Uses hash when present; fallback if not | Dev/testing |
| `NonDeterministic` | Random seed (non-replayable) | Debug only |

**Critical:** NonDeterministic rejected in strict mode (lines 964-969, 1028-1037)

---

## 3. TypedSeed Integrity

**File:** `crates/adapteros-core/src/seed.rs` (Lines 122-289)

### Version Matching
```rust
if seed.version != HKDF_ALGORITHM_VERSION {
    return Err(DeterminismViolation);
}
```
- **Strict mode:** Fails closed on mismatch
- **Location:** Lines 132-137, 262-267

### Checksum Validation
```rust
checksum == BLAKE3(seed_bytes)
```
- **Purpose:** Detects corruption/tampering
- **Location:** Lines 134-135, 202-212

### Fail-Closed Behavior
- Version/checksum mismatches cause immediate failure
- No silent drift

---

## 4. Full Entropy Isolation

**File:** `crates/adapteros-core/src/seed.rs` (Lines 1046-1078)

**Ruleset #2 (Line 1056):**
```
manifest_hash || adapter_dir_hash || worker_id || label || nonce
```
- **Rule:** All seeds incorporate full context
- **Invariant:** Same context = identical seeds; any change = distinct seed

### Seed Reuse Prevention
- Registry tracks `(label, nonce)` pairs (lines 1107-1130)
- `derive_adapter_seed()` checks registry
- `clear_seed_registry()` at inference boundaries

---

## 5. Q15 Quantization Invariants

**File:** `crates/adapteros-lora-router/src/quantization.rs`

### Q15 Denominator (LOCKED)
```rust
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;  // NOT 32768!
```
- **Why 32767:** i16::MAX = 32767; 32768 would overflow
- **Compile-time check:** Lines 28-35 panic if changed
- **Location:** Lines 6-22

### Q15 Maximum Value
```rust
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;  // i16::MAX
```
- **Invariant:** Max gate = 32767/32767.0 = 1.0 exactly

### Gate Format (VERSIONED)
```rust
struct GateQuantFormat {
    q_format: "Q15",
    denom: 32767.0
}
```
- **Validation:** Format must match, else `DeterminismViolation`
- **Location:** Lines 40-68, 106-124

### Encode/Decode
```rust
// Encode
gate_q15 = (gate_f32 * 32767.0).round() as i16;

// Decode
gate_f32 = gate_q15 as f32 / 32767.0;
```
- **Invariant:** Same f32 → same Q15
- **Clamping:** [0, 32767] (line 154)
- **Tests:** Lines 483-502

---

## 6. Router Determinism Invariants

**File:** `crates/adapteros-lora-router/src/router.rs`

### Determinism via Sorting (NOT Seed)
- **Rule:** Routing is deterministic via stable sorting
- **Seed role:** Telemetry sampling only, not routing
- **Tests:** Lines 33-35, 264-304

### Score Sorting Order (STRICT)
```rust
scores.sort_by(|a, b|
    b.1.total_cmp(&a.1)           // Score DESC
       .then_with(|| a.0.cmp(&b.0)) // Index ASC
);
```
- **Primary:** Score descending (highest first)
- **Tie-break:** Index ascending (lowest wins)
- **IEEE 754:** Uses `total_cmp()` for deterministic NaN ordering
- **Tests:** Lines 530-567

### Tie Detection Epsilon
```rust
const TIE_BREAK_RELATIVE_EPSILON: f32 = 1e-6;
```
- **Rule:** Scores within relative epsilon are ties
- **Location:** `constants.rs:53-56`

### Softmax Determinism
- **Method:** IEEE 754 deterministic, f64 intermediate precision
- **Stability:** Kahan summation for accumulation
- **Tests:** Lines 664-716

### Kahan Summation
```rust
// Loss-compensated accumulation
new_val = (val - c) + sum;
c = (new_sum - sum) - new_val;
```
- **Purpose:** Reduce rounding drift
- **Location:** `router.rs`, `features.rs`

### Gate Normalization
- **Rule:** Gates sum to ~1.0 after quantization
- **Tolerance:** Error < 0.01
- **Enforcement:** All K gates non-negative and ≤ 32767
- **Tests:** Lines 168-174, 253-260, 811-857

### K-Sparse Boundaries
- **Valid:** 1 ≤ K ≤ MAX_K
- **K=0:** Clamped to 1
- **K > MAX_K:** Clamped with warning
- **Unique:** All K indices distinct
- **Tests:** Lines 610-661

### Entropy Floor
- **Rule:** Each gate ≥ (entropy_floor / K)
- **Purpose:** Prevent gate collapse
- **Tests:** Lines 211-260, 719-763

### Adaptive Routing Determinism
- **Requirement:** Determinism context MUST be provided
- **Seed role:** RNG for tie-breaking only
- **Tests:** Lines 83-117

### Cross-Instance Determinism
- **Rule:** Different router instances with same config = identical results
- **Proof:** Seed doesn't affect routing
- **Tests:** Lines 766-808

---

## 7. Determinism Context

**File:** `crates/adapteros-core/src/determinism.rs` (Lines 19-172)

### Request Seed Derivation
- **Source:** Manifest hash + tenant ID + request ID
- **Format:** 32-byte seed with low64 extraction
- **Location:** Lines 35-51, 137-144

### Routing Determinism Mode
```rust
enum RoutingDeterminismMode {
    Deterministic,  // Hard sorting
    Adaptive,       // Seeded tie-breaking
}
```
- **Location:** Lines 108-109

### Sampler Seed (Per-Step)
```rust
sampler_seed(step) = derive_seed(request_seed, "sample:<step>")
```
- **Invariant:** Different steps get different seeds
- **Location:** Lines 122-125, 149-154

### Router Tie-Break Seed
```rust
derive_router_tiebreak_seed(router_seed_hex)
```
- **Purpose:** Seeded RNG for adaptive tie-breaking
- **Location:** Lines 127-130, 167-171

---

## 8. Global Determinism Config

**File:** `crates/adapteros-core/src/seed.rs` (Lines 342-437)

| Field | Effect |
|-------|--------|
| `fixed_seed` | Overrides RNG derivation |
| `fixed_timestamp` | Fixes all time ops |
| `stable_ordering` | Force sorted iteration |
| `strict_mode` | Fail-closed validation |
| `trace_seeds` | Debug logging |

### Strict Mode
- Rejects NonDeterministic seed mode
- Immediate error on schema/version mismatch
- **Location:** Lines 381-436

### Fully Deterministic Config
```rust
DeterminismConfig {
    fixed_seed: Some(0),
    fixed_timestamp: Some(UNIX_EPOCH),
    stable_ordering: true,
    strict_mode: true,
}
```
- **Location:** Lines 403-412

---

## 9. Floating Point Restrictions

### No Fast-Math
- **Rule:** FastMath disabled (violates IEEE 754)
- **Verification:** No `-ffast-math` in production builds

### IEEE 754 Determinism
- **Sorting:** `f32::total_cmp()` instead of standard `cmp()`
- **Guarantees:** Deterministic NaN ordering across platforms

---

## 10. Path Normalization

**File:** `crates/adapteros-core/src/seed.rs` (Lines 1080-1105)

- **Rule:** Paths normalized before hashing
- **Method:** Convert separators to forward slashes
- **Purpose:** Same logical path = identical hash on all platforms
- **Implementation:** `hash_adapter_dir()` via `path_normalization`

---

## 11. Seed Registry

**File:** `crates/adapteros-core/src/seed.rs` (Lines 115-118, 1107-1164)

```rust
static SEED_REGISTRY: Mutex<HashMap<(String, u64), bool>>
```
- **Key:** `(label, nonce)`
- **Purpose:** Detect accidental seed reuse
- **Lifecycle:** Cleared at inference boundaries
- **Poison handling:** Graceful recovery with warning

---

## 12. Validation & Enforcement

### Compile-Time
- Q15 denominator panic guards (`quantization.rs:28-35`)

### Runtime
- `TypedSeed::validate()` - version & checksum
- `GateQuantFormat::validate()` - format matching
- `DeterminismConfig` - strict mode

### Test Suite
- 1,500+ lines in `determinism.rs`
- Property-based tests via proptest
- Golden vector tests (lines 1451-1475)

---

## Environment Variable

```bash
AOS_DEBUG_DETERMINISM=1
```
- **Values:** "1", "true", "yes" (case-insensitive)
- **Effect:** Logs detailed seed derivation
- **Location:** `seed.rs:714-724`, `router.rs:24-33`

---

## Test Coverage

**File:** `crates/adapteros-lora-router/tests/determinism.rs` (1,563 lines)

| Lines | Test |
|-------|------|
| 32-80 | Deterministic top-K with ties |
| 120-126 | Q15 denominator = 32767.0 |
| 262-304 | Multiple calls identical |
| 483-502 | Q15 round-trip precision |
| 530-567 | Score DESC, index ASC |
| 664-716 | Softmax determinism |
| 766-808 | Cross-instance determinism |
| 1090-1179 | Adaptive routing context |
| 1182-1305 | Relative epsilon edges |
| 1436-1486 | Seed version validation |

---

## Key Constants

| Constant | Value | Location |
|----------|-------|----------|
| `HKDF_OUTPUT_LENGTH` | 32 | `seed.rs:109` |
| `HKDF_ALGORITHM_VERSION` | 2 | `seed.rs:98` |
| `ROUTER_GATE_Q15_DENOM` | 32767.0 | `quantization.rs:6` |
| `ROUTER_GATE_Q15_MAX` | 32767 | `quantization.rs:22` |
| `TIE_BREAK_RELATIVE_EPSILON` | 1e-6 | `constants.rs:53` |
