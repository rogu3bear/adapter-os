-- Migration: Add CoreML replay metadata fields and fallback backend tracking
-- Adds structured fields to capture CoreML compute selection and GPU usage,
-- plus fallback backend identification for replay and audit trails.

ALTER TABLE inference_replay_metadata
    ADD COLUMN coreml_compute_preference TEXT;

ALTER TABLE inference_replay_metadata
    ADD COLUMN coreml_compute_units TEXT;

ALTER TABLE inference_replay_metadata
    ADD COLUMN coreml_gpu_used INTEGER;

ALTER TABLE inference_replay_metadata
    ADD COLUMN fallback_backend TEXT;


