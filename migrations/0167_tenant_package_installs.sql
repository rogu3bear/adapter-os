-- Tenant-level installation state for adapter packages
-- Ensures packages are explicitly enabled per tenant with isolation guards

CREATE TABLE IF NOT EXISTS tenant_package_installs (
    tenant_id TEXT NOT NULL,
    package_id TEXT NOT NULL,
    installed_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, package_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (package_id) REFERENCES adapter_packages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tenant_package_installs_tenant ON tenant_package_installs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_package_installs_package ON tenant_package_installs(package_id);

-- Enforce that packages cannot be installed across tenants
CREATE TRIGGER IF NOT EXISTS validate_package_install_tenant
BEFORE INSERT ON tenant_package_installs
FOR EACH ROW
WHEN (SELECT tenant_id FROM adapter_packages WHERE id = NEW.package_id) IS NOT NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Package tenant mismatch for install');
END;

