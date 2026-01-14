# Adapter Repository Pattern

## Overview

The Adapter Repository provides a clean abstraction layer for adapter storage operations, replacing SQL queries with KV-based operations. This is part of the broader SQL → KV migration strategy and supports legacy UUID keys migrating to adapter_id keys.

## Architecture

### Components

```
repos/
├── adapter.rs          # Main adapter repository
└── mod.rs              # Public exports

models/
├── adapter.rs          # AdapterKv model
└── mod.rs

kv/
├── backend.rs          # KvBackend trait
├── indexing.rs         # Secondary index management
└── mod.rs
```

### Key Design Decisions

1. **Repository Pattern**: Encapsulates all adapter storage operations
2. **Secondary Indexes**: Replaces SQL indexes with KV-based prefix scans (including adapter_id mapping)
3. **Lineage Traversal**: Implements recursive CTE logic in Rust with cycle guards and caps
4. **Tenant Isolation**: All operations enforce tenant boundaries

## AdapterRepository API

### CRUD Operations

```rust
// Create a new adapter
let id = repo.create(adapter).await?;

// Get adapter by ID (index lookup + tenant guard)
let adapter = repo.get(tenant_id, adapter_id).await?;

// Update existing adapter
repo.update(adapter).await?;

// Delete adapter
let deleted = repo.delete(tenant_id, adapter_id).await?;
```

### Query Operations

```rust
// List all adapters for a tenant
let adapters = repo.list_by_tenant(tenant_id).await?;

// Filter by state
let warm_adapters = repo.list_by_state(tenant_id, "warm").await?;

// Filter by tier
let persistent = repo.list_by_tier(tenant_id, "persistent").await?;

// Find by content hash
let adapter = repo.find_by_hash(hash_b3).await?;
```

### Lineage Queries

Replaces SQL recursive CTEs with Rust-based traversal:

```rust
// Get all ancestors (parent, grandparent, etc.)
let ancestors = repo.get_ancestors(tenant_id, adapter_id).await?;

// Get all descendants (children, grandchildren, etc.)
let descendants = repo.get_descendants(tenant_id, adapter_id).await?;
```

**SQL Equivalent (replaced):**
```sql
WITH RECURSIVE ancestry AS (
    SELECT adapter_id, parent_id FROM adapters WHERE adapter_id = ?
    UNION ALL
    SELECT a.adapter_id, a.parent_id
    FROM adapters a
    JOIN ancestry ON a.adapter_id = ancestry.parent_id
)
SELECT * FROM ancestry;
```

**Rust Implementation:**
- Iterative BFS/DFS traversal
- Cycle detection (prevents infinite loops)
- Safety limits (max 100 ancestors, 1000 descendants)
- Structured logging for debugging

### Pagination

```rust
let result = repo.list_paginated(tenant_id, cursor, limit).await?;

// result.items: Vec<AdapterKv>
// result.next_cursor: Option<String>
// result.has_more: bool
```

## Secondary Indexes

### Index Definitions

Located in `kv/indexing.rs`:

```rust
pub mod adapter_indexes {
    pub const BY_STATE: &str = "adapters_by_state";
    pub const BY_TIER: &str = "adapters_by_tier";
    pub const BY_TENANT: &str = "adapters_by_tenant";
    pub const BY_HASH: &str = "adapters_by_hash";
    pub const BY_LIFECYCLE_STATE: &str = "adapters_by_lifecycle_state";
    pub const BY_ACTIVE: &str = "adapters_by_active";
    pub const BY_PINNED: &str = "adapters_by_pinned";
    pub const BY_PARENT: &str = "adapters_by_parent";
    pub const BY_ADAPTER_ID: &str = "adapters_by_adapter_id";
}
```

### Index Key Format

```
index:{index_name}:{index_value}:{entity_id}
```

**Examples:**
```
index:adapters_by_tenant:default:adapter-123
index:adapters_by_state:warm:adapter-456
index:adapters_by_hash:abc123def:adapter-789
index:adapters_by_parent:adapter-parent:adapter-child
```

### Index Maintenance

Indexes are automatically maintained on:
- **Create**: All applicable indexes updated
- **Update**: Legacy UUID keys migrated to adapter_id keys; old index entries removed before adds when key changes
- **Delete**: All index entries removed

## Migration from SQL

### Before (SQL)

```rust
// Database query
let adapters = sqlx::query_as::<_, Adapter>(
    "SELECT * FROM adapters WHERE tenant_id = ? AND current_state = ?"
)
.bind(tenant_id)
.bind(state)
.fetch_all(&db.pool())
.await?;
```

### After (KV Repository)

```rust
// Repository query
let adapters = repo.list_by_state(tenant_id, state).await?;
```

