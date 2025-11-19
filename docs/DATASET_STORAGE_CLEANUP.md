# Dataset Storage Cleanup and Management

This document describes the dataset storage cleanup and management utilities in AdapterOS.

## Overview

The dataset cleanup system provides comprehensive storage management capabilities:

1. **Orphaned File Cleanup** - Automatically remove files not referenced in the database
2. **Storage Quota Management** - Track and enforce storage limits per tenant
3. **Dataset Archival** - Compress old/unused datasets to save space
4. **Storage Health Monitoring** - Track storage usage and detect issues
5. **Background Cleanup Tasks** - Periodically clean up orphaned files

## Architecture

### Core Components

#### `DatasetCleanupManager`

The main manager for all storage cleanup operations. Initialize with configuration and database connection:

```rust
use adapteros_orchestrator::{CleanupConfig, DatasetCleanupManager};
use adapteros_db::Db;

let config = CleanupConfig {
    quota_per_tenant_bytes: 100 * 1024 * 1024 * 1024, // 100GB per tenant
    archive_age_days: 30,
    dataset_storage_path: PathBuf::from("/var/aos/datasets"),
    auto_cleanup_on_startup: true,
    cleanup_interval_secs: 3600,
};

let db = Db::connect("sqlite:var/aos-cp.sqlite3").await?;
let manager = DatasetCleanupManager::new(config, db);
```

#### `CleanupConfig`

Configuration for cleanup behavior:

```rust
pub struct CleanupConfig {
    pub quota_per_tenant_bytes: u64,          // Max storage per tenant (0 = unlimited)
    pub archive_age_days: u32,                // Age threshold for archival
    pub dataset_storage_path: PathBuf,        // Storage directory path
    pub auto_cleanup_on_startup: bool,        // Run cleanup on server startup
    pub cleanup_interval_secs: u64,           // Background cleanup interval
}
```

#### `StorageQuotaStatus`

Per-tenant storage quota status:

```rust
pub struct StorageQuotaStatus {
    pub tenant_id: String,
    pub used_bytes: u64,
    pub quota_bytes: u64,
    pub percent_used: f64,
    pub datasets_count: u32,
    pub is_over_quota: bool,
}

// Helper methods
status.is_critical()  // Returns true if >= 90% used
status.is_high()      // Returns true if >= 75% used
```

#### `StorageHealthReport`

Complete storage health overview:

```rust
pub struct StorageHealthReport {
    pub total_storage_bytes: u64,
    pub total_used_bytes: u64,
    pub total_quota_bytes: u64,
    pub num_datasets: u32,
    pub num_orphaned_files: u32,
    pub orphaned_bytes: u64,
    pub tenant_quotas: Vec<StorageQuotaStatus>,
    pub has_issues: bool,
}
```

### Operations

#### Orphaned File Cleanup

Find and remove files not referenced in the database:

```rust
let result = manager.cleanup_orphaned_files().await?;
println!("Removed {} files, freed {} bytes",
    result.orphaned_files_removed,
    result.bytes_freed);
```

The cleanup process:
1. Queries database for all dataset files
2. Scans filesystem for actual files
3. Identifies files not in database
4. Safely removes orphaned files
5. Tracks errors for reporting

#### Dataset Archival

Archive old datasets to save space:

```rust
// Archive datasets older than 30 days
let result = manager.archive_old_datasets(None).await?;

// Or specify custom threshold
let result = manager.archive_old_datasets(Some(60)).await?;
println!("Archived {} datasets, freed {} bytes",
    result.datasets_archived,
    result.bytes_freed);
```

#### Quota Management

Check and validate tenant storage quotas:

```rust
// Get quota status
let status = manager.get_tenant_quota_status("tenant-123").await?;
if status.is_critical() {
    println!("WARNING: {} is at {}% of quota",
        status.tenant_id, status.percent_used);
}

// Check if over quota
let over_quota = manager.is_tenant_over_quota("tenant-123").await?;

// Get remaining quota in bytes
let remaining = manager.get_tenant_remaining_quota("tenant-123").await?;

// Validate before adding files
let can_add = manager.validate_quota("tenant-123", 1000000).await?;
if !can_add {
    return Err("Would exceed quota".into());
}
```

#### Storage Health Checks

Get comprehensive health report:

```rust
let report = manager.get_storage_health_report().await?;

println!("Total storage: {} bytes", report.total_used_bytes);
println!("Datasets: {}", report.num_datasets);
println!("Orphaned files: {} ({} bytes)",
    report.num_orphaned_files,
    report.orphaned_bytes);

for tenant_quota in report.tenant_quotas {
    println!("{}: {:.1}% used",
        tenant_quota.tenant_id,
        tenant_quota.percent_used);
}
```

#### Background Cleanup Task

Start a background cleanup task that runs periodically:

```rust
// Spawns a tokio task that runs cleanup every hour
let handle = manager.start_background_cleanup();

// Task runs in background until program terminates
```

## Integration with `aosctl doctor`

The `aosctl doctor` command now includes storage health checks:

