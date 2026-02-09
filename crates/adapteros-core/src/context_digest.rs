//! Receipt context digest serialization.
//!
//! This module is the single source of truth for the `context_digest` byte layout used by:
//! - the worker when emitting receipts / computing prefix-cache keys, and
//! - third-party verification paths that recompute digests from claimed inputs.
//!
//! IMPORTANT: This layout is part of determinism / verification semantics. Any change here will
//! change digest outputs and can invalidate existing receipts.

use crate::B3Hash;

/// Serialize the receipt context into bytes for `context_digest`.
///
/// Byte layout (in order):
/// - `tenant_namespace` UTF-8 bytes (no length prefix)
/// - `stack_hash` raw bytes
/// - if `tokenizer_hash_b3` is Some:
///   - `tokenizer_hash_b3` raw bytes
///   - if `tokenizer_version` is Some: `u32_le(len)` + UTF-8 bytes
///   - if `tokenizer_normalization` is Some: `u32_le(len)` + UTF-8 bytes
/// - `u32_le(prompt_tokens.len())`
/// - each prompt token as `u32_le(token)`
pub fn compute_context_digest_bytes(
    tenant_namespace: &str,
    stack_hash: &[u8],
    tokenizer_hash_b3: Option<&[u8]>,
    tokenizer_version: Option<&str>,
    tokenizer_normalization: Option<&str>,
    prompt_tokens: &[u32],
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        tenant_namespace.len()
            + stack_hash.len()
            + 4
            + (prompt_tokens.len() * 4)
            // headroom for optional tokenizer identity strings
            + 96,
    );

    buf.extend_from_slice(tenant_namespace.as_bytes());
    buf.extend_from_slice(stack_hash);

    if let Some(tokenizer_hash) = tokenizer_hash_b3 {
        buf.extend_from_slice(tokenizer_hash);
        if let Some(version) = tokenizer_version {
            buf.extend_from_slice(&(version.len() as u32).to_le_bytes());
            buf.extend_from_slice(version.as_bytes());
        }
        if let Some(norm) = tokenizer_normalization {
            buf.extend_from_slice(&(norm.len() as u32).to_le_bytes());
            buf.extend_from_slice(norm.as_bytes());
        }
    }

    buf.extend_from_slice(&(prompt_tokens.len() as u32).to_le_bytes());
    for token in prompt_tokens {
        buf.extend_from_slice(&token.to_le_bytes());
    }

    buf
}

/// Compute the BLAKE3 context digest for a receipt context.
pub fn compute_context_digest(
    tenant_namespace: &str,
    stack_hash: &[u8],
    tokenizer_hash_b3: Option<&[u8]>,
    tokenizer_version: Option<&str>,
    tokenizer_normalization: Option<&str>,
    prompt_tokens: &[u32],
) -> B3Hash {
    B3Hash::hash(&compute_context_digest_bytes(
        tenant_namespace,
        stack_hash,
        tokenizer_hash_b3,
        tokenizer_version,
        tokenizer_normalization,
        prompt_tokens,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_context_digest_worker_layout() {
        let tenant_namespace = "test-tenant";
        let stack_hash = B3Hash::hash(b"stack-xyz");
        let tokenizer_hash = B3Hash::hash(b"tokenizer-abc");
        let prompt_tokens = vec![11u32, 22u32, 33u32];

        let tokenizer_hash_bytes: &[u8] = tokenizer_hash.as_bytes();
        let digest = compute_context_digest(
            tenant_namespace,
            stack_hash.as_bytes(),
            Some(tokenizer_hash_bytes),
            None,
            None,
            &prompt_tokens,
        );

        // NOTE: If this changes, existing receipts/bundles may no longer verify.
        assert_eq!(
            digest.to_hex(),
            "cda16825ebb7a23df8b6a3e5c5a4a6558102dc92dd39c74a71133ac1902caf53",
            "Context digest must remain stable"
        );
    }
}
