# Dataset Storage Cleanup API Reference

## Quick Reference Guide

### Core Types

#### `DatasetCleanupManager`
Main manager for all storage operations.

```rust
pub fn new(config: CleanupConfig, db: Db) -> Self
pub async fn initialize(&self) -> Result<()>
pub async fn cleanup_orphaned_files(&self) -> Result<CleanupResult>
pub async fn archive_old_datasets(&self, days_threshold: Option<u32>) -> Result<CleanupResult>
pub async fn get_storage_health_report(&self) -> Result<StorageHealthReport>
pub async fn get_tenant_quota_status(&self, tenant_id: &str) -> Result<StorageQuotaStatus>
pub async fn is_tenant_over_quota(&self, tenant_id: &str) -> Result<bool>
pub async fn get_tenant_remaining_quota(&self, tenant_id: &str) -> Result<i64>
pub async fn validate_quota(&self, tenant_id: &str, bytes_to_add: u64) -> Result<bool>
pub fn start_background_cleanup(&self) -> tokio::task::JoinHandle<()>
```

#### `CleanupConfig`
Configuration for cleanup behavior.

```rust
pub struct CleanupConfig {
    pub quota_per_tenant_bytes: u64,      // Storage limit per tenant (0 = unlimited)
    pub archive_age_days: u32,            // Age threshold for archival (days)
    pub dataset_storage_path: PathBuf,    // Path to dataset storage directory
    pub auto_cleanup_on_startup: bool,    // Run cleanup on initialization
    pub cleanup_interval_secs: u64,       // Interval for background task (seconds, 0 = disabled)
}

impl Default for CleanupConfig {
    // quota_per_tenant_bytes: 100GB
    // archive_age_days: 30
    // auto_cleanup_on_startup: true
    // cleanup_interval_secs: 3600 (1 hour)
}
```

#### `StorageQuotaStatus`
Quota information for a single tenant.

```rust
pub struct StorageQuotaStatus {
    pub tenant_id: String,
    pub used_bytes: u64,
    pub quota_bytes: u64,
    pub percent_used: f64,
    pub datasets_count: u32,
    pub is_over_quota: bool,
}

// Helper methods:
pub fn is_critical(&self) -> bool      // Returns true if percent_used >= 90%
pub fn is_high(&self) -> bool          // Returns true if percent_used >= 75%
```

#### `StorageHealthReport`
Complete storage health overview.

```rust
pub struct StorageHealthReport {
    pub total_storage_bytes: u64,         // Total bytes including orphaned
    pub total_used_bytes: u64,            // Bytes used by valid datasets
    pub total_quota_bytes: u64,           // Total quota available
    pub num_datasets: u32,                // Number of datasets
    pub num_orphaned_files: u32,          // Number of orphaned files found
    pub orphaned_bytes: u64,              // Bytes in orphaned files
    pub tenant_quotas: Vec<StorageQuotaStatus>,
    pub has_issues: bool,                 // True if orphaned files found
}
```

#### `CleanupResult`
Result of a cleanup operation.

```rust
pub struct CleanupResult {
    pub orphaned_files_removed: usize,    // Files removed
    pub bytes_freed: u64,                 // Space reclaimed
    pub datasets_archived: usize,         // Datasets archived
    pub cleanup_errors: Vec<String>,      // Errors encountered
}
```

## Common Usage Patterns

### Initialize Manager
```rust
use adapteros_orchestrator::{CleanupConfig, DatasetCleanupManager};
use adapteros_db::Db;

let config = CleanupConfig::default();
let db = Db::connect_env().await?;
let manager = DatasetCleanupManager::new(config, db);
manager.initialize().await?;
```

### Clean Orphaned Files
```rust
let result = manager.cleanup_orphaned_files().await?;
println!("Removed {} files, freed {} bytes",
    result.orphaned_files_removed,
    result.bytes_freed);

// Check for errors
if !result.cleanup_errors.is_empty() {
    for error in result.cleanup_errors {
        eprintln!("Error: {}", error);
    }
}
```

### Archive Old Datasets
```rust
// Archive datasets older than 30 days (default)
let result = manager.archive_old_datasets(None).await?;

// Archive datasets older than 90 days
let result = manager.archive_old_datasets(Some(90)).await?;

println!("Archived {} datasets, freed {} bytes",
    result.datasets_archived,
    result.bytes_freed);
```

### Check Tenant Quota
```rust
let status = manager.get_tenant_quota_status("tenant-123").await?;

if status.is_critical() {
    eprintln!("CRITICAL: {} is at {:.1}% of quota",
        status.tenant_id,
        status.percent_used);
}

println!("Usage: {}/{} bytes ({:.1}%)",
    status.used_bytes,
    status.quota_bytes,
    status.percent_used);
```

### Validate Before Adding Files
```rust
let file_size = 1_000_000; // 1MB

if manager.validate_quota("tenant-123", file_size).await? {
    // Safe to add file
} else {
    eprintln!("Cannot add file - would exceed quota");
}

// Or check remaining quota
let remaining = manager.get_tenant_remaining_quota("tenant-123").await?;
if remaining > 0 {
    println!("Remaining quota: {} bytes", remaining);
}
```

