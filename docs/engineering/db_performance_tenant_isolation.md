# Database Performance & Tenant Isolation Guide

## Optimization-First Mindset

All database operations in AdapterOS must prioritize **tenant isolation performance**. As a multi-tenant system, our database queries must never degrade linearly with the total data volume. They must scale with the **tenant's data volume**.

### The Golden Rule
> **Every tenant-scoped query must start with `WHERE tenant_id = ?` and use a composite index starting with `tenant_id`.**

## Critical Indexes (Migration 0210)

We have established a set of "Golden Indexes" in Migration 0210 (`migrations/0210_tenant_scoped_query_optimization.sql`). You must target these indexes for common operations.

| Table | Index Columns | Intended Use |
|-------|---------------|--------------|
| `adapters` | `tenant_id, active, tier, created_at` | Active adapter listing (most common path) |
| `adapters` | `tenant_id, hash_b3, active` | Deduplication & exact lookup |
| `adapters` | `tenant_id, expires_at` | TTL enforcement / cleanup |
| `documents` | `tenant_id, created_at DESC` | Document listing (recency sorted) |
| `repository_training_jobs` | `tenant_id, status, created_at DESC` | Job monitoring dashboards |
| `chat_messages` | `tenant_id, created_at DESC` | Chat history scrolling |

## Best Practices

### 1. Force Index Usage for Critical Paths
For high-frequency queries (e.g., listing adapters, checking status), use the `INDEXED BY` clause to guarantee the query plan. This prevents the query optimizer from choosing a sub-optimal plan during stats drift.

**Good:**
```rust
// Explicitly targets the 0210 index
sqlx::query(
    "SELECT * FROM documents
     INDEXED BY idx_documents_tenant_created
     WHERE tenant_id = ?
     ORDER BY created_at DESC"
)
```

**Bad:**
```rust
// Relies on implicit choice; might scan if stats are stale
sqlx::query("SELECT * FROM documents WHERE tenant_id = ?")
```

### 2. Avoid "OR" in Tenant Filters
Never use `OR` conditions that mix indexed and non-indexed columns for tenant scoping.

**Bad:**
```sql
WHERE tenant_id = ? OR created_by LIKE '%@tenant' -- Forces full scan/filter
```

**Good:**
```sql
WHERE tenant_id = ? -- Uses index
```
*Note: Ensure `tenant_id` is backfilled correctly so legacy fallbacks aren't needed.*

### 3. Match Sort Order
Your `ORDER BY` clause must match the index suffix.
- Index: `(tenant_id, created_at DESC)`
- Query: `ORDER BY created_at DESC` ✅
- Query: `ORDER BY created_at ASC` ❌ (Requires B-Tree sort)
- Query: `ORDER BY updated_at DESC` ❌ (Unindexed sort)

### 4. Zero-Egress Scoping
Cross-tenant queries (e.g., admin dashboards) must be explicitly marked and protected. They should essentially never happen in the "hot path" of inference or user interaction.

## Verification

Before submitting DB changes:
1. Run `EXPLAIN QUERY PLAN` on your new query.
2. Ensure it uses `SEARCH TABLE ... USING INDEX ... (tenant_id=?)`.
3. If it says `SCAN TABLE`, you have a bug.

## Maintenance

If you introduce a new access pattern:
1. Check if it fits an existing 0210 index.
2. If not, evaluate if the feature justifies a new index (cost of write amplification vs read speed).
3. Do not resort to client-side filtering of large result sets.

MLNavigator Inc 2025-12-17.



