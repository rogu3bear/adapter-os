# adapteros-crypto

Ed25519 signing and BLAKE3 hashing for receipts and audit trails.

## Patterns

- **Receipts**: Sign with `sign_receipt()`, never raw `sign()`
- **Key loading**: Use `KeyProvider` trait, not direct file reads
- **Envelope encryption**: For data protection, not receipt binding

## Fail-Closed

All signing operations fail-closed. Missing keys = hard error, not fallback.
