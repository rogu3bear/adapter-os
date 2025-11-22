# Database Structure Audit Report
**Generated:** 2025-01-XX  
**Database Type:** SQLite (registry.db)  
**Audit Scope:** Schema, migrations, integrity, and architecture

## Executive Summary

### Critical Findings
- **Migration Gap**: 44 migration files exist but only 18 tables exist in `registry.db`
- **Dual Database System**: Parallel SQLite and PostgreSQL schemas with potential divergence
- **Disabled Migration**: `0043_patch_system.sql.disabled` - intentionally disabled
- **Missing Tables**: Many tables defined in migrations are not present in the live database
- **Schema Drift**: Documentation shows 30+ tables expected, but only 18 exist

### Statistics
- **Total Migration Files**: 44
- **Active Tables**: 18
- **Database Indexes**: 51
- **Database Views**: 0
- **PostgreSQL Migrations**: 2

---

## Database Architecture Analysis

### 1. Multi-Database System

#### SQLite Database (`registry.db`)
**Location**: `/Users/star/Dev/adapter-os/registry.db`  
**Purpose**: Primary control plane database  
**Configuration**: WAL mode enabled for concurrent reads  
**Connection**: Via `adapteros-db` crate

**Tables Present:**
```startLine:1:registry.db
adapters
artifacts
audits
base_model_imports
base_model_status
cp_pointers
incidents
jobs
manifests
models
nodes
onboarding_journeys
plans
policies
telemetry_bundles
tenants
users
workers
```

#### PostgreSQL Database (`migrations_postgres/`)
**Purpose**: Production-grade database backend  
**Status**: Parallel schema, potentially unused in development  
**Migrations**: 2 consolidated files
- `0001_init_pg.sql` - Consolidated schema initialization
- `0002_pgvector.sql` - Vector similarity search support

**Schema Differences**:
- Uses `TIMESTAMPTZ` instead of `TEXT` for timestamps
- Uses `BOOLEAN` instead of `INTEGER` for booleans
- Has more extensive foreign key constraints
- Includes pgvector extension for RAG

---

## Schema Analysis

### 2. Core Identity & Access Management

#### `users` Table
```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    pw_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('admin','operator','sre','compliance','auditor','viewer')),
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Issues Identified**:
- ✅ Proper RBAC with role checking
- ✅ Argon2 password hashing support
- ⚠️ No password complexity requirements in schema
- ⚠️ No account lockout mechanism

#### `tenants` Table
```sql
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    itar_flag INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Issues Identified**:
- ✅ Multi-tenant isolation enabled
- ✅ ITAR compliance flag present
- ⚠️ No resource quotas enforced at schema level
- ⚠️ No tenant deletion cascade verification

---

### 3. Infrastructure Management

#### `nodes` Table
**Purpose**: Worker host management  
**Status**: ✅ Present, properly indexed

