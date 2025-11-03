# SQLX Offline Mode

AdapterOS uses SQLX offline mode to provide compile-time validation of database queries, ensuring type safety and preventing runtime SQL errors.

## ✅ Simplified Implementation

The implementation has been simplified to use standard SQLX offline mode patterns without custom tooling.

## Overview

SQLX offline mode generates a cache of database schema information that allows queries to be validated at compile time rather than runtime. This provides:

- **Type Safety**: Compile-time guarantees that query results match expected Rust types
- **Schema Validation**: Queries are checked against the actual database schema
- **Performance**: No runtime query preparation or validation overhead
- **Developer Experience**: Catch SQL errors during development, not production

## Setup

### 1. Environment Configuration

Set the following environment variables:

```bash
export SQLX_OFFLINE=true
export DATABASE_URL="sqlite://./target/sqlx-cache.db"
```

### 2. Enable Offline Mode

Set environment variables:

```bash
export SQLX_OFFLINE=true
export DATABASE_URL="sqlite://./target/sqlx-cache.db"
```

Or use the setup script:

```bash
source ./scripts/setup_sqlx_offline.sh
```

### 3. Build Project

Build to generate the offline cache:

```bash
cargo build --workspace
```

## Development Workflow

### Normal Development

For regular development with live database connections:

```bash
unset SQLX_OFFLINE
cargo build
```

### Offline Development

When working offline or ensuring query correctness:

```bash
export SQLX_OFFLINE=true
cargo build  # This will use cached schema validation
```

### CI/CD Integration

If using SQLX offline mode in CI, ensure environment variables are set:

```yaml
# .github/workflows/ci.yml
- name: Build with SQLX offline
  env:
    SQLX_OFFLINE: true
    DATABASE_URL: sqlite://./target/ci-cache.db
  run: cargo build --workspace
```

## Query Patterns

### Compile-Time Checked Queries

Use `sqlx::query!` for full compile-time validation:

```rust
use sqlx::Row;

// ✅ GOOD: Compile-time checked query
let user = sqlx::query!(
    "SELECT id, name, email FROM users WHERE id = ?",
    user_id
)
.fetch_optional(&pool)
.await?;

// Type-safe access to columns
if let Some(user) = user {
    println!("User: {} ({})", user.name, user.email);
}
```

### Runtime Queries (Limited Use)

Use `sqlx::query` only when query structure is dynamic:

```rust
// ⚠️ LIMITED USE: Not compile-time checked
let rows = sqlx::query("SELECT * FROM users WHERE status = ?")
    .bind(status)
    .fetch_all(&pool)
    .await?;
```

## Troubleshooting

### Cache Out-of-Date Error

```
error: sqlx::query! statement expects 3 columns but database schema has 4
```

**Solution:**
1. Update your query to match the current schema
2. Regenerate the offline cache: `make sqlx-setup`
3. Commit the updated `.sqlx/` directory

### Missing Cache Files

```
error: no .sqlx directory found
```

**Solution:**
1. Ensure you're in the correct workspace directory
2. Run: `make sqlx-setup`
3. Check that `.sqlx/` directory exists and is committed

### Build Script Issues

If the build script fails:

1. Check that migrations are valid: `cargo run --bin run_migrations`
2. Verify DATABASE_URL is set correctly
3. Ensure SQLite/PostgreSQL tools are available

## Multi-Database Support

AdapterOS supports both SQLite (development) and PostgreSQL (production). The offline cache is generated for SQLite but queries work with both backends due to SQLX's cross-database compatibility.

For PostgreSQL-specific features, maintain separate cache generation:

```bash
export DATABASE_URL="postgresql://localhost/adapteros"
make sqlx-setup
```

## Best Practices

### 1. Commit Cache Files

Always commit the `.sqlx/` directory to version control:

```bash
git add .sqlx/
git commit -m "Update SQLX offline cache"
```

### 2. Update on Schema Changes

Whenever migrations are added/modified:

```bash
make sqlx-setup
make sqlx-check
git add .sqlx/
```

### 3. Use Type-Safe Queries

Prefer `sqlx::query!` over `sqlx::query`:

```rust
// ✅ Preferred
let user = sqlx::query!("SELECT id, name FROM users WHERE id = ?", id)
    .fetch_one(&pool)
    .await?;

// ❌ Avoid
let row = sqlx::query("SELECT id, name FROM users WHERE id = ?")
    .bind(id)
    .fetch_one(&pool)
    .await?;
```

### 4. Handle Schema Evolution

When adding nullable columns, use `Option<T>` in query results:

```rust
// Migration adds optional column
sqlx::query!("SELECT id, name, email FROM users")
// email: Option<String>  <- Automatically inferred as nullable
```

## Migration Integration

The offline cache generation automatically runs all migrations. Ensure:

- Migrations are deterministic
- Down migrations are available for rollback testing
- Migration files are committed and versioned

## Performance Impact

- **Compile Time**: ~5-10% increase due to query validation
- **Runtime**: No performance impact (validation happens at compile time)
- **Binary Size**: Minimal increase from embedded schema metadata

## What Was Simplified

### ✅ Minimal UI
- Removed complex tooling and scripts
- Environment variables only - no custom commands needed
- SQLX handles everything automatically once configured

### ✅ Dependency Architecture
- `sqlx` remains optional in `adapteros-core` for maximum flexibility
- Feature flags work correctly: `adapteros-core = { features = ["sqlx"] }`
- No breaking changes to existing API

### ✅ Standard Workflow
- Just set environment variables and build
- No special setup or validation scripts needed
- Follows standard SQLX offline mode patterns

## References

- [SQLX Offline Mode Documentation](https://github.com/launchbadge/sqlx/blob/main/sqlx-cli/README.md#offline-mode)
- [Compile-Time Query Checking](https://github.com/launchbadge/sqlx/blob/main/FAQ.md#how-do-i-check-queries-at-compile-time)
