# Test Utilities for KV Integration Tests

This module provides comprehensive test utilities for testing the KV storage backend integration in adapterOS. It includes helpers for database setup, test data factories, cleanup utilities, and custom assertions.

## Module Structure

```
common/
├── mod.rs              # Module exports
├── db_helpers.rs       # Database setup and configuration
├── factories.rs        # Test data builders (adapters, tenants, stacks)
├── cleanup.rs          # Resource cleanup utilities
└── assertions.rs       # Custom assertion functions
```

## Quick Start

### Basic In-Memory Test

```rust
use adapteros_db::tests::common::{create_test_db, TestAdapterFactory};

#[tokio::test]
async fn test_adapter_registration() -> adapteros_core::Result<()> {
    // Create in-memory test database with migrations applied
    let db = create_test_db().await?;

    // Create test adapter using factory
    let params = TestAdapterFactory::new("test-adapter-1")
        .rank(16)
        .tier("warm")
        .category("code")
        .build()?;

    // Register adapter
    let adapter_id = db.register_adapter(params).await?;
    assert!(!adapter_id.is_empty());

    Ok(())
}
```

### Testing with Storage Modes

```rust
use adapteros_db::tests::common::{TestDb, TestAdapterFactory};
use adapteros_db::StorageMode;

#[tokio::test]
async fn test_dual_write_mode() {
    // Create database with dual-write mode
    let test_db = TestDb::with_mode(StorageMode::DualWrite).await;
    let db = test_db.db();

    // Create and register adapter
    let params = TestAdapterFactory::random()
        .rank(8)
        .build()
        .unwrap();

    let adapter_id = db.register_adapter(params).await.unwrap();

    // Verify in SQL
    let adapter = db.get_adapter(&adapter_id).await.unwrap();
    assert!(adapter.is_some());

    // Cleanup happens automatically when test_db is dropped
}
```

### Testing with KV Backend

```rust
use adapteros_db::tests::common::{create_test_db_with_kv, TestAdapterFactory};
use adapteros_db::StorageMode;

#[tokio::test]
async fn test_kv_operations() -> adapteros_core::Result<()> {
    // Create database with KV backend in temporary directory
    let (db, _temp_dir) = create_test_db_with_kv(StorageMode::KvPrimary).await?;

    // Use database for KV testing
    let params = TestAdapterFactory::new("kv-test-1")
        .rank(16)
        .build()?;

    db.register_adapter(params).await?;

    // TempDir cleanup happens automatically
    Ok(())
}
```

## Test Data Factories

### TestAdapterFactory

Create adapters with sensible defaults:

```rust
use adapteros_db::tests::common::TestAdapterFactory;

// Simple adapter with random ID
let adapter1 = TestAdapterFactory::random()
    .rank(16)
    .build()
    .unwrap();

// Adapter with specific configuration
let adapter2 = TestAdapterFactory::new("my-adapter")
    .name("My Test Adapter")
    .rank(24)
    .tier("persistent")
    .category("framework")
    .framework("rust")
    .parent_id("parent-adapter-id")
    .fork_type("parameter")
    .build()
    .unwrap();

// Semantic naming
let adapter3 = TestAdapterFactory::new("semantic-adapter")
    .adapter_name("tenant/domain/purpose/r001")
    .tenant_namespace("tenant")
    .domain("domain")
    .purpose("purpose")
    .revision("r001")
    .build()
    .unwrap();
```

### TestTenantFactory

Create test tenants:

```rust
use adapteros_db::tests::common::TestTenantFactory;

let tenant = TestTenantFactory::new("test-tenant")
    .name("Test Tenant")
    .system();

let tenant_id = tenant.tenant_id();
let is_system = tenant.is_system();
```

### TestStackFactory

Create adapter stacks:

```rust
use adapteros_db::tests::common::TestStackFactory;

let stack = TestStackFactory::new("my-stack")
    .add_adapter("adapter-1")
    .add_adapter("adapter-2")
    .tenant_id("test-tenant")
    .description("Test stack")
    .build();
```

## Cleanup Utilities

### Automatic Cleanup with TestDb

```rust
use adapteros_db::tests::common::TestDb;

#[tokio::test]
async fn test_with_auto_cleanup() {
    let test_db = TestDb::new().await;

    // Use test_db.db()...

    // Cleanup happens automatically when test_db goes out of scope
}
```

### Manual Cleanup

```rust
use adapteros_db::tests::common::{create_test_db, cleanup_test_db};

#[tokio::test]
async fn test_with_manual_cleanup() {
    let db = create_test_db().await.unwrap();

    // ... test code ...

    cleanup_test_db(&db).await.unwrap();
}
```

### File Cleanup

```rust
use adapteros_db::tests::common::{cleanup_test_files, cleanup_test_file};
use std::path::Path;

#[tokio::test]
async fn test_file_cleanup() {
    let temp_dir = Path::new("var/test-dir");

    // ... create test files ...

    cleanup_test_files(temp_dir).await.unwrap();
}
```

