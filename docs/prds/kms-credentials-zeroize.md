# PRD: Zeroize KMS Credentials

## Problem
KMS credential types store secrets as plain strings. This risks accidental exposure via debug output and leaves secrets resident in memory beyond their intended lifetime.

## Non-goals
- Redesigning KMS provider behavior or adding new providers.
- Changing how KMS credentials are sourced or configured.
- Implementing cloud KMS integrations beyond existing stubs.

## Proposed Approach
- Wrap secret fields in `KmsCredentials` with `SensitiveData`.
- Add a custom `Debug` implementation for `KmsCredentials` that redacts secrets.
- Implement `Zeroize` + `ZeroizeOnDrop` for `KmsCredentials` to guarantee zeroization on drop.
- Keep non-secret fields (e.g., access key ID, tenant ID) as `String`.
- Update tests to validate redaction, zeroize behavior, and serialization failures.
- Update KMS testing guide to reflect the new secret handling.

## Acceptance Criteria
- `format!("{:?}", kms_config)` does not include secret material and contains `[REDACTED]`.
- Secret fields in `KmsCredentials` are zeroized on drop.
- Serializing credentials with secrets fails (by design).
- Existing KMS behavior remains unchanged for non-secret flows.

## Test Plan
- `cargo test -p adapteros-crypto`.
- Verify updated redaction tests in `crates/adapteros-crypto/tests/kms_security.rs`.
- Verify zeroize test in `crates/adapteros-crypto/src/providers/kms.rs`.

## Rollout Plan
- Merge as a direct update with no feature flags.
- Monitor downstream usage for any unexpected reliance on credential serialization.
- If needed, add follow-up guidance for configuration loading in docs.
