-- Index for stack_id lookups on training jobs
-- Note: stack_id and adapter_id columns already added in migration 0118
-- This migration adds the missing stack_id index for chat bootstrap queries

-- Create index for stack_id lookups (adapter_id index already exists from 0118)
CREATE INDEX IF NOT EXISTS idx_training_jobs_stack_id ON repository_training_jobs(stack_id)
    WHERE stack_id IS NOT NULL;
