//! SingleFlight: Deduplication for concurrent load operations.
//!
//! This module provides utilities to deduplicate concurrent requests for the same
//! resource. When multiple requests arrive for the same key simultaneously, only
//! one actually executes the load; others wait for and share the result.
//!
//! # Variants
//!
//! - [`SingleFlight`]: Async variant using tokio primitives (`Notify`, `OnceCell`)
//! - [`SingleFlightSync`]: Sync variant using parking_lot primitives (`Condvar`)
//!
//! # Metrics
//!
//! Both variants support optional metrics via the [`SingleFlightMetrics`] trait.
//! Metrics recorded:
//! - `leader`: Request that triggered the load
//! - `waiter`: Request that waited for an in-progress load
//! - `error`: Load operation failed
//!
//! # Example (Async)
//!
//! ```ignore
//! use adapteros_core::singleflight::SingleFlight;
//!
//! let sf = SingleFlight::<String, MyData, MyError>::new("model_load");
//!
//! // Concurrent calls deduplicate - only one load executes
//! let data = sf.get_or_load("model-123".to_string(), || async {
//!     expensive_model_load("model-123").await
//! }).await?;
//! ```
//!
//! # Example (Sync)
//!
//! ```ignore
//! use adapteros_core::singleflight::SingleFlightSync;
//!
//! let sf = SingleFlightSync::<String, MyData, MyError>::new("kv_build");
//!
//! // Concurrent calls deduplicate - only one build executes
//! let data = sf.get_or_load("key-456".to_string(), || {
//!     expensive_kv_build("key-456")
//! })?;
//! ```
//!
//! # Invariants
//!
//! - **No deadlocks**: Uses lock-free structures (DashMap) or minimal lock scope
//! - **Bounded memory**: Entries removed immediately after completion
//! - **Error propagation**: All waiters receive cloned error on failure
//! - **No cache poisoning**: Errors don't persist; retries create new entries
//! - **Panic safety**: Leader panics propagate to waiters; entry is cleaned up
//!
//! # Error Type Requirements
//!
//! The error type `E` must implement `Clone + Send + Sync` because errors are
//! shared across all waiters. This has implications for types like `AosError`
//! which don't derive `Clone`:
//!
//! ```ignore
//! // Option 1: Use String for errors (loses type information)
//! let sf = SingleFlight::<String, MyData, String>::new("op");
//! sf.get_or_load(key, || async {
//!     do_thing().await.map_err(|e| e.to_string())
//! }).await.map_err(|e| AosError::Lifecycle(e))
//!
//! // Option 2: Wrap error in Arc (preserves type but adds indirection)
//! let sf = SingleFlight::<String, MyData, Arc<MyError>>::new("op");
//!
//! // Option 3: Use a cloneable error subset
//! #[derive(Clone)]
//! enum SingleFlightError { LoadFailed(String), Timeout, ... }
//! ```
//!
//! The codebase uses Option 1 (`String`) for simplicity, converting back to
//! `AosError` at the call site. This trades off error type fidelity for simpler
//! integration with the existing error hierarchy.
//!
//! # Panic Behavior
//!
//! If the leader panics during load:
//! 1. The cleanup guard removes the in-flight entry
//! 2. Waiters detect the panic and propagate it (panic themselves)
//! 3. The cache is not poisoned - future loads can succeed
//!
//! Callers should wrap SingleFlight calls in `catch_unwind` if panic recovery
//! is needed.

mod async_impl;
mod metrics;
mod sync_impl;

pub use async_impl::SingleFlight;
pub use metrics::{NoOpMetrics, SharedMetrics, SingleFlightMetrics, SingleFlightStats};
pub use sync_impl::SingleFlightSync;

#[cfg(test)]
mod tests;
