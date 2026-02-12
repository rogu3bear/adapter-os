//! Canonical error code constants for API responses.
//!
//! Centralizes error codes to prevent typos and ensure consistency across
//! the codebase. All error codes used in API responses should be defined here.
//!
//! # Usage
//!
//! ```rust
//! use adapteros_core::error_codes;
//!
//! let code = error_codes::NOT_FOUND;
//! assert_eq!(code, "NOT_FOUND");
//! ```
//!
//! # Categories
//!
//! Error codes are organized by HTTP status code category:
//! - 4xx Client Errors (validation, auth, not found, conflicts)
//! - 5xx Server Errors (internal, gateway, service unavailable)

// =============================================================================
// 400 Bad Request - Validation and Parse Errors
// =============================================================================

/// Generic bad request error
pub const BAD_REQUEST: &str = "BAD_REQUEST";

/// Input validation failed
pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";

/// JSON/data serialization error
pub const SERIALIZATION_ERROR: &str = "SERIALIZATION_ERROR";

/// Input parsing failed
pub const PARSE_ERROR: &str = "PARSE_ERROR";

/// Hash format or value is invalid
pub const INVALID_HASH: &str = "INVALID_HASH";

/// Checkpoint ID format is invalid
pub const INVALID_CPID: &str = "INVALID_CPID";

/// Adapter manifest is malformed or invalid
pub const INVALID_MANIFEST: &str = "INVALID_MANIFEST";

/// Adapter not present in manifest
pub const ADAPTER_NOT_IN_MANIFEST: &str = "ADAPTER_NOT_IN_MANIFEST";

/// Adapter not in effective adapter set
pub const ADAPTER_NOT_IN_EFFECTIVE_SET: &str = "ADAPTER_NOT_IN_EFFECTIVE_SET";

/// Kernel layout does not match expected configuration
pub const KERNEL_LAYOUT_MISMATCH: &str = "KERNEL_LAYOUT_MISMATCH";

/// Chat template processing error
pub const CHAT_TEMPLATE_ERROR: &str = "CHAT_TEMPLATE_ERROR";

/// Required field is missing
pub const MISSING_FIELD: &str = "MISSING_FIELD";

/// Tenant ID format is invalid
pub const INVALID_TENANT_ID: &str = "INVALID_TENANT_ID";

/// Session ID format is invalid
pub const INVALID_SESSION_ID: &str = "INVALID_SESSION_ID";

/// Sealed data format or integrity check failed
pub const INVALID_SEALED_DATA: &str = "INVALID_SEALED_DATA";

/// Requested feature is disabled
pub const FEATURE_DISABLED: &str = "FEATURE_DISABLED";

/// Preflight checks failed
pub const PREFLIGHT_FAILED: &str = "PREFLIGHT_FAILED";

/// Schema version is incompatible
pub const INCOMPATIBLE_SCHEMA_VERSION: &str = "INCOMPATIBLE_SCHEMA_VERSION";

/// Adapter base model does not match request
pub const ADAPTER_BASE_MODEL_MISMATCH: &str = "ADAPTER_BASE_MODEL_MISMATCH";

/// Determinism validation failed
pub const DETERMINISM_ERROR: &str = "DETERMINISM_ERROR";

// =============================================================================
// 401 Unauthorized - Authentication Errors
// =============================================================================

/// Generic unauthorized error
pub const UNAUTHORIZED: &str = "UNAUTHORIZED";

/// No authentication token provided
pub const TOKEN_MISSING: &str = "TOKEN_MISSING";

/// Token format is invalid
pub const TOKEN_INVALID: &str = "TOKEN_INVALID";

/// Token signature verification failed
pub const TOKEN_SIGNATURE_INVALID: &str = "TOKEN_SIGNATURE_INVALID";

/// Token has expired
pub const TOKEN_EXPIRED: &str = "TOKEN_EXPIRED";

/// Token was explicitly revoked
pub const TOKEN_REVOKED: &str = "TOKEN_REVOKED";

