## Privacy and Secret Handling

- Encrypt prompt/response at rest via per-tenant keys served by `aos-secd`; enable with `AOS_CRYPTO_AT_REST=1` and set `AOS_SECD_SOCKET` if using a non-default socket.
- Keyed BLAKE3 digests are stored alongside ciphertext; `AOS_CRYPTO_DIGEST_ONLY=1` stores only digests (no ciphertext) for regulated tenants.
- Tests/development can use the local fallback (`AOS_CRYPTO_FAKE=1`) to avoid a running secd daemon while still exercising encryption/digest code paths.
- Exporting tenant keys requires `AOS_SECD_EXPORT_TOKEN` to match the permission token supplied to the export request; other operations are denied.
- Logging of prompt/response fields is redacted (`[redacted]`) to avoid PII leakage; decrypted values are surfaced only in-memory for callers that have access.

MLNavigator Inc 2025-12-11.
