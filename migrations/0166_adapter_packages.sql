-- Adapter packages: user-visible named stacks
-- Binds a stack to a tenant with optional tags, domain, scope, and strength metadata

CREATE TABLE IF NOT EXISTS adapter_packages (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    stack_id TEXT NOT NULL,
    tags_json TEXT,
    domain TEXT,
    scope_path TEXT,
    adapter_strengths_json TEXT,
    determinism_mode TEXT,
    routing_determinism_mode TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (stack_id) REFERENCES adapter_stacks(id) ON DELETE CASCADE,
    UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_adapter_packages_tenant ON adapter_packages(tenant_id);
CREATE INDEX IF NOT EXISTS idx_adapter_packages_stack ON adapter_packages(stack_id);

-- Enforce stack/tenant alignment on insert
CREATE TRIGGER IF NOT EXISTS validate_package_stack_tenant
BEFORE INSERT ON adapter_packages
FOR EACH ROW
WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) IS NOT NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Stack tenant mismatch for package');
END;

-- Enforce stack/tenant alignment on update
CREATE TRIGGER IF NOT EXISTS validate_package_stack_tenant_update
BEFORE UPDATE OF stack_id ON adapter_packages
FOR EACH ROW
WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) IS NOT NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Stack tenant mismatch for package');
END;

-- Keep updated_at current on any update
CREATE TRIGGER IF NOT EXISTS adapter_packages_updated_at
AFTER UPDATE ON adapter_packages
FOR EACH ROW
BEGIN
    UPDATE adapter_packages
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;

