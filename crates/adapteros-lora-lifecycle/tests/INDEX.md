# Lifecycle Tests - Documentation Index

## Quick Navigation

### For Running Tests
Start here: **[README.md](README.md)**
- Quick start instructions
- Running tests (5 different scenarios)
- Common test patterns

### For Using Fixtures
Start here: **[FIXTURES.md](FIXTURES.md)**
- Complete fixture reference
- Usage examples
- Utility function guide
- Best practices

### For Setup Overview
Start here: **[SETUP_COMPLETE.md](SETUP_COMPLETE.md)**
- Setup summary
- Key features
- Quick reference
- Integration guide

### For Implementation Details
Start here: **[fixtures.rs](fixtures.rs)** and **[lifecycle_db.rs](lifecycle_db.rs)**
- Core fixture system implementation
- Integration test implementations
- 11 comprehensive tests

---

## File Descriptions

### Code Files

#### fixtures.rs (566 lines)
**Purpose:** Core test fixture system

**Components:**
- `TestDbFixture` - Database setup/teardown/cloning
- `TestAdapterBuilder` - Fluent adapter builder
- `fixtures` module - 12 pre-built fixture sets
- `utils` module - 8 helper functions
- Internal test suite

**Usage:**
```rust
let fixture = TestDbFixture::new().await;
let adapter_id = fixtures::single_cold(fixture.db()).await;
assert!(utils::verify_adapter_state(fixture.db(), &adapter_id, "cold").await);
```

#### lifecycle_db.rs (386 lines)
**Purpose:** Integration tests for lifecycle management

**Tests:**
- 4 refactored original tests (using fixtures)
- 7 new tests (additional coverage)
- Total: 11 comprehensive tests

**Coverage:**
- State persistence and transitions
- Activation tracking and counting
- Memory management and eviction
- Multi-adapter scenarios
- Category-based behavior
- Memory pressure handling
- Pinning/unpinning
- Parallel execution safety
- Utility function validation

**Usage:**
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db
```

### Documentation Files

#### README.md (237 lines)
**Best for:** Learning how to run and structure tests

**Sections:**
- Files overview
- Running tests (5 scenarios)
- Architecture explanation
- Common test patterns
- Debugging guide
- Known issues

**Key Content:**
```bash
cargo test -p adapteros-lora-lifecycle --test lifecycle_db
cargo test -p adapteros-lora-lifecycle --test lifecycle_db -- --nocapture
cargo test -p adapteros-lora-lifecycle --test lifecycle_db -- --test-threads=1
```

#### FIXTURES.md (261 lines)
**Best for:** Understanding fixtures and utilities

**Sections:**
- Fixture overview
- Usage examples
- Complete fixture reference (12 sets)
- Utility function reference (8 functions)
- Parallel test execution
- Adding new fixtures
- Best practices
- Examples
- Troubleshooting

**Key Content:**
```rust
// Single adapters
let id = fixtures::single_cold(db).await;

// Multi-adapters
let (cold, warm, hot) = fixtures::multi_state_lifecycle(db).await;

// Utilities
let state_ok = utils::verify_adapter_state(db, &id, "cold").await;
let count = utils::count_all_adapters(db).await;
```

#### SETUP_COMPLETE.md (343 lines)
**Best for:** Quick reference and implementation summary

**Sections:**
- Overview
- What was set up (4 components)
- Key features (4 areas)
- Quick start (3 patterns)
- Running tests (4 commands)
- Architecture explanation
- File structure
- Performance characteristics
- Best practices
- Integration with lifecycle manager
- Next steps
- Known limitations
- Summary

**Key Content:**
- Visual architecture diagrams
- Performance metrics
- File structure overview
- Integration patterns

#### INDEX.md (This File)
**Best for:** Navigation and quick lookup

**Purpose:** Help you find the right documentation for your task

---

## Task-Based Quick Links

### "I want to run the tests"
1. Read: [README.md - Running Tests section](README.md#running-tests)
2. Run: `cargo test -p adapteros-lora-lifecycle --test lifecycle_db`

### "I want to write a new test"
1. Read: [README.md - Common Patterns section](README.md#common-patterns)
2. Read: [FIXTURES.md - Usage section](FIXTURES.md#usage)
3. Look at examples in [lifecycle_db.rs](lifecycle_db.rs)

### "I want to understand fixtures"
1. Read: [FIXTURES.md - Overview section](FIXTURES.md#overview)
2. Read: [FIXTURES.md - Fixture Sets section](FIXTURES.md#fixture-sets)
3. Review: [SETUP_COMPLETE.md - Quick Start section](SETUP_COMPLETE.md#quick-start)

### "I want to add a new fixture"
1. Read: [FIXTURES.md - Adding New Fixture Sets section](FIXTURES.md#adding-new-fixture-sets)
2. Read: [SETUP_COMPLETE.md - Next Steps section](SETUP_COMPLETE.md#next-steps)
3. Edit: [fixtures.rs](fixtures.rs) in `fixtures::fixtures` module

### "Tests are failing"
1. Read: [README.md - Debugging section](README.md#debugging)
2. Read: [FIXTURES.md - Troubleshooting section](FIXTURES.md#troubleshooting)
3. Read: [SETUP_COMPLETE.md - Troubleshooting section](SETUP_COMPLETE.md#troubleshooting)

### "I want performance information"
1. Read: [README.md - Performance section](README.md#performance)
2. Read: [SETUP_COMPLETE.md - Performance Characteristics section](SETUP_COMPLETE.md#performance-characteristics)

### "I want architecture details"
1. Read: [README.md - Architecture section](README.md#architecture)
2. Read: [SETUP_COMPLETE.md - Architecture section](SETUP_COMPLETE.md#architecture)
3. Read: [SETUP_COMPLETE.md - Integration with Lifecycle Manager section](SETUP_COMPLETE.md#integration-with-lifecycle-manager)

---

## File Relationships

```
fixtures.rs (Core System)
    ├── TestDbFixture class
    │   └── Used by all tests
    ├── TestAdapterBuilder class
    │   └── Used by fixture sets and custom tests
    ├── fixtures::fixtures module
    │   └── 12 pre-built sets used by lifecycle_db.rs
    ├── fixtures::utils module
    │   └── 8 helper functions used by lifecycle_db.rs
    └── Internal tests
        └── Validate fixture system

