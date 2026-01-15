# PRD: Complete Adapter Signature Verification Chain

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-crypto/src/bundle_sign.rs`, `crates/adapteros-cli/src/commands/registry.rs`, `crates/adapteros-cli/src/commands/import.rs`, `crates/adapteros-telemetry/src/signature_audit.rs`

---

## 1. Summary

Adapters imported via `aosctl registry` and `aosctl import` currently bypass cryptographic signature verification or use placeholder signatures. This PRD defines the end-to-end verification chain so adapterOS rejects unsigned or tampered adapters by default and records audit events for all verification attempts.

---

## 2. Problem Statement

The adapter supply chain is intended to be verifiable. Today, registry sync and import operations allow artifacts with missing or placeholder signatures, which weakens the security model and breaks chain-of-custody guarantees.

---

## 3. Goals

- Enforce Ed25519 verification for adapter artifacts on registry pull/sync and import.
- Require trusted public keys loaded from a local keyring.
- Emit audit events for verification successes and failures.
- Provide a supported signing command for adapter bundles.

---

## 4. Non-Goals

- Introducing new cryptographic algorithms beyond Ed25519.
- Replacing existing bundle verification commands.
- Distributing keys over the network at runtime.
- Covering model or kernel signature verification (handled separately).

---

## 5. Current State

- `crates/adapteros-cli/src/commands/registry.rs` parses signatures but skips verification with a placeholder block.
- `crates/adapteros-cli/src/commands/import.rs` stores a hardcoded placeholder signature.
- `crates/adapteros-crypto` already provides `BundleSignature::verify`, `verify_signature`, and key primitives.
- `crates/adapteros-telemetry/src/signature_audit.rs` provides a structured audit logger.

---

## 6. Proposed Approach

### 6.1 Trusted Keyring

- Store trusted keys at `~/.aos/keys/trusted_publishers.json`.
- Support PEM and raw 32-byte public keys.
- Include `key_id`, `valid_from`, and `revoked` metadata.

### 6.2 Verification Flow

1. Load adapter bytes and signature data (manifest or `.sig`).
2. Compute BLAKE3 hash of adapter bytes or bundle hash (consistent with `bundle_sign`).
3. Load trusted keys and match `key_id`.
4. Verify signature with Ed25519.
5. On failure: record audit event and fail the command (unless explicitly bypassed).

### 6.3 CLI Behavior

- `aosctl registry sync` and `aosctl registry pull` verify signatures by default.
- `aosctl import` verifies signatures by default and supports `--skip-verify` for controlled dev use.
- `aosctl adapter sign` produces `.sig` files using the local signing key provider.

### 6.4 Audit Logging

- Use `SignatureAuditLogger` to record verification attempts (success or failure).
- Include `adapter_hash`, `key_id`, and failure reason in the audit context.

---

## 7. Acceptance Criteria

- Registry sync/pull rejects adapters with invalid or missing signatures.
- `aosctl import` rejects unsigned adapters unless `--skip-verify` is set.
- Audit log entries are emitted for every verification attempt.
- `aosctl adapter sign` can generate a valid `.sig` file for a bundle.
- Verification time for a 1GB adapter completes within 100 ms on M-series hardware.

---

## 8. Test Plan

- Unit tests for trusted keyring parsing (PEM and raw formats).
- Unit tests for signature verification using known test vectors.
- Integration test: sign adapter, publish, pull, verify succeeds.
- Integration test: tamper adapter, pull, verify fails.
- Audit log test: failure event includes key_id and hash.

---

## 9. Rollout Plan

1. Phase 1: Introduce keyring parsing and verification in CLI commands; keep `--skip-verify` for dev.
2. Phase 2: Ship a default trusted keyring with release artifacts.
3. Phase 3: Remove `--skip-verify` (or gate it behind dev-only flags) in the next major release.

---

## 10. Follow-up Tasks (Tracked)

- TASK-1: Implement trusted key loader in `adapteros-crypto`.
  - Acceptance: parses JSON, rejects revoked keys, unit tests cover PEM/raw input.
- TASK-2: Wire verification into `aosctl registry sync` and `aosctl import`.
  - Acceptance: invalid or missing signatures fail with a structured error code.
- TASK-3: Add `aosctl adapter sign` command for bundle signing.
  - Acceptance: generates `.sig` files verifiable by existing `verify` commands.
- TASK-4: Emit signature audit events on verification success/failure.
  - Acceptance: audit log contains adapter hash, key_id, and result.
- TASK-5: Add integration tests for sign -> publish -> pull -> verify and tamper -> verify failure.
  - Acceptance: tests run in CI without external dependencies.
