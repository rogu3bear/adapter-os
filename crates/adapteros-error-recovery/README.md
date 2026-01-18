# adapteros-error-recovery

Error recovery, corruption detection, and automatic retry mechanisms for adapterOS filesystem operations.

## Purpose

This crate provides a centralized error handling layer that:

1. **Classifies errors** by type (corruption, permission, disk space, network, etc.)
2. **Determines recovery strategy** based on error type and configuration
3. **Executes recovery** with automatic retry, backup restore, or file recreation
4. **Tracks history** of all recovery attempts for diagnostics

This supports adapterOS's requirement for resilient operation in air-gapped and production environments.

## Error Classification Decision Tree

```
AosError received
    |
    v
+-- Is it an IO error? --+
|                        |
v (yes)                  v (no)
Parse message:           Check error type:
- "corrupt" -> FileCorruption    - Timeout -> TimeoutError
- "permission" -> PermissionError - Network -> NetworkError
- "no space" -> DiskSpaceError   - Authz -> PermissionError
- "lock/busy" -> LockError       - ResourceExhaustion -> DiskSpaceError
- else -> FileCorruption         - else -> Unknown
```

## Recovery Strategies

| Error Type | Strategy | Action |
|------------|----------|--------|
| FileCorruption | RestoreFromBackup | Copy most recent backup to original path |
| FileCorruption (no backup) | RecreateFile | Create empty file, backup corrupted data |
| DirectoryCorruption | RecreateDirectory | Remove and recreate directory |
| PermissionError | Manual | Return error, require human intervention |
| DiskSpaceError | Manual | Return error, require human intervention |
| NetworkError | Retry | Exponential backoff retry |
| TimeoutError | Retry | Exponential backoff retry |
| LockError | Retry | Exponential backoff retry |

## Key Types

| Type | Purpose |
|------|---------|
| `ErrorRecoveryManager` | Central coordinator for all recovery operations |
| `ErrorRecoveryConfig` | Configuration for retry counts, delays, features |
| `ErrorType` | Classification enum for error categorization |
| `RecoveryStrategy` | Enum of possible recovery actions |
| `RecoveryResult` | Outcome: Success, PartialSuccess, Failed, Skipped, ManualRequired |
| `RecoveryRecord` | Audit entry for a single recovery attempt |

## Usage

```rust
use adapteros_error_recovery::{
    ErrorRecoveryManager, ErrorRecoveryConfig, RecoveryResult
};
use std::path::Path;

// Create manager with default config
let config = ErrorRecoveryConfig::default();
let manager = ErrorRecoveryManager::new(config)?;

// Handle an error with automatic recovery
match manager.handle_error(error, Path::new("/data/file.dat")).await {
    Ok(()) => println!("Recovery successful"),
    Err(e) => println!("Recovery failed: {}", e),
}

// Check recovery statistics
let stats = manager.get_recovery_statistics().await;
println!("Success rate: {:.1}%", stats.success_rate * 100.0);
```

## Configuration

```rust
ErrorRecoveryConfig {
    enabled: true,                          // Master enable
    enable_corruption_detection: true,      // Check files for corruption
    enable_automatic_retry: true,           // Auto-retry transient errors
    max_retry_attempts: 3,                  // Retry count limit
    retry_delay: Duration::from_millis(100),// Initial retry delay
    backoff_multiplier: 2.0,                // Exponential backoff factor
    max_retry_delay: Duration::from_secs(30), // Retry delay cap
    enable_partial_recovery: true,          // Allow partial success
    enable_backup_restore: true,            // Enable backup-based recovery
    backup_retention_count: 5,              // Backups to keep
}
```

## Modules

- **`corruption`**: BLAKE3-based file integrity verification
- **`recovery`**: Backup management and file restoration
- **`retry`**: Exponential backoff retry logic
- **`validation`**: File and directory integrity validation

## Integration

Used by:
- `adapteros-secure-fs`: Filesystem operations with recovery
- `adapteros-lora-lifecycle`: Adapter loading with corruption detection
- `adapteros-db`: Database file recovery