/// Token issuer does not match expected value
pub const INVALID_ISSUER: &str = "INVALID_ISSUER";

/// Token audience does not match expected value
pub const INVALID_AUDIENCE: &str = "INVALID_AUDIENCE";

/// API key is invalid or not found
pub const INVALID_API_KEY: &str = "INVALID_API_KEY";

/// Session has expired
pub const SESSION_EXPIRED: &str = "SESSION_EXPIRED";

/// Session is locked due to suspicious activity
pub const SESSION_LOCKED: &str = "SESSION_LOCKED";

/// Device ID mismatch between token and session
pub const DEVICE_MISMATCH: &str = "DEVICE_MISMATCH";

/// Username/password credentials are invalid
pub const INVALID_CREDENTIALS: &str = "INVALID_CREDENTIALS";

// =============================================================================
// 403 Forbidden - Authorization and Policy Errors
// =============================================================================

/// Generic forbidden error
pub const FORBIDDEN: &str = "FORBIDDEN";

/// User lacks required permission
pub const PERMISSION_DENIED: &str = "PERMISSION_DENIED";

/// Cross-tenant access violation
pub const TENANT_ISOLATION_ERROR: &str = "TENANT_ISOLATION_ERROR";

/// CSRF token validation failed
pub const CSRF_ERROR: &str = "CSRF_ERROR";

/// User role insufficient for operation
pub const INSUFFICIENT_ROLE: &str = "INSUFFICIENT_ROLE";

/// Multi-factor authentication required
pub const MFA_REQUIRED: &str = "MFA_REQUIRED";

/// Policy rule violation
pub const POLICY_VIOLATION: &str = "POLICY_VIOLATION";

/// Policy evaluation error
pub const POLICY_ERROR: &str = "POLICY_ERROR";

/// Determinism invariant violated
pub const DETERMINISM_VIOLATION: &str = "DETERMINISM_VIOLATION";

/// Network egress policy violated
pub const EGRESS_VIOLATION: &str = "EGRESS_VIOLATION";

/// Outbound request resolved to a private/reserved IP range (SSRF protection)
pub const SSRF_BLOCKED: &str = "SSRF_BLOCKED";

/// Tenant isolation boundary violated
pub const ISOLATION_VIOLATION: &str = "ISOLATION_VIOLATION";

/// Performance budget exceeded
pub const PERFORMANCE_VIOLATION: &str = "PERFORMANCE_VIOLATION";

/// Anomalous behavior detected
pub const ANOMALY_DETECTED: &str = "ANOMALY_DETECTED";

/// System is in quarantine mode
pub const SYSTEM_QUARANTINED: &str = "SYSTEM_QUARANTINED";

/// Adapter belongs to different tenant
pub const ADAPTER_TENANT_MISMATCH: &str = "ADAPTER_TENANT_MISMATCH";

/// Data integrity violation (checksum, hash, or validation failure)
pub const INTEGRITY_VIOLATION: &str = "INTEGRITY_VIOLATION";

/// Checkpoint cryptographic integrity verification failed (BLAKE3 + Ed25519)
pub const CHECKPOINT_INTEGRITY_FAILED: &str = "CHECKPOINT_INTEGRITY_FAILED";

// =============================================================================
// 404 Not Found
// =============================================================================

/// Generic not found error
pub const NOT_FOUND: &str = "NOT_FOUND";

/// Requested adapter not found
pub const ADAPTER_NOT_FOUND: &str = "ADAPTER_NOT_FOUND";

/// Requested model not found
pub const MODEL_NOT_FOUND: &str = "MODEL_NOT_FOUND";

/// Requested cache entry not found
pub const CACHE_ENTRY_NOT_FOUND: &str = "CACHE_ENTRY_NOT_FOUND";

// =============================================================================
// 409 Conflict - Hash Mismatches and State Conflicts
// =============================================================================

/// Generic conflict error
pub const CONFLICT: &str = "CONFLICT";

