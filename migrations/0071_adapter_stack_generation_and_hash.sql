-- Migration 0071: Add generation and hash fields to adapter_stacks (PRD 3)
-- Adds generation counter and content hash for stack validity tracking

-- Add generation counter (incremented on each activation)
ALTER TABLE adapter_stacks ADD COLUMN generation INTEGER NOT NULL DEFAULT 0;

-- Add stack hash (computed from adapter_ids + per-adapter content hashes)
ALTER TABLE adapter_stacks ADD COLUMN stack_hash TEXT;

-- Index for hash-based lookups
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_hash ON adapter_stacks(stack_hash);

-- Update existing stacks to have initial generation=0
UPDATE adapter_stacks SET generation = 0 WHERE generation IS NULL;
