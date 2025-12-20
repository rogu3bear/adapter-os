# Tenant Isolation Performance Training

## Overview

This guide covers the performance optimization strategies for tenant-isolated queries in AdapterOS, specifically focusing on the improvements introduced in Migration 0210.

## Core Concepts

### 1. Composite Indexes
We use composite indexes to ensure queries are filtered by tenant *and* ordered efficiently without temporary B-trees.

**Example:**
```sql
-- Bad: Uses separate indexes or file sort
SELECT * FROM adapters WHERE tenant_id = ? ORDER BY created_at DESC;

-- Good: Uses idx_adapters_tenant_active_tier_created
SELECT * FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC;
```

### 2. Covering Indexes
For high-frequency lookups (like hash-based deduplication), we include all required columns in the index itself to avoid looking up the main table row.

**Example:** `idx_adapters_tenant_hash_active_covering` includes `id`, `name`, `tier`, etc.

### 3. EXPLAIN QUERY PLAN
Always verify your queries use the expected index.

```bash
sqlite3 var/aos-cp.sqlite3 "EXPLAIN QUERY PLAN SELECT ..."
```

Look for:
- `USING INDEX idx_...` (Good)
- `SCAN TABLE` (Bad, unless table is tiny)
- `USE TEMP B-TREE` (Bad, implies sorting on disk/mem instead of index order)

## Hands-on Exercises

1. **Analyze an Adapter Listing**
   Run the monitor script: `./scripts/monitor_index_performance.sh` and observe the output for the adapter listing query.

2. **Detect a Regression**
   Modify a query in `adapters.rs` to remove `ORDER BY tier ASC`. Run the test `test_query_plan_analysis`. It should fail or warn about missing index usage.

3. **Optimize a New Query**
   When adding a new tenant-scoped query, first create a migration for a composite index starting with `tenant_id`, then `filter_column`, then `sort_column`.

## Best Practices

- Always start indexes with `tenant_id`.
- Use `INDEXED BY` hints for critical queries to prevent planner regression.
- Keep `active = 1` filters in both query and partial index definition.

---
MLNavigator Inc 2025-12-17.