/// Adapter content hash mismatch
pub const ADAPTER_HASH_MISMATCH: &str = "ADAPTER_HASH_MISMATCH";

/// Adapter layer hash mismatch
pub const ADAPTER_LAYER_HASH_MISMATCH: &str = "ADAPTER_LAYER_HASH_MISMATCH";

/// Policy hash mismatch
pub const POLICY_HASH_MISMATCH: &str = "POLICY_HASH_MISMATCH";

/// Promotion workflow error
pub const PROMOTION_ERROR: &str = "PROMOTION_ERROR";

/// Model acquisition already in progress
pub const MODEL_ACQUISITION_IN_PROGRESS: &str = "MODEL_ACQUISITION_IN_PROGRESS";

/// Duplicate request (idempotency violation)
pub const DUPLICATE_REQUEST: &str = "DUPLICATE_REQUEST";

/// Adapter is currently in flight (loading/unloading in progress)
pub const ADAPTER_IN_FLIGHT: &str = "ADAPTER_IN_FLIGHT";

// =============================================================================
// 422 Unprocessable Entity
// =============================================================================

/// Reasoning loop detected during inference
pub const REASONING_LOOP_DETECTED: &str = "REASONING_LOOP_DETECTED";

// =============================================================================
// 429 Too Many Requests
// =============================================================================

/// Rate limit exceeded
pub const TOO_MANY_REQUESTS: &str = "TOO_MANY_REQUESTS";

/// System under memory pressure (backpressure)
pub const BACKPRESSURE: &str = "BACKPRESSURE";

// =============================================================================
// 499 Client Closed Request
// =============================================================================

/// Client closed connection before response completed
pub const CLIENT_CLOSED_REQUEST: &str = "CLIENT_CLOSED_REQUEST";

// =============================================================================
// 500 Internal Server Error
// =============================================================================

/// Generic internal error
pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";

/// Export operation failed
pub const EXPORT_FAILED: &str = "EXPORT_FAILED";

/// Database operation failed
pub const DATABASE_ERROR: &str = "DATABASE_ERROR";

/// Cryptographic operation failed
pub const CRYPTO_ERROR: &str = "CRYPTO_ERROR";

/// Configuration error
pub const CONFIG_ERROR: &str = "CONFIG_ERROR";

/// RAG retrieval error
pub const RAG_ERROR: &str = "RAG_ERROR";

/// Routing bypass (should never happen)
pub const ROUTING_BYPASS: &str = "ROUTING_BYPASS";

/// Replay operation failed
pub const REPLAY_ERROR: &str = "REPLAY_ERROR";

// =============================================================================
// 502 Bad Gateway - External Service Errors
// =============================================================================

/// Generic bad gateway error
pub const BAD_GATEWAY: &str = "BAD_GATEWAY";

/// Network communication error
pub const NETWORK_ERROR: &str = "NETWORK_ERROR";

/// Base LLM backend error
pub const BASE_LLM_ERROR: &str = "BASE_LLM_ERROR";

/// Unix domain socket connection failed
pub const UDS_CONNECTION_FAILED: &str = "UDS_CONNECTION_FAILED";

/// Invalid response from external service
pub const INVALID_RESPONSE: &str = "INVALID_RESPONSE";

/// File download failed
pub const DOWNLOAD_FAILED: &str = "DOWNLOAD_FAILED";

// =============================================================================
// 503 Service Unavailable - Resource Exhaustion
// =============================================================================

/// Generic service unavailable
pub const SERVICE_UNAVAILABLE: &str = "SERVICE_UNAVAILABLE";

/// Memory pressure threshold exceeded
pub const MEMORY_PRESSURE: &str = "MEMORY_PRESSURE";

/// Worker process not responding
pub const WORKER_NOT_RESPONDING: &str = "WORKER_NOT_RESPONDING";

/// Circuit breaker is open
pub const CIRCUIT_BREAKER_OPEN: &str = "CIRCUIT_BREAKER_OPEN";

