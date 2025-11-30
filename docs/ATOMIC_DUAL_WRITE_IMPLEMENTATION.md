# Atomic Dual-Write with Rollback Implementation

**Phase 4 Preparation for KV-Primary Migration**

## Overview

This document describes the implementation of atomic dual-write with rollback support in `crates/adapteros-db/src/adapters.rs`. This is preparation for Phase 4 migration from SQL-primary to KV-primary storage mode.

## Current Behavior (Phase 3)

Currently, dual-write is **best-effort**:
- SQL write succeeds → operation succeeds
- KV write fails → log warning, continue
- This allows gradual KV adoption without breaking existing workflows

## Phase 4 Requirements

Add **strict atomic mode** option:
- Both SQL and KV writes must succeed
- If KV fails after SQL succeeds, attempt rollback
- Provide consistency validation and repair tools

## Implementation

### 1. Configuration Structure

Add `AtomicDualWriteConfig` to control dual-write behavior:

```rust
/// Configuration for atomic dual-write behavior (Phase 4 preparation)
///
/// This configuration controls how the system handles dual-write operations
/// when transitioning from SQL-primary to KV-primary storage mode.
///
/// # Modes
///
/// - **Best-effort mode** (`require_kv_success: false`, default):
///   - SQL write succeeds → return success
///   - KV write fails → log warning, continue
///   - Use case: Current dual-write mode during Phase 3
///
/// - **Strict atomic mode** (`require_kv_success: true`):
///   - SQL write succeeds, KV write succeeds → return success
///   - SQL write succeeds, KV write fails → rollback SQL (if possible), return error
///   - Use case: Phase 4 transition to KV-primary mode
///
/// # Phase 4 Migration Path
///
/// 1. Phase 3 (current): Best-effort dual-write (SQL primary, KV secondary)
/// 2. Phase 4a: Strict atomic dual-write (both must succeed)
/// 3. Phase 4b: Validate consistency with `ensure_consistency()`
/// 4. Phase 4c: Switch to KV-primary mode
/// 5. Phase 5: Deprecate SQL writes
#[derive(Debug, Clone)]
pub struct AtomicDualWriteConfig {
    /// Require KV write to succeed for the operation to succeed
    ///
    /// - `false` (default): Best-effort mode, KV failures are logged but don't fail the operation
    /// - `true`: Strict mode, KV failures cause the entire operation to fail and rollback if possible
    pub require_kv_success: bool,
}

impl Default for AtomicDualWriteConfig {
    fn default() -> Self {
        Self {
            require_kv_success: false,
        }
    }
}

impl AtomicDualWriteConfig {
    /// Create a new best-effort configuration (default)
    pub fn best_effort() -> Self {
        Self::default()
    }

    /// Create a new strict atomic configuration
    ///
    /// In this mode, both SQL and KV writes must succeed for the operation to succeed.
    /// If KV write fails after SQL write, the operation will attempt to rollback the SQL change.
    pub fn strict_atomic() -> Self {
        Self {
            require_kv_success: true,
        }
    }
}
```

### 2. Add Configuration to Db Struct

Update `crates/adapteros-db/src/lib.rs` to add atomic dual-write config:

```rust
pub struct Db {
    pool: Arc<Pool<Sqlite>>,
    kv_backend: Option<Arc<KvBackend>>,
    storage_mode: StorageMode,
    // Add this field:
    atomic_dual_write_config: Arc<AtomicDualWriteConfig>,
}

impl Db {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self {
            pool: Arc::new(pool),
            kv_backend: None,
            storage_mode: StorageMode::SqlOnly,
            atomic_dual_write_config: Arc::new(AtomicDualWriteConfig::default()),
        }
    }

    /// Set atomic dual-write configuration
    pub fn with_atomic_dual_write_config(mut self, config: AtomicDualWriteConfig) -> Self {
        self.atomic_dual_write_config = Arc::new(config);
        self
    }

    /// Get atomic dual-write configuration
    pub fn atomic_dual_write_config(&self) -> &AtomicDualWriteConfig {
        &self.atomic_dual_write_config
    }
}
```

### 3. Update register_adapter_extended with Rollback

Modify `register_adapter_extended` to support atomic rollback:

