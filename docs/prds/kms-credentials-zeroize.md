# PRD: Apply Zeroize Pattern to KMS Credentials

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** crates/adapteros-crypto/KMS_TESTING_GUIDE.md, crates/adapteros-crypto/src/secret.rs

---

## 1. Problem Statement

KMS credential material is currently stored as plain strings in `KmsCredentials`, which makes secrets visible in debug output and leaves them in memory after drop. This is a security risk for local logs, error handling, and memory inspection scenarios.

## 2. Non-Goals

- Implementing new KMS providers or cloud integrations.
- Changing the KMS request/response protocol.
- Introducing a new credential storage subsystem (keychain, vault, etc.).

## 3. Proposed Approach

- Wrap secret fields in `KmsCredentials` with `SensitiveData` so sensitive bytes are zeroized on drop.
- Implement a custom `Debug` for `KmsCredentials` that redacts secret fields.
- Block serialization/deserialization of `KmsCredentials` to prevent accidental export of secrets.
- Update tests to assert redaction and non-serialization behavior.

## 4. Acceptance Criteria

- Debug output for `KmsCredentials` and `KmsConfig` includes `[REDACTED]` and does not contain raw secrets.
- Secret fields in `KmsCredentials` use `SensitiveData` and zeroize on drop.
- `serde` serialization and deserialization for `KmsCredentials` fail with a security error.
- Existing KMS provider behavior remains unchanged for supported backends.

## 5. Test Plan

- Update `crates/adapteros-crypto/tests/kms_security.rs` to assert redaction and non-serialization.
- Update `crates/adapteros-crypto/src/providers/kms.rs` unit tests to expect serialization failure.
- Run `cargo test -p adapteros-crypto`.

## 6. Rollout Plan

- Ship as a security-focused patch in the crypto crate.
- No config or runtime flags required.
- Monitor logs for any unexpected serialization attempts and update callers if needed.
