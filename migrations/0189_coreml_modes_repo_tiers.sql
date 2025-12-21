-- CoreML mode + repository tier defaults
-- Adds explicit CoreML mode and repo tier to repository policies for backend selection.

PRAGMA foreign_keys = ON;

ALTER TABLE adapter_repository_policies
    ADD COLUMN coreml_mode TEXT NOT NULL DEFAULT 'coreml_preferred'
    CHECK (coreml_mode IN ('coreml_strict', 'coreml_preferred', 'backend_auto'));

ALTER TABLE adapter_repository_policies
    ADD COLUMN repo_tier TEXT NOT NULL DEFAULT 'normal'
    CHECK (repo_tier IN ('high_assurance', 'normal', 'experimental'));