/// Circuit breaker is half-open (testing)
pub const CIRCUIT_BREAKER_HALF_OPEN: &str = "CIRCUIT_BREAKER_HALF_OPEN";

/// Health check failed
pub const HEALTH_CHECK_FAILED: &str = "HEALTH_CHECK_FAILED";

/// Adapter not loaded into memory
pub const ADAPTER_NOT_LOADED: &str = "ADAPTER_NOT_LOADED";

/// Adapter exists but cannot be loaded
pub const ADAPTER_NOT_LOADABLE: &str = "ADAPTER_NOT_LOADABLE";

/// Model cache budget exceeded
pub const CACHE_BUDGET_EXCEEDED: &str = "CACHE_BUDGET_EXCEEDED";

/// CPU thermal throttling active
pub const CPU_THROTTLED: &str = "CPU_THROTTLED";

/// System out of memory
pub const OUT_OF_MEMORY: &str = "OUT_OF_MEMORY";

/// File descriptors exhausted
pub const FD_EXHAUSTED: &str = "FD_EXHAUSTED";

/// Thread pool at capacity
pub const THREAD_POOL_SATURATED: &str = "THREAD_POOL_SATURATED";

/// GPU not available
pub const GPU_UNAVAILABLE: &str = "GPU_UNAVAILABLE";

/// Disk space exhausted
pub const DISK_FULL: &str = "DISK_FULL";

/// Temporary directory unavailable
pub const TEMP_DIR_UNAVAILABLE: &str = "TEMP_DIR_UNAVAILABLE";

/// Model not ready for inference
pub const MODEL_NOT_READY: &str = "MODEL_NOT_READY";

/// No compatible worker for request
pub const NO_COMPATIBLE_WORKER: &str = "NO_COMPATIBLE_WORKER";

/// Worker in degraded mode
pub const WORKER_DEGRADED: &str = "WORKER_DEGRADED";

/// Worker ID unavailable for token generation
pub const WORKER_ID_UNAVAILABLE: &str = "WORKER_ID_UNAVAILABLE";

// =============================================================================
// 504 Gateway Timeout
// =============================================================================

/// Request timed out
pub const GATEWAY_TIMEOUT: &str = "GATEWAY_TIMEOUT";

/// Request timeout (alias)
pub const REQUEST_TIMEOUT: &str = "REQUEST_TIMEOUT";

// =============================================================================
// Boot-Time Fatal Errors (used during startup)
// =============================================================================

/// Dev bypass requested in release build
pub const DEV_BYPASS_IN_RELEASE: &str = "DEV_BYPASS_IN_RELEASE";

/// JWT authentication mode not properly configured
pub const JWT_MODE_NOT_CONFIGURED: &str = "JWT_MODE_NOT_CONFIGURED";

/// API key authentication mode not properly configured
pub const API_KEY_MODE_NOT_CONFIGURED: &str = "API_KEY_MODE_NOT_CONFIGURED";

// =============================================================================
// Payload Errors
// =============================================================================

/// Request payload exceeds size limit
pub const PAYLOAD_TOO_LARGE: &str = "PAYLOAD_TOO_LARGE";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes_are_uppercase_snake_case() {
        // Verify format consistency for a sampling of codes
        let codes = [
            NOT_FOUND,
            INTERNAL_ERROR,
            DATABASE_ERROR,
            INVALID_HASH,
            INVALID_CPID,
            POLICY_VIOLATION,
            SERVICE_UNAVAILABLE,
            CHECKPOINT_INTEGRITY_FAILED,
        ];

        for code in codes {
            assert!(
                code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "Error code '{}' should be UPPERCASE_SNAKE_CASE",
                code
            );
        }
    }

    #[test]
    fn test_error_codes_not_empty() {
        assert!(!NOT_FOUND.is_empty());
        assert!(!INTERNAL_ERROR.is_empty());
        assert!(!DATABASE_ERROR.is_empty());
    }
}