**Schema**:
```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    hostname TEXT UNIQUE NOT NULL,
    agent_endpoint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','active','offline','maintenance')),
    last_seen_at TEXT,
    labels_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Issues**:
- ⚠️ `labels_json` - JSON stored as TEXT (no validation)
- ⚠️ No foreign key to `tenants` table (allows unaffiliated nodes)
- ✅ Proper status constraints

#### `workers` Table
**Purpose**: Active worker process tracking  
**Status**: ✅ Present, indexed

**Schema**:
```sql
CREATE TABLE workers (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    plan_id TEXT NOT NULL REFERENCES plans(id),
    uds_path TEXT NOT NULL,
    pid INTEGER,
    status TEXT NOT NULL DEFAULT 'starting' CHECK(status IN ('starting','serving','draining','stopped','crashed')),
    memory_headroom_pct REAL,
    k_current INTEGER,
    adapters_loaded_json TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_heartbeat_at TEXT
);
```

**Issues**:
- ✅ Proper CASCADE deletes
- ⚠️ `adapters_loaded_json` - No validation for adapter IDs
- ⚠️ No orphaned worker cleanup mechanism
- ✅ Good indexing on tenant, node, status

---

### 4. Adapter Management

#### `adapters` Table
**Purpose**: LoRA adapter lifecycle  
**Status**: ✅ Present, heavily modified

**Evolution**:
- Created in `0001_init.sql`
- Enhanced in `0012_enhanced_adapter_schema.sql` (added code intelligence fields)
- Enhanced in `0031_adapter_load_state.sql` (added load state tracking)
- Enhanced in `0042_aos_adapters.sql` (added .aos file support)

**Columns Added Over Time**:
- `category` - 'code', 'framework', 'codebase', 'ephemeral'
- `scope` - 'global', 'tenant', 'repo', 'commit'
- `current_state` - 'unloaded', 'cold', 'warm', 'hot', 'resident'
- `pinned` - Boolean for memory pinning
- `memory_bytes` - Memory usage tracking
- `activation_count` - Usage statistics
- `aos_file_path` - Path to .aos file
- `aos_file_hash` - Hash of .aos file

**Issues**:
- ✅ Comprehensive state management
- ✅ Good indexing strategy
- ⚠️ No foreign key validation for `framework_id`, `repo_id`
- ⚠️ No size limits on JSON fields
- ✅ Proper tier and state constraints

---

### 5. Missing Tables (Expected but Not Present)

Based on migration files, these tables should exist but are missing:

#### Missing Core Tables
1. **`jwt_secrets`** - JWT token rotation (no migration found)
2. **`patch_proposals`** - Code change tracking (0002_patch_proposals.sql)
3. **`ephemeral_adapters`** - Commit-aware adapters (0003_ephemeral_adapters.sql)
4. **`repositories`** - Repository tracking (0005_code_intelligence.sql)
5. **`commits`** - Commit metadata (0005_code_intelligence.sql)
6. **`code_policies`** - Code-specific policies (0005_code_intelligence.sql)
7. **`adapter_provenance`** - Cryptographic signer tracking (0007_adapter_provenance.sql)
8. **`replication_journal`** - Cross-node replication (0007_adapter_provenance.sql)
9. **`replication_artifacts`** - Replication artifact tracking (0007_adapter_provenance.sql)
10. **`signing_keys`** - Signing key storage (0004_signing_keys.sql)
11. **`promotions`** - Signed promotion records (0030_cab_promotion_workflow.sql)
12. **`cab_lineage`** - CAB lineage tracking (0033_cab_lineage.sql)
13. **`policy_quarantine`** - Policy quarantine (0034_policy_quarantine.sql)
14. **`tick_ledger_entries`** - Deterministic execution tracking (0032_tick_ledger.sql)
15. **`tick_ledger_consistency_reports`** - Cross-host verification (0032_tick_ledger.sql)
16. **`training_datasets`** - Training data management (0041_training_datasets.sql)
17. **`dataset_files`** - Dataset file tracking (0041_training_datasets.sql)
18. **`dataset_statistics`** - Dataset statistics cache (0041_training_datasets.sql)
19. **`federation_peers`** - Federated host registry (0038_federation.sql)
20. **`federation_output_hashes`** - Cross-host verification (0038_federation.sql)
21. **`federation_bundle_quorum`** - Bundle signature quorum (0038_federation.sql)
22. **`bundle_signatures`** - Cryptographic verification (0039_federation_bundle_signatures.sql)
23. **`policy_hashes`** - Policy hash tracking (0037_policy_hashes.sql)
24. **`git_repositories`** - Git integration (0013_git_repository_integration.sql)
25. **`repository_training_jobs`** - Training job tracking (0013_git_repository_integration.sql)
26. **`repository_evidence_spans`** - Evidence tracking (0013_git_repository_integration.sql)
27. **`repository_security_violations`** - Security scanning (0013_git_repository_integration.sql)
28. **`repository_analysis_cache`** - Analysis caching (0013_git_repository_integration.sql)
29. **`repository_training_metrics`** - Training metrics (0013_git_repository_integration.sql)
30. **`system_metrics`** - Performance monitoring (0011_system_metrics.sql)
31. **`aos_adapter_metadata`** - .aos file metadata (0042_aos_adapters.sql)
32. **`contacts`** - Contact management (0014_contacts_and_streams.sql)
33. **`streams`** - Stream management (0014_contacts_and_streams.sql)
34. **`git_sessions`** - Git session tracking (0015_git_sessions.sql)
35. **`replay_sessions`** - Replay session tracking (0016_replay_sessions.sql)
36. **Various process monitoring tables** (0017-0025)
37. **`evidence_indices`** - Evidence indexing (0026_evidence_indices.sql)
38. **`evidence_index_entries`** - Evidence entries (0026_evidence_indices.sql)
39. **`rag_documents`** - RAG document storage (0029_pgvector_rag.sql)
40. **`rag_embedding_models`** - Embedding model tracking (0029_pgvector_rag.sql)
41. **`rag_document_embeddings`** - Document embeddings (0029_pgvector_rag.sql)
42. **`rag_document_revisions`** - Document revision tracking (0029_pgvector_rag.sql)
43. **`rag_retrieval_audit`** - RAG retrieval auditing (0029_pgvector_rag.sql)
44. **`onboarding_journeys`** - User onboarding (present but undocumented)

---

## Migration Analysis

### Migration Files Inventory

**Total**: 44 migration files

#### Core Migrations (Applied)
- `0001_init.sql` - Initial schema ✅
- `0006_production_safety.sql` - Production enhancements ✅
- `0008_enclave_audit.sql` - Security auditing ✅
- `0011_system_metrics.sql` - Systems monitoring ✅
- `0028_base_model_status.sql` - Base model tracking ✅
- `0030_base_model_ui_support.sql` - UI integration ✅
- `0031_adapter_load_state.sql` - Adapter state management ✅
- `0044_add_adapter_ttl.sql` - Adapter TTL ✅

#### Disabled Migration
- `0043_patch_system.sql.disabled` - ⚠️ Intentionally disabled

**Risks**:
- Many migrations appear to not have been applied
- No migration tracking table (`schema_migrations`) visible
- Potential data loss if migrations are run retroactively

---

## Data Integrity Issues

### 1. Foreign Key Gaps

**Missing Foreign Keys**:
- `adapters.framework_id` → No reference table
- `adapters.repo_id` → No reference table (should reference `repositories`)
- `adapters.commit_sha` → No reference table
- `nodes` → No foreign key to `tenants`

**Impact**: 
- Data integrity not enforced at database level
- Potential orphaned records
- No cascade behavior for dependent records

### 2. JSON Field Validation

**Unvalidated JSON Fields**:
- `adapters.targets_json`
- `adapters.acl_json`
- `adapters.languages_json`
- `workers.adapters_loaded_json`
- `nodes.labels_json`
- `models.metadata_json`
- `plans.metadata_json`
- `jobs.payload_json`
- `jobs.result_json`
- `audits.details_json`

**Impact**:
- No schema validation at insert time
- Potential for malformed JSON
- No type checking

### 3. Type Inconsistencies

**SQLite Type Limitations**:
- Using `TEXT` for timestamps (should use proper date functions)
- Using `INTEGER` for booleans (0/1 pattern)
- Using `REAL` for floats (may lose precision)

**PostgreSQL Differences**:
- Uses `TIMESTAMPTZ` for proper timezone handling
- Uses `BOOLEAN` for proper boolean values
- Uses `BIGSERIAL` for auto-incrementing IDs

---

## Index Analysis

### Current Indexes (51 total)

**Well-Indexed Tables**:
- `adapters` - 9 indexes (good coverage)
- `jobs` - 2 indexes (status + tenant)
- `telemetry_bundles` - 2 indexes (CPID + tenant)
- `audits` - 2 indexes (CPID + verdict)

**Missing Indexes**:
- `nodes` - No index on `status` (present in PostgreSQL)
- `workers` - No composite index on (tenant_id, status)
- `telemetry_bundles` - No index on `created_at` alone
- `plans` - No index on `manifest_hash_b3`

**Performance Considerations**:
- Composite indexes missing for common query patterns
- No covering indexes for JSON extraction
- Foreign key columns should have indexes

---

## Security Audit

### Strengths
1. ✅ Proper foreign key constraints where present
2. ✅ CHECK constraints for enums
3. ✅ UNIQUE constraints on critical fields
4. ✅ CASCADE deletes for data consistency
5. ✅ Cryptographic hashing (BLAKE3)
6. ✅ Password hashing support (Argon2)

### Weaknesses
1. ⚠️ No audit triggers for data changes
2. ⚠️ No row-level security
3. ⚠️ No encryption at rest
4. ⚠️ No connection encryption enforcement
5. ⚠️ Sensitive data stored as plaintext (passwords hashed but no additional protection)
6. ⚠️ No rate limiting at database level
7. ⚠️ No soft deletes for critical data

---

## Architecture Recommendations

### Critical Actions Required

#### 1. Migration Reconciliation
```bash
# Check which migrations have been applied
sqlite3 registry.db "SELECT * FROM schema_migrations;"

