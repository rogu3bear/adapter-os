# Migration Rollback Quick Reference

## Files Created
```
migrations/rollbacks/
├── 0021_process_security_compliance_rollback.sql
├── 0048_workspaces_and_messaging_rollback.sql
├── 0064_adapter_stacks_rollback.sql
├── 0070_routing_decisions_rollback.sql
├── README.md (comprehensive guide)
├── IMPLEMENTATION_SUMMARY.txt (detailed analysis)
└── QUICK_REFERENCE.md (this file)
```

## Rollback Execution Order

**MUST follow this sequence:**

```bash
# Step 1: Rollback routing decisions (depends on nothing)
sqlite3 aos.db < migrations/rollbacks/0070_routing_decisions_rollback.sql

# Step 2: Rollback adapter stacks (after routing_decisions)
sqlite3 aos.db < migrations/rollbacks/0064_adapter_stacks_rollback.sql

# Step 3: Rollback workspaces (independent)
sqlite3 aos.db < migrations/rollbacks/0048_workspaces_and_messaging_rollback.sql

# Step 4: Rollback security/compliance (independent)
sqlite3 aos.db < migrations/rollbacks/0021_process_security_compliance_rollback.sql
```

## What Each Rollback Does

### 0070_routing_decisions_rollback.sql
- Drops 3 views: `routing_decisions_enriched`, `routing_decisions_high_overhead`, `routing_decisions_low_entropy`
- Drops 4 indexes on routing_decisions
- Drops `routing_decisions` table
- **Size**: 1.1 KB | **Complexity**: MEDIUM

### 0064_adapter_stacks_rollback.sql
- Drops 1 trigger: `validate_stack_name_format`
- Drops 2 indexes on adapter_stacks
- Drops `adapter_stacks` table
- **Size**: 879 bytes | **Complexity**: LOW

### 0048_workspaces_and_messaging_rollback.sql
- Drops 2 views: `notification_summary`, `workspace_summary`
- Drops 2 triggers: workspace update handlers
- Drops 6 tables: activity_events, notifications, messages, workspace_resources, workspace_members, workspaces
- **Size**: 1.1 KB | **Complexity**: HIGH

### 0021_process_security_compliance_rollback.sql
- Drops 8 tables in hierarchical order:
  - compliance_findings → compliance_assessments → compliance_standards
  - vulnerability_findings → vulnerability_scans
  - security_audit_logs → security_policies
  - access_controls
- **Size**: 1.1 KB | **Complexity**: HIGH

## Pre-Rollback Checklist

- [ ] Database backup exists
- [ ] All active services stopped (inference, routing, API)
- [ ] Compliance audit logs exported (for 0021)
- [ ] Routing telemetry exported (for 0070)
- [ ] User activity archived (for 0048)
- [ ] Foreign key constraints enabled: `PRAGMA foreign_keys = ON;`

## Post-Rollback Verification

```sql
-- Check tables removed
SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'adapter_stacks%';
SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'routing_decisions%';

-- Check views removed
SELECT name FROM sqlite_master WHERE type='view' AND name LIKE '%workspace%';

-- Verify schema integrity
PRAGMA integrity_check;
```

## Emergency Contacts & References

- Migration files: `migrations/`
- Forward migration: `0064_adapter_stacks.sql`
- Forward migration: `0070_routing_decisions.sql`
- Forward migration: `0048_workspaces_and_messaging.sql`
- Forward migration: `0021_process_security_compliance.sql`
- Comprehensive docs: `migrations/rollbacks/README.md`

## Common Issues & Solutions

| Issue | Solution |
|-------|----------|
| "Foreign key constraint failed" | Enable: `PRAGMA foreign_keys = ON;` |
| View won't drop | Drop dependent views first (check 0070) |
| Can't drop table | Verify triggers are dropped first |
| Orphaned indexes | Re-run rollback, indexes are safe to re-drop |

## Testing (Recommended)

```bash
# Test in development first
cp aos.db aos_test.db
sqlite3 aos_test.db < migrations/rollbacks/0070_routing_decisions_rollback.sql

# Verify it worked
sqlite3 aos_test.db "SELECT name FROM sqlite_master WHERE type='table' LIKE 'routing_decisions%';"

# Re-apply to test reversibility
sqlite3 aos_test.db < migrations/0070_routing_decisions.sql
```

## Key Takeaways

1. **Order matters**: Must rollback in sequence 0070 → 0064 → 0048 → 0021
2. **Data loss**: These rollbacks DESTROY data. Export before executing.
3. **Compliance**: 0021 contains audit logs - export before rollback
4. **Safe guards**: All scripts use `DROP IF EXISTS` to prevent errors
5. **Idempotent**: Safe to run multiple times if needed
6. **SQL syntax**: All scripts are valid SQLite (validated 2025-11-19)

## Full Documentation

For detailed information including:
- Dependencies and foreign keys
- Prerequisites by migration
- Complete execution procedures
- Troubleshooting guide
- Testing procedures
- Compliance considerations

See: `migrations/rollbacks/README.md`
