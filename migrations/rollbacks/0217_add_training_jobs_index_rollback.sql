-- Rollback for Migration 0217: Training Jobs Index
-- Optimization-ID: dbopt-0217-training-jobs-dashboard-index
-- Purpose: Remove specialized index for tenant-scoped training job listings.
-- Warning: Dropping indexes may degrade UI dashboard performance.

DROP INDEX IF EXISTS idx_training_jobs_tenant_status_created_adapter;

ANALYZE repository_training_jobs;

