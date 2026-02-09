# Execution Contract (Audit Entry Point)

This document is the audit entry point for deterministic inference verification in AdapterOS.
It defines what is receipt-bound, what is evidence-bound, and what requires a trusted build allowlist.

**Document version:** 2026-02-09

**Canonical artifacts (code is source of truth):**
- `ReceiptDigest` schema: v7 (`crates/adapteros-core/src/receipt_digest.rs`)
- `EvidenceEnvelope` schema: v6 (`crates/adapteros-core/src/evidence_envelope.rs`)
- `CryptoReceipt` schema: v1 (`crates/adapteros-core/src/crypto_receipt.rs`)

## 0. Verifier Checklist (One Page)

### Receipt-Only (No Build Allowlist, No Evidence Bundle)

- [ ] Compute the receipt digest using schema v7 and require an exact match. (`crates/adapteros-core/src/receipt_digest.rs`)
- [ ] Enforce deterministic sentinel encoding:
  - [ ] `stop_reason_token_index: None` MUST be encoded as `0xFFFFFFFF` (u32 max) in the digest. (`crates/adapteros-core/src/receipt_digest.rs`)
  - [ ] `stop_eos_q15: None` MUST be encoded as `i16::MIN` in the digest. (`crates/adapteros-core/src/receipt_digest.rs`)
- [ ] Enforce cache credit rules: if `prefix_cached_token_count > 0`, a cache attestation MUST be present and MUST verify (signature and field matches). (`crates/adapteros-db/tests/cache_attestation_enforcement.rs`)

### Routing-Policy Compliance (Requires Allowlisted Build Hash)

- [ ] `model_build_hash_b3` MUST be present and allowlisted to claim routing-policy compliance (router tie-break behavior and policy constants are code-bound). (`crates/adapteros-core/src/version.rs`, `crates/adapteros-core/src/third_party_verification.rs`)
- [ ] If adapter-policy code is in-scope, `adapter_build_hash_b3` MUST be present and allowlisted as well.
- [ ] `-dirty` builds MUST NOT be considered compliant.

### Evidence Bundle (Requires EvidenceEnvelope Chain + Signature Verification)

- [ ] Verify `EvidenceEnvelope` signatures (Ed25519) over canonical bytes. (`crates/adapteros-core/src/evidence_envelope.rs`)
- [ ] Verify per-tenant, per-scope chain linking via `previous_root` and `root`. (`crates/adapteros-core/src/evidence_envelope.rs`)
- [ ] Verify pinned degradation outcomes from evidence (not receipt). (Section 3)
- [ ] If `determinism_mode == "strict"`, strict completeness (EP-5) MUST hold for inference evidence export and replay (fail closed on incomplete receipts). (Section 5)

## 1. Artifact Boundary (Receipt vs Evidence)

### 1.1 ReceiptDigest (v7)

`ReceiptDigest` is the canonical BLAKE3 digest for an inference execution.
It is used to bind determinism-critical identity and execution parameters into a single value.

Current schema version is v7. (`crates/adapteros-core/src/receipt_digest.rs`)

### 1.2 EvidenceEnvelope (v6)

`EvidenceEnvelope` is the canonical, signed, chain-linked container for evidence in three scopes:
`telemetry`, `policy`, and `inference`.

Evidence properties:
- Digest-only payload references (hashes, counts, and small metadata).
- Chain linking per tenant and scope via `previous_root`.
- Ed25519 signature over canonical bytes (signature fields are excluded from the digest). (`crates/adapteros-core/src/evidence_envelope.rs`)

### 1.3 Boundary Rule

Receipts bind what was executed and what must not be malleable.
Evidence binds outcomes and diagnostics that must be signed and chain-linked but are not part of the receipt digest.

## 2. Canonical Routing Spec (Deterministic Ordering + Q15 Gates)

### 2.1 Deterministic Tie-Break (Selection)

When the router ranks adapters, it MUST produce a total ordering with this tie-break:
- Primary: score DESC (higher score wins), compared with `f32::total_cmp()` for IEEE total ordering.
- Secondary: `stable_id` ASC (lower stable_id wins ties).

**Code reference:** `crates/adapteros-lora-router/src/router.rs` (`sort_scores_deterministic`).

### 2.2 Q15 Gate Quantization (Deterministic Gate Encoding)

Router gates are recorded and compared in Q15 fixed-point form.

Requirements:
- Denominator MUST be exactly `32767.0`.
- Encode (router gates are non-negative): `gate_q15 = round(gate_f32 * 32767.0)`, then clamp to `[0, 32767]`.
- Decode: `gate_f32 = gate_q15 as f32 / 32767.0`.

**Code reference:** `crates/adapteros-lora-router/src/quantization.rs` (`ROUTER_GATE_Q15_DENOM`).

### 2.3 Canonical Candidate Ordering (Emission/Storage)

When a candidate list is emitted/stored for verification, it MUST be ordered:
`(gate_q15 DESC, raw_score DESC, stable_id ASC, adapter_idx ASC)`.

**Code reference:** `crates/adapteros-lora-router/src/router.rs` (`sort_candidates_by_quantized_gate_canonical`).

## 3. Pins and Degradation: Evidence-Only (Not Receipt-Bound)

**Pins are preference, degradation is evidence-only.**

Pinned degradation records *outcomes* when pinned adapters are unavailable at execution time.
It intentionally stores no raw adapter IDs.

### 3.1 Evidence Fields (No Raw IDs)

