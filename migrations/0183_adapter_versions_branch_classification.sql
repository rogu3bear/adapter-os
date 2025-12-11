-- Track branch classification for promotion guardrails (protected/high/sandbox)
PRAGMA foreign_keys = ON;

ALTER TABLE adapter_versions
ADD COLUMN branch_classification TEXT NOT NULL DEFAULT 'protected';

-- Normalize any null/empty values to protected
UPDATE adapter_versions
SET branch_classification = 'protected'
WHERE branch_classification IS NULL OR branch_classification = '';
