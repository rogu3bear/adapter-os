# PRD: Apply Zeroize Pattern to KMS Credentials

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-crypto/src/secret.rs`, `crates/adapteros-crypto/src/providers/kms.rs`

---

## 1. Summary

KMS credential structs currently hold secrets as plain strings. This PRD defines how to apply the existing zeroize wrappers to KMS credentials to prevent accidental logging and ensure memory zeroization on drop.

---

## 2. Problem Statement

Sensitive credentials are stored as plain `String` fields and can appear in debug output or remain in memory after use. The crate already provides `SensitiveData` and `SecretKey` wrappers but they are not used by KMS credentials.

---

## 3. Goals

- Wrap secret fields in `SensitiveData` (or equivalent) for automatic zeroization.
- Redact sensitive fields in debug output.
- Keep KMS functionality unchanged for consumers.

---

## 4. Non-Goals

- Introducing new crypto algorithms or KMS providers.
- Changing credential acquisition mechanisms.
- Persisting credentials to disk.

---

## 5. Proposed Approach

- Update `KmsCredentials` variants to wrap secret fields with `SensitiveData`.
- Implement custom `Debug` to redact secrets.
- Provide helper constructors/accessors to avoid leaking raw strings.
- Add tests that assert redaction and zeroization behavior.

---

## 6. Acceptance Criteria

- Debug output for KMS credentials never prints secrets.
- Secret fields zeroize on drop.
- Serialization of credential types is disallowed or fails by design.
- Existing KMS flows compile without API breakage.

---

## 7. Test Plan

- Unit test for debug redaction of AWS/GCP/Azure credentials.
- Unit test to confirm zeroization of `SensitiveData` fields.
- Regression test for current KMS provider integrations.

---

## 8. Rollout Plan

1. Add zeroize wrappers and custom Debug.
2. Update tests and docs.
3. Monitor for any integration regressions.

---

## 9. Follow-up Tasks (Tracked)

- TASK-1: Wrap secret fields in `SensitiveData` for all KMS variants.
  - Acceptance: secrets are not stored as plain `String`.
- TASK-2: Add redacted `Debug` implementation.
  - Acceptance: debug output shows `[REDACTED]` for secrets.
- TASK-3: Add unit tests for redaction and zeroization.
  - Acceptance: tests fail if secrets appear in debug strings.
