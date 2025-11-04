use adapteros_core::{RetryPolicy, RetryManager, RetryBudgetConfig, AosError, Result};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Testing retry budget functionality...");

    // Test 1: Basic budget integration
    println!("\n=== Test 1: Basic Budget Integration ===");
    let budget_config = RetryBudgetConfig {
        max_concurrent_retries: 2,
        max_retry_rate_per_second: 10.0,
        budget_window: std::time::Duration::from_secs(1),
        max_budget_tokens: 10,
    };

    let manager = RetryManager::with_budget(budget_config.clone());
    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay: std::time::Duration::from_millis(10),
        budget: Some(budget_config),
        ..RetryPolicy::fast("test")
    };

    let active_operations = Arc::new(AtomicUsize::new(0));
    let max_concurrent_seen = Arc::new(AtomicUsize::new(0));

    // Create multiple concurrent operations
    let mut handles = vec![];
    for i in 0..5 {
        let manager = manager.clone();
        let policy = policy.clone();
        let active_operations = active_operations.clone();
        let max_concurrent_seen = max_concurrent_seen.clone();

        let handle = tokio::spawn(async move {
            let result = manager
                .execute_with_policy(&policy, || {
                    let active_operations = active_operations.clone();
                    let max_concurrent_seen = max_concurrent_seen.clone();
                    Box::pin(async move {
                        let current = active_operations.fetch_add(1, Ordering::SeqCst) + 1;
                        max_concurrent_seen.fetch_max(current, Ordering::SeqCst);

                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

                        active_operations.fetch_sub(1, Ordering::SeqCst);
                        Err::<(), _>(AosError::Network(format!("operation {} failed", i)))
                    })
                })
                .await;

            result
        });

        handles.push(handle);
    }

    // Wait for all operations
    let mut budget_exhaustion_count = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Err(AosError::ResourceExhaustion(msg)) if msg.contains("budget") => {
                budget_exhaustion_count += 1;
            }
            _ => {}
        }
    }

    let max_concurrent = max_concurrent_seen.load(Ordering::SeqCst);
    println!("Budget exhaustion errors: {}", budget_exhaustion_count);
    println!("Max concurrent operations seen: {}", max_concurrent);

    assert!(budget_exhaustion_count > 0, "Budget should have been exhausted");
    assert!(max_concurrent <= 2, "Should not exceed max_concurrent_retries limit");
    println!("✅ Test 1 passed: Budget properly limits concurrent operations");

    // Test 2: Rate limiting
    println!("\n=== Test 2: Rate Limiting ===");
    let rate_limit_config = RetryBudgetConfig {
        max_concurrent_retries: 10,
        max_retry_rate_per_second: 2.0, // Very low rate limit
        budget_window: std::time::Duration::from_secs(1),
        max_budget_tokens: 5,
    };

    let manager = RetryManager::with_budget(rate_limit_config.clone());
    let policy = RetryPolicy {
        max_attempts: 1,
        base_delay: std::time::Duration::from_millis(1),
        budget: Some(rate_limit_config),
        ..RetryPolicy::fast("test")
    };

    let start_time = std::time::Instant::now();
    let mut rate_limit_errors = 0;

    // Try many operations quickly
    for i in 0..10 {
        let manager = manager.clone();
        let policy = policy.clone();

        let result = manager
            .execute_with_policy(&policy, || {
                Box::pin(async move {
                    Err::<(), _>(AosError::Network(format!("operation {} failed", i)))
                })
            })
            .await;

        if let Err(AosError::ResourceExhaustion(msg)) = result {
            if msg.contains("rate limit") {
                rate_limit_errors += 1;
            }
        }
    }

    let elapsed = start_time.elapsed();
    println!("Rate limit errors: {}", rate_limit_errors);
    println!("Total time: {:?}", elapsed);

    assert!(rate_limit_errors > 0, "Rate limiting should have occurred");
    assert!(elapsed >= std::time::Duration::from_secs(1), "Operations should have taken at least 1 second");
    println!("✅ Test 2 passed: Rate limiting works correctly");

    // Test 3: No budget (should work normally)
    println!("\n=== Test 3: No Budget (Control Test) ===");
    let manager = RetryManager::new();
    let policy = RetryPolicy {
        budget: None, // No budget
        ..RetryPolicy::fast("test")
    };

    let result = manager
        .execute_with_policy(&policy, || {
            Box::pin(async move {
                Ok::<_, AosError>("success")
            })
        })
        .await;

    assert!(result.is_ok(), "Should succeed without budget");
    println!("✅ Test 3 passed: Operations work normally without budget");

    println!("\n🎉 All retry budget tests passed!");
    println!("✅ Budget properly limits concurrent operations");
    println!("✅ Rate limiting prevents excessive retries");
    println!("✅ Normal operation works without budget");
    println!("\nThe retry budget implementation is now complete and working correctly.");

    Ok(())
}
