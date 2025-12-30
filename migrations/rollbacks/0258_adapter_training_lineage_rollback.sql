-- Rollback Migration 0258: Remove adapter_training_lineage table

DROP INDEX IF EXISTS idx_atl_dataset_hash;
DROP INDEX IF EXISTS idx_atl_tenant_id;
DROP INDEX IF EXISTS idx_atl_training_job_id;
DROP INDEX IF EXISTS idx_atl_dataset_version_id;
DROP INDEX IF EXISTS idx_atl_dataset_id;
DROP INDEX IF EXISTS idx_atl_adapter_id;

DROP TABLE IF EXISTS adapter_training_lineage;
