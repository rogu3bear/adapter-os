-- ============================================================================
-- Evidence Entries Table (PRD-DATA-01)
-- ============================================================================
-- File: migrations/0085_evidence_entries.sql
-- Purpose: Track evidence entries for datasets and adapters (cp-evidence-004)
-- Status: New migration for PRD-DATA-01 Dataset Lab & Evidence Explorer
-- Dependencies: training_datasets (0041), adapters (0001)
-- Notes: Links evidence (docs, tickets, commits, approvals) to datasets/adapters
-- ============================================================================

-- Evidence entries table for tracking provenance and compliance
CREATE TABLE IF NOT EXISTS evidence_entries (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT REFERENCES training_datasets(id) ON DELETE CASCADE,
    adapter_id TEXT REFERENCES adapters(id) ON DELETE CASCADE,
    evidence_type TEXT NOT NULL CHECK(evidence_type IN ('doc', 'ticket', 'commit', 'policy_approval', 'data_agreement', 'review', 'audit', 'other')),
    reference TEXT NOT NULL,  -- URL, commit SHA, ticket ID, document path
    description TEXT,
    confidence TEXT NOT NULL DEFAULT 'medium' CHECK(confidence IN ('high', 'medium', 'low')),
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata_json TEXT,  -- Additional structured metadata
    CHECK (dataset_id IS NOT NULL OR adapter_id IS NOT NULL)  -- At least one must be set
);

-- Indices for common queries
CREATE INDEX idx_evidence_entries_dataset ON evidence_entries(dataset_id);
CREATE INDEX idx_evidence_entries_adapter ON evidence_entries(adapter_id);
CREATE INDEX idx_evidence_entries_type ON evidence_entries(evidence_type);
CREATE INDEX idx_evidence_entries_confidence ON evidence_entries(confidence);
CREATE INDEX idx_evidence_entries_created_at ON evidence_entries(created_at DESC);

-- Dataset-to-adapter mapping for tracking which datasets trained which adapters
CREATE TABLE IF NOT EXISTS dataset_adapter_links (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    adapter_id TEXT NOT NULL REFERENCES adapters(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL CHECK(link_type IN ('training', 'eval', 'validation', 'test')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(dataset_id, adapter_id, link_type)
);

-- Indices for link queries
CREATE INDEX idx_dataset_adapter_links_dataset ON dataset_adapter_links(dataset_id);
CREATE INDEX idx_dataset_adapter_links_adapter ON dataset_adapter_links(adapter_id);
CREATE INDEX idx_dataset_adapter_links_type ON dataset_adapter_links(link_type);