# If no tracking table exists, create one
# Identify which migrations need to be applied
```

**Risk**: Applying migrations retroactively may fail if data exists

#### 2. Dual Database Consolidation
**Decision Required**: Choose primary database system
- Option A: Use SQLite for development, PostgreSQL for production
- Option B: Standardize on PostgreSQL
- Option C: Keep both but sync schemas

**Recommendation**: Option A with automated schema syncing

#### 3. Enable Disabled Migration
**Action**: Review `0043_patch_system.sql.disabled`
- Document why it was disabled
- Either enable it or document permanently disabled state
- Update migration sequence

#### 4. Add Missing Foreign Keys
**Priority**: High
```sql
-- Add missing foreign keys
ALTER TABLE adapters ADD CONSTRAINT fk_framework 
    FOREIGN KEY (framework_id) REFERENCES adapter_frameworks(id);
ALTER TABLE adapters ADD CONSTRAINT fk_repo 
    FOREIGN KEY (repo_id) REFERENCES repositories(repo_id);
ALTER TABLE nodes ADD CONSTRAINT fk_tenant 
    FOREIGN KEY (tenant_id) REFERENCES tenants(id);
```

#### 5. JSON Validation
**Approach**: Add CHECK constraints
```sql
-- Example: Validate JSON structure
ALTER TABLE adapters ADD CONSTRAINT check_targets_json 
    CHECK (json_valid(targets_json));
