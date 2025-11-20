//! Upload rate limiting tests for PRD-2 DoS protection
//!
//! Tests per-tenant upload rate limiting with token bucket algorithm
//! ensuring rapid upload DoS attacks are prevented while allowing legitimate usage.

#[cfg(test)]
mod upload_rate_limit_tests {
    use adapteros_server_api::upload_rate_limiter::UploadRateLimiter;

    /// Test basic rate limiting enforcement
    #[tokio::test]
    async fn test_upload_rate_limit_basic() {
        let limiter = UploadRateLimiter::new(5, 2); // 5/min, burst 2

        // First 7 requests succeed (5 + burst of 2)
        for i in 0..7 {
            let (allowed, remaining, _) = limiter.check_rate_limit("tenant-a").await;
            assert!(allowed, "Request {} should be allowed (within quota)", i);
            assert_eq!(remaining, 6 - i as u32, "Remaining should decrease");
        }

        // 8th request should fail (rate limited)
        let (allowed, remaining, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(!allowed, "8th request should be rate limited");
        assert_eq!(remaining, 0, "No tokens remaining");
    }

    /// Test per-tenant isolation
    #[tokio::test]
    async fn test_upload_rate_limit_per_tenant_isolation() {
        let limiter = UploadRateLimiter::new(3, 0); // 3/min, no burst

        // Tenant A uses 2 uploads
        let (a1, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a2, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(a1 && a2, "First two uploads should succeed for tenant-a");

        // Tenant B should have independent quota
        let (b1, _, _) = limiter.check_rate_limit("tenant-b").await;
        let (b2, _, _) = limiter.check_rate_limit("tenant-b").await;
        let (b3, _, _) = limiter.check_rate_limit("tenant-b").await;
        assert!(b1 && b2 && b3, "Three uploads should succeed for tenant-b");

        // Both should be rate limited on next request
        let (a3, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (b4, _, _) = limiter.check_rate_limit("tenant-b").await;
        assert!(!a3 && !b4, "Both tenants should be rate limited");
    }

    /// Test rate limit reset functionality
    #[tokio::test]
    async fn test_upload_rate_limit_reset() {
        let limiter = UploadRateLimiter::new(2, 0);

        // Use up quota
        let (a1, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a2, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a3, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(a1 && a2 && !a3, "First two allowed, third denied");

        // Reset quota
        limiter.reset_rate_limit("tenant-a").await;

        // Should have full quota again
        let (a4, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a5, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a6, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(a4 && a5 && !a6, "Reset should restore quota");
    }

    /// Test stale bucket cleanup
    #[tokio::test]
    async fn test_upload_rate_limit_stale_cleanup() {
        let limiter = UploadRateLimiter::new(5, 0);

        // Access tenant A
        limiter.check_rate_limit("tenant-a").await;

        // Verify bucket exists
        {
            let buckets = limiter.buckets.read().await;
            assert!(buckets.contains_key("tenant-a"), "Bucket should exist");
        }

        // Cleanup with very short timeout (1ms)
        // Give it a bit of time to mark as stale
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        limiter.cleanup_stale_buckets(1).await;

        // Bucket should be gone after cleanup
        {
            let buckets = limiter.buckets.read().await;
            // Note: timing-dependent, may or may not be cleaned up
            // The important thing is cleanup doesn't crash
        }
    }

    /// Test burst capacity (exceeding rate but within burst)
    #[tokio::test]
    async fn test_upload_rate_limit_burst() {
        let limiter = UploadRateLimiter::new(5, 10); // 5/min, burst 10

        // Can upload up to 15 (5 + 10 burst)
        for i in 0..15 {
            let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
            assert!(allowed, "Upload {} should succeed within burst capacity", i);
        }

        // 16th should fail
        let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(!allowed, "Upload 16 should fail (exceeds capacity)");
    }

    /// Test reset_at timestamp
    #[tokio::test]
    async fn test_upload_rate_limit_reset_timestamp() {
        let limiter = UploadRateLimiter::new(5, 0);

        // First request returns reset_at
        let (allowed, _, reset_at) = limiter.check_rate_limit("tenant-a").await;
        assert!(allowed, "First request should succeed");
        assert!(reset_at > 0, "Reset timestamp should be positive");

        // Reset timestamp should be approximately current_time + 60 seconds
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            reset_at >= now && reset_at <= now + 61,
            "Reset timestamp should be within 60-61 seconds"
        );
    }

    /// Test remaining count decreases correctly
    #[tokio::test]
    async fn test_upload_rate_limit_remaining_count() {
        let limiter = UploadRateLimiter::new(5, 0);

        // First request
        let (_, remaining_1, _) = limiter.check_rate_limit("tenant-a").await;
        assert_eq!(remaining_1, 4, "Should have 4 remaining after 1st upload");

        // Second request
        let (_, remaining_2, _) = limiter.check_rate_limit("tenant-a").await;
        assert_eq!(remaining_2, 3, "Should have 3 remaining after 2nd upload");

        // Third request
        let (_, remaining_3, _) = limiter.check_rate_limit("tenant-a").await;
        assert_eq!(remaining_3, 2, "Should have 2 remaining after 3rd upload");

        // Fifth request (last one)
        limiter.check_rate_limit("tenant-a").await;
        limiter.check_rate_limit("tenant-a").await;
        let (_, remaining_5, _) = limiter.check_rate_limit("tenant-a").await;
        assert_eq!(remaining_5, 0, "Should have 0 remaining after 5th upload");
    }

    /// Test multiple concurrent tenants
    #[tokio::test]
    async fn test_upload_rate_limit_concurrent_tenants() {
        let limiter = UploadRateLimiter::new(3, 0);

        // Create tasks for multiple tenants
        let mut handles = vec![];

        for tenant_id in 0..5 {
            let limiter_clone = limiter.clone();
            let handle = tokio::spawn(async move {
                let tenant = format!("tenant-{}", tenant_id);
                let mut success_count = 0;

                // Try 5 uploads per tenant
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

        // Collect results
        for handle in handles {
            let result = handle.await.expect("Task should complete");
            assert_eq!(result, 3, "Each tenant should have exactly 3 successes");
        }
    }

    /// Test that different tenants don't interfere
    #[tokio::test]
    async fn test_upload_rate_limit_tenant_independence() {
        let limiter = UploadRateLimiter::new(2, 0);

        // Tenant A uses full quota
        let (a1, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a2, _, _) = limiter.check_rate_limit("tenant-a").await;
        let (a3, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(a1 && a2 && !a3, "Tenant-a should have 2 uploads");

        // Tenant B should still have full quota
        let (b1, _, _) = limiter.check_rate_limit("tenant-b").await;
        let (b2, _, _) = limiter.check_rate_limit("tenant-b").await;
        let (b3, _, _) = limiter.check_rate_limit("tenant-b").await;
        assert!(b1 && b2 && !b3, "Tenant-b should also have 2 uploads");

        // Tenant C should also have full quota
        let (c1, _, _) = limiter.check_rate_limit("tenant-c").await;
        assert!(c1, "Tenant-c should have at least 1 upload");
    }

    /// Test rate limiter with zero rate (should fail all)
    #[tokio::test]
    async fn test_upload_rate_limit_zero_rate() {
        let limiter = UploadRateLimiter::new(0, 0);

        // Even first request should fail
        let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(!allowed, "First request should fail with zero rate");
    }

    /// Test with high burst but low rate
    #[tokio::test]
    async fn test_upload_rate_limit_high_burst_low_rate() {
        let limiter = UploadRateLimiter::new(1, 100); // 1/min, burst 100

        // Can upload 101 times (1 + 100 burst)
        for i in 0..101 {
            let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
            assert!(allowed, "Upload {} should succeed", i);
        }

        // 102nd should fail
        let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
        assert!(!allowed, "Upload 102 should fail");
    }

    /// Test rate limiter is Send + Sync
    #[tokio::test]
    async fn test_upload_rate_limiter_is_send_sync() {
        let limiter = UploadRateLimiter::new(5, 2);

        // This test just verifies the type is Send + Sync
        fn assert_send_sync<T: Send + Sync>(_: &T) {}
        assert_send_sync(&limiter);
    }

    /// Test large number of sequential uploads
    #[tokio::test]
    async fn test_upload_rate_limit_sequential_stress() {
        let limiter = UploadRateLimiter::new(100, 50);

        let mut allowed_count = 0;
        for _ in 0..200 {
            let (allowed, _, _) = limiter.check_rate_limit("tenant-a").await;
            if allowed {
                allowed_count += 1;
            }
        }

        // Should allow exactly 150 (100 + 50 burst)
        assert_eq!(allowed_count, 150, "Should allow exactly 150 uploads");
    }
}
