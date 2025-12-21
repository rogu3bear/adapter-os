# Lifecycle Test Fixtures

Test database fixtures for lifecycle management tests. Provides isolated, parallelizable test environments with reusable adapter configurations.

## Overview

### Key Components

1. **TestDbFixture** - Database setup/teardown with migrations
2. **TestAdapterBuilder** - Fluent builder for creating test adapters
3. **fixtures** module - Pre-built fixture sets for common scenarios
4. **utils** module - Helper functions for test verification

## Usage

### Basic Setup

```rust
use fixtures::TestDbFixture;

#[tokio::test]
async fn my_test() {
    let fixture = TestDbFixture::new().await;
    let adapter_id = fixtures::single_cold(fixture.db()).await;

    // Test your code
    assert!(utils::verify_adapter_state(fixture.db(), &adapter_id, "cold").await);
}
```

### Creating Custom Adapters

```rust
use fixtures::TestAdapterBuilder;

let adapter_id = TestAdapterBuilder::new("my-adapter")
    .with_name("Custom Name")
    .with_category("framework")
    .with_state("warm")
    .with_memory(1024 * 500)  // 500 KB
    .with_activation_count(10)
    .register(fixture.db())
    .await;
```

## Fixture Sets

### Single Adapter Fixtures

- **single_unloaded()** - Unloaded adapter (not in memory)
- **single_cold()** - Cold adapter (100 KB memory, 1 activation)
- **single_warm()** - Warm adapter (200 KB memory, 5 activations)
- **single_hot()** - Hot adapter (300 KB memory, 15 activations)
- **single_resident()** - Resident/pinned adapter (400 KB memory)

### Multi-Adapter Fixtures

- **multi_state_lifecycle()** - Returns (cold, warm, hot) tuple
  - Tests state transitions across multiple adapters
  - Useful for checking eviction priorities

- **high_memory_pressure()** - 5 warm adapters @ 10 MB each (50 MB total)
  - Tests memory pressure handling
  - Simulates OOM scenarios

- **category_adapters()** - Returns (code, framework, codebase) tuple
  - Code category with 10 activations
  - Framework category with 5 activations
  - Codebase category with 3 activations

- **pinned_and_unpinned()** - Returns (pinned, unpinned) tuple
  - Tests pinning behavior
  - Pinned adapter in resident state
  - Unpinned adapter in warm state

- **ttl_adapters()** - Returns (expired, expiring) tuple
  - For TTL/eviction testing
  - Both start in warm state

- **high_activation()** - Single hot adapter with 100 activations

- **low_activation()** - Single cold adapter with 0 activations

## Utility Functions

### Verification

```rust
// Check adapter state
assert!(utils::verify_adapter_state(db, "adapter_id", "warm").await);

// Get adapter memory
let mem = utils::get_adapter_memory(db, "adapter_id").await;

// List all adapters with state
let adapters = utils::list_adapters_with_state(db).await;
```

### Counting

```rust
// Count adapters in specific state
let warm_count = utils::count_adapters_in_state(db, "warm").await;

// Count all adapters
let total = utils::count_all_adapters(db).await;

// Get total memory usage
let total_mem = utils::total_memory_usage(db).await;
```

### Cleanup

```rust
// Reset database between tests
utils::cleanup_adapters(db).await;
```

## Parallel Test Execution

All fixtures use in-memory SQLite databases by default, ensuring complete isolation between tests. Tests can run in parallel without conflicts:

```rust
#[tokio::test]
async fn test_parallel_isolation() {
    // Each test gets independent database
    let (result1, result2) = tokio::join!(
        async {
            let fixture = TestDbFixture::new().await;
            fixtures::single_cold(fixture.db()).await
        },
        async {
            let fixture = TestDbFixture::new().await;
            fixtures::single_hot(fixture.db()).await
        }
    );
    // Both succeed without conflicts
}
```

## File-Based Fixtures

For tests requiring persistent database:

```rust
let fixture = TestDbFixture::with_file().await;
// Database persists in temporary directory
// Cleaned up when fixture is dropped
```

## Test Database Features

- **In-memory by default** - Fast, isolated test execution
- **Automatic migrations** - Database schema applied on creation
- **Parallel safe** - Each test gets independent database
- **Cleanup on drop** - Automatic resource management

## Adding New Fixture Sets

```rust
pub async fn my_custom_fixture(db: &Db) -> String {
    let id = TestAdapterBuilder::new("test-custom")
        .with_state("warm")
        .with_category("code")
        .register(db)
        .await;

    // Set up any additional state
    db.update_adapter_state(&id, "warm", "fixture_setup")
        .await
        .expect("Failed to set state");

    id
}
```

Add to `fixtures` module in `fixtures.rs`.

## Best Practices

1. **Use fixture sets** for common scenarios instead of custom setup
2. **Verify expectations** with utility functions (cleaner assertions)
3. **Cleanup temp directories** for file-based tests
4. **Don't share state** between tests (each should be independent)
5. **Use adapters sparingly** - add only what test needs

## Examples

### Testing State Transitions

```rust
#[tokio::test]
async fn test_state_transition() {
    let fixture = TestDbFixture::new().await;
    let adapter_id = fixtures::single_cold(fixture.db()).await;

    // Update state
    fixture.db().update_adapter_state(&adapter_id, "warm", "test").await?;

    // Verify
    assert!(utils::verify_adapter_state(fixture.db(), &adapter_id, "warm").await);
}
```

### Testing Memory Pressure

```rust
#[tokio::test]
async fn test_memory_eviction() {
    let fixture = TestDbFixture::new().await;
    let adapters = fixtures::high_memory_pressure(fixture.db()).await;

    let initial_mem = utils::total_memory_usage(fixture.db()).await;
    assert!(initial_mem > 1024 * 1024 * 40);

    // Trigger eviction...
    let final_mem = utils::total_memory_usage(fixture.db()).await;
    assert!(final_mem < initial_mem);
}
```

### Testing Category Policies

```rust
#[tokio::test]
async fn test_category_policies() {
    let fixture = TestDbFixture::new().await;
    let (code, framework, codebase) = fixtures::category_adapters(fixture.db()).await;

    // Test different promotion rules per category...
}
```

## Troubleshooting

### Tests timeout during database operations

- Increase poll timeout in test (default 50ms between attempts)
- Check migrations are running (should complete in <1s)
- Ensure async runtime is configured (tokio 1.x)

### Parallel test conflicts

- Verify using `TestDbFixture::new()` (in-memory, isolated)
- Don't use shared file paths across tests
- Check test names don't collide with temp directory paths

### Memory leaks in tests

- Ensure cleanup: `let _ = std::fs::remove_dir_all(&temp_dir);`
- Use `TestDbFixture::with_file()` for automatic cleanup
- Check for unclosed connections in fixture

## Performance

- In-memory database creation: ~50ms
- Fixture setup (single adapter): ~5ms
- Verification query: ~1-2ms
- Full test execution: ~100-200ms

Use `--test-threads=1` to debug test ordering issues.