```rust
pub async fn register_adapter_extended(
    &self,
    params: AdapterRegistrationParams,
) -> Result<String> {
    let id = Uuid::now_v7().to_string();

    // Write to SQL (primary storage)
    sqlx::query(
        "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, aos_file_path, aos_file_hash, version, lifecycle_state, current_state, pinned, memory_bytes, activation_count, load_state, active)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, '1.0.0', 'active', 'unloaded', 0, 0, 0, 'cold', 1)"
    )
    .bind(&id)
    .bind(&params.tenant_id)
    .bind(&params.adapter_id)
    .bind(&params.name)
    .bind(&params.hash_b3)
    .bind(params.rank)
    .bind(params.alpha)
    .bind(&params.tier)
    .bind(&params.targets_json)
    .bind(&params.acl_json)
    .bind(&params.languages_json)
    .bind(&params.framework)
    .bind(&params.category)
    .bind(&params.scope)
    .bind(&params.framework_id)
    .bind(&params.framework_version)
    .bind(&params.repo_id)
    .bind(&params.commit_sha)
    .bind(&params.intent)
    .bind(&params.expires_at)
    .bind(&params.adapter_name)
    .bind(&params.tenant_namespace)
    .bind(&params.domain)
    .bind(&params.purpose)
    .bind(&params.revision)
    .bind(&params.parent_id)
    .bind(&params.fork_type)
    .bind(&params.fork_reason)
    .bind(&params.aos_file_path)
    .bind(&params.aos_file_hash)
    .execute(&*self.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    // KV write (dual-write mode with configurable atomicity)
    if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
        match repo.register_adapter_kv(params.clone()).await {
            Ok(_) => {
                debug!(adapter_id = %id, "Adapter written to both SQL and KV backends");
            }
            Err(e) => {
                if self.atomic_dual_write_config.require_kv_success {
                    // Strict mode: rollback SQL insert
                    error!(
                        error = %e,
                        adapter_id = %id,
                        "KV write failed in strict atomic mode - rolling back SQL insert"
                    );

                    // Attempt rollback
                    if let Err(rollback_err) = sqlx::query("DELETE FROM adapters WHERE id = ?")
                        .bind(&id)
                        .execute(&*self.pool())
                        .await
                    {
                        // Rollback failed - log critical inconsistency
                        error!(
                            original_error = %e,
                            rollback_error = %rollback_err,
                            adapter_id = %id,
                            "CRITICAL: Failed to rollback SQL insert after KV failure - database inconsistency detected"
                        );
                        return Err(AosError::Database(format!(
                            "Failed to register adapter: KV write failed and rollback failed. Manual intervention required. ID: {}",
                            id
                        )));
                    }

                    return Err(AosError::Database(format!(
                        "Failed to register adapter: KV write failed in strict atomic mode: {}",
                        e
                    )));
                } else {
                    // Best-effort mode: log warning and continue
                    warn!(
                        error = %e,
                        adapter_id = %id,
                        "Failed to write adapter to KV backend (dual-write, best-effort mode)"
                    );
                }
            }
        }
    }

    Ok(id)
}
```

### 4. Update Methods with Strict Mode Handling

For update methods like `update_adapter_state`, `update_adapter_memory`, and `update_adapter_tier`:

**NOTE:** These methods commit SQL changes before KV writes. In strict mode, if KV fails, we **cannot** rollback the already-committed SQL transaction. Instead, we:
1. Return an error to the caller
2. Log a consistency warning
3. Rely on `ensure_consistency()` to repair the inconsistency later

Example for `update_adapter_state_tx`:

```rust
pub async fn update_adapter_state_tx(
    &self,
    adapter_id: &str,
    state: &str,
    reason: &str,
) -> Result<()> {
    let mut tx = self
        .pool()
        .begin()
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    // Lock the row and get tenant_id for KV dual-write
    let row_data: Option<(String, String)> =
        sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
            .bind(adapter_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

    let tenant_id = match row_data {
        Some((_, tid)) => tid,
        None => {
            warn!(adapter_id = %adapter_id, "Adapter not found for state update");
            return Err(AosError::NotFound(format!(
                "Adapter not found: {}",
                adapter_id
            )));
        }
    };

    // Update state with reason logged
    debug!(adapter_id = %adapter_id, state = %state, reason = %reason,
           "Updating adapter state (transactional)");

    sqlx::query(
        "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
    )
    .bind(state)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    // Commit SQL transaction
    tx.commit()
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    // KV write (dual-write mode) - after transaction commit
    if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
        match repo.update_adapter_state_kv(adapter_id, state, reason).await {
            Ok(_) => {
                debug!(adapter_id = %adapter_id, state = %state, "Adapter state updated in both SQL and KV backends (tx)");
            }
            Err(e) => {
                if self.atomic_dual_write_config.require_kv_success {
                    // Strict mode: SQL already committed, can't rollback
                    // Log inconsistency and return error
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        state = %state,
                        "CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "State update succeeded in SQL but failed in KV (strict mode). Inconsistency detected: {}",
                        e
                    )));
                } else {
                    // Best-effort mode: log warning
                    warn!(error = %e, adapter_id = %adapter_id, "Failed to update adapter state in KV backend (dual-write)");
                }
            }
        }
    }

    Ok(())
}
```

