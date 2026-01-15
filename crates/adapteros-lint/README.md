# adapterOS Architectural Lint

Architectural lint rules for detecting violations of adapterOS patterns.

## Purpose

Detects "AI code slop" - code that compiles but violates architectural patterns:

- Lifecycle manager bypasses (direct DB updates before lifecycle manager)
- Non-transactional updates in handler fallbacks (should use `update_adapter_state_tx`)
- Direct SQL queries in handlers (should use Db trait methods when available)
- Non-deterministic spawns in deterministic contexts

## Usage

### Command Line

```bash
# Check a single file
cargo run -p adapteros-lint -- check crates/adapteros-server-api/src/handlers.rs

# Check all handlers
cargo run -p adapteros-lint -- check-all
```

### As Library

```rust
use adapteros_lint::architectural::check_file;
use std::path::Path;

let violations = check_file(Path::new("src/handlers.rs"));
for violation in violations {
    println!("Violation: {:?}", violation);
}
```

### rustc / clippy Driver (IDE warnings)

To surface adapterOS architectural rules as standard warnings in VS Code / Cursor:

```bash
# Use adapteros-lint as the rustc workspace wrapper
export RUSTC_WORKSPACE_WRAPPER="$(pwd)/target/debug/adapteros-lint"
# Optional: pick the underlying compiler (defaults to clippy-driver then rustc fallback)
export ADAPTEROS_LINT_UNDERLYING=clippy-driver

cargo clippy -p adapteros-server-api
```

Notes:
- The driver runs the existing architectural checks against the crate source (prefers `src/`), then forwards all arguments to `clippy-driver`/`rustc`.
- Diagnostics are emitted in rustc JSON when `--error-format=json` is present, so editors show red squiggles without waiting for CI.
- `RUSTC_WORKSPACE_WRAPPER` only wraps workspace crates, keeping third-party dependency builds fast.

## Violation Types

### LifecycleManagerBypass

Direct database update before lifecycle manager check.

**Violation:**
```rust
// Wrong: Direct DB update without lifecycle manager
state.db.update_adapter_state_tx(&adapter_id, "cold", "direct").await?;
```

**Correct:**
```rust
// Correct: Check lifecycle manager first
if let Some(ref lifecycle) = state.lifecycle_manager {
    let manager = lifecycle.lock().await;
    manager.update_adapter_state(adapter_idx, AdapterState::Cold, "reason").await?;
} else {
    // Fallback: use transactional version
    state.db.update_adapter_state_tx(&adapter_id, "cold", "fallback").await?;
}
```

### NonTransactionalFallback

Non-transactional `update_adapter_state()` in handler fallback.

**Violation:**
```rust
// Wrong: Non-transactional in handler fallback
} else {
    state.db.update_adapter_state(&adapter_id, "loading", "fallback").await?;
}
```

**Correct:**
```rust
// Correct: Use transactional version in handlers
} else {
    state.db.update_adapter_state_tx(&adapter_id, "loading", "fallback").await?;
}
```

### DirectSqlInHandler

Direct SQL query in handler when Db trait method exists.

**Acceptable:**
- Read-only SELECT queries (per AGENTS.md)
- Queries inside transaction contexts
- Specialized operations without Db trait methods

**Violation:**
```rust
// Wrong: Complex UPDATE without Db trait method
sqlx::query("UPDATE adapters SET tier = ? WHERE adapter_id = ?")
    .bind(&new_tier)
    .bind(&adapter_id)
    .execute(state.db.pool())
    .await?;
```

**Correct:**
```rust
// Correct: Use tenant-scoped Db trait method
state.db.update_adapter_tier_for_tenant(&tenant_id, &adapter_id, &new_tier).await?;
```

### NonDeterministicSpawn

Non-deterministic spawn (`tokio::spawn`) in deterministic context.

**Violation:**
```rust
// Wrong: tokio::spawn in training context
tokio::spawn(async move {
    run_training_job(job_id).await?;
});
```

**Correct:**
```rust
// Correct: Use deterministic spawn
use adapteros_deterministic_exec::spawn_deterministic;
spawn_deterministic(format!("training-job-{}", job_id), async move {
    run_training_job(job_id).await?;
})?;
```

## Context-Aware Detection

The lint tool uses AST parsing and context detection to distinguish:

- **Acceptable patterns:** Fallbacks, transaction contexts, lifecycle manager usage
- **Violations:** Direct DB updates, non-transactional fallbacks, complex SQL without Db methods

## Configuration

The lint tool is conservative - it flags potential violations for review. Some patterns may be acceptable in specific contexts:

- Read-only SELECT queries are always acceptable (per AGENTS.md)
- Transaction contexts allow direct SQL
- Lifecycle manager contexts allow non-transactional updates

## Integration

### CI/CD

The lint tool is integrated into CI via `.github/workflows/architectural-lint.yml`.

### Pre-commit Hook

Install the pre-commit hook:

```bash
cp .githooks/pre-commit-architectural .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

## References

- [AGENTS.md](../../AGENTS.md) - Architectural patterns and standards
- [docs/ARCHITECTURAL_VIOLATIONS_ANALYSIS.md](../../docs/ARCHITECTURAL_VIOLATIONS_ANALYSIS.md) - Violation analysis
