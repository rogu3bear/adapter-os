-- ============================================================================
-- Extend Training Datasets Schema (PRD-DATA-01)
-- ============================================================================
-- File: migrations/0084_extend_training_datasets.sql
-- Purpose: Add dataset type, purpose, source, ownership fields for Dataset Lab
-- Status: New migration for PRD-DATA-01 Dataset Lab & Evidence Explorer
-- Dependencies: training_datasets table (migration 0041)
-- Notes: Extends datasets to be first-class assets with roles and evidence ties
-- ============================================================================

-- Add dataset_type column to categorize dataset purpose
ALTER TABLE training_datasets ADD COLUMN dataset_type TEXT NOT NULL DEFAULT 'training'
    CHECK(dataset_type IN ('training', 'eval', 'red_team', 'logs', 'other'));

-- Add purpose column for human-readable description
ALTER TABLE training_datasets ADD COLUMN purpose TEXT;

-- Add source_location for provenance tracking (URI/path/Git ref)
ALTER TABLE training_datasets ADD COLUMN source_location TEXT;

-- Add collection_method to track how dataset was created
ALTER TABLE training_datasets ADD COLUMN collection_method TEXT NOT NULL DEFAULT 'manual'
    CHECK(collection_method IN ('manual', 'sync', 'api', 'pipeline', 'scrape', 'other'));

-- Add ownership for accountability (email or team ID)
ALTER TABLE training_datasets ADD COLUMN ownership TEXT;

-- Add tenant_id for multi-tenancy support
ALTER TABLE training_datasets ADD COLUMN tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE;

-- Create indices for common queries
CREATE INDEX idx_training_datasets_type ON training_datasets(dataset_type);
CREATE INDEX idx_training_datasets_tenant ON training_datasets(tenant_id);
CREATE INDEX idx_training_datasets_ownership ON training_datasets(ownership);
