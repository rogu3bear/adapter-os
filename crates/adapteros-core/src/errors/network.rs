//! Network-related errors
//!
//! Covers HTTP, TCP, UDS, timeouts, circuit breakers, and connectivity issues.

use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/// Network and connectivity errors
#[derive(Error, Debug)]
pub enum AosNetworkError {
    /// Generic HTTP error
    #[error("HTTP error: {0}")]
    Http(String),

    /// Generic network error
    #[error("Network error: {0}")]
    Network(String),

    /// Request timeout
    #[error("Timeout waiting for response after {duration:?}")]
    Timeout { duration: Duration },

    /// Circuit breaker is open (rejecting requests)
    #[error("Circuit breaker is open for service '{service}'")]
    CircuitBreakerOpen { service: String },

    /// Circuit breaker is half-open (testing recovery)
    #[error("Circuit breaker is half-open for service '{service}'")]
    CircuitBreakerHalfOpen { service: String },

    /// Unix domain socket connection failed
    #[error("UDS connection failed: {path}")]
    UdsConnectionFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Worker process not responding
    #[error("Worker not responding at {path}")]
    WorkerNotResponding { path: PathBuf },

    /// Invalid response from remote service
    #[error("Invalid response from worker: {reason}")]
    InvalidResponse { reason: String },

    /// Download operation failed
    #[error("Download failed for {repo_id}: {reason}")]
    DownloadFailed {
        repo_id: String,
        reason: String,
        is_resumable: bool,
    },

    /// Health check failed
    #[error("Health check failed for model {model_id}: {reason} (attempt {retry_count})")]
    HealthCheckFailed {
        model_id: String,
        reason: String,
        retry_count: u32,
    },

    /// Service unavailable (503-like errors)
    #[error("Service unavailable: {0}")]
    Unavailable(String),

    // =========================================================================
    // Network errors (Category 3)
    // =========================================================================
    /// DNS resolution failed for a host
    #[error("DNS resolution failed for host '{host}': {reason}")]
    DnsResolutionFailed {
        /// The hostname that failed to resolve
        host: String,
        /// Reason for the failure (e.g., "NXDOMAIN", "timeout")
        reason: String,
    },

    /// TLS/SSL certificate error
    #[error("TLS certificate error for '{host}': {reason}")]
    TlsCertificateError {
        /// The host with the certificate error
        host: String,
        /// Reason for the error
        reason: String,
        /// Whether the certificate is self-signed
        is_self_signed: bool,
        /// Whether the certificate has expired
        is_expired: bool,
    },

    /// Proxy connection failed
    #[error("Proxy connection failed: {proxy_url} - {reason}")]
    ProxyConnectionFailed {
        /// URL of the proxy that failed
        proxy_url: String,
        /// Reason for the failure
        reason: String,
    },

    /// Environment mismatch (e.g., prod client hitting staging API)
    #[error("Environment mismatch: expected {expected}, got {actual}")]
    EnvironmentMismatch {
        /// Expected environment (e.g., "production")
        expected: String,
        /// Actual environment detected
        actual: String,
        /// Hint for how to fix this
        hint: String,
    },
}