### Get Storage Health Report
```rust
let report = manager.get_storage_health_report().await?;

println!("Storage Health Report:");
println!("  Total used: {} bytes", report.total_used_bytes);
println!("  Datasets: {}", report.num_datasets);
println!("  Orphaned files: {} ({} bytes)",
    report.num_orphaned_files,
    report.orphaned_bytes);
println!("  Issues detected: {}", report.has_issues);

for quota in report.tenant_quotas {
    println!("  {}: {:.1}% used ({}{})",
        quota.tenant_id,
        quota.percent_used,
        if quota.is_critical() { "CRITICAL " } else { "" },
        if quota.is_high() { "HIGH" } else { "OK" },
    );
}
```

### Start Background Cleanup
```rust
// Spawns tokio task that runs cleanup at configured interval
let handle = manager.start_background_cleanup();

// Task continues running in background
// Can optionally await handle to monitor completion (never returns unless error)
```

## aosctl doctor Integration

### Command Line
```bash
# Standard health check (without storage)
aosctl doctor

# Include storage checks
aosctl doctor --check-storage

# Custom database and storage paths
aosctl doctor --check-storage \
    --database-url "sqlite:var/aos-cp.sqlite3" \
    --dataset-storage-path "/var/aos/datasets"

# With environment variables
export DATABASE_URL=sqlite:var/aos-cp.sqlite3
export DATASET_STORAGE_PATH=/var/aos/datasets
aosctl doctor --check-storage
```

### Output Format
```
Component | Status | Message
----------|--------|--------
Storage   | ⚠      | Storage has 5 orphaned files (536870912 bytes to reclaim)

Storage Details:
{
  "total_storage_bytes": 1073741824,
  "total_used_bytes": 536870912,
  "total_quota_bytes": 107374182400,
  "num_datasets": 42,
  "num_orphaned_files": 5,
  "orphaned_bytes": 536870912,
  "has_issues": true,
  "tenant_quotas": [
    {
      "tenant_id": "tenant-123",
      "used_bytes": 268435456,
      "quota_bytes": 10737418240,
      "percent_used": "2.5%",
      "is_over_quota": false,
      "is_critical": false
    }
  ]
}
```

## Configuration Examples

### Development
```rust
CleanupConfig {
    quota_per_tenant_bytes: 10 * 1024 * 1024 * 1024,  // 10 GB
    archive_age_days: 7,
    dataset_storage_path: PathBuf::from("var/datasets"),
    auto_cleanup_on_startup: true,
    cleanup_interval_secs: 300,  // 5 minutes
}
```

### Production
```rust
CleanupConfig {
    quota_per_tenant_bytes: 500 * 1024 * 1024 * 1024,  // 500 GB
    archive_age_days: 90,
    dataset_storage_path: PathBuf::from("/srv/aos/datasets"),
    auto_cleanup_on_startup: true,
    cleanup_interval_secs: 86400,  // 24 hours
}
```

### Unlimited Storage
```rust
CleanupConfig {
    quota_per_tenant_bytes: 0,  // No limit
    archive_age_days: 180,
    dataset_storage_path: PathBuf::from("/srv/aos/datasets"),
    auto_cleanup_on_startup: true,
    cleanup_interval_secs: 86400,
}
```

## Threshold Reference

| Threshold | Value | Meaning |
|-----------|-------|---------|
| Normal | 0-75% | Healthy |
| High | 75-90% | Warning |
| Critical | 90-100% | Error |
| Over quota | >100% | Critical |

## Error Handling

### Pattern 1: Continue on Partial Errors
```rust
match manager.cleanup_orphaned_files().await {
    Ok(result) => {
        println!("Removed: {}", result.orphaned_files_removed);
        if !result.cleanup_errors.is_empty() {
            eprintln!("Errors: {:?}", result.cleanup_errors);
        }
    }
    Err(e) => eprintln!("Cleanup failed: {}", e),
}
```

### Pattern 2: Fail Fast
```rust
let result = manager.cleanup_orphaned_files().await?;
if !result.cleanup_errors.is_empty() {
    anyhow::bail!("Cleanup had errors: {:?}", result.cleanup_errors);
}
```

### Pattern 3: Graceful Degradation
```rust
match manager.get_storage_health_report().await {
    Ok(report) => process_report(report),
    Err(e) => {
        eprintln!("Storage check failed: {}", e);
        // Continue with default assumptions
    }
}
```

## Performance Tips

1. **Batch Operations**: Call cleanup methods together rather than frequently
2. **Configurable Intervals**: Set `cleanup_interval_secs` based on dataset size
3. **Large Datasets**: Consider increasing cleanup interval for large storage directories
4. **Quota Validation**: Use `validate_quota()` for pre-flight checks before expensive operations

## Logging

Enable debug logging:
```bash
RUST_LOG=adapteros=debug RUST_LOG=adapteros_orchestrator=debug aosctl doctor --check-storage
```

Expected log output:
```
INFO: Starting orphaned file cleanup scan
INFO: Found 1000 files referenced in database
INFO: Found 1050 files in dataset storage directory
INFO: Found orphaned file: /var/aos/datasets/abandoned.tar
INFO: Removed orphaned file: /var/aos/datasets/abandoned.tar
INFO: Orphaned file cleanup completed: removed 5 files, freed 536870912 bytes
```

## Module Location

Core module: `/Users/star/Dev/aos/crates/adapteros-orchestrator/src/dataset_cleanup.rs`
Tests: `/Users/star/Dev/aos/crates/adapteros-orchestrator/tests/dataset_cleanup_tests.rs`
Doctor integration: `/Users/star/Dev/aos/crates/adapteros-cli/src/commands/doctor.rs`
