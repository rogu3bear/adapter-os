# Retry Policy Standardization

## Overview

AdapterOS now provides a standardized retry policy system that ensures consistent retry behavior across all services. This system includes exponential backoff, jitter, circuit breaker integration, and retry budget management to prevent resource exhaustion.

## Key Components

### RetryPolicy

The core configuration structure that defines retry behavior:

```rust
use adapteros_core::{RetryPolicy, ServiceType};

let policy = RetryPolicy::fast("my-service");
// or
let policy = RetryPolicy {
    max_attempts: 3,
    base_delay: Duration::from_millis(100),
    max_delay: Duration::from_secs(30),
    backoff_factor: 2.0,
    jitter: true,
    circuit_breaker: Some(CircuitBreakerConfig::default()),
    budget: Some(RetryBudgetConfig::default()),
    service_type: "my-service".to_string(),
};
```

### RetryManager

The execution engine that applies retry policies:

```rust
use adapteros_core::RetryManager;

let manager = RetryManager::new();

// Execute with retry policy
let result = manager.execute_with_policy(&policy, || {
    Box::pin(async {
        // Your operation here
        my_fallible_operation().await
    })
}).await;
```

### Service-Specific Strategies

Pre-configured retry strategies optimized for different service types:

```rust
use adapteros_core::{get_strategy, ServiceType};

// Get pre-configured strategy
let strategy = get_strategy(ServiceType::Database).unwrap();

// Use the strategy's policy
let policy = &strategy.policy;
```

## Service Types

The system provides optimized retry strategies for common service types:

- **FastApi**: Quick API operations (50ms base delay, 3 attempts)
- **Database**: Database operations (200ms base delay, 3 attempts)
- **Network**: External network calls (100ms base delay, 5 attempts)
- **FileSystem**: File system operations (200ms base delay, 3 attempts)
- **ModelInference**: ML model inference (500ms base delay, 2 attempts)
- **BackgroundTask**: Long-running background tasks (1s base delay, 5 attempts)
- **CriticalSystem**: Critical system operations (100ms base delay, 10 attempts)
- **BatchProcessing**: Batch processing operations (5s base delay, 3 attempts)

## Features

### Exponential Backoff

Automatic delay calculation with exponential growth:

```
Attempt 1: base_delay
Attempt 2: base_delay * backoff_factor
Attempt 3: base_delay * backoff_factor^2
...
```

### Jitter

Adds randomness to prevent thundering herd problems:

```rust
let jitter = if policy.jitter {
    (delay * 0.1) * random_factor // 10% jitter
} else {
    Duration::ZERO
};
let actual_delay = delay + jitter;
```

### Circuit Breaker Integration

Prevents cascading failures by temporarily stopping requests to failing services:

```rust
let policy = RetryPolicy {
    circuit_breaker: Some(CircuitBreakerConfig {
        failure_threshold: 5,      // Open after 5 failures
        success_threshold: 3,      // Close after 3 successes
        timeout: Duration::from_secs(60), // Try again after 60s
        half_open_max_requests: 3, // Limited requests in half-open
    }),
    // ...
};
```

### Retry Budget Management

Prevents resource exhaustion by limiting concurrent retries:

```rust
let policy = RetryPolicy {
    budget: Some(RetryBudgetConfig {
        max_concurrent_retries: 50,
        max_retry_rate_per_second: 100.0,
        budget_window: Duration::from_secs(60),
        max_budget_tokens: 500,
    }),
    // ...
};
```

### Metrics and Telemetry

Comprehensive metrics collection for monitoring and debugging:

```rust
use adapteros_core::{create_retry_manager_with_metrics, SimpleRetryMetrics};

let metrics = SimpleRetryMetrics::new();
let manager = create_retry_manager_with_metrics(Some(metrics.clone()));

// Later, check metrics
let snapshot = metrics.snapshot();
println!("Total retries: {}", snapshot.attempts);
println!("Successful retries: {}", snapshot.successes);
println!("Failed retries: {}", snapshot.failures);
```