impl AosNetworkError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Timeout { .. } => true,
            Self::CircuitBreakerHalfOpen { .. } => true,
            Self::DownloadFailed { is_resumable, .. } => *is_resumable,
            Self::HealthCheckFailed { .. } => true,
            Self::UdsConnectionFailed { .. } => true,
            Self::WorkerNotResponding { .. } => true,
            Self::CircuitBreakerOpen { .. } => false, // Must wait for timeout
            Self::Unavailable(_) => true,             // Service might become available
            // DNS failures are transient and may resolve
            Self::DnsResolutionFailed { .. } => true,
            // Proxy errors are transient (proxy may recover)
            Self::ProxyConnectionFailed { .. } => true,
            // TLS certificate errors require human intervention
            Self::TlsCertificateError { .. } => false,
            // Environment mismatch requires configuration fix
            Self::EnvironmentMismatch { .. } => false,
            Self::Http(_) | Self::Network(_) | Self::InvalidResponse { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_error_display() {
        let err = AosNetworkError::Http("404 Not Found".to_string());
        assert!(err.to_string().contains("HTTP error"));
        assert!(err.to_string().contains("404"));
    }

    #[test]
    fn test_timeout_display() {
        let err = AosNetworkError::Timeout {
            duration: Duration::from_secs(30),
        };
        let msg = err.to_string();
        assert!(msg.contains("Timeout"));
        assert!(msg.contains("30"));
    }

    #[test]
    fn test_circuit_breaker_open_display() {
        let err = AosNetworkError::CircuitBreakerOpen {
            service: "inference-api".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Circuit breaker"));
        assert!(msg.contains("open"));
        assert!(msg.contains("inference-api"));
    }

    #[test]
    fn test_uds_connection_failed_display() {
        let err = AosNetworkError::UdsConnectionFailed {
            path: PathBuf::from("/var/run/worker.sock"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "socket not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("UDS connection failed"));
        assert!(msg.contains("/var/run/worker.sock"));
    }

    #[test]
    fn test_download_failed_display() {
        let err = AosNetworkError::DownloadFailed {
            repo_id: "org/model".to_string(),
            reason: "network interrupted".to_string(),
            is_resumable: true,
        };
        let msg = err.to_string();
        assert!(msg.contains("org/model"));
        assert!(msg.contains("network interrupted"));
    }

    #[test]
    fn test_health_check_failed_display() {
        let err = AosNetworkError::HealthCheckFailed {
            model_id: "qwen-7b".to_string(),
            reason: "timeout".to_string(),
            retry_count: 3,
        };
        let msg = err.to_string();
        assert!(msg.contains("qwen-7b"));
        assert!(msg.contains("attempt 3"));
    }

    // DNS/TLS/Proxy error tests
    #[test]
    fn test_dns_resolution_failed_display() {
        let err = AosNetworkError::DnsResolutionFailed {
            host: "api.example.com".to_string(),
            reason: "NXDOMAIN".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("DNS resolution failed"));
        assert!(msg.contains("api.example.com"));
        assert!(msg.contains("NXDOMAIN"));
    }

    #[test]
    fn test_tls_certificate_error_display() {
        let err = AosNetworkError::TlsCertificateError {
            host: "api.example.com".to_string(),
            reason: "certificate has expired".to_string(),
            is_self_signed: false,
            is_expired: true,
        };
        let msg = err.to_string();
        assert!(msg.contains("TLS certificate error"));
        assert!(msg.contains("api.example.com"));
        assert!(msg.contains("expired"));
    }

    #[test]
    fn test_proxy_connection_failed_display() {
        let err = AosNetworkError::ProxyConnectionFailed {
            proxy_url: "http://proxy.corp.local:8080".to_string(),
            reason: "connection refused".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Proxy connection failed"));
        assert!(msg.contains("proxy.corp.local"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_environment_mismatch_display() {
        let err = AosNetworkError::EnvironmentMismatch {
            expected: "production".to_string(),
            actual: "staging".to_string(),
            hint: "Check API_BASE_URL configuration".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Environment mismatch"));
        assert!(msg.contains("production"));
        assert!(msg.contains("staging"));
    }

    // Retryable tests
    #[test]
    fn test_timeout_is_retryable() {
        let err = AosNetworkError::Timeout {
            duration: Duration::from_secs(30),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_circuit_breaker_open_not_retryable() {
        let err = AosNetworkError::CircuitBreakerOpen {
            service: "api".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_circuit_breaker_half_open_is_retryable() {
        let err = AosNetworkError::CircuitBreakerHalfOpen {
            service: "api".to_string(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_download_resumable_is_retryable() {
        let err = AosNetworkError::DownloadFailed {
            repo_id: "org/model".to_string(),
            reason: "network error".to_string(),
            is_resumable: true,
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_download_not_resumable_not_retryable() {
        let err = AosNetworkError::DownloadFailed {
            repo_id: "org/model".to_string(),
            reason: "checksum mismatch".to_string(),
            is_resumable: false,
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_dns_resolution_is_retryable() {
        let err = AosNetworkError::DnsResolutionFailed {
            host: "api.example.com".to_string(),
            reason: "timeout".to_string(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_tls_certificate_not_retryable() {
        let err = AosNetworkError::TlsCertificateError {
            host: "api.example.com".to_string(),
            reason: "expired".to_string(),
            is_self_signed: false,
            is_expired: true,
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_proxy_connection_is_retryable() {
        let err = AosNetworkError::ProxyConnectionFailed {
            proxy_url: "http://proxy:8080".to_string(),
            reason: "timeout".to_string(),
        };
        assert!(err.is_retryable());
    }

    #[test]
    fn test_environment_mismatch_not_retryable() {
        let err = AosNetworkError::EnvironmentMismatch {
            expected: "prod".to_string(),
            actual: "staging".to_string(),
            hint: "fix config".to_string(),
        };
        assert!(!err.is_retryable());
    }

    // =========================================================================
    // Consistency tests: All variants implement Display consistently
    // =========================================================================

    /// Verify that all error variants produce non-empty display strings
    #[test]
    fn test_all_variants_display_non_empty() {
        let variants: Vec<AosNetworkError> = vec![
            AosNetworkError::Http("test".to_string()),
            AosNetworkError::Network("test".to_string()),
            AosNetworkError::Timeout {
                duration: Duration::from_secs(1),
            },
            AosNetworkError::CircuitBreakerOpen {
                service: "test".to_string(),
            },
            AosNetworkError::CircuitBreakerHalfOpen {
                service: "test".to_string(),
            },
            AosNetworkError::UdsConnectionFailed {
                path: PathBuf::from("/test"),
                source: std::io::Error::new(std::io::ErrorKind::Other, "test"),
            },
            AosNetworkError::WorkerNotResponding {
                path: PathBuf::from("/test"),
            },
            AosNetworkError::InvalidResponse {
                reason: "test".to_string(),
            },
            AosNetworkError::DownloadFailed {
                repo_id: "test".to_string(),
                reason: "test".to_string(),
                is_resumable: false,
            },
            AosNetworkError::HealthCheckFailed {
                model_id: "test".to_string(),
                reason: "test".to_string(),
                retry_count: 0,
            },
            AosNetworkError::Unavailable("test".to_string()),
            AosNetworkError::DnsResolutionFailed {
                host: "test".to_string(),
                reason: "test".to_string(),
            },
            AosNetworkError::TlsCertificateError {
                host: "test".to_string(),
                reason: "test".to_string(),
                is_self_signed: false,
                is_expired: false,
            },
            AosNetworkError::ProxyConnectionFailed {
                proxy_url: "test".to_string(),
                reason: "test".to_string(),
            },
            AosNetworkError::EnvironmentMismatch {
                expected: "test".to_string(),
                actual: "test".to_string(),
                hint: "test".to_string(),
            },
        ];

        for variant in variants {
            let display = variant.to_string();
            assert!(
                !display.is_empty(),
                "Display should not be empty for {:?}",
                variant
            );
            assert!(
                display.len() > 5,
                "Display should be informative for {:?}, got: {}",
                variant,
                display
            );
        }
    }

    /// Verify that display strings contain expected keywords for each variant type
    #[test]
    fn test_display_contains_variant_identifier() {
        // Each variant should have an identifying keyword in its display
        let test_cases: Vec<(AosNetworkError, &str)> = vec![
            (AosNetworkError::Http("msg".to_string()), "HTTP"),
            (AosNetworkError::Network("msg".to_string()), "Network"),
            (
                AosNetworkError::Timeout {
                    duration: Duration::from_secs(1),
                },
                "Timeout",
            ),
            (
                AosNetworkError::CircuitBreakerOpen {
                    service: "svc".to_string(),
                },
                "Circuit breaker",
            ),
            (
                AosNetworkError::CircuitBreakerHalfOpen {
                    service: "svc".to_string(),
                },
                "Circuit breaker",
            ),
            (
                AosNetworkError::UdsConnectionFailed {
                    path: PathBuf::from("/sock"),
                    source: std::io::Error::new(std::io::ErrorKind::Other, "err"),
                },
                "UDS",
            ),
            (
                AosNetworkError::WorkerNotResponding {
                    path: PathBuf::from("/sock"),
                },
                "Worker",
            ),
            (
                AosNetworkError::InvalidResponse {
                    reason: "bad".to_string(),
                },
                "Invalid response",
            ),
            (
                AosNetworkError::DownloadFailed {
                    repo_id: "repo".to_string(),
                    reason: "err".to_string(),
                    is_resumable: false,
                },
                "Download failed",
            ),
            (
                AosNetworkError::HealthCheckFailed {
                    model_id: "model".to_string(),
                    reason: "err".to_string(),
                    retry_count: 1,
                },
                "Health check failed",
            ),
            (
                AosNetworkError::Unavailable("msg".to_string()),
                "unavailable",
            ),
            (
                AosNetworkError::DnsResolutionFailed {
                    host: "host".to_string(),
                    reason: "err".to_string(),
                },
                "DNS",
            ),
            (
                AosNetworkError::TlsCertificateError {
                    host: "host".to_string(),
                    reason: "err".to_string(),
                    is_self_signed: false,
                    is_expired: false,
                },
                "TLS",
            ),
            (
                AosNetworkError::ProxyConnectionFailed {
                    proxy_url: "url".to_string(),
                    reason: "err".to_string(),
                },
                "Proxy",
            ),
            (
                AosNetworkError::EnvironmentMismatch {
                    expected: "prod".to_string(),
                    actual: "dev".to_string(),
                    hint: "fix".to_string(),
                },
                "Environment",
            ),
        ];

        for (error, expected_keyword) in test_cases {
            let display = error.to_string();
            assert!(
                display
                    .to_lowercase()
                    .contains(&expected_keyword.to_lowercase()),
                "Display for {:?} should contain '{}', got: {}",
                error,
                expected_keyword,
                display
            );
        }
    }

    // =========================================================================
    // Exhaustive is_retryable() tests for all variants
    // =========================================================================

    /// Test is_retryable() returns expected values for every variant
    #[test]
    fn test_is_retryable_exhaustive() {
        // (error, expected_retryable)
        let test_cases: Vec<(AosNetworkError, bool)> = vec![
            // Not retryable - permanent failures
            (AosNetworkError::Http("error".to_string()), false),
            (AosNetworkError::Network("error".to_string()), false),
            (
                AosNetworkError::InvalidResponse {
                    reason: "bad json".to_string(),
                },
                false,
            ),
            (
                AosNetworkError::CircuitBreakerOpen {
                    service: "api".to_string(),
                },
                false,
            ),
            (
                AosNetworkError::TlsCertificateError {
                    host: "example.com".to_string(),
                    reason: "expired".to_string(),
                    is_self_signed: false,
                    is_expired: true,
                },
                false,
            ),
            (
                AosNetworkError::EnvironmentMismatch {
                    expected: "prod".to_string(),
                    actual: "staging".to_string(),
                    hint: "check config".to_string(),
                },
                false,
            ),
            (
                AosNetworkError::DownloadFailed {
                    repo_id: "org/model".to_string(),
                    reason: "checksum failed".to_string(),
                    is_resumable: false,
                },
                false,
            ),
            // Retryable - transient failures
            (
                AosNetworkError::Timeout {
                    duration: Duration::from_secs(30),
                },
                true,
            ),
            (
                AosNetworkError::CircuitBreakerHalfOpen {
                    service: "api".to_string(),
                },
                true,
            ),
            (
                AosNetworkError::UdsConnectionFailed {
                    path: PathBuf::from("/var/run/worker.sock"),
                    source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
                },
                true,
            ),
            (
                AosNetworkError::WorkerNotResponding {
                    path: PathBuf::from("/var/run/worker.sock"),
                },
                true,
            ),
            (
                AosNetworkError::HealthCheckFailed {
                    model_id: "llama-7b".to_string(),
                    reason: "timeout".to_string(),
                    retry_count: 1,
                },
                true,
            ),
            (AosNetworkError::Unavailable("overloaded".to_string()), true),
            (
                AosNetworkError::DnsResolutionFailed {
                    host: "api.example.com".to_string(),
                    reason: "timeout".to_string(),
                },
                true,
            ),
            (
                AosNetworkError::ProxyConnectionFailed {
                    proxy_url: "http://proxy:8080".to_string(),
                    reason: "connection reset".to_string(),
                },
                true,
            ),
            (
                AosNetworkError::DownloadFailed {
                    repo_id: "org/model".to_string(),
                    reason: "network interrupted".to_string(),
                    is_resumable: true,
                },
                true,
            ),
        ];

        for (error, expected) in test_cases {
            assert_eq!(
                error.is_retryable(),
                expected,
                "is_retryable() mismatch for {:?}: expected {}, got {}",
                error,
                expected,
                error.is_retryable()
            );
        }
    }

    // =========================================================================
    // Security tests: Error messages don't leak sensitive information
    // =========================================================================

    /// Verify error messages don't expose sensitive keywords beyond what's in the path
    #[test]
    fn test_no_sensitive_info_leak() {
        // Even if a home directory path is used, it should be included
        // (this is expected behavior), but we verify no extra info is added
        let err = AosNetworkError::UdsConnectionFailed {
            path: PathBuf::from("/Users/testuser/.adapter/worker.sock"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        let display = err.to_string();

        // The path is intentionally shown (for debugging), but verify no extra
        // system information is leaked beyond what's in the path
        assert!(!display.contains("password"));
        assert!(!display.contains("secret"));
        assert!(!display.contains("token"));
        assert!(!display.contains("key="));
        // Note: "auth" check removed as it could appear in legitimate path components
    }

    /// Verify proxy URLs don't accidentally include credentials in display
    #[test]
    fn test_proxy_url_with_credentials_warning() {
        // Note: The error type accepts any string, so callers should sanitize
        // Here we verify the display doesn't add extra credential-related text
        let err = AosNetworkError::ProxyConnectionFailed {
            proxy_url: "http://proxy.example.com:8080".to_string(),
            reason: "connection refused".to_string(),
        };
        let display = err.to_string();

        // Display should only contain what was passed
        assert!(display.contains("proxy.example.com"));
        assert!(!display.contains("user:"));
        assert!(!display.contains("password"));
    }

    /// Verify HTTP errors don't expose authorization headers or tokens
    #[test]
    fn test_http_error_no_auth_leak() {
        let err = AosNetworkError::Http("401 Unauthorized".to_string());
        let display = err.to_string();

        assert!(display.contains("401"));
        assert!(!display.contains("Bearer"));
        assert!(!display.contains("Authorization"));
    }

    /// Verify download errors don't expose API keys or tokens
    #[test]
    fn test_download_error_no_token_leak() {
        let err = AosNetworkError::DownloadFailed {
            repo_id: "huggingface/llama-2-7b".to_string(),
            reason: "rate limited".to_string(),
            is_resumable: true,
        };
        let display = err.to_string();

        assert!(display.contains("huggingface/llama-2-7b"));
        assert!(!display.contains("hf_"));
        assert!(!display.contains("token"));
        assert!(!display.contains("api_key"));
    }

    // =========================================================================
    // Edge case tests: Empty strings and boundary conditions
    // =========================================================================

    /// Test behavior with empty string fields
    #[test]
    fn test_empty_string_fields() {
        let test_cases: Vec<AosNetworkError> = vec![
            AosNetworkError::Http(String::new()),
            AosNetworkError::Network(String::new()),
            AosNetworkError::CircuitBreakerOpen {
                service: String::new(),
            },
            AosNetworkError::CircuitBreakerHalfOpen {
                service: String::new(),
            },
            AosNetworkError::InvalidResponse {
                reason: String::new(),
            },
            AosNetworkError::DownloadFailed {
                repo_id: String::new(),
                reason: String::new(),
                is_resumable: false,
            },
            AosNetworkError::HealthCheckFailed {
                model_id: String::new(),
                reason: String::new(),
                retry_count: 0,
            },
            AosNetworkError::Unavailable(String::new()),
            AosNetworkError::DnsResolutionFailed {
                host: String::new(),
                reason: String::new(),
            },
            AosNetworkError::TlsCertificateError {
                host: String::new(),
                reason: String::new(),
                is_self_signed: false,
                is_expired: false,
            },
            AosNetworkError::ProxyConnectionFailed {
                proxy_url: String::new(),
                reason: String::new(),
            },
            AosNetworkError::EnvironmentMismatch {
                expected: String::new(),
                actual: String::new(),
                hint: String::new(),
            },
        ];

        for error in test_cases {
            // Should not panic when displaying
            let display = error.to_string();
            // Display should still have the error type prefix
            assert!(
                !display.is_empty(),
                "Display should not be completely empty for {:?}",
                error
            );
            // is_retryable should still work
            let _ = error.is_retryable();
        }
    }

    /// Test with very long hostnames (boundary condition)
    #[test]
    fn test_very_long_hostname() {
        let long_hostname = "a".repeat(1000);
        let err = AosNetworkError::DnsResolutionFailed {
            host: long_hostname.clone(),
            reason: "timeout".to_string(),
        };
        let display = err.to_string();

        // Should contain the full hostname (no truncation by error type)
        assert!(display.contains(&long_hostname));
        assert!(display.contains("DNS"));
    }

    /// Test with unicode in error messages
    #[test]
    fn test_unicode_in_messages() {
        let err = AosNetworkError::Http("错误: 服务不可用".to_string());
        let display = err.to_string();
        assert!(display.contains("错误"));
        assert!(display.contains("HTTP"));
    }

    /// Test with special characters in hostnames
    #[test]
    fn test_special_characters_in_hostname() {
        let err = AosNetworkError::DnsResolutionFailed {
            host: "api-v2.example-test.co.uk".to_string(),
            reason: "NXDOMAIN".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("api-v2.example-test.co.uk"));
    }

    /// Test with newlines and control characters in messages
    #[test]
    fn test_newlines_in_messages() {
        let err = AosNetworkError::Network("line1\nline2\ttab".to_string());
        let display = err.to_string();
        // Should include the content as-is (caller responsibility to sanitize)
        assert!(display.contains("line1"));
        assert!(display.contains("line2"));
    }

    /// Test timeout with zero duration
    #[test]
    fn test_timeout_zero_duration() {
        let err = AosNetworkError::Timeout {
            duration: Duration::ZERO,
        };
        let display = err.to_string();
        assert!(display.contains("Timeout"));
        // Zero duration should still be retryable
        assert!(err.is_retryable());
    }

    /// Test timeout with very large duration
    #[test]
    fn test_timeout_large_duration() {
        let err = AosNetworkError::Timeout {
            duration: Duration::from_secs(86400 * 365), // 1 year
        };
        let display = err.to_string();
        assert!(display.contains("Timeout"));
        assert!(err.is_retryable());
    }

    /// Test health check with max retry count
    #[test]
    fn test_health_check_max_retry_count() {
        let err = AosNetworkError::HealthCheckFailed {
            model_id: "model".to_string(),
            reason: "timeout".to_string(),
            retry_count: u32::MAX,
        };
        let display = err.to_string();
        assert!(display.contains(&u32::MAX.to_string()));
        // Should still be retryable regardless of count
        assert!(err.is_retryable());
    }

    /// Test empty path for UDS and worker errors
    #[test]
    fn test_empty_path() {
        let err = AosNetworkError::UdsConnectionFailed {
            path: PathBuf::new(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        let display = err.to_string();
        assert!(display.contains("UDS"));
        assert!(err.is_retryable());

        let err2 = AosNetworkError::WorkerNotResponding {
            path: PathBuf::new(),
        };
        let display2 = err2.to_string();
        assert!(display2.contains("Worker"));
        assert!(err2.is_retryable());
    }

    /// Test TLS error with both self-signed and expired flags
    #[test]
    fn test_tls_both_flags_set() {
        let err = AosNetworkError::TlsCertificateError {
            host: "example.com".to_string(),
            reason: "multiple issues".to_string(),
            is_self_signed: true,
            is_expired: true,
        };
        // Should not panic and should not be retryable
        assert!(!err.is_retryable());
        let display = err.to_string();
        assert!(display.contains("TLS"));
    }

    /// Test TLS error with neither flag set (valid but untrusted cert)
    #[test]
    fn test_tls_no_flags_set() {
        let err = AosNetworkError::TlsCertificateError {
            host: "example.com".to_string(),
            reason: "untrusted CA".to_string(),
            is_self_signed: false,
            is_expired: false,
        };
        assert!(!err.is_retryable());
    }

    // =========================================================================
    // Error trait implementation tests
    // =========================================================================

    /// Verify errors implement std::error::Error properly
    #[test]
    fn test_error_trait_implementation() {
        let err = AosNetworkError::UdsConnectionFailed {
            path: PathBuf::from("/test.sock"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "socket missing"),
        };

        // Should implement Error trait
        let error_ref: &dyn std::error::Error = &err;
        assert!(!error_ref.to_string().is_empty());

        // UdsConnectionFailed should have a source
        assert!(error_ref.source().is_some());
    }

    /// Verify errors without source return None for source()
    #[test]
    fn test_error_without_source() {
        let err = AosNetworkError::Http("error".to_string());
        let error_ref: &dyn std::error::Error = &err;
        assert!(error_ref.source().is_none());
    }

    /// Test Debug implementation produces useful output
    #[test]
    fn test_debug_implementation() {
        let err = AosNetworkError::DownloadFailed {
            repo_id: "org/model".to_string(),
            reason: "network error".to_string(),
            is_resumable: true,
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("DownloadFailed"));
        assert!(debug_str.contains("org/model"));
        assert!(debug_str.contains("is_resumable"));
    }
}
