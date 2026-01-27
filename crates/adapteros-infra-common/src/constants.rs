//! System-wide constants for adapterOS
//!
//! This module provides canonical constants to eliminate magic numbers
//! and ensure consistency across the codebase.
//!
//! # Categories
//!
//! - **Memory**: Byte size conversions and memory thresholds
//! - **Time**: Duration constants and timeout defaults
//! - **Limits**: System limits and boundaries
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::constants::{BYTES_PER_MB, BYTES_PER_GB, mb_to_bytes, gb_to_bytes};
//!
//! // Use named constants instead of magic numbers
//! let max_file_size = 100 * BYTES_PER_MB;  // 100 MB
//! let cache_size = gb_to_bytes(4);          // 4 GB
//!
//! assert_eq!(BYTES_PER_MB, 1024 * 1024);
//! assert_eq!(BYTES_PER_GB, 1024 * 1024 * 1024);
//! ```

// =============================================================================
// Memory Size Constants
// =============================================================================

/// Bytes per kilobyte (1,024 bytes)
pub const BYTES_PER_KB: u64 = 1024;

/// Bytes per megabyte (1,048,576 bytes)
pub const BYTES_PER_MB: u64 = 1024 * 1024;

/// Bytes per gigabyte (1,073,741,824 bytes)
pub const BYTES_PER_GB: u64 = 1024 * 1024 * 1024;

/// Bytes per terabyte (1,099,511,627,776 bytes)
pub const BYTES_PER_TB: u64 = 1024 * 1024 * 1024 * 1024;

/// Convert kilobytes to bytes
#[inline]
pub const fn kb_to_bytes(kb: u64) -> u64 {
    kb * BYTES_PER_KB
}

/// Convert megabytes to bytes
#[inline]
pub const fn mb_to_bytes(mb: u64) -> u64 {
    mb * BYTES_PER_MB
}

/// Convert gigabytes to bytes
#[inline]
pub const fn gb_to_bytes(gb: u64) -> u64 {
    gb * BYTES_PER_GB
}

/// Convert bytes to megabytes (integer division)
#[inline]
pub const fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / BYTES_PER_MB
}

/// Convert bytes to gigabytes (integer division)
#[inline]
pub const fn bytes_to_gb(bytes: u64) -> u64 {
    bytes / BYTES_PER_GB
}

// =============================================================================
// Time Constants
// =============================================================================

/// Seconds per minute
pub const SECONDS_PER_MINUTE: u64 = 60;

/// Seconds per hour
pub const SECONDS_PER_HOUR: u64 = 3600;

/// Seconds per day
pub const SECONDS_PER_DAY: u64 = 86400;

/// Milliseconds per second
pub const MILLIS_PER_SECOND: u64 = 1000;

/// Microseconds per second
pub const MICROS_PER_SECOND: u64 = 1_000_000;

/// Nanoseconds per second
pub const NANOS_PER_SECOND: u64 = 1_000_000_000;

// =============================================================================
// Default Timeouts
// =============================================================================

/// Default timeout for fast operations (30 seconds)
///
/// Used for circuit breakers, API calls, and quick operations.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default timeout for slow operations (120 seconds)
///
/// Used for expensive computations, large file operations.
pub const SLOW_TIMEOUT_SECS: u64 = 120;

/// Default timeout for database operations (60 seconds)
pub const DATABASE_TIMEOUT_SECS: u64 = 60;

/// Default timeout for network requests (30 seconds)
pub const NETWORK_TIMEOUT_SECS: u64 = 30;

/// SQLite busy timeout (30 seconds)
pub const SQLITE_BUSY_TIMEOUT_SECS: u64 = 30;

/// Graceful shutdown drain timeout (30 seconds)
pub const DRAIN_TIMEOUT_SECS: u64 = 30;

/// Circuit breaker pause timeout (5 minutes = 300 seconds)
pub const CIRCUIT_BREAKER_PAUSE_SECS: u64 = 300;

// =============================================================================
// Memory Policy Constants
// =============================================================================

/// Default minimum memory headroom percentage (15%)
///
/// The memory subsystem should maintain at least this much free memory.
pub const DEFAULT_MIN_HEADROOM_PCT: f32 = 15.0;

/// Default minimum memory headroom as a fraction (0.15)
pub const DEFAULT_MIN_HEADROOM_FRACTION: f32 = 0.15;

/// Warning threshold for memory pressure (85% usage)
pub const MEMORY_WARNING_THRESHOLD: f32 = 0.85;

/// Critical threshold for memory pressure (95% usage)
pub const MEMORY_CRITICAL_THRESHOLD: f32 = 0.95;

// =============================================================================
// Retry Policy Constants
// =============================================================================

/// Default maximum retry attempts
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default initial retry delay in milliseconds
pub const DEFAULT_RETRY_INITIAL_DELAY_MS: u64 = 100;

/// Default retry backoff multiplier
pub const DEFAULT_RETRY_BACKOFF_FACTOR: f64 = 2.0;

// =============================================================================
// Key Rotation and Security
// =============================================================================

