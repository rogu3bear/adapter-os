-- Migration: Add model_cache_identity_v2_digest_b3 column to inference_trace_receipts
-- PRD-06: ModelCacheIdentity v2 Canonicalization and Enforcement
--
-- This column stores the BLAKE3 digest of the ModelCacheIdentityV2 canonical bytes,
-- binding each receipt to the exact kernel/quant/fusion/tokenizer/tenant/worker combination
-- used during inference.

ALTER TABLE inference_trace_receipts
    ADD COLUMN model_cache_identity_v2_digest_b3 BYTEA;