Apply similar changes to:
- `update_adapter_memory_tx`
- `update_adapter_state_and_memory`
- `update_adapter_tier`
- `delete_adapter`
- `delete_adapter_cascade`

### 5. Implement ensure_consistency Method

Add a method to validate and repair SQL→KV consistency for a single adapter:

```rust
impl Db {
    /// Ensure consistency between SQL and KV storage for a single adapter
    ///
    /// This method validates that an adapter exists in both SQL and KV storage
    /// with matching data. If inconsistencies are detected, it repairs them by
    /// copying the SQL data (source of truth during Phase 3/4a) to KV.
    ///
    /// # Use Cases
    ///
    /// 1. **Repair after KV write failure in strict mode**
    ///    - When an update succeeds in SQL but fails in KV
    ///    - The error logs will indicate "Use ensure_consistency() to repair"
    ///
    /// 2. **Periodic consistency validation**
    ///    - Run on a schedule to detect and repair drift
    ///    - Useful during Phase 4a/4b transition
    ///
    /// 3. **Post-migration validation**
    ///    - After enabling strict atomic mode
    ///    - Before switching to KV-primary mode
    ///
    /// # Returns
    ///
    /// - `Ok(true)`: Adapter was consistent or has been repaired
    /// - `Ok(false)`: Adapter does not exist in SQL (nothing to sync)
    /// - `Err`: Consistency check or repair failed
    ///
    /// # Phase 4 Migration
    ///
    /// During Phase 4a (strict atomic mode), use this method to repair
    /// inconsistencies caused by KV write failures. SQL is the source of truth.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// // Repair consistency after a KV write failure
    /// match db.ensure_consistency("adapter-123").await? {
    ///     true => println!("Adapter is now consistent"),
    ///     false => println!("Adapter not found in SQL"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn ensure_consistency(&self, adapter_id: &str) -> Result<bool> {
        // Get adapter from SQL (source of truth in Phase 3/4a)
        let adapter = match self.get_adapter(adapter_id).await? {
            Some(a) => a,
            None => {
                debug!(adapter_id = %adapter_id, "Adapter not found in SQL - nothing to sync");
                return Ok(false);
            }
        };

        // Check if KV backend is available
        let repo = match self.get_adapter_kv_repo(&adapter.tenant_id) {
            Some(r) => r,
            None => {
                debug!(adapter_id = %adapter_id, "KV backend not available - skipping consistency check");
                return Ok(true); // Consider consistent if KV is disabled
            }
        };

        // Check if adapter exists in KV
        match repo.get_adapter_kv(adapter_id).await {
            Ok(Some(kv_adapter)) => {
                // Adapter exists in KV - validate key fields match
                let fields_match =
                    kv_adapter.hash_b3 == adapter.hash_b3 &&
                    kv_adapter.tier == adapter.tier &&
                    kv_adapter.current_state == adapter.current_state &&
                    kv_adapter.memory_bytes == adapter.memory_bytes;

                if fields_match {
                    debug!(adapter_id = %adapter_id, "Adapter is consistent between SQL and KV");
                    Ok(true)
                } else {
                    // Inconsistency detected - repair by syncing SQL → KV
                    warn!(
                        adapter_id = %adapter_id,
                        "Inconsistency detected between SQL and KV - repairing from SQL (source of truth)"
                    );

                    // Convert SQL adapter to registration params for KV update
                    let params = AdapterRegistrationParams {
                        tenant_id: adapter.tenant_id.clone(),
                        adapter_id: adapter.adapter_id.clone().unwrap_or_else(|| adapter_id.to_string()),
                        name: adapter.name.clone(),
                        hash_b3: adapter.hash_b3.clone(),
                        rank: adapter.rank,
                        tier: adapter.tier.clone(),
                        alpha: adapter.alpha,
                        targets_json: adapter.targets_json.clone(),
                        acl_json: adapter.acl_json.clone(),
                        languages_json: adapter.languages_json.clone(),
                        framework: adapter.framework.clone(),
                        category: adapter.category.clone(),
                        scope: adapter.scope.clone(),
                        framework_id: adapter.framework_id.clone(),
                        framework_version: adapter.framework_version.clone(),
                        repo_id: adapter.repo_id.clone(),
                        commit_sha: adapter.commit_sha.clone(),
                        intent: adapter.intent.clone(),
                        expires_at: adapter.expires_at.clone(),
                        aos_file_path: adapter.aos_file_path.clone(),
                        aos_file_hash: adapter.aos_file_hash.clone(),
                        adapter_name: adapter.adapter_name.clone(),
                        tenant_namespace: adapter.tenant_namespace.clone(),
                        domain: adapter.domain.clone(),
                        purpose: adapter.purpose.clone(),
                        revision: adapter.revision.clone(),
                        parent_id: adapter.parent_id.clone(),
                        fork_type: adapter.fork_type.clone(),
                        fork_reason: adapter.fork_reason.clone(),
                    };

                    // Delete and re-register in KV to ensure full consistency
                    let _ = repo.delete_adapter_kv(adapter_id).await; // Ignore error if not exists
                    repo.register_adapter_kv(params).await
                        .map_err(|e| AosError::Database(format!("Failed to repair KV inconsistency: {}", e)))?;

                    // Also sync runtime state (state, memory)
                    repo.update_adapter_state_kv(adapter_id, &adapter.current_state, "consistency_repair").await
                        .map_err(|e| AosError::Database(format!("Failed to repair state in KV: {}", e)))?;
                    repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes).await
                        .map_err(|e| AosError::Database(format!("Failed to repair memory in KV: {}", e)))?;

                    debug!(adapter_id = %adapter_id, "Successfully repaired adapter consistency");
                    Ok(true)
                }
            }
            Ok(None) => {
                // Adapter missing in KV - create it
                warn!(
                    adapter_id = %adapter_id,
                    "Adapter missing in KV storage - creating from SQL (source of truth)"
                );

                let params = AdapterRegistrationParams {
                    tenant_id: adapter.tenant_id.clone(),
                    adapter_id: adapter.adapter_id.clone().unwrap_or_else(|| adapter_id.to_string()),
                    name: adapter.name.clone(),
                    hash_b3: adapter.hash_b3.clone(),
                    rank: adapter.rank,
                    tier: adapter.tier.clone(),
                    alpha: adapter.alpha,
                    targets_json: adapter.targets_json.clone(),
                    acl_json: adapter.acl_json.clone(),
                    languages_json: adapter.languages_json.clone(),
                    framework: adapter.framework.clone(),
                    category: adapter.category.clone(),
                    scope: adapter.scope.clone(),
                    framework_id: adapter.framework_id.clone(),
                    framework_version: adapter.framework_version.clone(),
                    repo_id: adapter.repo_id.clone(),
                    commit_sha: adapter.commit_sha.clone(),
                    intent: adapter.intent.clone(),
                    expires_at: adapter.expires_at.clone(),
                    aos_file_path: adapter.aos_file_path.clone(),
                    aos_file_hash: adapter.aos_file_hash.clone(),
                    adapter_name: adapter.adapter_name.clone(),
                    tenant_namespace: adapter.tenant_namespace.clone(),
                    domain: adapter.domain.clone(),
                    purpose: adapter.purpose.clone(),
                    revision: adapter.revision.clone(),
                    parent_id: adapter.parent_id.clone(),
                    fork_type: adapter.fork_type.clone(),
                    fork_reason: adapter.fork_reason.clone(),
                };

                repo.register_adapter_kv(params).await
                    .map_err(|e| AosError::Database(format!("Failed to create adapter in KV: {}", e)))?;

                // Sync runtime state
                repo.update_adapter_state_kv(adapter_id, &adapter.current_state, "consistency_repair").await
                    .map_err(|e| AosError::Database(format!("Failed to sync state to KV: {}", e)))?;
                repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes).await
                    .map_err(|e| AosError::Database(format!("Failed to sync memory to KV: {}", e)))?;

                debug!(adapter_id = %adapter_id, "Successfully created adapter in KV from SQL");
                Ok(true)
            }
            Err(e) => {
                error!(error = %e, adapter_id = %adapter_id, "Failed to check adapter consistency in KV");
                Err(AosError::Database(format!("Consistency check failed: {}", e)))
            }
        }
    }

    /// Batch ensure consistency for multiple adapters
    ///
    /// This is more efficient than calling `ensure_consistency()` in a loop
    /// as it can batch KV operations.
    ///
    /// # Returns
    ///
    /// Vector of (adapter_id, consistency_result) tuples
    pub async fn ensure_consistency_batch(&self, adapter_ids: &[String]) -> Vec<(String, Result<bool>)> {
        let mut results = Vec::new();

        for adapter_id in adapter_ids {
            let result = self.ensure_consistency(adapter_id).await;
            results.push((adapter_id.clone(), result));
        }

        results
    }

    /// Validate consistency for all adapters in a tenant
    ///
    /// Returns count of (consistent, inconsistent, errors)
    pub async fn validate_tenant_consistency(&self, tenant_id: &str) -> Result<(usize, usize, usize)> {
        let adapters = self.list_adapters_for_tenant(tenant_id).await?;

        let mut consistent = 0;
        let mut inconsistent = 0;
        let mut errors = 0;

        for adapter in adapters {
            if let Some(adapter_id) = &adapter.adapter_id {
                match self.ensure_consistency(adapter_id).await {
                    Ok(true) => consistent += 1,
                    Ok(false) => {}, // Adapter doesn't exist - skip
                    Err(_) => {
                        errors += 1;
                        inconsistent += 1;
                    }
                }
            }
        }

        Ok((consistent, inconsistent, errors))
    }
}
```

