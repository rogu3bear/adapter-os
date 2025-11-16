# DEPRECATED: Crate-Local Migrations

**Status:** DEPRECATED as of 2025-01-16
**Migration Target:** `/migrations/` (root directory)
**Reason:** Migration tree consolidation to prevent schema drift

---

## Background

This migration directory (`/crates/adapteros-db/migrations/`) was created as a crate-local
migration tree during development. Over time, it diverged from the canonical root migrations
directory (`/migrations/`), creating schema inconsistencies and conflicts.

### Migration Drift Analysis

**Critical Conflicts Identified:**

1. **Migration 0055 Divergence**
   - Root: `0055_add_model_backend_fields.sql` (backend field with 'metal' default)
   - Crate: `0055_add_model_loading_fields.sql` (backend field with 'mlx-ffi' default + last_error)
   - **Resolution:** Merged both into root version with 'metal' default and last_error column

2. **Migration 0057 SQLite Compatibility**
   - Crate: `0057_fix_domain_adapters_sqlite_compatibility.sql` (CRITICAL)
   - Root: `0057_dashboard_configs.sql` (different purpose)
   - **Resolution:** Ported to root as `0066_fix_domain_adapters_sqlite_compatibility.sql`

3. **Migration 0059 aos_file Columns**
   - Crate: `0059_remove_unused_adapter_columns.sql` (removes aos_file_path/aos_file_hash)
   - Root: Retains these columns (referenced in Adapter struct)
   - **Resolution:** NOT ported; columns kept in canonical schema

4. **Migration 0060 Pinned Adapters**
   - Crate: `0060_create_pinned_adapters_table.sql`
   - Root: Missing
   - **Resolution:** Ported to root as `0068_create_pinned_adapters_table.sql`

---

## Consolidation Status

### Migrations Ported to Root (`/migrations/`)

| Crate Migration | Root Migration | Purpose |
|-----------------|----------------|---------|
| 0057_fix_domain_adapters_sqlite_compatibility.sql | 0066_fix_domain_adapters_sqlite_compatibility.sql | Convert PostgreSQL types to SQLite |
| 0058_cleanup_unused_tables.sql | 0067_cleanup_unused_tables.sql | Drop 15+ unused tables |
| 0060_create_pinned_adapters_table.sql | 0068_create_pinned_adapters_table.sql | Pinned adapters with TTL |

### Migrations Merged into Root

| Crate Migration | Root Migration | Notes |
|-----------------|----------------|-------|
| 0055_add_model_loading_fields.sql | 0055_add_model_backend_fields.sql | Merged: added last_error column, kept 'metal' default |

### Migrations NOT Ported (Intentional)

| Crate Migration | Reason |
|-----------------|--------|
| 0059_remove_unused_adapter_columns.sql | Contradicts root schema; aos_file columns are used |

---

## Migration Path

### For New Development

**DO NOT** create new migrations in this directory. All new migrations must be added to:

```
/migrations/NNNN_description.sql
```

And signed via:

```bash
./scripts/sign_migrations.sh
```

### For Existing Databases Using Crate Migrations

If you have a database that applied crate-local migrations (unlikely in production):

1. **Audit your schema:**
   ```sql
   SELECT version FROM refinery_schema_history ORDER BY version;
   ```

2. **Check for conflicts:**
   - If you have 0059 applied (aos_file columns removed), you may need to re-add them:
     ```sql
     ALTER TABLE adapters ADD COLUMN aos_file_path TEXT;
     ALTER TABLE adapters ADD COLUMN aos_file_hash TEXT;
     ```

3. **Migrate to root migrations:**
   - The root migration tree is now canonical
   - New migrations (0066+) will not be available in this directory

---

## Code References Updated

The following code now points to root migrations:

- `sqlx::migrate!("./migrations")` in tests/server_api_integration.rs:70
- Db::new_in_memory() applies root migrations
- refinery.toml points to root directory

---

## Questions?

For migration issues, consult:

- `/migrations/signatures.json` - Canonical migration signatures
- `/crates/adapteros-db/tests/schema_consistency_tests.rs` - Schema validation tests
- `CLAUDE.md` - Developer guide (updated with migration consolidation)

**Maintained by:** James KC Auchterlonie
**Last Updated:** 2025-01-16
**Schema Consolidation Plan:** Multi-agent schema audit (Agent B)
