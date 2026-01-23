-- Migration: Normalize model paths to use consistent format
-- Strips leading './' prefix and migrates from old model-cache location

-- Step 1: Strip leading './' prefix for consistent storage
UPDATE models
SET model_path = SUBSTR(model_path, 3)
WHERE model_path LIKE './%';

-- Step 2: Migrate paths from old model-cache location to new var/models
UPDATE models
SET model_path = REPLACE(model_path, 'var/model-cache/models', 'var/models')
WHERE model_path LIKE '%model-cache%';

-- Step 3: Track migration timestamp
UPDATE models
SET updated_at = datetime('now')
WHERE model_path LIKE 'var/models%';
