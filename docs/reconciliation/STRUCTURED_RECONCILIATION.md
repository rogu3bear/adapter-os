# Structured Reconciliation Report (Authoritative)

Artifact ID: `adapteros.reconciliation.deterministic_system.v1`

Repository: AdapterOS (`/Users/star/Dev/adapter-os`)

Authoritative anchor:
- Branch: `main`
- Commit: `4df3dab0ea6a11c299038b4ba6b3a8480c839c2e`
- Generation date (local): 2026-02-09

Scope statement: This report reconciles and unifies the determinism substrate, token-level auditability, receipt verifiability, and cache attribution correctness into a single canonical system description. The report is falsifiable by direct code inspection at the anchor commit and by the verification commands listed below.

Verification commands (smallest relevant set):
```bash
bash scripts/check_fast_math_flags.sh
cargo test -p adapteros-lora-router --test determinism
cargo test --test determinism_replay_harness -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api golden_replay_produces_identical_receipt
```

---

## Executive Summary

This system produces and verifies deterministic evidence for inference runs by:

1. Deriving all randomness from a versioned HKDF-SHA256 seed hierarchy and isolating thread-local seed state at request boundaries.
2. Hashing per-token routing/policy/backend decision material using a canonical decision hash function and chaining decisions into a run-head hash.
3. Computing a schema-versioned `receipt_digest` by hashing canonically ordered receipt fields, including token accounting and determinism- and cache-binding metadata (current schema is V7).
4. Enforcing cache attribution correctness by checking billing arithmetic and requiring a signed cache attestation for any non-zero cache credit claim.
5. Providing recomputation-based receipt verification (from stored trace evidence or from an uploaded evidence bundle) that must fail on mismatch or unsupported schema.

The reconciliation outcome is a single canonical set of hashing, chaining, and verification functions that are shared across production, server verification endpoints, and offline verification tooling. Drift is treated as a determinism or integrity failure, not a tolerable discrepancy.

---

## Invariants And Enforcement Mapping

This section states the invariants explicitly and maps each to the enforcing mechanisms. Each mapping includes conflict resolution and falsification conditions.

### 1) Determinism

Invariant (determinism):
Given the same determinism-critical inputs (seed lineage, backend identity/attestation, routing tie-break rules, and recorded per-token decisions), recomputation of the receipt components must yield the same `receipt_digest`. Any deviation must be detectable as a receipt mismatch and/or replay verification failure.

Enforcing mechanisms:
- HKDF-seeded randomness with explicit algorithm versioning and fixed output length:
  - `adapteros_core::seed::derive_seed` and `HKDF_ALGORITHM_VERSION` in `crates/adapteros-core/src/seed.rs`.
- Request-boundary seed isolation (prevents cross-request seed state leakage):
  - `seed_isolation_middleware` and `seed_isolation_middleware_fast` in `crates/adapteros-server-api/src/middleware/seed_isolation.rs`.
- Deterministic routing tie-break and deterministic float ordering:
  - Router sorting uses score DESC then index ASC and `total_cmp` for IEEE 754 total ordering in `crates/adapteros-lora-router/src/router.rs` (see `route_deprecated` implementation and related routing paths).
- Determinism-critical Q15 constants:
  - `ROUTER_GATE_Q15_DENOM == 32767.0` and compile-time assertions in `crates/adapteros-lora-router/src/quantization.rs`.
- Deterministic execution tracking via a global tick ledger with atomic tick assignment:
  - `GlobalTickLedger::record_tick` in `crates/adapteros-deterministic-exec/src/global_ledger.rs`.
- Build-time determinism guard against unsafe floating point compiler flags:
  - `scripts/check_fast_math_flags.sh` rejects `-ffast-math` and `-funsafe-math-optimizations`.

Conflicts and resolutions:
- Conflict: thread-local seed state can leak across requests (async boundaries, pooled threads), corrupting determinism.
  - Resolution: request middleware asserts clean state in debug builds and resets state at request entry/exit (`seed_isolation_middleware`).
- Conflict: float ordering can vary in the presence of NaN or platform edge cases.
  - Resolution: routing uses `total_cmp` and explicit tie-break rules; Q15 quantization constants are fixed and asserted.
- Conflict: wall-clock time introduces nondeterminism in execution tracking.
  - Resolution: tick ledger supports deterministic timestamps and can hard-fail wall-clock access under `strict-determinism` feature (`GlobalTickLedger`).

