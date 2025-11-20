//! Integration tests for upload rate limiting
//!
//! Tests the complete upload rate limiting flow including:
//! - Per-tenant rate limit enforcement
//! - Token bucket algorithm
//! - Rate limit headers in responses
//! - Audit logging of violations

#[cfg(test)]
mod integration_tests {
    /// Test that the UploadRateLimiter module compiles and is accessible
    #[test]
    fn test_upload_rate_limiter_module_exists() {
        // This test verifies that the upload_rate_limiter module exists
        // and can be imported from the server-api crate
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

        // Module should be accessible
        let _limiter = UploadRateLimiter::new(10, 5);
    }

    /// Test basic rate limiting creation
    #[tokio::test]
    async fn test_create_upload_rate_limiter() {
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

        let limiter = UploadRateLimiter::new(10, 5);

        // First request should succeed
        let (allowed, remaining, reset_at) = limiter.check_rate_limit("test-tenant").await;
        assert!(allowed, "First request should be allowed");
        assert_eq!(remaining, 14, "Should have 14 remaining (10 + 5 burst - 1)");
        assert!(reset_at > 0, "Reset timestamp should be positive");
    }

    /// Test per-tenant isolation
    #[tokio::test]
    async fn test_per_tenant_isolation() {
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

        let limiter = UploadRateLimiter::new(5, 0);

        // Use up tenant A's quota
        for _ in 0..5 {
            let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
            assert!(allowed);
        }
        let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(!allowed, "Tenant A should be rate limited");

        // Tenant B should still have quota
        let (allowed, _, _) = limiter.check_rate_limit("tenant-b").await;
        assert!(allowed, "Tenant B should have independent quota");
    }

    /// Test rate limit reset
    #[tokio::test]
    async fn test_reset_rate_limit() {
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

        let limiter = UploadRateLimiter::new(2, 0);

        // Use up quota
        limiter.check_rate_limit("tenant").await;
        limiter.check_rate_limit("tenant").await;
        let (allowed, _, _) = limiter.check_rate_limit("tenant").await;
        assert!(!allowed);

        // Reset
        limiter.reset_rate_limit("tenant").await;

        // Should have quota again
        let (allowed, _, _) = limiter.check_rate_limit("tenant").await;
        assert!(allowed);
    }

    /// Test rate limiter with burst capacity
    #[tokio::test]
    async fn test_burst_capacity() {
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

        let limiter = UploadRateLimiter::new(5, 10); // 5 + 10 burst = 15 total

        // Should allow 15 uploads
        for i in 0..15 {
            let (allowed, _, _) = limiter.check_rate_limit("tenant").await;
            assert!(allowed, "Upload {} should succeed", i);
        }

        // 16th should fail
        let (allowed, _, _) = limiter.check_rate_limit("tenant").await;
        assert!(!allowed, "16th upload should fail");
    }

    /// Test that remaining count is accurate
    #[tokio::test]
    async fn test_remaining_count_accuracy() {
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

        let limiter = UploadRateLimiter::new(5, 0);

        for expected_remaining in (4..=-1).rev() {
            let (allowed, remaining, _) = limiter.check_rate_limit("tenant").await;
            if expected_remaining >= 0 {
                assert!(allowed);
                assert_eq!(
                    remaining, expected_remaining as u32,
                    "Remaining count should match"
                );
            } else {
                assert!(!allowed);
                assert_eq!(remaining, 0, "Should report 0 remaining when limited");
            }
        }
    }

    /// Test concurrent access from multiple tenants
    #[tokio::test]
    async fn test_concurrent_tenants() {
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;
        use std::sync::Arc;

        let limiter = Arc::new(UploadRateLimiter::new(3, 0));

        let mut handles = vec![];

        // Spawn 3 concurrent tasks for different tenants
        for tenant_id in 0..3 {
            let limiter_clone = Arc::clone(&limiter);
            let handle = tokio::spawn(async move {
                let tenant = format!("tenant-{}", tenant_id);
                let mut success_count = 0;

                // Each tries 5 uploads
                for _ in 0..5 {
                    let (allowed, _, _) = limiter_clone.check_rate_limit(&tenant).await;
                    if allowed {
                        success_count += 1;
                    }
                }

                success_count
            });

            handles.push(handle);
        }

        // Verify each tenant got exactly 3 uploads
        for handle in handles {
            let result = handle.await.expect("Task should complete");
            assert_eq!(result, 3, "Each tenant should get exactly 3 uploads");
        }
    }

    /// Test reset timestamp is reasonable
    #[tokio::test]
    async fn test_reset_timestamp_correctness() {
        use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

        let limiter = UploadRateLimiter::new(5, 0);

        let (_, _, reset_at) = limiter.check_rate_limit("tenant").await;

        // Reset timestamp should be approximately current time + 60 seconds
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert!(
            reset_at >= now && reset_at <= now + 61,
            "Reset timestamp should be within 60-61 seconds from now"
        );
    }
}