```bash
# Run standard health checks
aosctl doctor

# Include storage health checks
aosctl doctor --check-storage

# Customize database and storage paths
aosctl doctor --check-storage \
    --database-url "sqlite:var/aos-cp.sqlite3" \
    --dataset-storage-path "/var/aos/datasets"
```

Output includes:
- Storage usage statistics
- Dataset count
- Orphaned file detection
- Per-tenant quota status
- Critical threshold warnings

## Environment Variables

Configure cleanup behavior via environment variables:

```bash
# Database URL for cleanup operations
DATABASE_URL=sqlite:var/aos-cp.sqlite3

# Dataset storage path
DATASET_STORAGE_PATH=/var/aos/datasets

# Server URL for health checks
AOS_SERVER_URL=http://localhost:8080
```

## Configuration Examples

### Development Setup

```rust
let config = CleanupConfig {
    quota_per_tenant_bytes: 10 * 1024 * 1024 * 1024, // 10GB per tenant
    archive_age_days: 7,
    dataset_storage_path: PathBuf::from("var/datasets"),
    auto_cleanup_on_startup: true,
    cleanup_interval_secs: 300, // 5 minutes
};
```

### Production Setup

```rust
let config = CleanupConfig {
    quota_per_tenant_bytes: 500 * 1024 * 1024 * 1024, // 500GB per tenant
    archive_age_days: 90,
    dataset_storage_path: PathBuf::from("/srv/aos/datasets"),
    auto_cleanup_on_startup: true,
    cleanup_interval_secs: 86400, // 24 hours
};
```

### No Quotas

```rust
let config = CleanupConfig {
    quota_per_tenant_bytes: 0, // Unlimited storage
    archive_age_days: 180,
    dataset_storage_path: PathBuf::from("/srv/aos/datasets"),
    auto_cleanup_on_startup: true,
    cleanup_interval_secs: 86400,
};
```

## Thresholds and Alerts

Storage status is categorized by usage percentage:

| Usage | Status | Alert |
|-------|--------|-------|
| 0-75% | Healthy | None |
| 75-90% | High | Warning |
| >90% | Critical | Error |
| >100% | Over Quota | Critical |

The `aosctl doctor` command returns:
- `Healthy` status for < 90% usage
- `Degraded` status for orphaned files or > 90% usage
- `Unhealthy` status if cannot connect to database

## Error Handling

All operations include comprehensive error handling:

```rust
match manager.cleanup_orphaned_files().await {
    Ok(result) => {
        println!("Removed {} files", result.orphaned_files_removed);

        // Check for partial failures
        if !result.cleanup_errors.is_empty() {
            eprintln!("Cleanup errors:");
            for error in result.cleanup_errors {
                eprintln!("  - {}", error);
            }
        }
    }
    Err(e) => eprintln!("Cleanup failed: {}", e),
}
```

### Common Error Scenarios

- **Storage path not found**: Returns early with empty results
- **File system permission denied**: Logged and skipped, continues cleanup
- **Database connection failure**: Returns error, prevents initialization
- **Invalid timestamps**: Gracefully handled in archival logic

## Logging

The cleanup system emits detailed logs at different levels:

```
INFO: Starting orphaned file cleanup scan
INFO: Found 1000 files referenced in database
INFO: Found 1050 files in dataset storage directory
INFO: Found orphaned file: /var/aos/datasets/abandoned.tar
INFO: Removed orphaned file: /var/aos/datasets/abandoned.tar
INFO: Orphaned file cleanup completed: removed 5 files, freed 536870912 bytes
```

Enable debug logging for more details:

```bash
RUST_LOG=adapteros=debug aosctl doctor --check-storage
```

## Testing

Run the test suite:

```bash
# Run all cleanup tests
cargo test -p adapteros-orchestrator --test dataset_cleanup_tests

# Run specific test
cargo test -p adapteros-orchestrator --test dataset_cleanup_tests -- test_quota_status_critical

# With output
cargo test -p adapteros-orchestrator --test dataset_cleanup_tests -- --nocapture
```

## Performance Considerations

### Filesystem Scanning

- Uses `walkdir` for efficient directory traversal
- Metadata calls are minimized
- Large directories are handled progressively

### Database Queries

- Single query to fetch all dataset files
- Uses `LIMIT` clause to prevent memory issues
- Indexes on `dataset_files.dataset_id` recommended

### Background Tasks

- Non-blocking async operations
- Configurable interval to manage CPU usage
- Failed cleanup doesn't stop other operations

## Migration Notes

When upgrading to this system:

1. Initial cleanup may take longer due to filesystem scan
2. No data loss - orphaned files are simply removed
3. Database schema already includes necessary tables
4. No breaking changes to existing APIs

## Future Enhancements

Potential improvements:

- [ ] Compression algorithms for dataset archival
- [ ] Scheduled archival policies per tenant
- [ ] Archive retrieval and restoration
- [ ] Storage metrics collection and trending
- [ ] Automatic quota enforcement (prevent new uploads when full)
- [ ] Storage usage notifications/alerts
- [ ] Per-dataset retention policies