/// Default key rotation period (30 days in seconds)
pub const DEFAULT_KEY_ROTATION_SECS: u64 = 30 * SECONDS_PER_DAY;

/// Default key rotation period in days
pub const DEFAULT_KEY_ROTATION_DAYS: u64 = 30;

/// Grace period for key rotation (30 days)
pub const DEFAULT_GRACE_PERIOD_DAYS: u64 = 30;

// =============================================================================
// File and Upload Limits
// =============================================================================

/// Minimum chunk size for uploads (1 MB)
pub const MIN_CHUNK_SIZE: u64 = BYTES_PER_MB;

/// Default chunk size for uploads (10 MB)
pub const DEFAULT_CHUNK_SIZE: u64 = 10 * BYTES_PER_MB;

/// Maximum chunk size for uploads (100 MB)
pub const MAX_CHUNK_SIZE: u64 = 100 * BYTES_PER_MB;

/// Maximum file size for document uploads (100 MB)
pub const MAX_DOCUMENT_SIZE: u64 = 100 * BYTES_PER_MB;

/// Maximum adapter file size (500 MB)
pub const MAX_ADAPTER_SIZE: u64 = 500 * BYTES_PER_MB;

/// Maximum artifact size (10 GB)
pub const MAX_ARTIFACT_SIZE: u64 = 10 * BYTES_PER_GB;

/// Stream buffer size for file operations (64 KB)
pub const STREAM_BUFFER_SIZE: usize = 64 * 1024;

// =============================================================================
// Cache Defaults
// =============================================================================

/// Default model cache size (4 GB)
pub const DEFAULT_MODEL_CACHE_SIZE: u64 = 4 * BYTES_PER_GB;

/// Default adapter cache size (4 GB)
pub const DEFAULT_ADAPTER_CACHE_SIZE: u64 = 4 * BYTES_PER_GB;

/// Default AOS cache size (1 GB)
pub const DEFAULT_AOS_CACHE_SIZE: u64 = BYTES_PER_GB;

/// Model hub cache size (100 GB)
pub const MODEL_HUB_CACHE_SIZE: u64 = 100 * BYTES_PER_GB;

// =============================================================================
// Crypto Algorithm Names
// =============================================================================

/// AES-256-GCM algorithm identifier
pub const ALGORITHM_AES_256_GCM: &str = "AES-256-GCM";

/// Ed25519 algorithm identifier
pub const ALGORITHM_ED25519: &str = "Ed25519";

/// BLAKE3 algorithm identifier
pub const ALGORITHM_SHA256: &str = "SHA-256";
pub const ALGORITHM_BLAKE3: &str = "BLAKE3";

// =============================================================================
// HKDF Constants and Invariants
// =============================================================================

/// HKDF algorithm version for schema compatibility tracking.
pub const HKDF_ALGORITHM_VERSION: u32 = 2;

/// Required output length for all seed derivations (32 bytes).
pub const HKDF_OUTPUT_LENGTH: usize = 32;

/// AES-256 key size in bytes (32)
pub const AES_256_KEY_SIZE: usize = 32;

/// Ed25519 signature size in bytes (64)
pub const ED25519_SIGNATURE_SIZE: usize = 64;

/// BLAKE3 hash size in bytes (32)
pub const BLAKE3_HASH_SIZE: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_conversions() {
        assert_eq!(BYTES_PER_KB, 1024);
        assert_eq!(BYTES_PER_MB, 1024 * 1024);
        assert_eq!(BYTES_PER_GB, 1024 * 1024 * 1024);
        assert_eq!(BYTES_PER_TB, 1024 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_conversion_functions() {
        assert_eq!(kb_to_bytes(1), BYTES_PER_KB);
        assert_eq!(mb_to_bytes(1), BYTES_PER_MB);
        assert_eq!(gb_to_bytes(1), BYTES_PER_GB);

        assert_eq!(bytes_to_mb(BYTES_PER_MB), 1);
        assert_eq!(bytes_to_gb(BYTES_PER_GB), 1);

        // Test larger values
        assert_eq!(mb_to_bytes(100), 100 * BYTES_PER_MB);
        assert_eq!(gb_to_bytes(4), 4 * BYTES_PER_GB);
    }

    #[test]
    fn test_time_constants() {
        assert_eq!(SECONDS_PER_MINUTE, 60);
        assert_eq!(SECONDS_PER_HOUR, 3600);
        assert_eq!(SECONDS_PER_DAY, 86400);
    }

    #[test]
    fn test_memory_policy_constants() {
        assert!((DEFAULT_MIN_HEADROOM_PCT - 15.0).abs() < f32::EPSILON);
        assert!((DEFAULT_MIN_HEADROOM_FRACTION - 0.15).abs() < f32::EPSILON);
        assert!((MEMORY_WARNING_THRESHOLD - 0.85).abs() < f32::EPSILON);
        assert!((MEMORY_CRITICAL_THRESHOLD - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_key_rotation_constants() {
        assert_eq!(DEFAULT_KEY_ROTATION_DAYS, 30);
        assert_eq!(DEFAULT_KEY_ROTATION_SECS, 30 * 86400);
    }
}
