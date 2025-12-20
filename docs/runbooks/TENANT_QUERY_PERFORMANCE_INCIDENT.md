# Runbook: Tenant Query Performance Incident

## Triggers
- Alert: "Slow query detected > 50ms"
- Dashboard: "Index Usage < 50%"
- User Report: "Adapter listing is slow"

## Diagnosis Steps

1. **Identify the Tenant and Query**
   Check `var/query_performance.log` or Splunk/Logs.
   ```bash
   grep "Slow query" var/query_performance.log | tail
   ```

2. **Analyze the Query Plan**
   Use the monitor script for the specific query or manually:
   ```bash
   sqlite3 var/aos-cp.sqlite3 "EXPLAIN QUERY PLAN SELECT ..."
   ```
   *If you see `SCAN TABLE`, the index is missing or ignored.*

3. **Check Index Health**
   ```bash
   sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
   sqlite3 var/aos-cp.sqlite3 "SELECT * FROM sqlite_stat1 WHERE idx = 'idx_adapters_tenant_active_tier_created';"
   ```
   *If stats are empty, run `ANALYZE`.*

## Mitigation

### Scenario A: Planner Ignoring Index
**Action:** Force `ANALYZE`.
```sql
ANALYZE adapters;
ANALYZE;
```
*Wait 1 minute. If no improvement, proceed to B.*

### Scenario B: Index Corrupted
**Action:** Rebuild Index.
```sql
REINDEX idx_adapters_tenant_active_tier_created;
```

### Scenario C: New Query Pattern
**Action:** The code might be using a new query variant not covered by Migration 0210.
1. Capture the exact SQL.
2. Create a hotfix migration `migrations/hotfix_XXXX_new_index.sql`.
3. Deploy.

## Rollback
If Migration 0210 caused regression:
1. Run `migrations/rollbacks/0210_tenant_scoped_query_optimization_rollback.sql`.
2. Restart Service.

---
MLNavigator Inc 2025-12-17.


