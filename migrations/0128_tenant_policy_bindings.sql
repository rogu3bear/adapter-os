-- Per-tenant policy pack bindings
-- Allows tenants to enable/disable policy packs independently

CREATE TABLE IF NOT EXISTS tenant_policy_bindings (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    policy_pack_id TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'global',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_by TEXT,
    UNIQUE(tenant_id, policy_pack_id, scope)
);

CREATE INDEX IF NOT EXISTS idx_tpb_tenant ON tenant_policy_bindings(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tpb_policy ON tenant_policy_bindings(policy_pack_id);
CREATE INDEX IF NOT EXISTS idx_tpb_enabled ON tenant_policy_bindings(tenant_id, enabled);

-- Backfill existing tenants with default bindings
-- Core policies (egress, determinism, isolation, evidence) = enabled
-- All others = disabled
INSERT INTO tenant_policy_bindings (id, tenant_id, policy_pack_id, scope, enabled, created_by)
SELECT
    lower(hex(randomblob(16))),
    t.id,
    p.policy_id,
    'global',
    CASE WHEN p.policy_id IN ('egress', 'determinism', 'isolation', 'evidence') THEN 1 ELSE 0 END,
    'migration'
FROM tenants t
CROSS JOIN (
    SELECT 'egress' as policy_id UNION ALL
    SELECT 'determinism' UNION ALL
    SELECT 'router' UNION ALL
    SELECT 'evidence' UNION ALL
    SELECT 'refusal' UNION ALL
    SELECT 'numeric' UNION ALL
    SELECT 'rag' UNION ALL
    SELECT 'isolation' UNION ALL
    SELECT 'telemetry' UNION ALL
    SELECT 'retention' UNION ALL
    SELECT 'performance' UNION ALL
    SELECT 'memory' UNION ALL
    SELECT 'artifacts' UNION ALL
    SELECT 'secrets' UNION ALL
    SELECT 'build_release' UNION ALL
    SELECT 'compliance' UNION ALL
    SELECT 'incident' UNION ALL
    SELECT 'output' UNION ALL
    SELECT 'adapters' UNION ALL
    SELECT 'deterministic_io' UNION ALL
    SELECT 'drift' UNION ALL
    SELECT 'mplora' UNION ALL
    SELECT 'naming' UNION ALL
    SELECT 'dependency_security'
) p;