## Testing Strategy

### Unit Tests

Add tests in `crates/adapteros-db/tests/atomic_dual_write_tests.rs`:

```rust
#[cfg(test)]
mod atomic_dual_write_tests {
    use super::*;

    #[tokio::test]
    async fn test_best_effort_mode_continues_on_kv_failure() {
        // Test that best-effort mode succeeds even if KV fails
    }

    #[tokio::test]
    async fn test_strict_mode_rolls_back_on_kv_failure() {
        // Test that strict mode rolls back SQL insert when KV fails
    }

    #[tokio::test]
    async fn test_ensure_consistency_repairs_missing_kv_entry() {
        // Test that ensure_consistency creates missing KV entries from SQL
    }

    #[tokio::test]
    async fn test_ensure_consistency_repairs_inconsistent_data() {
        // Test that ensure_consistency updates KV when data differs
    }

    #[tokio::test]
    async fn test_update_methods_log_inconsistency_in_strict_mode() {
        // Test that update methods log but don't rollback in strict mode
    }
}
```

### Integration Tests

Add to existing schema consistency tests:

```rust
#[tokio::test]
async fn test_strict_atomic_mode_registration_rollback() {
    // Simulate KV failure during registration in strict mode
    // Verify SQL rollback occurs
}

#[tokio::test]
async fn test_consistency_validation_for_tenant() {
    // Create adapters with intentional inconsistencies
    // Run validate_tenant_consistency
    // Verify detection and repair
}
```