## Usage Examples

### Basic Usage

```rust
use adapteros_core::{RetryPolicy, RetryManager, ServiceType};

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    // Create a retry policy for network operations
    let policy = RetryPolicy::network("api-client");

    // Create a retry manager
    let manager = RetryManager::new();

    // Execute operation with retries
    let result = manager.execute_with_policy(&policy, || {
        Box::pin(async {
            // Your operation that might fail
            call_external_api().await
        })
    }).await?;

    Ok(())
}
```

### Advanced Configuration

```rust
use adapteros_core::{RetryPolicy, RetryManager, CircuitBreakerConfig, RetryBudgetConfig};
use std::time::Duration;

async fn advanced_example() -> Result<(), Box<dyn std::error::Error>> {
    // Custom policy with all features enabled
    let policy = RetryPolicy {
        max_attempts: 5,
        base_delay: Duration::from_millis(200),
        max_delay: Duration::from_secs(10),
        backoff_factor: 1.5,
        jitter: true,
        circuit_breaker: Some(CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(30),
            half_open_max_requests: 2,
        }),
        budget: Some(RetryBudgetConfig {
            max_concurrent_retries: 10,
            max_retry_rate_per_second: 20.0,
            budget_window: Duration::from_secs(60),
            max_budget_tokens: 100,
        }),
        service_type: "my-critical-service".to_string(),
    };

    let manager = RetryManager::with_circuit_breaker(policy.circuit_breaker.unwrap());

    let result = manager.execute_with_policy(&policy, || {
        Box::pin(async {
            critical_business_operation().await
        })
    }).await?;

    Ok(())
}
```

### Using Pre-configured Strategies

```rust
use adapteros_core::{get_strategy, ServiceType, RetryManager};

async fn strategy_example() -> Result<(), Box<dyn std::error::Error>> {
    // Get optimized strategy for database operations
    let strategy = get_strategy(ServiceType::Database)
        .ok_or("Strategy not found")?;

    let manager = RetryManager::new();

    // Use strategy with both circuit breaker and budget
    let result = manager.execute_with_policy(&strategy.effective_policy(true, true), || {
        Box::pin(async {
            database_query().await
        })
    }).await?;

    Ok(())
}
```

## Migration Guide

### From adapteros-error-recovery

Old code:
```rust
use adapteros_error_recovery::retry::RetryManager;

let config = ErrorRecoveryConfig::default();
let manager = RetryManager::new(&config)?;

manager.retry_with("operation", || async {
    fallible_operation().await
}).await?;
```

New code:
```rust
use adapteros_core::{RetryPolicy, RetryManager};

let policy = RetryPolicy::fast("operation");
let manager = RetryManager::new();

manager.execute_with_policy(&policy, || {
    Box::pin(async {
        fallible_operation().await
    })
}).await?;
```

### From adapteros-server-api

Old code:
```rust
use adapteros_server_api::retry::{RetryConfig, retry_async_with_metrics};

let config = RetryConfig::default();
let result = retry_async_with_metrics(&config, || async {
    operation().await
}, Some(&metrics), "service").await;
```

New code:
```rust
use adapteros_core::{RetryPolicy, RetryManager, create_retry_manager_with_metrics};

let policy = RetryPolicy::fast("service");
let manager = create_retry_manager_with_metrics(Some(metrics));

let result = manager.execute_with_policy(&policy, || {
    Box::pin(async {
        operation().await
    })
}).await;
```

## Best Practices

### 1. Choose Appropriate Service Types

Use the predefined service types when possible:

```rust
// Good: Use predefined strategy
let policy = get_strategy(ServiceType::Network).unwrap().policy;

// Avoid: Manual configuration without good reason
let policy = RetryPolicy {
    max_attempts: 5,
    // ... many manual settings
};
```

### 2. Configure Circuit Breakers for External Services

Always enable circuit breakers for external dependencies:

```rust
let policy = RetryPolicy {
    circuit_breaker: Some(CircuitBreakerConfig::default()),
    // ... other config
};
```

