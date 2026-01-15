# Migration Rollback Procedures

This directory contains rollback scripts for critical adapterOS migrations. These scripts reverse the effects of forward migrations by dropping tables, views, triggers, and indexes in the correct dependency order.

## Overview

Migration rollback is essential for:
- Development iteration when migrations need refinement
- Emergency rollback if a migration causes issues in production
- Testing migration reversibility
- Schema version control and auditability

## Critical Migrations with Rollbacks

### 1. 0064_adapter_stacks_rollback.sql
**Forward Migration**: Creates `adapter_stacks` table for named adapter workflow grouping

**Rollback Actions**:
1. Drop `validate_stack_name_format` trigger
2. Drop indexes:
   - `idx_adapter_stacks_name`
   - `idx_adapter_stacks_created_at`
3. Drop `adapter_stacks` table

**Dependencies**:
- Referenced by `routing_decisions.stack_id` (foreign key with ON DELETE SET NULL)
- Used by stack versioning and adapter workflow management
- **Safe to rollback**: Foreign keys allow graceful handling

**Prerequisites Before Rollback**:
- Verify no active processes depend on stack-based routing
- Backup any stack configurations that need preservation
- Ensure `routing_decisions` table will have SET NULL values applied

---

### 2. 0070_routing_decisions_rollback.sql
**Forward Migration**: Creates `routing_decisions` table for router decision tracking with timing metrics

**Rollback Actions**:
1. Drop dependent views:
   - `routing_decisions_low_entropy`
   - `routing_decisions_high_overhead`
   - `routing_decisions_enriched`
2. Drop indexes:
   - `idx_routing_decisions_tenant_timestamp`
   - `idx_routing_decisions_stack_id`
   - `idx_routing_decisions_request_id`
   - `idx_routing_decisions_timestamp`
3. Drop `routing_decisions` table

**Dependencies**:
- Foreign key to `tenants(id)` with ON DELETE CASCADE
- Foreign key to `adapter_stacks(id)` with ON DELETE SET NULL
- Views provide enriched data for UI dashboards
- **Critical**: This affects router telemetry and decision tracing

**Prerequisites Before Rollback**:
- Stop all active inference/routing operations
- Backup routing decision logs if needed for analysis
- Ensure tenant deletion cascade is understood
- Update any UI/API endpoints consuming routing_decisions views

---

### 3. 0048_workspaces_and_messaging_rollback.sql
**Forward Migration**: Creates workspace, messaging, and activity tracking infrastructure for cross-tenant collaboration

**Rollback Actions**:
1. Drop dependent views:
   - `notification_summary`
   - `workspace_summary`
2. Drop triggers:
   - `update_workspace_on_resource_change`
   - `update_workspace_on_member_change`
3. Drop tables in dependency order:
   - `activity_events`
   - `notifications`
   - `messages` (has self-reference via thread_id)
   - `workspace_resources`
   - `workspace_members`
4. Drop `workspaces` table

**Dependencies**:
- Complex inter-table references maintaining workspace isolation
- Foreign keys to `users` and `tenants`
- Self-reference in `messages.thread_id`
- Triggers maintain `workspace.updated_at`
- **Critical**: Contains user activity audit trail

**Prerequisites Before Rollback**:
- Export workspace data and activity logs for compliance/audit
- Notify users of workspace feature removal
- Verify no applications depend on workspace API
- Ensure GDPR/audit trail compliance before deletion

---

### 4. 0021_process_security_compliance_rollback.sql
**Forward Migration**: Creates security policies, compliance standards, and access control tables

**Rollback Actions**:
1. Drop dependent tables first (leaf nodes):
   - `process_compliance_findings` (references `process_compliance_assessments`)
   - `process_vulnerability_findings` (references `process_vulnerability_scans`)
2. Drop parent tables:
   - `process_compliance_assessments` (references `process_compliance_standards`)
   - `process_vulnerability_scans`
   - `process_security_audit_logs` (references `process_security_policies`)
   - `process_access_controls`
3. Drop root tables:
   - `process_compliance_standards`
   - `process_security_policies`

**Dependencies**:
- Multi-level referential integrity (assessments→standards, findings→assessments)
- Audit logs track security events across entire system
- Access controls enforce permission boundaries
- **Critical**: Contains compliance audit trails

**Prerequisites Before Rollback**:
- **CRITICAL**: Export all security audit logs for compliance archival
- Backup process_compliance_findings for regulatory records
- Verify no active policy enforcement depends on these tables
- Ensure compliance audit trail is immutable elsewhere
- Coordinate with security/compliance team

---

## Execution Procedures

### Safe Rollback Steps

#### Step 1: Pre-Rollback Checks
```bash
# Verify database backup exists
ls -lh /path/to/backups/aos_*.db

# Check current migration version
SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1;

# Verify foreign key constraints
PRAGMA foreign_keys = ON;
```

#### Step 2: Data Preservation (When Needed)
```bash
# Export critical data before rollback
.mode csv
.output backup_routing_decisions.csv
SELECT * FROM routing_decisions;

.output backup_security_audit.csv
SELECT * FROM process_security_audit_logs;
```

