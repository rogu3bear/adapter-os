-- Migration: Add runtime metadata columns to workers table
-- Replaces runtime patch: ensure_worker_runtime_metadata_columns()
--
-- These fields are populated during worker registration and surfaced by listing endpoints.

ALTER TABLE workers ADD COLUMN backend TEXT;
ALTER TABLE workers ADD COLUMN model_hash_b3 TEXT;
ALTER TABLE workers ADD COLUMN capabilities_json TEXT;

CREATE INDEX IF NOT EXISTS idx_workers_model_hash_b3 ON workers(model_hash_b3);