### Benefits

1. **Type Safety**: Compile-time checks for all operations
2. **Backend Agnostic**: Can swap KV backend without changing code
3. **Performance**: Optimized batch operations, prefix scans
4. **Testability**: Easy to mock for unit tests
5. **Observability**: Structured logging throughout

## Error Handling

```rust
use adapteros_storage::error_types::KvStorageError;

match repo.get(tenant_id, adapter_id).await {
    Ok(Some(adapter)) => { /* found */ },
    Ok(None) => { /* not found */ },
    Err(KvStorageError::NotFound(id)) => { /* handle */ },
    Err(KvStorageError::ConflictError(msg)) => { /* handle */ },
    Err(e) => { /* other errors */ },
}
```

## Performance Considerations

### Batch Operations

The repository uses batch operations for efficiency:

```rust
// Instead of N queries
for id in adapter_ids {
    let adapter = backend.get(&format!("adapter:{}", id)).await?;
}

// Use batch get
let values = backend.batch_get(&keys).await?;
```

### Index Queries

Indexes use prefix scans, which are O(log n) in most KV stores:

```rust
// Efficient prefix scan
let keys = backend.scan_prefix("index:adapters_by_tenant:default").await?;
```

### Lineage Traversal

- **Ancestors**: O(depth) - typically small (< 10 levels)
- **Descendants**: O(n) where n = total descendants - uses BFS
- Safety limits prevent runaway queries

## Testing Strategy

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Mock KV backend for testing
    struct MockBackend { /* ... */ }

    #[tokio::test]
    async fn test_create_adapter() {
        let backend = Arc::new(MockBackend::new());
        let index_mgr = Arc::new(IndexManager::new(backend.clone()));
        let repo = AdapterRepository::new(backend, index_mgr);

        let adapter = AdapterKv { /* ... */ };
        let id = repo.create(adapter).await.unwrap();

        assert_eq!(id, adapter.id);
    }
}
```

## Future Enhancements

1. **Batch CRUD**: Batch create/update/delete operations
2. **Transactions**: Atomic multi-entity updates
3. **Caching**: In-memory cache layer for hot adapters
4. **Index Rebuilding**: Background index consistency checks
5. **Composite Indexes**: Multi-field index support
6. **Query Builder**: Fluent API for complex queries

## Related Documentation

- [Database Schema](../../../../docs/DATABASE.md)
- [Adapter Lifecycle](../../../../docs/LIFECYCLE.md)
- [Migration Strategy](../migration/README.md)
- [KV Backend Trait](../kv/backend.rs)

## Migration Timeline

| Phase | Status | Description |
|-------|--------|-------------|
| 1. Repository Pattern | ✅ Complete | Core repository implementation |
| 2. Index System | ✅ Complete | Secondary index infrastructure (incl. adapter_id) |
| 3. Lineage Queries | ✅ Complete | Recursive CTE replacement with caps |
| 4. Integration | 🔄 In Progress | Wire up to existing code |
| 5. Dual-Write | 🔄 In Progress | SQL + KV registration path |
| 6. Migration Tool | 📅 Planned | Batch data migration |
| 7. Read Cutover | 📅 Planned | Switch reads to KV |
| 8. SQL Deprecation | 📅 Planned | Remove SQL code |

## Example: Complete Workflow

```rust
use adapteros_storage::{AdapterRepository, AdapterKv, IndexManager};
use std::sync::Arc;

// Setup
let backend = Arc::new(create_backend()?);
let index_mgr = Arc::new(IndexManager::new(backend.clone()));
let repo = AdapterRepository::new(backend, index_mgr);

// Create adapter
let adapter = AdapterKv {
    id: "adapter-uuid".to_string(),
    adapter_id: Some("adapter-123".to_string()),
    tenant_id: "default".to_string(),
    tier: "warm".to_string(),
    current_state: "hot".to_string(),
    parent_id: Some("adapter-parent".to_string()),
    // ... other fields
};

repo.create(adapter.clone()).await?;

// Query by state
let hot_adapters = repo.list_by_state("default", "hot").await?;
assert!(hot_adapters.iter().any(|a| a.id == "adapter-123"));

// Get lineage
let ancestors = repo.get_ancestors("default", "adapter-123").await?;
assert!(ancestors.iter().any(|a| a.id == "adapter-parent"));

// Update state
let mut updated = adapter.clone();
updated.current_state = "resident".to_string();
repo.update(updated).await?;

// Verify index updated (state + adapter_id mapping)
let resident = repo.list_by_state("default", "resident").await?;
assert!(resident.iter().any(|a| a.key_id() == "adapter-123"));
```

MLNavigator Inc Dec 11, 2025.