#### Step 3: Execute Rollback
```bash
# For a single migration:
sqlite3 aos.db < migrations/rollbacks/0070_routing_decisions_rollback.sql

# For multiple migrations in reverse order:
sqlite3 aos.db < migrations/rollbacks/0070_routing_decisions_rollback.sql
sqlite3 aos.db < migrations/rollbacks/0064_adapter_stacks_rollback.sql
sqlite3 aos.db < migrations/rollbacks/0048_workspaces_and_messaging_rollback.sql
sqlite3 aos.db < migrations/rollbacks/0021_process_security_compliance_rollback.sql
```

#### Step 4: Verification
```bash
# Verify tables are dropped
SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'routing_decisions%';

# Verify views are gone
SELECT name FROM sqlite_master WHERE type='view' AND name LIKE 'routing_decisions%';

# Verify indexes are removed
SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_routing_decisions%';

# Check schema integrity
PRAGMA integrity_check;
```

#### Step 5: Post-Rollback
```bash
# Update migration tracking (if applicable)
DELETE FROM schema_migrations WHERE version = 70;

# Restart services to clear cached schema
systemctl restart adapteros-server
systemctl restart adapteros-cli
```

---

## Rollback Dependencies & Order

**Critical Ordering for Safe Rollback** (must rollback in this order):

```
1. 0070_routing_decisions_rollback.sql
   (depends on: none within rollbacks)
   ↓
2. 0064_adapter_stacks_rollback.sql
   (safe after routing_decisions removed)
   ↓
3. 0048_workspaces_and_messaging_rollback.sql
   (independent, can run before or after 0064)
   ↓
4. 0021_process_security_compliance_rollback.sql
   (independent, safe to run last)
```

## Database States

### Schema State Before Migration
Before executing any rollback, these tables do NOT exist:
- `adapter_stacks` (only if before 0064)
- `routing_decisions` (only if before 0070)
- Workspace tables (only if before 0048)
- Security/compliance tables (only if before 0021)

### Schema State After Rollback
After executing the rollback files, the schema returns to the state BEFORE that migration was applied. All dependent tables created by that migration are removed.

---

## Emergency Rollback Checklist

For production emergency rollback:

- [ ] Confirm issue is migration-related (not application bug)
- [ ] Backup current database to immutable storage
- [ ] Export audit logs/compliance data if applicable
- [ ] Notify stakeholders of rollback plan
- [ ] Stop all application services (inference, routing, API)
- [ ] Run rollback script in test environment first
- [ ] Verify rollback success in test environment
- [ ] Execute rollback in production
- [ ] Verify schema integrity with PRAGMA integrity_check
- [ ] Clear application caches and restart services
- [ ] Run smoke tests on critical functions
- [ ] Document rollback reason and resolution

---

## Troubleshooting

### Foreign Key Constraint Violations
If you see "FOREIGN KEY constraint failed":
```sql
-- Enable foreign keys before rollback
PRAGMA foreign_keys = ON;

-- The rollback should handle ON DELETE CASCADE properly
-- If it fails, identify which table holds the reference
SELECT * FROM sqlite_master
WHERE sql LIKE '%FOREIGN KEY%'
AND sql LIKE '%table_name%';
```

### Trigger Dependencies
If triggers can't be dropped:
```sql
-- List all triggers
SELECT name, tbl_name, sql FROM sqlite_master WHERE type='trigger';

-- Triggers are usually safe to drop, but may impact data consistency
-- Review trigger logic before proceeding
```

### View Dependencies
If views can't be dropped due to dependencies:
```sql
-- Views may be referenced by other views
-- Drop in dependency order: child views first, parent views last
SELECT name, sql FROM sqlite_master WHERE type='view';
```

---

## Rollback Testing

### Local Development Testing
```bash
# Create test database with current schema
cp aos.db aos_rollback_test.db

# Execute rollback
sqlite3 aos_rollback_test.db < migrations/rollbacks/0070_routing_decisions_rollback.sql

# Verify results
sqlite3 aos_rollback_test.db ".tables"
sqlite3 aos_rollback_test.db ".schema"
```

### Re-apply Migration Testing
```bash
# After rollback verification, re-apply the migration
sqlite3 aos_rollback_test.db < migrations/0070_routing_decisions.sql

# Verify schema matches original
diff <(sqlite3 aos.db ".schema") <(sqlite3 aos_rollback_test.db ".schema")
```

---

## Migration Rollback Matrix

| Migration | Tables | Views | Triggers | Indexes | Complexity |
|-----------|--------|-------|----------|---------|------------|
| 0021 | 8 | 0 | 0 | 8 | HIGH |
| 0048 | 5 | 2 | 2 | 8 | HIGH |
| 0064 | 1 | 0 | 1 | 2 | LOW |
| 0070 | 1 | 3 | 0 | 4 | MEDIUM |

---

## References

- Main migrations directory: `/Users/star/Dev/aos/migrations/`
- Forward migration files for reference schema
- Database schema documentation in `/Users/star/Dev/aos/docs/`
- Foreign key constraint tracking in SQLite: `PRAGMA foreign_key_list(table_name);`

---

## Support & Questions

For questions about specific rollbacks:
1. Review the corresponding forward migration file
2. Check foreign key dependencies: `PRAGMA foreign_keys = ON;`
3. Verify table existence before rollback: `SELECT name FROM sqlite_master WHERE type='table';`
4. Contact the database schema team for complex scenarios