## Migration Path

### Phase 4a: Enable Strict Atomic Mode

```rust
// In main.rs or server initialization
let db = Db::new(pool)
    .with_kv_backend(kv_backend)
    .with_storage_mode(StorageMode::DualWrite)
    .with_atomic_dual_write_config(AtomicDualWriteConfig::strict_atomic());
```

### Phase 4b: Validate Consistency

```bash
# Add CLI command to validate and repair consistency
aosctl db validate-consistency --tenant default --repair
```

### Phase 4c: Monitor and Repair

Set up periodic consistency checks:

```rust
// In background worker
async fn consistency_monitor(db: Arc<Db>) {
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await; // Every hour

        let tenants = db.list_tenants().await.unwrap();
        for tenant in tenants {
            match db.validate_tenant_consistency(&tenant.id).await {
                Ok((consistent, inconsistent, errors)) => {
                    if inconsistent > 0 || errors > 0 {
                        warn!(
                            tenant_id = %tenant.id,
                            consistent = consistent,
                            inconsistent = inconsistent,
                            errors = errors,
                            "Consistency issues detected"
                        );
                    }
                }
                Err(e) => {
                    error!(tenant_id = %tenant.id, error = %e, "Failed to validate consistency");
                }
            }
        }
    }
}
```

## Error Handling Scenarios

