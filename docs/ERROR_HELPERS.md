# Error Helpers Guide

**Location:** `crates/adapteros-core/src/error_helpers.rs`
**Version:** v0.3-alpha
**Last Updated:** 2025-11-29

## Overview

The error helpers module provides extension traits that simplify error handling in AdapterOS by reducing boilerplate and ensuring consistent error messages.

## Available Traits

### DbErrorExt

For database operations using SQLx, rusqlite, or any database abstraction.

```rust
use adapteros_core::error_helpers::DbErrorExt;

// Before
sqlx::query("SELECT * FROM adapters WHERE id = ?")
    .bind(adapter_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| AosError::Database(format!("Failed to fetch adapter {}: {}", adapter_id, e)))?;

// After
sqlx::query("SELECT * FROM adapters WHERE id = ?")
    .bind(adapter_id)
    .fetch_one(&pool)
    .await
    .db_err("fetch adapter")?;

// With dynamic context
sqlx::query("UPDATE adapters SET state = ? WHERE id = ?")
    .bind(state)
    .bind(adapter_id)
    .execute(&pool)
    .await
    .db_context(|| format!("update adapter {} state to {}", adapter_id, state))?;
```

### IoErrorExt

For file system and I/O operations.

```rust
use adapteros_core::error_helpers::IoErrorExt;
use std::fs;
use std::path::Path;

// Before
fs::read_to_string(&manifest_path)
    .map_err(|e| AosError::Io(format!("Failed to read manifest at {}: {}", manifest_path.display(), e)))?;

// After - Simple operation
fs::create_dir(&adapter_dir)
    .io_err("create adapter directory")?;

// After - With path context
fs::read_to_string(&manifest_path)
    .io_err_path("read adapter manifest", &manifest_path)?;
```

### CryptoErrorExt

For cryptographic operations (signing, verification, hashing).

```rust
use adapteros_core::error_helpers::CryptoErrorExt;

// Before
sign_data(&key, &payload)
    .map_err(|e| AosError::Crypto(format!("Failed to sign adapter manifest: {}", e)))?;

// After
sign_data(&key, &payload)
    .crypto_err("sign adapter manifest")?;

verify_signature(&public_key, &signature, &data)
    .crypto_err("verify policy pack signature")?;
```

### ValidationErrorExt

For input validation and field checking.

```rust
use adapteros_core::error_helpers::ValidationErrorExt;

// Before
if adapter_name.is_empty() {
    return Err(AosError::Validation("Invalid adapter_name: cannot be empty".to_string()));
}

// After
if adapter_name.is_empty() {
    return Err("cannot be empty").validation_err("adapter_name");
}

// Parsing with validation context
let rank: usize = rank_str
    .parse()
    .validation_err("lora_rank")?;

if rank == 0 || rank > 128 {
    return Err("must be between 1 and 128").validation_err("lora_rank");
}
```

### ConfigErrorExt

For configuration parsing and validation.

```rust
use adapteros_core::error_helpers::ConfigErrorExt;

// Before
let port: u16 = port_str
    .parse()
    .map_err(|e| AosError::Config(format!("Invalid server_port: {}", e)))?;

// After
let port: u16 = port_str.parse().config_err("server_port")?;

let k_sparse: usize = env::var("AOS_ROUTER_K_SPARSE")
    .unwrap_or_else(|_| "4".to_string())
    .parse()
    .config_err("AOS_ROUTER_K_SPARSE")?;
```

## Real-World Examples

### Example 1: Adapter Registration

```rust
use adapteros_core::error_helpers::{DbErrorExt, IoErrorExt, ValidationErrorExt};
use adapteros_core::Result;

async fn register_adapter(
    db: &Db,
    adapter_id: &str,
    manifest_path: &Path,
) -> Result<()> {
    // Validate inputs
    if adapter_id.is_empty() {
        return Err("cannot be empty").validation_err("adapter_id");
    }

    // Read manifest from disk
    let manifest_content = fs::read_to_string(manifest_path)
        .io_err_path("read adapter manifest", manifest_path)?;

    // Parse manifest
    let manifest: AdapterManifest = serde_json::from_str(&manifest_content)
        .validation_err("adapter_manifest")?;

    // Insert into database
    sqlx::query(
        "INSERT INTO adapters (id, manifest, created_at) VALUES (?, ?, ?)"
    )
    .bind(adapter_id)
    .bind(&manifest_content)
    .bind(chrono::Utc::now())
    .execute(&db.pool())
    .await
    .db_context(|| format!("register adapter {}", adapter_id))?;

    Ok(())
}
```

### Example 2: Training Job Creation