Falsification conditions:
- Any change to `HKDF_ALGORITHM_VERSION`, seed derivation bytes, or seed isolation behavior that changes derived seeds without version migration support falsifies replay equivalence.
- Any change to router tie-break ordering or Q15 denominator falsifies routing determinism for the same inputs.
- Presence of forbidden fast-math flags falsifies numeric determinism assumptions.

---

### 2) Token-Level Auditability

Invariant (token-level auditability):
For each generated token, the system must be able to compute a canonical decision hash over the token’s decision material, and must chain these decision hashes into a `run_head_hash` such that any per-token decision modification changes `run_head_hash`.

Enforcing mechanisms:
- Canonical token decision hashing:
  - `adapteros_core::receipt_digest::hash_token_decision` in `crates/adapteros-core/src/receipt_digest.rs`.
- Canonical chaining for the run-head hash:
  - `adapteros_core::receipt_digest::update_run_head` in `crates/adapteros-core/src/receipt_digest.rs`.
- Production trace sink delegates hashing/chaining to the canonical implementation:
  - `SqlTraceSink::hash_decision` and `SqlTraceSink::update_head` in `crates/adapteros-db/src/inference_trace.rs` call the canonical functions.
- Receipt recomputation reports the first mismatched token index and emits an observability event on mismatch:
  - `adapteros_db::recompute_receipt` and mismatch emission in `crates/adapteros-db/src/inference_trace.rs`.

Conflicts and resolutions:
- Conflict: duplicated hashing implementations across components can drift (production vs verification).
  - Resolution: production code delegates to the canonical hashing/chaining functions in `adapteros-core`; recomputation uses the same canonical functions.

Falsification conditions:
- If a per-token decision mutation does not change `run_head_hash`, then the canonical decision hash or chaining logic is not binding the relevant decision material.
- If recomputation cannot identify mismatched token indices in trace evidence, token-level auditability is incomplete.

---

### 3) Receipt Verifiability

Invariant (receipt verifiability):
Given either (a) stored trace evidence, or (b) an evidence bundle payload, a verifier must be able to recompute the component digests and the final `receipt_digest` using a canonical, schema-versioned algorithm, and compare the recomputed digest to the claimed digest. Mismatch must be reported; unsupported schema must be rejected.

Enforcing mechanisms:
- Canonical receipt digest computation (single source of truth):
  - `adapteros_core::receipt_digest::compute_receipt_digest` and schema constants in `crates/adapteros-core/src/receipt_digest.rs`.
  - Current schema version constant: `RECEIPT_SCHEMA_CURRENT` in `crates/adapteros-core/src/receipt_digest.rs` (V7 at anchor commit).
- Third-party/offline verification recomputes and compares using the same digest algorithm:
  - `adapteros_core::third_party_verification::verify_receipt` in `crates/adapteros-core/src/third_party_verification.rs`.
- Server verification endpoints recompute against stored trace evidence:
  - `/v1/replay/verify/trace` and related verification types in `crates/adapteros-server-api/src/handlers/replay.rs`.
  - Standalone verification (`/v1/adapteros/replay`) does digest lookup then recomputation in `crates/adapteros-server-api/src/handlers/adapteros_receipts.rs`.
- CLI verification delegates verification logic to avoid drift between server and CLI:
  - `aosctl verify-receipt` logic in `crates/adapteros-cli/src/commands/verify_receipt.rs` delegates to `adapteros_crypto::verify_bundle_bytes` and re-exports schema constants from `adapteros-core`.

Conflicts and resolutions:
- Conflict: multiple receipt digest implementations can drift (DB layer vs core library vs CLI).
  - Resolution: production uses the canonical `compute_receipt_digest` surface; recomputation and third-party verification call the same canonical function; CLI delegates to shared verification logic to prevent independent drift.
- Conflict: verification must not silently accept unknown digest formats.
  - Resolution: verification checks schema version bounds and returns explicit failure for unsupported schemas (`verify_receipt`).

Falsification conditions:
- If two verifiers using the same schema and the same evidence inputs compute different `receipt_digest` values, the canonicalization contract is broken.
- If an unsupported schema version is accepted as verified, schema gating is broken.

---

### 4) Cache Attribution Correctness

Invariant (cache attribution correctness):
`billed_input_tokens` must equal `logical_prompt_tokens - prefix_cached_token_count` (saturating at 0), and any non-zero cache credit claim (`prefix_cached_token_count > 0`) must be accompanied by a verifiable cryptographic attestation that binds the cache key and the claimed token count to a worker identity.

