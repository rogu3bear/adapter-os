-- Migration: 20260204130000
-- Purpose: Add receipt V7 fields for determinism envelope and cache/retrieval bindings (Postgres)

ALTER TABLE inference_trace_receipts ADD COLUMN tokenizer_hash_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN tokenizer_version TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN tokenizer_normalization TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN model_build_hash_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN adapter_build_hash_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN decode_algo TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN temperature_q15 INTEGER;
ALTER TABLE inference_trace_receipts ADD COLUMN top_p_q15 INTEGER;
ALTER TABLE inference_trace_receipts ADD COLUMN top_k INTEGER;
ALTER TABLE inference_trace_receipts ADD COLUMN seed_digest_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN sampling_backend TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN thread_count INTEGER;
ALTER TABLE inference_trace_receipts ADD COLUMN reduction_strategy TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN stop_eos_q15 INTEGER;
ALTER TABLE inference_trace_receipts ADD COLUMN stop_window_digest_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN cache_scope TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN cached_prefix_digest_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN cached_prefix_len INTEGER;
ALTER TABLE inference_trace_receipts ADD COLUMN cache_key_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN retrieval_merkle_root_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN retrieval_order_digest_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN tool_call_inputs_digest_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN tool_call_outputs_digest_b3 BYTEA;
ALTER TABLE inference_trace_receipts ADD COLUMN disclosure_level TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN receipt_signing_kid TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN receipt_signed_at TEXT;

CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_tokenizer_hash_b3
    ON inference_trace_receipts (tokenizer_hash_b3)
    WHERE tokenizer_hash_b3 IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_cache_key_b3
    ON inference_trace_receipts (cache_key_b3)
    WHERE cache_key_b3 IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_retrieval_merkle_root_b3
    ON inference_trace_receipts (retrieval_merkle_root_b3)
    WHERE retrieval_merkle_root_b3 IS NOT NULL;
