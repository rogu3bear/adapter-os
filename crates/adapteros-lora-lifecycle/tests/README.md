# Lifecycle Database Tests

Integration tests for adapter lifecycle management using the database fixture system.

## Files

### Test Files

- **lifecycle_db.rs** - Main integration tests for database operations
  - `test_update_adapter_state_persists_to_db` - Verifies state changes persist
  - `test_record_adapter_activation_updates_db` - Verifies activation tracking
  - `test_evict_adapter_updates_state_and_memory` - Verifies eviction cleanup
  - `test_multiple_activations_increment_count` - Verifies activation counting
  - `test_multi_state_lifecycle_verification` - Verifies multi-adapter scenarios
  - `test_category_based_adapters` - Tests category-specific behavior
  - `test_high_memory_pressure_scenario` - Tests memory pressure handling
  - `test_pinned_adapter_cannot_be_unpinned_in_lifecycle` - Pinning tests
  - `test_parallel_fixture_isolation` - Verifies parallel test safety
  - `test_activation_tracking_with_utilities` - Tests utility functions
  - `test_list_adapters_with_state` - Tests listing and querying

### Support Files

- **fixtures.rs** - Test fixture system
  - `TestDbFixture` - Manages test database lifecycle
  - `TestAdapterBuilder` - Builder for creating test adapters
  - `fixtures` module - Pre-built fixture sets
  - `utils` module - Helper functions for assertions

- **FIXTURES.md** - Comprehensive fixture documentation
- **README.md** - This file

## Running Tests

### All lifecycle tests
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db
```

### Specific test
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db test_update_adapter_state_persists_to_db
```

### Tests with output
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db -- --nocapture
```

### Parallel test execution (default, safe with fixtures)
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db -- --test-threads=4
```

### Sequential execution (for debugging)
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db -- --test-threads=1
```

## Architecture

### Fixture Isolation

Each test gets its own in-memory SQLite database:
- **Isolation:** Complete database independence
- **Safety:** No cross-test contamination
- **Performance:** In-memory, ~50ms creation + migrations

### Lifecycle Manager Integration

Tests create `LifecycleManager` instances with:
- Real database connection (from fixture)
- Temporary filesystem for adapter loader
- Standard policies configuration
- Deterministic executor for state updates

### Async Patterns

Tests use:
- `tokio::test` for async test runtime
- `tokio::join!` for parallel test verification
- Polling loops for async state verification (50ms retry interval)

## Common Patterns

### Basic Test Structure

```rust
#[tokio::test]
async fn test_something() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let fixture = TestDbFixture::new().await;
    let adapter_id = fixtures::single_warm(fixture.db()).await;

    // Create lifecycle manager
    let tmp_root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&tmp_root)?;
    let temp_dir = tempfile::TempDir::new_in(&tmp_root)?;

    let manager = LifecycleManager::new_with_db(
        vec![adapter_id.clone()],
        &test_policies(),
        temp_dir.path().to_path_buf(),
        None,
        3,
        fixture.db().clone(),
    );

    // Action
    manager.some_operation().await?;

    // Verification
    assert!(utils::verify_adapter_state(fixture.db(), &adapter_id, "expected").await);

    Ok(())
}
```

### Polling for Async Updates

```rust
let mut attempts = 0;
loop {
    if utils::verify_adapter_state(fixture.db(), &adapter_id, "warm").await {
        break;
    }
    attempts += 1;
    if attempts > 50 {
        panic!("Update did not complete within timeout");
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}
```

## Fixture Features

### Pre-built Scenarios

| Fixture | Purpose | Returns |
|---------|---------|---------|
| `single_unloaded` | Isolated adapter test | adapter_id: String |
| `single_cold` | Cold state testing | adapter_id: String |
| `single_warm` | Warm state testing | adapter_id: String |
| `single_hot` | Hot state testing | adapter_id: String |
| `single_resident` | Pinned adapter testing | adapter_id: String |
| `multi_state_lifecycle` | Multi-adapter state testing | (cold, warm, hot): (String, String, String) |
| `high_memory_pressure` | Memory pressure testing | adapters: Vec<String> |
| `category_adapters` | Category-based testing | (code, framework, codebase): (String, String, String) |
| `pinned_and_unpinned` | Pin/unpin testing | (pinned, unpinned): (String, String) |
| `ttl_adapters` | TTL/expiration testing | (expired, expiring): (String, String) |
| `high_activation` | High activation testing | adapter_id: String |
| `low_activation` | Low activation testing | adapter_id: String |

### Utility Functions

| Function | Purpose |
|----------|---------|
| `verify_adapter_state` | Check adapter state matches expected |
| `get_adapter_memory` | Get adapter memory usage |
| `count_adapters_in_state` | Count adapters in specific state |
| `count_all_adapters` | Get total adapter count |
| `total_memory_usage` | Get total memory across all adapters |
| `list_adapters_with_state` | List all adapters with states |
| `cleanup_adapters` | Reset database between tests |

## Test Dependencies

- `tokio` - Async runtime
- `sqlx` - Database driver
- `tempfile` - Temporary directories
- `uuid` - Unique adapter IDs
- `adapteros-db` - Database module
- `adapteros-lora-lifecycle` - Lifecycle manager

## Expected Behavior

### State Updates
- Updates are asynchronous (spawn_deterministic)
- Database updates complete within 50ms (default timeout)
- Multiple updates queue correctly

### Activation Tracking
- Each activation increments counter
- Last activation timestamp updates
- Multiple activations accumulate correctly

### Memory Management
- Eviction zeros out memory bytes
- State changes to unloaded on eviction
- Pinned adapters cannot be evicted

### Parallel Execution
- Tests use independent databases
- No ordering dependencies
- All tests can run simultaneously

## Debugging

### Enable test output
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db -- --nocapture
```

### Run single test sequentially
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db test_name -- --test-threads=1
```

### Check fixture state manually
```rust
let adapters = utils::list_adapters_with_state(fixture.db()).await;
for (id, state) in adapters {
    println!("{}: {}", id, state);
}
```

## Known Issues

### Database migrations
- First test run takes ~100-200ms (migrations execute once)
- Subsequent tests are faster

### Temp directory cleanup
- Tests use `tempfile::TempDir` under `var/tmp` (prefix `lifecycle_test_`)
- On failures, clean with `rm -rf var/tmp/lifecycle_test_*`

### Async timeout
- Default 50ms polling interval adequate for in-memory database
- Increase if running on slower systems

## Future Improvements

- [ ] Metric tracking for fixture performance
- [ ] Fixture templates for common scenarios
- [ ] Automatic temp directory cleanup
- [ ] Snapshot testing for adapter states
- [ ] Property-based testing with proptest
