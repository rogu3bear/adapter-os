-- Allow multiple tags per adapter version.
-- Tag uniqueness remains per-repo (repo_id, tag_name) for deterministic tag resolution.

PRAGMA foreign_keys = ON;

-- Migration 0176 added a UNIQUE index on (version_id), which prevented multiple tags
-- from pointing to the same version. Drop it and replace with a non-unique lookup index.
DROP INDEX IF EXISTS idx_adapter_version_tags_version;

CREATE INDEX IF NOT EXISTS idx_adapter_version_tags_version
    ON adapter_version_tags(version_id);
