//! Unified Recovery Orchestrator
//!
//! This module provides a unified handler that coordinates retry, circuit breaker,
//! budget, and fallback mechanisms for resilient operation execution.
//!
//! # Overview
//!
//! The recovery orchestrator ties together several existing primitives:
//! - **Retry Policy**: Exponential backoff with jitter
//! - **Circuit Breaker**: Protects against cascading failures
//! - **Retry Budget**: Prevents resource exhaustion during mass failures
//! - **Fallback**: Alternative execution path when primary fails
//!
//! # Example
//!
//! ```rust,ignore
//! use adapteros_core::recovery::{RecoveryOrchestrator, RecoveryConfig};
//!
//! // Create an orchestrator for database operations
//! let orchestrator = RecoveryOrchestrator::new(RecoveryConfig::database("user-db"));
//!
//! // Execute with automatic retry and circuit breaker
//! let outcome = orchestrator.execute(|| {
//!     Box::pin(async { db.query("SELECT * FROM users").await })
//! }).await;
//!
//! match outcome.result {
//!     Ok(users) => println!("Got {} users after {} attempts", users.len(), outcome.stats.retry_attempts),
//!     Err(e) => eprintln!("Failed: {}", e),
//! }
//! ```
//!
//! # With Fallback
//!
//! ```rust,ignore
//! let outcome = orchestrator.execute_with_fallback(
//!     || Box::pin(primary_api.fetch_data()),
//!     |err| Box::pin(async move {
//!         tracing::warn!("Primary failed: {}, using cache", err);
//!         cache.get_cached_data().await
//!     }),
//! ).await;
//! ```
//!
//! # Builder Pattern
//!
//! ```rust,ignore
//! let orchestrator = RecoveryOrchestratorBuilder::new("inference-pipeline")
//!     .with_retry_policy(RetryPolicy::slow("inference"))
//!     .use_global_circuit_breaker()
//!     .deterministic_jitter(true)  // For reproducible inference
//!     .with_fallback(FallbackConfig::always())
//!     .build();
//! ```
//!
//! # Pipeline Flow
//!
//! The orchestrator executes operations through a defined pipeline:
//!
//! 1. **Check Budget**: Acquire budget guard (RAII pattern)
//! 2. **Check Circuit Breaker**: Reject if open
//! 3. **Execute with Retry Loop**:
//!    - Execute operation
//!    - On success: update circuit breaker, return
//!    - On failure: classify error, retry or exhaust
//! 4. **Invoke Fallback** (if configured and appropriate)
//! 5. **Return RecoveryOutcome** with detailed statistics

mod classifier;
mod config;
mod orchestrator;
mod outcome;

// Re-export main types
pub use classifier::{RecoveryClassifier, RecoveryClassifierExt};
pub use config::{
    FallbackConfig, LogLevel, RecoveryBudgetConfig, RecoveryCircuitBreakerConfig, RecoveryConfig,
    TelemetryConfig,
};
pub use orchestrator::{RecoveryOrchestrator, RecoveryOrchestratorBuilder};
pub use outcome::{RecoveryError, RecoveryOutcome, RecoveryStats};