```rust
use adapteros_core::error_helpers::{DbErrorExt, IoErrorExt, ConfigErrorExt};
use adapteros_core::Result;

async fn create_training_job(
    db: &Db,
    config: &TrainingConfig,
    dataset_path: &Path,
) -> Result<String> {
    // Validate configuration
    let rank = config.rank
        .ok_or_else(|| "rank is required").config_err("training_config")?;

    if rank == 0 || rank > 128 {
        return Err("rank must be between 1 and 128").validation_err("lora_rank");
    }

    // Check dataset exists
    if !dataset_path.exists() {
        return Err("dataset file not found")
            .io_err_path("check dataset", dataset_path);
    }

    // Read dataset
    let dataset = fs::read_to_string(dataset_path)
        .io_err_path("read training dataset", dataset_path)?;

    // Create job in database
    let job_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO training_jobs (id, config, dataset, status) VALUES (?, ?, ?, 'pending')"
    )
    .bind(&job_id)
    .bind(serde_json::to_string(config).unwrap())
    .bind(&dataset)
    .execute(&db.pool())
    .await
    .db_context(|| format!("create training job {}", job_id))?;

    Ok(job_id)
}
```

### Example 3: Policy Pack Verification

```rust
use adapteros_core::error_helpers::{CryptoErrorExt, IoErrorExt, ValidationErrorExt};
use adapteros_core::Result;

fn verify_policy_pack(
    pack_path: &Path,
    public_key: &PublicKey,
) -> Result<PolicyPack> {
    // Read policy pack file
    let pack_bytes = fs::read(pack_path)
        .io_err_path("read policy pack", pack_path)?;

    // Extract signature (last 64 bytes)
    if pack_bytes.len() < 64 {
        return Err("file too small to contain signature")
            .validation_err("policy_pack_size");
    }

    let (content, signature) = pack_bytes.split_at(pack_bytes.len() - 64);

    // Verify signature
    verify_signature(public_key, signature, content)
        .crypto_err("verify policy pack signature")?;

    // Parse policy pack
    let pack: PolicyPack = serde_json::from_slice(content)
        .validation_err("policy_pack_content")?;

    Ok(pack)
}
```

## Migration Guide

When refactoring existing code to use error helpers:

### Step 1: Add Import

```rust
use adapteros_core::error_helpers::{DbErrorExt, IoErrorExt, ValidationErrorExt};
```

### Step 2: Identify Patterns

Look for common patterns:

```rust
// Database operations
.map_err(|e| AosError::Database(format!("Failed to {}: {}", op, e)))?

// I/O operations
.map_err(|e| AosError::Io(format!("Failed to {}: {}", op, e)))?
.map_err(|e| AosError::Io(format!("Failed to {} at {}: {}", op, path, e)))?

// Validation
Err(AosError::Validation(format!("Invalid {}: {}", field, msg)))

// Configuration
.map_err(|e| AosError::Config(format!("Invalid {}: {}", setting, e)))?

// Crypto
.map_err(|e| AosError::Crypto(format!("Failed to {}: {}", op, e)))?
```

### Step 3: Replace with Helpers

```rust
// Database
.db_err("operation")?
.db_context(|| format!("operation with {}", context))?

// I/O
.io_err("operation")?
.io_err_path("operation", path)?

// Validation
.validation_err("field")?
Err("message").validation_err("field")

// Configuration
.config_err("setting")?

// Crypto
.crypto_err("operation")?
```

## Best Practices

### 1. Use Static Strings When Possible

```rust
// Good - no dynamic allocation for simple operations
.db_err("fetch adapter")?
.io_err("create directory")?

// Use dynamic context only when needed
.db_context(|| format!("fetch adapter {}", id))?
```

### 2. Be Specific in Operation Descriptions

```rust
// Too vague
.db_err("operation")?

// Better
.db_err("fetch adapter by ID")?
.db_err("update training job status")?
```

### 3. Include Entity Context in Dynamic Messages

```rust
// Good - includes entity identifier
.db_context(|| format!("delete adapter {}", adapter_id))?
.io_err_path("read weights", &weights_path)?
```

### 4. Chain with Existing Context Traits

```rust
use adapteros_core::ResultExt; // For .context()

result
    .db_err("fetch training job")?
    .context("processing training request")?
```

### 5. Validation Messages Should Be Actionable

```rust
// Good - tells user what's wrong
Err("must be non-zero").validation_err("port")?
Err("must be between 1 and 128").validation_err("lora_rank")?
Err("cannot be empty").validation_err("adapter_name")?

// Avoid vague messages
Err("invalid").validation_err("port")? // Too vague
```

## Performance Notes

- **Lazy evaluation**: `db_context()` closure is only called on error path
- **Zero overhead on success**: Error helpers have zero cost in happy path
- **Static strings**: Use `.db_err("op")` instead of `.db_context()` when possible to avoid allocation

## Testing

All error helpers include comprehensive tests:

```bash
# Run all error helper tests
cargo test -p adapteros-core error_helpers

# Run specific test
cargo test -p adapteros-core error_helpers::tests::test_db_err
```

Run the example to see usage patterns:

```bash
cargo run --example error_helpers_demo -p adapteros-core
```

## See Also

- [crates/adapteros-core/src/error.rs](../crates/adapteros-core/src/error.rs) - Core error types
- [crates/adapteros-core/src/error_helpers.rs](../crates/adapteros-core/src/error_helpers.rs) - Implementation
- [CLAUDE.md](../CLAUDE.md#error-handling) - Error handling standards