### 3. Set Appropriate Budgets

Configure retry budgets based on your service capacity:

```rust
let policy = RetryPolicy {
    budget: Some(RetryBudgetConfig {
        max_concurrent_retries: 10, // Based on your service limits
        max_retry_rate_per_second: 50.0,
        // ... other settings
    }),
    // ...
};
```

### 4. Use Metrics for Monitoring

Enable metrics to monitor retry behavior:

```rust
let metrics = SimpleRetryMetrics::new();
let manager = create_retry_manager_with_metrics(Some(metrics));

// Monitor metrics in your application
// metrics.snapshot().attempts, etc.
```

### 5. Handle Circuit Breaker Errors

Circuit breaker errors should be handled differently from operation errors:

```rust
match result {
    Ok(value) => println!("Success: {:?}", value),
    Err(AosError::CircuitBreakerOpen(_)) => {
        // Service is unavailable, don't retry
        println!("Service unavailable");
    }
    Err(other) => {
        // Other errors might still be retryable
        println!("Operation failed: {:?}", other);
    }
}
```

## Configuration Reference

### RetryPolicy Fields

- `max_attempts`: Maximum retry attempts (excluding initial attempt)
- `base_delay`: Initial delay between attempts
- `max_delay`: Maximum delay between attempts
- `backoff_factor`: Exponential backoff multiplier
- `jitter`: Whether to add random jitter to delays
- `circuit_breaker`: Optional circuit breaker configuration
- `budget`: Optional retry budget configuration
- `service_type`: Identifier for metrics categorization

### CircuitBreakerConfig Fields

- `failure_threshold`: Failures before opening circuit
- `success_threshold`: Successes before closing circuit
- `timeout`: Time before attempting to close circuit
- `half_open_max_requests`: Max requests in half-open state

### RetryBudgetConfig Fields

- `max_concurrent_retries`: Maximum concurrent retry operations
- `max_retry_rate_per_second`: Maximum retry rate per second
- `budget_window`: Time window for rate limiting
- `max_budget_tokens`: Maximum budget tokens

## Troubleshooting

### High Retry Rates

If you're seeing high retry rates:

1. Check if the underlying service is healthy
2. Review circuit breaker metrics to see if circuits are opening
3. Adjust retry policies to be less aggressive
4. Check for thundering herd problems (add more jitter)

### Circuit Breaker Not Opening

If circuit breakers aren't opening when expected:

1. Verify `failure_threshold` is appropriate for your use case
2. Check that operations are actually failing (not timing out)
3. Ensure circuit breaker metrics are being collected

### Resource Exhaustion

If retry budgets are being exceeded:

1. Increase budget limits if appropriate
2. Reduce retry aggressiveness
3. Check for cascading failure patterns
4. Implement proper backpressure mechanisms

### Performance Issues

For performance problems:

1. Use appropriate service type strategies
2. Enable circuit breakers to fail fast
3. Monitor retry metrics to identify hotspots
4. Consider reducing retry attempts for expensive operations

## Implementation Details

### Thread Safety

All retry components are thread-safe and can be shared across multiple async tasks:

```rust
let manager = Arc::new(RetryManager::new());

// Safe to use from multiple tasks
let manager_clone = manager.clone();
tokio::spawn(async move {
    manager_clone.execute_with_policy(&policy, operation).await
});
```

### Memory Usage

The retry system is designed to be memory-efficient:

- Circuit breaker state is minimal (counters and timestamps)
- Retry budgets use atomic counters
- Metrics collection is optional and bounded
- No unbounded queues or buffers

### Performance Characteristics

- Low overhead for successful operations (no retries)
- Exponential backoff prevents resource spikes
- Circuit breakers provide fast failure detection
- Budget management prevents cascade failures

## Related Documentation

- [Circuit Breaker Pattern](https://martinfowler.com/bliki/CircuitBreaker.html)
- [Exponential Backoff](https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/)
- [AdapterOS Error Recovery](../adapteros-error-recovery/)
- [AdapterOS Server API](../adapteros-server-api/)