Enforcing mechanisms:
- Arithmetic enforcement at trace finalization:
  - `SqlTraceSink::finalize` validates that `billed_input_tokens == logical_prompt_tokens - prefix_cached_token_count` in `crates/adapteros-db/src/inference_trace.rs`.
- Cache credit provability via signed cache attestation:
  - `adapteros_core::cache_attestation::CacheAttestation` and `CacheAttestation::verify` in `crates/adapteros-core/src/cache_attestation.rs` (Ed25519 signature over canonical bytes).
  - Finalization requires and verifies the attestation when cached tokens are claimed, and checks token count and cache key consistency in `crates/adapteros-db/src/inference_trace.rs`.
- Worker-side attestation generation on cache hit:
  - `CacheLookupResult::hit_with_attestation` in `crates/adapteros-lora-worker/src/cache_prefix_lookup.rs` uses `CacheAttestationBuilder::build_and_sign`.
- Receipt binding for cache accounting fields:
  - `logical_prompt_tokens`, `prefix_cached_token_count`, and `billed_input_tokens` are part of `ReceiptDigestInput` in `crates/adapteros-core/src/receipt_digest.rs`.

Conflicts and resolutions:
- Conflict: a malicious or compromised worker could claim arbitrary cache credits to reduce billing.
  - Resolution: control plane rejects non-zero credits without a valid signature and rejects mismatched token counts; the attestation binds the cache key hash and claimed token count to a worker identity and logical tick.

Falsification conditions:
- If `prefix_cached_token_count > 0` can be finalized without a valid attestation, the fraud-prevention invariant is violated.
- If `billed_input_tokens` does not match the defined arithmetic and is still accepted, accounting correctness is violated.

---

## Reconciliation Decisions With References

This section documents how partial implementations and concepts were unified into a single deterministic system, including removed paths, merged logic, and the canonical execution flow.

### Decision A: Canonicalize receipt digest computation

Merged logic:
- The canonical receipt digest algorithm is centralized in `adapteros_core::receipt_digest::compute_receipt_digest` (`crates/adapteros-core/src/receipt_digest.rs`).
- Production trace finalization computes the receipt digest via this canonical function (current schema V7) in `SqlTraceSink::finalize` (`crates/adapteros-db/src/inference_trace.rs`).
- Verification recomputes receipt digests using the same canonical function:
  - Third-party: `adapteros_core::third_party_verification::verify_receipt` (`crates/adapteros-core/src/third_party_verification.rs`).
  - Server-side: `adapteros_db::recompute_receipt` (called by handlers in `crates/adapteros-server-api/src/handlers/replay.rs` and `crates/adapteros-server-api/src/handlers/adapteros_receipts.rs`).

Removed paths (as canonical sources of truth):
- Independent, component-specific receipt digest algorithms are not authoritative for current receipts. The authoritative digest is the output of `compute_receipt_digest` for `RECEIPT_SCHEMA_CURRENT` and the evidence inputs that feed it.

Conflict resolution:
- “Production vs verification” drift is treated as a mismatch condition. Receipt recomputation emits a receipt mismatch observability event when parity fails (`crates/adapteros-db/src/inference_trace.rs` mismatch emission path).

---

### Decision B: Canonicalize token decision hashing and run-head chaining

Merged logic:
- Canonical decision hash: `adapteros_core::receipt_digest::hash_token_decision` (`crates/adapteros-core/src/receipt_digest.rs`).
- Canonical chaining: `adapteros_core::receipt_digest::update_run_head` (`crates/adapteros-core/src/receipt_digest.rs`).
- Production trace persistence delegates to these canonical functions:
  - `SqlTraceSink::hash_decision` and `SqlTraceSink::update_head` in `crates/adapteros-db/src/inference_trace.rs`.

Removed paths:
- Any local hashing/chaining logic that does not delegate to the canonical functions is non-authoritative for audit parity. Audit parity requires canonical hashing and canonical chaining.

Conflict resolution:
- Token-level mismatch is surfaced as a concrete `mismatched_token` index in recomputation reports (stored-evidence verification path).

---

### Decision C: Make cache credit claims provable (attestation-required)

Merged logic:
- Attestation schema and canonical signing bytes are centralized in `crates/adapteros-core/src/cache_attestation.rs`.
- Worker produces attestation for cache hits in `crates/adapteros-lora-worker/src/cache_prefix_lookup.rs`.
- Control plane verifies attestation and rejects invalid/missing proofs at finalization in `crates/adapteros-db/src/inference_trace.rs`.