### Cleanup Guard (RAII)

```rust
use adapteros_db::tests::common::{create_test_db, TestCleanupGuard};
use std::path::PathBuf;

#[tokio::test]
async fn test_with_cleanup_guard() {
    let db = create_test_db().await.unwrap();
    let temp_paths = vec![PathBuf::from("var/test-adapters")];

    let _guard = TestCleanupGuard::new(db.clone(), temp_paths);

    // Test code - cleanup happens automatically on drop
}
```

## Custom Assertions

### Compare Adapters

```rust
use adapteros_db::tests::common::assert_adapters_equal;

#[tokio::test]
async fn test_adapter_equality() {
    let adapter_sql = db.get_adapter("test-1").await.unwrap().unwrap();
    let adapter_kv = db.get_adapter_kv("test-1").await.unwrap().unwrap();

    // Compare all fields
    assert_adapters_equal(&adapter_sql, &adapter_kv);
}
```

### Compare Specific Fields

```rust
use adapteros_db::tests::common::assert_adapter_fields_match;

#[tokio::test]
async fn test_core_fields() {
    let adapter_sql = db.get_adapter("test-1").await.unwrap().unwrap();
    let adapter_kv = db.get_adapter_kv("test-1").await.unwrap().unwrap();

    // Only compare specific fields
    assert_adapter_fields_match(
        &adapter_sql,
        &adapter_kv,
        &["name", "rank", "tier", "hash_b3", "category"]
    );
}
```

### Compare Adapter Lists

```rust
use adapteros_db::tests::common::{assert_adapter_lists_equal, assert_adapter_ids_match};

#[tokio::test]
async fn test_lineage() {
    let lineage_sql = db.get_adapter_lineage("parent").await.unwrap();
    let lineage_kv = db.get_adapter_lineage_kv("parent").await.unwrap();

    // Compare full adapter lists
    assert_adapter_lists_equal(&lineage_sql, &lineage_kv);

    // Or just compare IDs
    assert_adapter_ids_match(&lineage_sql, &lineage_kv);
}
```

## Complete Example

```rust
use adapteros_db::tests::common::{
    TestDb, TestAdapterFactory, TestCleanupGuard,
    assert_adapters_equal, assert_adapter_ids_match
};
use adapteros_db::StorageMode;
use std::path::PathBuf;

#[tokio::test]
async fn test_dual_write_lineage() {
    // Setup database with dual-write mode
    let test_db = TestDb::with_mode(StorageMode::DualWrite).await;
    let db = test_db.db();

    // Setup cleanup for test files
    let temp_paths = vec![PathBuf::from("var/test-adapters")];
    let _guard = TestCleanupGuard::files_only(temp_paths);

    // Create parent adapter
    let parent_params = TestAdapterFactory::new("parent-adapter")
        .rank(16)
        .tier("persistent")
        .build()
        .unwrap();
    let parent_id = db.register_adapter(parent_params).await.unwrap();

    // Create child adapters
    for i in 1..=3 {
        let child_params = TestAdapterFactory::new(&format!("child-{}", i))
            .rank(8)
            .parent_id(parent_id.clone())
            .fork_type("extension")
            .build()
            .unwrap();
        db.register_adapter(child_params).await.unwrap();
    }

    // Query lineage from SQL
    let lineage_sql = db.get_adapter_lineage(&parent_id).await.unwrap();

    // TODO: Query lineage from KV when implemented
    // let lineage_kv = db.get_adapter_lineage_kv(&parent_id).await.unwrap();

    // Verify results
    assert_eq!(lineage_sql.len(), 4, "Should have parent + 3 children");

    // Verify IDs match (when KV is implemented)
    // assert_adapter_ids_match(&lineage_sql, &lineage_kv);

    // Cleanup happens automatically
}
```

## Usage Guidelines

1. **Use TestDb for most tests**: It provides automatic cleanup and supports all storage modes
2. **Use factories for test data**: They provide sensible defaults and make tests more readable
3. **Leverage cleanup utilities**: Prevent test pollution and resource leaks
4. **Use custom assertions**: They provide better error messages than generic assertions
5. **Test all storage modes**: Validate behavior in SqlOnly, DualWrite, KvPrimary, and KvOnly modes

## Storage Mode Testing Matrix

| Test Scenario | Recommended Mode | Notes |
|--------------|------------------|-------|
| Basic CRUD | SqlOnly | Fastest, no KV overhead |
| Dual-write validation | DualWrite | Validates both backends |
| KV read path | KvPrimary | Tests KV reads with SQL fallback |
| Full KV migration | KvOnly | Tests pure KV mode |
| Migration scenarios | All modes | Test transitions between modes |

## Contributing

When adding new test utilities:

1. Add comprehensive doc comments with examples
2. Include unit tests for the utilities themselves
3. Update this README with usage examples
4. Keep utilities focused and composable
5. Follow the builder pattern for factories

---

**Copyright:** 2025 JKCA / James KC Auchterlonie
