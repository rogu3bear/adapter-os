-- Deterministic version-aware canary weighting for adapter routing.
--
-- Adds per-version canary weights and adapter-level effective weight projection.
-- Backfill is best-effort: only unique (tenant_id, repo_id, version) matches are linked.

PRAGMA foreign_keys = ON;

ALTER TABLE adapter_versions
ADD COLUMN version_weight REAL NOT NULL DEFAULT 1.0
CHECK (version_weight >= 0.0 AND version_weight <= 2.0);

ALTER TABLE adapters
ADD COLUMN adapter_version_id TEXT;

ALTER TABLE adapters
ADD COLUMN effective_version_weight REAL NOT NULL DEFAULT 1.0
CHECK (effective_version_weight >= 0.0 AND effective_version_weight <= 2.0);

WITH unique_matches AS (
    SELECT
        tenant_id,
        repo_id,
        version,
        MIN(id) AS version_id,
        MIN(version_weight) AS version_weight
    FROM adapter_versions
    GROUP BY tenant_id, repo_id, version
    HAVING COUNT(*) = 1
)
UPDATE adapters
SET adapter_version_id = (
        SELECT um.version_id
        FROM unique_matches um
        WHERE um.tenant_id = adapters.tenant_id
          AND um.repo_id = adapters.repo_id
          AND um.version = adapters.version
    ),
    effective_version_weight = COALESCE((
        SELECT um.version_weight
        FROM unique_matches um
        WHERE um.tenant_id = adapters.tenant_id
          AND um.repo_id = adapters.repo_id
          AND um.version = adapters.version
    ), 1.0)
WHERE adapter_version_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_adapters_tenant_adapter_version_id
    ON adapters(tenant_id, adapter_version_id);

CREATE INDEX IF NOT EXISTS idx_adapter_versions_repo_version_weight
    ON adapter_versions(repo_id, version_weight DESC);
