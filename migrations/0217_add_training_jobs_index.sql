-- Add specialized index for tenant-scoped training job listings to accelerate UI dashboard performance
-- Migration: 0217
-- Optimization-ID: dbopt-0217-training-jobs-dashboard-index

CREATE INDEX IF NOT EXISTS idx_training_jobs_tenant_status_created_adapter
    ON repository_training_jobs(tenant_id, status, created_at DESC, adapter_id);

ANALYZE repository_training_jobs;
