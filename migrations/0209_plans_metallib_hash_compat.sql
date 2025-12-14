-- Storage migration compat:
-- The KV migration tool expects `plans.metallib_hash_b3`, but the canonical schema uses `layout_hash_b3`.
-- Add the missing column and backfill from `layout_hash_b3`.

ALTER TABLE plans
ADD COLUMN metallib_hash_b3 TEXT;

UPDATE plans
SET metallib_hash_b3 = layout_hash_b3
WHERE metallib_hash_b3 IS NULL;
