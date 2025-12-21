-- Document Processing State Machine Enhancement
-- Adds failed state support, processing lock fields, and retry mechanism

-- Add new columns to documents table
ALTER TABLE documents ADD COLUMN error_message TEXT;
ALTER TABLE documents ADD COLUMN error_code TEXT;
ALTER TABLE documents ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE documents ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3;
ALTER TABLE documents ADD COLUMN processing_started_at TEXT;
ALTER TABLE documents ADD COLUMN processing_completed_at TEXT;

-- Add unique constraint for atomic deduplication (tenant-scoped)
-- This prevents TOCTOU race conditions on concurrent uploads with same content
CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_tenant_content_hash
    ON documents(tenant_id, content_hash);

-- Index for efficiently finding retryable failed documents
CREATE INDEX IF NOT EXISTS idx_documents_failed_retryable
    ON documents(tenant_id, status) WHERE status = 'failed' AND retry_count < max_retries;

-- Index for finding documents stuck in processing state (for cleanup)
CREATE INDEX IF NOT EXISTS idx_documents_processing_started
    ON documents(processing_started_at) WHERE status = 'processing';
