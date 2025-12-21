-- Cleanup: duplicate 0206 migration collision
-- Canonical migration: 0206_replay_dataset_version.sql
--
-- Ensure we end up with the canonical index name and do not error on re-application.

CREATE INDEX IF NOT EXISTS idx_irm_dataset_version_id
ON inference_replay_metadata(dataset_version_id);

DROP INDEX IF EXISTS idx_replay_meta_dataset_version;