Removed paths:
- “Trust-based” cache credit claims are not accepted when `prefix_cached_token_count > 0`.

Conflict resolution:
- Billing fraud risk is converted into an explicit verification requirement: without a valid signature and matching cache key/token count, finalization fails.

---

### Decision D: Determinism boundary enforcement (seed isolation, numeric constraints, execution tracking)

Merged logic:
- Seed derivation and determinism configuration are centralized in `crates/adapteros-core/src/seed.rs`.
- Request boundary isolation is enforced by server middleware (`crates/adapteros-server-api/src/middleware/seed_isolation.rs`).
- Numeric determinism is constrained by:
  - router ordering rules (`crates/adapteros-lora-router/src/router.rs`),
  - Q15 constants (`crates/adapteros-lora-router/src/quantization.rs`),
  - fast-math scan (`scripts/check_fast_math_flags.sh`).
- Execution tracking uses an atomic tick ledger (`crates/adapteros-deterministic-exec/src/global_ledger.rs`).

Removed paths:
- Compilations that enable forbidden fast-math flags are rejected by policy/script and are non-conforming to determinism claims.
- In strict determinism configurations, wall-clock dependencies in determinism-critical components are treated as violations (feature-gated strict mode in tick ledger).

---

## Final Canonical Execution Flow

The canonical flow below is the unified execution-and-verification model for deterministic inference evidence.

1. Request enters the server; seed isolation resets thread-local determinism state at request boundaries:
   - `seed_isolation_middleware` (`crates/adapteros-server-api/src/middleware/seed_isolation.rs`).

2. Protected routes execute middleware in canonical order:
   - `auth -> tenant guard -> CSRF -> context -> policy -> audit` in route construction (`crates/adapteros-server-api/src/routes/mod.rs`).

3. Inference execution emits per-token decision material:
   - For each token: compute canonical decision hash (`hash_token_decision`).
   - Chain decision hashes into `run_head_hash` (`update_run_head`).

4. Trace persistence stores decision evidence and maintains the run-head chain:
   - `SqlTraceSink` in `crates/adapteros-db/src/inference_trace.rs`.

5. Prefix cache lookup (when used) produces receipt-bound, integer/hash-only attribution fields and, on cache credit, a signed attestation:
   - `CacheLookupResult` and `hit_with_attestation` (`crates/adapteros-lora-worker/src/cache_prefix_lookup.rs`).

6. Finalization computes output digest, validates token accounting arithmetic, verifies cache attestation when credits are claimed, then computes `receipt_digest` using the canonical schema:
   - `SqlTraceSink::finalize` (`crates/adapteros-db/src/inference_trace.rs`).
   - `compute_output_digest` and `compute_receipt_digest` (`crates/adapteros-core/src/receipt_digest.rs`).

7. Verification recomputes and compares:
   - From stored trace evidence:
     - server-side recomputation (`adapteros_db::recompute_receipt`) invoked by `/v1/replay/verify/trace` and `/v1/adapteros/replay`.
   - From evidence bundle:
     - offline/CLI verification delegates to shared verification logic (`crates/adapteros-cli/src/commands/verify_receipt.rs`).
   - Any mismatch is surfaced explicitly and may emit a receipt mismatch observability event.

---

## What This System Does NOT Claim (Explicit Non-Claims)

1. No claim of semantic correctness of generated outputs. The system proves integrity and replay comparability of bound evidence fields; it does not prove factual correctness.
2. No claim of invariance under configuration changes. If any receipt-bound field changes (seed lineage, backend identity/attestation, routing decisions, token accounting, cache proofs, schema version), the receipt is expected to change.
3. No claim that all backends or hardware produce identical outputs for the same inputs absent explicit backend identity binding. Backend substitution is treated as a detectable change, not an equivalence class.
4. No claim that all timestamps are deterministic. Determinism claims apply to canonical digests and deterministic execution paths; observational timestamps may exist and are not asserted to be stable unless explicitly derived from logical ticks.
5. No claim that all receipts are always signed in all deployment modes. Where signature fields are optional, digest recomputation remains the primary verifiability mechanism; signature policies are configuration and deployment dependent.
6. No claim that receipts provide confidentiality. Receipts are integrity and provenance artifacts; confidentiality and access control are separate concerns.
