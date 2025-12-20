# Tenant-Based Partitioning Strategy: Clustered Indexing

**Date:** 2025-12-17
**Status:** Proposed

## Problem Statement

As AdapterOS scales to thousands of tenants, query performance must remain consistent (sub-millisecond) and isolated. 

Current architecture uses standard SQLite B-Tree tables (rowid-based). While Migration 0210 introduced composite indexes (e.g., `(tenant_id, active, ...)`), the underlying table data is stored in insertion order (roughly time-based, mixed tenants).

This leads to:
1.  **Fragmentation**: A tenant's data is scattered across thousands of database pages.
2.  **Random I/O**: `SELECT *` queries for a tenant require fetching many distinct pages (High Random Read amplification).
3.  **Cache Pollution**: Loading a page for Tenant A brings in data for Tenant B, C, D (low buffer pool efficiency).

## Solution: Logical Partitioning via `WITHOUT ROWID`

We will implement **Tenant-Clustered Tables** using SQLite's `WITHOUT ROWID` optimization. By defining the Primary Key starting with `tenant_id`, we force the physical B-Tree layout to cluster data by tenant.

### Architecture

**Old Schema:**
```sql
CREATE TABLE adapters (
    id TEXT PRIMARY KEY, -- Clustered by ROWID (random insertion order)
    tenant_id TEXT,
    ...
);
CREATE INDEX idx_adapters_tenant ...; -- Secondary index
```

**New Partitioned Schema:**
```sql
CREATE TABLE adapters (
    id TEXT,
    tenant_id TEXT,
    ...,
    PRIMARY KEY (tenant_id, id) -- Clustered by Tenant, then ID
) WITHOUT ROWID;

CREATE UNIQUE INDEX idx_adapters_id ON adapters(id); -- Support FKs
```

#### Optimization for Time-Series Data (e.g. Telemetry)
For write-heavy, time-ordered data, we use `(tenant_id, created_at, id)` as the Primary Key. This optimizes both `ORDER BY created_at` queries and insertion locality.

```sql
CREATE TABLE telemetry_bundles (
    ...,
    PRIMARY KEY (tenant_id, created_at, id)
) WITHOUT ROWID;
```

### Benefits

1.  **Physical Partitioning**: All data for `tenant-a` is stored in a contiguous range of B-Tree pages.
2.  **Linear Scaling**: Querying Tenant A only touches Tenant A's pages. Performance is independent of total DB size or other tenants' activity.
3.  **IO Efficiency**: `SELECT *` becomes a sequential scan within the tenant's partition.
4.  **Migration 0210 Compatibility**: 
    -   Composite indexes from 0210 (e.g., `tenant_id, active, ...`) continue to work.
    -   They now point to `(tenant_id, id)` instead of `rowid`.
    -   For `SELECT *`, the lookup is optimized by the main table structure.

## Migration Strategy

Future migrations will apply this pattern to high-volume tables:
-   `telemetry_bundles` (Partitioned by `tenant_id, created_at, id`)
-   `audits`
-   `incidents`
-   `adapters` (Partitioned by `tenant_id, id`)
-   `training_jobs`

### Migration Steps (Per Table)

1.  **Create New Table**: Define with `PRIMARY KEY (tenant_id, id)` (or time variant) `WITHOUT ROWID`.
2.  **Dual Write / Backfill**: Copy existing data.
3.  **Switch**: Rename tables.
4.  **Restore Indexes**: Re-apply Migration 0210 indexes (if not subsumed by PK).
5.  **Restore Triggers**: Re-apply tenant isolation triggers.

## Performance Impact

| Metric | Rowid Table (Current) | Tenant-Clustered (New) |
|--------|-----------------------|------------------------|
| **Data Locality** | Random / Time-ordered | Contiguous per Tenant |
| **I/O Pattern** | Random Seek | Sequential Scan |
| **Buffer Hit Rate** | Lower (Cross-tenant noise) | Higher (Tenant-specific pages) |
| **Scaling** | Degrades with Fragmentation | Linear with Tenant Data |

MLNavigator Inc 2025-12-17.


