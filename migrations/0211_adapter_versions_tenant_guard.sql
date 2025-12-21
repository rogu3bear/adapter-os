-- Migration 0211: Enforce tenant isolation for adapter_versions -> adapter_repositories
--
-- Problem: adapter_versions has both repo_id and tenant_id, but the FK on repo_id
-- does not enforce that the referenced adapter_repositories row belongs to the
-- same tenant. This permits cross-tenant references.
--
-- Fix: Add trigger-based guards (SQLite cannot add composite FKs via ALTER).

PRAGMA foreign_keys = ON;

-- Guard: version tenant must match repository tenant (insert)
CREATE TRIGGER IF NOT EXISTS trg_adapter_versions_repo_tenant_match_insert
BEFORE INSERT ON adapter_versions
FOR EACH ROW
WHEN (
    SELECT tenant_id FROM adapter_repositories WHERE id = NEW.repo_id
) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: adapter_versions.tenant_id must match adapter_repositories.tenant_id');
END;

-- Guard: version tenant must match repository tenant (update repo_id)
CREATE TRIGGER IF NOT EXISTS trg_adapter_versions_repo_tenant_match_update_repo
BEFORE UPDATE OF repo_id ON adapter_versions
FOR EACH ROW
WHEN (
    SELECT tenant_id FROM adapter_repositories WHERE id = NEW.repo_id
) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: adapter_versions.tenant_id must match adapter_repositories.tenant_id');
END;

-- Guard: version tenant must match repository tenant (update tenant_id)
CREATE TRIGGER IF NOT EXISTS trg_adapter_versions_repo_tenant_match_update_tenant
BEFORE UPDATE OF tenant_id ON adapter_versions
FOR EACH ROW
WHEN (
    SELECT tenant_id FROM adapter_repositories WHERE id = NEW.repo_id
) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: adapter_versions.tenant_id must match adapter_repositories.tenant_id');
END;

