-- Migration: 0117_training_job_category_metadata
-- Purpose: Add category metadata and post-actions fields to repository_training_jobs
--          to support full Training Wizard integration.
--
-- This enables:
--   1. Category-specific adapter configuration (code, framework, codebase, docs, domain)
--   2. Post-training action configuration (package, register, tier)
--   3. Full field threading from UI through backend

-- Category and description fields
ALTER TABLE repository_training_jobs ADD COLUMN category TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN description TEXT;

-- Code adapter-specific fields
ALTER TABLE repository_training_jobs ADD COLUMN language TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN symbol_targets_json TEXT;

-- Framework adapter-specific fields
ALTER TABLE repository_training_jobs ADD COLUMN framework_id TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN framework_version TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN api_patterns_json TEXT;

-- Codebase adapter-specific fields
ALTER TABLE repository_training_jobs ADD COLUMN repo_scope TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN file_patterns_json TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN exclude_patterns_json TEXT;

-- Post-training actions configuration (JSON: {package, register, tier, adapters_root})
ALTER TABLE repository_training_jobs ADD COLUMN post_actions_json TEXT;

-- Index on category for filtering by adapter type
CREATE INDEX IF NOT EXISTS idx_training_jobs_category ON repository_training_jobs(category);

-- Composite index for category + status queries (e.g., "all running code adapter trainings")
CREATE INDEX IF NOT EXISTS idx_training_jobs_category_status ON repository_training_jobs(category, status);

-- Index on language for code adapter filtering
CREATE INDEX IF NOT EXISTS idx_training_jobs_language ON repository_training_jobs(language);

-- Index on framework_id for framework adapter filtering
CREATE INDEX IF NOT EXISTS idx_training_jobs_framework ON repository_training_jobs(framework_id);
