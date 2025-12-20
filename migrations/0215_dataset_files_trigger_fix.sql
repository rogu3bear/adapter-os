-- Fix trg_dataset_files_tenant_check to distinguish between:
-- 1. Dataset does not exist
-- 2. Dataset exists but has NULL tenant_id
--
-- The original trigger used a single check that conflated both cases:
--   WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) IS NULL
-- This returns NULL both when no row exists and when tenant_id is NULL.
--
-- The fix uses EXISTS to check for dataset existence first.

-- Drop the existing trigger
DROP TRIGGER IF EXISTS trg_dataset_files_tenant_check;

-- Create fixed trigger with clearer error messages
CREATE TRIGGER IF NOT EXISTS trg_dataset_files_tenant_check
BEFORE INSERT ON dataset_files
FOR EACH ROW
BEGIN
    SELECT CASE
        -- Check if dataset exists (distinct from NULL tenant_id)
        WHEN NOT EXISTS (SELECT 1 FROM training_datasets WHERE id = NEW.dataset_id)
        THEN RAISE(ABORT, 'Invalid dataset_id: dataset does not exist')
        -- Check if dataset has tenant_id set (required for tenant isolation)
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) IS NULL
        THEN RAISE(ABORT, 'Dataset has no tenant_id: set tenant_id on dataset before adding files')
        -- Check tenant_id match (only if file specifies tenant_id)
        WHEN NEW.tenant_id IS NOT NULL AND
             (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: dataset_file.tenant_id does not match dataset tenant')
    END;
END;

-- Also fix trg_dataset_statistics_tenant_check for consistency
DROP TRIGGER IF EXISTS trg_dataset_statistics_tenant_check;

CREATE TRIGGER IF NOT EXISTS trg_dataset_statistics_tenant_check
BEFORE INSERT ON dataset_statistics
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NOT EXISTS (SELECT 1 FROM training_datasets WHERE id = NEW.dataset_id)
        THEN RAISE(ABORT, 'Invalid dataset_id: dataset does not exist')
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) IS NULL
        THEN RAISE(ABORT, 'Dataset has no tenant_id: set tenant_id on dataset before adding statistics')
        WHEN NEW.tenant_id IS NOT NULL AND
             (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: dataset_statistics.tenant_id does not match dataset tenant')
    END;
END;
