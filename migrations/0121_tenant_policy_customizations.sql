-- Tenant Policy Customizations
-- Migration: 0120
-- Created: 2025-11-30
-- Purpose: Enable tenant-scoped policy customization with approval workflow
-- Citation: AGENTS.md - Policy Studio feature for tenant-safe policy authoring

-- Tenant policy customizations table: stores tenant-specific policy parameter overrides
CREATE TABLE IF NOT EXISTS tenant_policy_customizations (
    id TEXT PRIMARY KEY,                        -- customization ID (UUID)
    tenant_id TEXT NOT NULL,                    -- tenant ID
    base_policy_type TEXT NOT NULL,             -- base policy type (e.g., "egress", "router", "determinism")
    customizations_json TEXT NOT NULL,          -- JSON object with field overrides
    status TEXT NOT NULL DEFAULT 'draft',       -- status: draft, pending_review, approved, rejected, active
    submitted_at TEXT,                          -- when submitted for review (RFC3339)
    reviewed_at TEXT,                           -- when reviewed (RFC3339)
    reviewed_by TEXT,                           -- user who reviewed (email)
    review_notes TEXT,                          -- review comments/notes
    activated_at TEXT,                          -- when activated (RFC3339)
    created_at TEXT NOT NULL,                   -- creation timestamp (RFC3339)
    created_by TEXT NOT NULL,                   -- user who created (email)
    updated_at TEXT NOT NULL,                   -- last update timestamp (RFC3339)
    metadata_json TEXT,                         -- additional metadata
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customizations_tenant ON tenant_policy_customizations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customizations_status ON tenant_policy_customizations(status);
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customizations_type ON tenant_policy_customizations(base_policy_type);
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customizations_tenant_type ON tenant_policy_customizations(tenant_id, base_policy_type);
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customizations_pending ON tenant_policy_customizations(status, submitted_at) WHERE status = 'pending_review';
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customizations_active ON tenant_policy_customizations(tenant_id, base_policy_type, status) WHERE status = 'active';

-- Policy customization history: audit trail for changes
CREATE TABLE IF NOT EXISTS tenant_policy_customization_history (
    id TEXT PRIMARY KEY,                        -- history entry ID (UUID)
    customization_id TEXT NOT NULL,             -- references tenant_policy_customizations(id)
    action TEXT NOT NULL,                       -- action: created, submitted, approved, rejected, activated, deactivated, updated
    performed_by TEXT NOT NULL,                 -- user who performed action
    performed_at TEXT NOT NULL,                 -- timestamp (RFC3339)
    old_status TEXT,                            -- previous status
    new_status TEXT,                            -- new status
    notes TEXT,                                 -- action notes/reason
    changes_json TEXT,                          -- JSON diff of changes (for updates)
    FOREIGN KEY (customization_id) REFERENCES tenant_policy_customizations(id) ON DELETE CASCADE
);

-- Index for history queries
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customization_history_customization ON tenant_policy_customization_history(customization_id, performed_at DESC);
CREATE INDEX IF NOT EXISTS idx_tenant_policy_customization_history_performed_at ON tenant_policy_customization_history(performed_at DESC);

