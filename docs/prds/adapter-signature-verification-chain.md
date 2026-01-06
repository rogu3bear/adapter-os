# PRD: Adapter Signature Verification Chain

**Status:** Draft  
**Last Updated:** 2026-01-05  
**Owner:** Engineering  
**Related Docs:** docs/CONTENT_ADDRESSING_INTEGRITY_VERIFICATION.md, docs/SECURITY.md, docs/CLI_GUIDE.md

---

## 1. Summary

Adapters pulled via `aosctl registry pull` and `aosctl import` currently accept placeholder signatures. This PRD defines the work to wire Ed25519 signature verification into import and registry flows, establish a trusted publisher keyring, and emit security audit events when verification fails.

---

## 2. Problem Statement

Adapter artifacts are accepted without validating their signatures, which breaks chain-of-custody guarantees and undermines policy enforcement. Customers need deterministic verification that adapters are signed by trusted publishers before they are stored or activated.

---

## 3. Goals

1. Verify adapter signatures during registry pull and import.
2. Reject adapters with missing or invalid signatures unless explicitly skipped for development.
3. Load trusted public keys from a local keyring file.
4. Emit security audit events for signature verification outcomes.

---

## 4. Non-Goals

- Replacing the signature scheme (Ed25519 remains the standard).
- Building a network-based key distribution system.
- Reworking bundle formats or archive layout beyond signature metadata.

---

## 5. Proposed Approach

- Treat signatures as metadata over a bundle hash (BLAKE3) and verify using existing `adapteros-crypto` primitives.
- Store trusted publisher keys in a local JSON keyring under `~/.aos/keys/`.
- Add a `--skip-verify` flag for import and registry pull to support development and controlled bypasses.
- Emit audit events with adapter id, bundle hash, key id, and verification status.

---

## 6. Requirements and Implementation Plan

### R1: Trusted Keyring Loader

**Requirement:** Load trusted Ed25519 public keys from `~/.aos/keys/trusted_publishers.json`.

**Implementation Tasks:**
- Add a loader in `adapteros-crypto` that supports PEM and raw public key formats.
- Validate `revoked`, `valid_from`, and `key_id` fields.
- Return a structured list of trusted keys for verification.

**Acceptance Criteria:**
- Invalid key entries are rejected with actionable errors.
- PEM and raw key formats both load correctly.

---

### R2: Registry Pull Verification

**Requirement:** `aosctl registry pull` verifies signatures before storage.

**Implementation Tasks:**
- Extract signature metadata during pull.
- Verify bundle hash with trusted keys.
- Emit an audit event on success or failure.

**Acceptance Criteria:**
- Invalid signatures prevent storage by default.
- Successful verification logs a security audit event.

---

### R3: Import Verification + Skip Flag

**Requirement:** `aosctl import` requires valid signatures unless `--skip-verify` is set.

**Implementation Tasks:**
- Add `--skip-verify` flag to the import command.
- Reuse the trusted key loader for verification.
- Provide clear error messaging when verification fails.

**Acceptance Criteria:**
- Import fails on invalid signatures without `--skip-verify`.
- Import succeeds with `--skip-verify` and logs the bypass.

---

### R4: Security Audit Events

**Requirement:** Signature verification outcomes emit audit events.

**Implementation Tasks:**
- Add a new audit event type or reuse existing security audit logging.
- Include adapter id, bundle hash, key id, and result.

**Acceptance Criteria:**
- Audit event is emitted for both pass and fail cases.
- Event data is sufficient for compliance review.

---

### R5: Adapter Signing Command

**Requirement:** `aosctl adapter sign` signs adapter bundles.

**Implementation Tasks:**
- Add a new subcommand to sign a bundle using a local private key.
- Store signature metadata alongside the adapter bundle.

**Acceptance Criteria:**
- Command produces a signature that verifies in the pull/import path.
- Clear errors are returned for missing or invalid keys.

---

### R6: Integration Tests

**Requirement:** Provide integration coverage for success and failure cases.

**Implementation Tasks:**
- Add tests for signing, pulling, and importing a valid adapter.
- Add tests for tampered adapters and invalid signatures.
- Ensure tests are deterministic and run in CI.

**Acceptance Criteria:**
- Tests fail when signatures are invalid or missing.
- Tests pass for valid, signed adapters.

---

## 7. Test Plan

- Unit tests for key loading and signature verification helpers.
- CLI integration tests: sign -> publish -> pull -> verify succeeds.
- CLI integration tests: tamper -> pull/import -> verify fails.

---

## 8. Rollout Plan

1. Phase 1: Add keyring loader and `--skip-verify` flag (default verify in production).
2. Phase 2: Ship trusted keyring in installer and document key management.
3. Phase 3: Remove `--skip-verify` for production builds after adoption.

---

## 9. Open Questions

1. Who owns publishing and rotation of trusted keys for releases?
2. Should `AOS_DEBUG_SKIP_KERNEL_SIG` allow bypassing adapter signature verification in dev?