lifecycle_db.rs (Integration Tests)
    ├── Imports fixtures module
    ├── Uses TestDbFixture
    ├── Uses fixture sets
    ├── Uses utility functions
    └── 11 tests total

Documentation
    ├── README.md - How to run tests
    ├── FIXTURES.md - How to use fixtures
    ├── SETUP_COMPLETE.md - Implementation summary
    └── INDEX.md - This file
```

---

## Glossary

| Term | Definition |
|------|-----------|
| **TestDbFixture** | Test database setup/teardown manager |
| **TestAdapterBuilder** | Fluent builder for creating test adapters |
| **Fixture Sets** | Pre-built adapter configurations (12 types) |
| **Utility Functions** | Helper functions for test assertions (8 total) |
| **Parallelization** | Running tests concurrently with independent databases |
| **Isolation** | Each test has independent database, no contamination |
| **In-memory DB** | SQLite database in RAM (fast, isolated, temp) |
| **Migrations** | Database schema setup (automatic on fixture creation) |
| **Async State Updates** | Operations that complete asynchronously (polling required) |

---

## Statistics

| Metric | Value |
|--------|-------|
| Total Files | 5 (2 code + 3 docs) |
| Total Lines | 1,793 |
| Code Lines | 952 (fixtures.rs + lifecycle_db.rs) |
| Documentation Lines | 841 |
| Pre-built Fixtures | 12 |
| Utility Functions | 8 |
| Integration Tests | 11 |
| Test Coverage Areas | 4 (Database, Lifecycle, Utils, Execution) |

---

## Getting Started (5 Minutes)

1. **Understand the system**
   - Read: [SETUP_COMPLETE.md - Overview section](SETUP_COMPLETE.md#overview)

2. **Learn fixtures**
   - Read: [FIXTURES.md - Fixture Sets section](FIXTURES.md#fixture-sets)

3. **See examples**
   - Read: [SETUP_COMPLETE.md - Quick Start section](SETUP_COMPLETE.md#quick-start)

4. **Run tests**
   - Execute: `cargo test -p adapteros-lora-lifecycle --test lifecycle_db`

5. **Write your first test**
   - Copy pattern from [lifecycle_db.rs](lifecycle_db.rs)
   - Use fixtures from [fixtures.rs](fixtures.rs)

---

## Key Concepts

### Database Isolation
```
Test 1: [Independent DB] → [Adapters] → [Assertions]
Test 2: [Independent DB] → [Adapters] → [Assertions]
Test 3: [Independent DB] → [Adapters] → [Assertions]
Result: Zero conflicts, safe parallelization
```

### Builder Pattern
```rust
TestAdapterBuilder::new("id")
    .with_category("code")
    .with_state("warm")
    .with_memory(1024)
    .register(db)
    .await
```

### Fixture Composition
```rust
let (cold, warm, hot) = fixtures::multi_state_lifecycle(db).await;
// Returns 3 adapters in different states
```

### Utility Assertions
```rust
// Instead of raw queries:
// let rows = sqlx::query_as(...).fetch_all(db.pool()).await?;

// Use utilities:
let state_ok = utils::verify_adapter_state(db, &id, "cold").await;
let count = utils::count_all_adapters(db).await;
```

---

## Next Steps After Reading

1. **Run the existing tests**
   ```bash
   cargo test -p adapteros-lora-lifecycle --test lifecycle_db
   ```

2. **Review test examples**
   Open [lifecycle_db.rs](lifecycle_db.rs) and study the 11 test implementations

3. **Try a custom fixture**
   Use [TestAdapterBuilder](fixtures.rs) to create custom adapter configurations

4. **Add a new test**
   Follow patterns in [lifecycle_db.rs](lifecycle_db.rs) using fixtures and utilities

5. **Extend fixtures**
   Add new fixture sets to [fixtures.rs](fixtures.rs) `fixtures::fixtures` module

---

## Support Resources

### For Syntax Questions
See: [fixtures.rs](fixtures.rs) - inline documentation and examples

### For Best Practices
See: [FIXTURES.md - Best Practices section](FIXTURES.md#best-practices)

### For Patterns
See: [README.md - Common Patterns section](README.md#common-patterns)

### For Examples
See: [SETUP_COMPLETE.md - Examples section](SETUP_COMPLETE.md#examples)

---

**Last Updated:** 2025-11-21
**Status:** Ready for Production
**All Tests:** 11 comprehensive integration tests
**Documentation:** Complete with examples and guides
