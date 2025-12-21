-- Migration 0124: Backfill base_model_id from metadata_json
-- Purpose: Populate base_model_id for existing adapters where it can be extracted from metadata
-- Created: 2025-12-01
-- Dependencies: adapters (0001), models (0042), base_model_id column (0098)

-- Backfill base_model_id from metadata_json for adapters that have it stored there
UPDATE adapters
SET base_model_id = json_extract(metadata_json, '$.base_model_id')
WHERE base_model_id IS NULL
  AND metadata_json IS NOT NULL
  AND json_extract(metadata_json, '$.base_model_id') IS NOT NULL;

-- Also try to match adapters to models by name pattern for legacy adapters
-- This is a best-effort heuristic match
UPDATE adapters
SET base_model_id = (
    SELECT m.id FROM models m
    WHERE adapters.name LIKE '%' || m.name || '%'
       OR adapters.adapter_id LIKE '%' || m.name || '%'
    ORDER BY length(m.name) DESC  -- Prefer longer (more specific) matches
    LIMIT 1
)
WHERE base_model_id IS NULL;