```

#### 6. Index Optimization
**High Priority**:
```sql
-- Add missing composite indexes
CREATE INDEX idx_workers_tenant_status ON workers(tenant_id, status);
CREATE INDEX idx_nodes_status_hostname ON nodes(status, hostname);
CREATE INDEX idx_telemetry_created ON telemetry_bundles(created_at DESC);
```

---

## Testing Recommendations

### 1. Schema Validation Tests
```rust
// Verify all expected tables exist
// Verify all foreign keys are enforced
// Verify all constraints are active
```

### 2. Migration Tests
```rust
// Test migration up/down cycles
// Test migration rollback scenarios
// Test migration idempotency
```

### 3. Data Integrity Tests
```rust
// Test CASCADE delete behavior
// Test UNIQUE constraint violations
// Test CHECK constraint enforcement
```

### 4. Performance Tests
```rust
// Test query performance with current indexes
// Test query performance without indexes
// Test full table scans
```

---

## Documentation Gaps

### Missing Documentation
1. Migration application process
2. Database backup/restore procedures
3. Schema change procedures
4. Performance tuning guide
5. Production deployment checklist
6. Disaster recovery plan

### Existing Documentation
1. ✅ Schema diagram (`docs/database-schema/schema-diagram.md`)
2. ✅ Workflow documentation (`docs/database-schema/workflows/`)
3. ✅ Migration inventory (`tools/inventory/migrations.json`)

---

## Compliance & Audit Trail

### Current State
- ⚠️ No audit logging at database level
- ⚠️ No change tracking triggers
- ⚠️ No modification timestamp automation
- ✅ Foreign key CASCADE for data integrity
- ✅ User tracking in `jobs.created_by`

### Recommendations
1. Add audit trigger for sensitive tables
2. Implement temporal tables for history
3. Add modification tracking columns
4. Create audit views for compliance reports

---

## Conclusion

### Risk Assessment

**High Risk**:
- Migration gap (44 migrations vs 18 tables)
- Missing foreign keys
- No migration tracking

**Medium Risk**:
- Schema drift between SQLite and PostgreSQL
- Missing indexes for performance
- No JSON validation

**Low Risk**:
- Missing views (can be added as needed)
- Disabled migration (documented as disabled)

### Next Steps

1. **Immediate**: Audit which migrations have been applied
2. **Short-term**: Add missing foreign keys and indexes
3. **Medium-term**: Consolidate dual database approach
4. **Long-term**: Implement full audit trail and compliance features

### Success Criteria

- ✅ All tables from migrations exist in database
- ✅ All foreign keys properly defined
- ✅ All indexes optimized for query patterns
- ✅ Migration tracking automated
- ✅ Schema synced between SQLite and PostgreSQL
- ✅ Documentation complete and up-to-date

---

**Report Generated**: 2025-01-XX  
**Auditor**: AI Assistant  
**Database Version**: Unknown (no versioning table)