Fields (`crates/adapteros-core/src/evidence_envelope.rs`):
- `pinned_total_count: u32`
- `unavailable_pinned_count: u32`
- `unavailable_pinned_set_digest_b3: Option<[u8; 32]>`
- `pinned_fallback_mode: Option<String>` (expected values: `"partial"` or `"stack_only"`)

Deterministic sentinels:
- If pins are absent entirely: counts MUST be 0 and optional fields MUST be `None`.

### 3.2 Digest Algorithm (BLAKE3 over Sorted Unavailable Pin IDs)

`unavailable_pinned_set_digest_b3` is computed as:

```text
ids = sort_lex(unavailable_pin_ids)
H = blake3()
H.update("adapteros:pinned_unavailable_set_digest:v1\0")
for id in ids:
  H.update(u32_be(len(id_bytes)))
  H.update(id_bytes)
  H.update("\0")
digest = H.finalize()
```

**Code reference:** `crates/adapteros-core/src/evidence_envelope.rs` (`compute_unavailable_pinned_set_digest_b3`).

### 3.3 Computation, Scope, Storage, Export

Requirements:
- Computation MUST occur on the worker (availability is known there). (`crates/adapteros-lora-worker/src/lib.rs`)
- Persistence MUST be via `EvidenceEnvelope` in `telemetry` scope:
  - Telemetry is the canonical scope because pinned degradation is a runtime outcome (availability and fallback), not a policy decision.
  - Evidence MUST be chain-linked (`previous_root`) and signed (Ed25519) under existing envelope rules. (`crates/adapteros-core/src/evidence_envelope.rs`)
- Evidence export MUST include these fields (as values or deterministic sentinels) without exposing raw adapter IDs. (`crates/adapteros-server-api/src/handlers/run_evidence.rs`)

## 4. Build Provenance Requirement (Routing-Policy Compliance)

Routing-policy compliance includes code-bound constants and tie-break behavior that are not individually receipt-bound.
To claim routing-policy compliance, a verifier MUST validate build provenance against an allowlist.

Requirements:
- `model_build_hash_b3` MUST be present and allowlisted for routing-policy compliance claims.
- If adapter-policy code is in-scope, `adapter_build_hash_b3` MUST be present and allowlisted as well.
- `-dirty` builds MUST NOT be considered compliant.

**Code references:**
- Build provenance definition and caching: `crates/adapteros-core/src/version.rs`
- Build ID derivation and dirty markers: `build_support/aos_build_id.rs`
- Verifier guidance and semantics: `crates/adapteros-core/src/third_party_verification.rs`

## 5. Strict Completeness Enforcement (EP-5, Fail Closed)

When determinism mode is recorded as strict, incomplete inference receipts are a determinism violation.

Rule:
- In strict mode, `InferenceReceiptRef::validate_for_strict_mode()` MUST succeed.
- Missing any of these fields is a hard failure: `backend_used`, `backend_attestation_b3`, `seed_lineage_hash`, `output_digest`, `receipt_digest`.

Enforcement call sites:
- Evidence export: `crates/adapteros-server-api/src/handlers/run_evidence.rs`
- Replay: `crates/adapteros-server-api/src/handlers/replay.rs`

**Code reference:** `crates/adapteros-core/src/evidence_envelope.rs` (`InferenceReceiptRef::validate_for_strict_mode`).

## 6. Canonical Encoding (Deterministic Bytes)

### 6.1 ReceiptDigest (v7) Canonicalization

Rules (`crates/adapteros-core/src/receipt_digest.rs`):
- Integer encoding: little-endian (`to_le_bytes()`).
- Strings: length-prefixed UTF-8 (u32 LE length, followed by bytes).
- Optional/sentinel encoding MUST be stable:
  - `Option<u32>`: `None` encodes as `0xFFFFFFFF`.
  - `Option<i16>`: `None` encodes as `i16::MIN`.
  - Fixed-width 32-byte hash slots: `None` encodes as 32 zero bytes.
  - `backend_attestation_b3`: encoded as `u32_le(len) + bytes`; `None` encodes as `len=0` (empty).

### 6.2 EvidenceEnvelope (v6) Canonicalization

Rules (`crates/adapteros-core/src/evidence_envelope.rs`):
- Integer encoding: big-endian (`to_be_bytes()`).
- Strings: length-prefixed UTF-8 (u32 BE length, followed by bytes).
- Optional/sentinel encoding MUST be stable (empty string or 32 zero bytes as applicable).

## 7. Tests, Vectors, and CI Gate

Primary regression harness (CPU-only, deterministic, no wall-clock):
- `crates/adapteros-core/tests/determinism_regression_harness.rs`
  - stable_id order-independence (context_id)
  - Q15 denominator invariant
  - V7 receipt digest golden vector
  - stop sentinel encoding determinism
- `crates/adapteros-db/tests/cache_attestation_enforcement.rs`
  - cache attestation hard-fail on missing/bad proofs when cached tokens are credited

Additional coverage:
- Router determinism and Q15 invariants: `crates/adapteros-lora-router/tests/determinism.rs`, `crates/adapteros-lora-router/tests/q15_denominator_invariants.rs`
- Evidence export bundle shape (includes pinned degradation evidence JSON): `crates/adapteros-server-api/tests/run_evidence_tests.rs`

Commands:
- `docs/DETERMINISM_REGRESSION.md`

CI:
- Minimal determinism gate job: `.github/workflows/ci.yml` (`determinism-gate`)