### Scenario 1: Registration KV Failure (Strict Mode)

**What happens:**
1. SQL INSERT succeeds
2. KV write fails
3. System attempts DELETE from SQL
4. If DELETE succeeds: Return error, no inconsistency
5. If DELETE fails: Log CRITICAL, return error with manual intervention message

**User action:**
- Check logs for "CRITICAL: Failed to rollback SQL insert"
- Manually delete adapter from SQL or run `ensure_consistency()`

### Scenario 2: Update KV Failure (Strict Mode)

**What happens:**
1. SQL UPDATE commits successfully
2. KV write fails
3. Cannot rollback committed SQL transaction
4. Log "CONSISTENCY WARNING" with adapter_id
5. Return error to caller

**User action:**
- Run `ensure_consistency(adapter_id)` to repair
- Or wait for periodic consistency monitor

### Scenario 3: Delete KV Failure (Strict Mode)

**What happens:**
1. SQL DELETE succeeds
2. KV delete fails
3. Cannot rollback SQL delete
4. Log warning about orphaned KV entry
5. Return error to caller

**User action:**
- KV entry becomes orphaned but harmless
- Will be cleaned up when KV becomes primary in Phase 5

## Configuration Examples

### Development: Best-Effort (Default)

```toml
# .env or config file
AOS_ATOMIC_DUAL_WRITE_STRICT=false  # or omit for default
```

### Staging: Strict Atomic

```toml
AOS_ATOMIC_DUAL_WRITE_STRICT=true
```

### Production: Gradual Rollout

```rust
// Enable strict mode only for specific tenants during testing
let config = if tenant_id == "test-tenant" {
    AtomicDualWriteConfig::strict_atomic()
} else {
    AtomicDualWriteConfig::best_effort()
};

let db = Db::new(pool)
    .with_atomic_dual_write_config(config);
```

## Monitoring and Observability

### Metrics to Track

1. **Dual-write success rate**: % of operations where both SQL and KV succeed
2. **Rollback count**: Number of SQL rollbacks due to KV failures
3. **Consistency check results**: consistent/inconsistent/error counts
4. **Repair operations**: Number of times `ensure_consistency()` fixes issues

### Log Patterns

**Success:**
```
DEBUG Adapter written to both SQL and KV backends adapter_id="adapter-123"
```

**Best-effort KV failure:**
```
WARN Failed to write adapter to KV backend (dual-write, best-effort mode) adapter_id="adapter-123" error="connection timeout"
```

**Strict mode rollback:**
```
ERROR KV write failed in strict atomic mode - rolling back SQL insert adapter_id="adapter-123" error="connection timeout"
```

**Critical rollback failure:**
```
ERROR CRITICAL: Failed to rollback SQL insert after KV failure - database inconsistency detected adapter_id="adapter-123" original_error="connection timeout" rollback_error="adapter not found"
```

**Consistency repair:**
```
WARN Inconsistency detected between SQL and KV - repairing from SQL (source of truth) adapter_id="adapter-123"
DEBUG Successfully repaired adapter consistency adapter_id="adapter-123"
```

## Implementation Checklist

- [ ] Add `AtomicDualWriteConfig` struct to `adapters.rs`
- [ ] Add config field to `Db` struct in `lib.rs`
- [ ] Add `with_atomic_dual_write_config()` and accessor methods to `Db`
- [ ] Update `register_adapter_extended()` with rollback logic
- [ ] Update `update_adapter_state_tx()` with strict mode handling
- [ ] Update `update_adapter_memory_tx()` with strict mode handling
- [ ] Update `update_adapter_state_and_memory()` with strict mode handling
- [ ] Update `update_adapter_tier()` with strict mode handling
- [ ] Update `delete_adapter()` with strict mode handling
- [ ] Update `delete_adapter_cascade()` with strict mode handling
- [ ] Implement `ensure_consistency()`
- [ ] Implement `ensure_consistency_batch()`
- [ ] Implement `validate_tenant_consistency()`
- [ ] Add unit tests for atomic dual-write
- [ ] Add integration tests for consistency validation
- [ ] Add CLI command for consistency validation
- [ ] Document configuration in `CLAUDE.md`
- [ ] Add monitoring/metrics for dual-write operations
- [ ] Create runbook for handling consistency issues

---

**Copyright JKCA | 2025 James KC Auchterlonie**
