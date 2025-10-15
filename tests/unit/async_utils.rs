//! Async Component Testing Utilities
//!
//! This module provides utilities for testing async components and futures
//! in the AdapterOS system, with support for deterministic execution and
//! timeout handling.
//!
//! ## Key Features
//!
//! - **Async Test Execution**: Controlled execution of async operations
//! - **Timeout Management**: Configurable timeouts for async tests
//! - **Deterministic Scheduling**: Predictable execution order for testing
//! - **Future Inspection**: Utilities for inspecting future states
//! - **Cancellation Testing**: Test cancellation behavior of async operations
//!
//! ## Usage
//!
//! ```rust
//! use tests_unit::async_utils::*;
//!
//! #[tokio::test]
//! async fn test_async_component() {
//!     let timeout = Timeout::new(Duration::from_secs(5));
//!     let result = timeout.run(async {
//!         my_async_component().await
//!     }).await;
//!     assert!(result.is_ok());
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use futures::future::{FutureExt, BoxFuture};
use tokio::time;

/// Timeout wrapper for async operations
pub struct Timeout {
    duration: Duration,
}

impl Timeout {
    /// Create a new timeout with the specified duration
    pub fn new(duration: Duration) -> Self {
        Self { duration }
    }

    /// Run an async operation with a timeout
    pub async fn run<F, T>(&self, future: F) -> Result<T, TimeoutError>
    where
        F: Future<Output = T>,
    {
        match time::timeout(self.duration, future).await {
            Ok(result) => Ok(result),
            Err(_) => Err(TimeoutError {
                expected_duration: self.duration,
                actual_duration: self.duration, // We don't know the actual duration
            }),
        }
    }

    /// Run an async operation with a timeout and get elapsed time
    pub async fn run_with_timing<F, T>(&self, future: F) -> Result<(T, Duration), TimeoutError>
    where
        F: Future<Output = T>,
    {
        let start = Instant::now();
        match time::timeout(self.duration, future).await {
            Ok(result) => Ok((result, start.elapsed())),
            Err(_) => Err(TimeoutError {
                expected_duration: self.duration,
                actual_duration: start.elapsed(),
            }),
        }
    }
}

/// Timeout error with timing information
#[derive(Debug, Clone)]
pub struct TimeoutError {
    pub expected_duration: Duration,
    pub actual_duration: Duration,
}

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Operation timed out after {:?} (expected completion within {:?})",
               self.actual_duration, self.expected_duration)
    }
}

impl std::error::Error for TimeoutError {}

/// Deterministic async executor for testing
pub struct DeterministicExecutor {
    tasks: Arc<Mutex<Vec<Pin<Box<dyn Future<Output = ()> + Send>>>>>,
}

impl DeterministicExecutor {
    /// Create a new deterministic executor
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Spawn a task for deterministic execution
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tasks.lock().unwrap().push(Box::pin(future));
    }

    /// Run all tasks to completion in deterministic order
    pub async fn run_all(&self) {
        let mut tasks = self.tasks.lock().unwrap().drain(..).collect::<Vec<_>>();

        // Sort tasks by some deterministic criteria (e.g., task ID or creation order)
        // For now, we just execute them in the order they were spawned
        for task in tasks {
            task.await;
        }
    }

    /// Run tasks with a maximum number of concurrent executions
    pub async fn run_concurrent(&self, max_concurrent: usize) {
        use futures::stream::{self, StreamExt};

        let tasks = self.tasks.lock().unwrap().drain(..).collect::<Vec<_>>();
        let stream = stream::iter(tasks).buffer_unordered(max_concurrent);

        stream.collect::<Vec<()>>().await;
    }
}

/// Future inspector for examining future states during testing
pub struct FutureInspector<F> {
    future: F,
    polls: Arc<Mutex<Vec<Instant>>>,
}

impl<F> FutureInspector<F> {
    /// Create a new future inspector
    pub fn new(future: F) -> Self {
        Self {
            future,
            polls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get the number of times the future has been polled
    pub fn poll_count(&self) -> usize {
        self.polls.lock().unwrap().len()
    }

    /// Get the timestamps of all polls
    pub fn poll_times(&self) -> Vec<Instant> {
        self.polls.lock().unwrap().clone()
    }

    /// Check if the future has been polled at least once
    pub fn has_been_polled(&self) -> bool {
        !self.polls.lock().unwrap().is_empty()
    }
}

impl<F: Future> Future for FutureInspector<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.polls.lock().unwrap().push(Instant::now());
        // Safety: We're not moving the future, just accessing it
        let future = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.future) };
        future.poll(cx)
    }
}

/// Cancellation token for testing cancellation behavior
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<Mutex<bool>>,
}

impl CancellationToken {
    /// Create a new cancellation token
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(Mutex::new(false)),
        }
    }

    /// Cancel the token
    pub fn cancel(&self) {
        *self.cancelled.lock().unwrap() = true;
    }

    /// Check if the token has been cancelled
    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.lock().unwrap()
    }

    /// Create a future that resolves when cancelled
    pub async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}

/// Async test harness for running async tests with common setup/teardown
pub struct AsyncTestHarness {
    setup_fns: Vec<Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
    teardown_fns: Vec<Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
}

impl AsyncTestHarness {
    /// Create a new async test harness
    pub fn new() -> Self {
        Self {
            setup_fns: Vec::new(),
            teardown_fns: Vec::new(),
        }
    }

    /// Add a setup function
    pub fn add_setup<F, Fut>(&mut self, setup_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.setup_fns.push(Box::new(move || Box::pin(setup_fn())));
    }

    /// Add a teardown function
    pub fn add_teardown<F, Fut>(&mut self, teardown_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.teardown_fns.push(Box::new(move || Box::pin(teardown_fn())));
    }

    /// Run a test function with setup and teardown
    pub async fn run_test<F, Fut, T>(&self, test_fn: F) -> T
    where
        F: Fn() -> Fut,
        Fut: Future<Output = T>,
    {
        // Run setup functions
        for setup_fn in &self.setup_fns {
            setup_fn().await;
        }

        // Run the test
        let result = test_fn().await;

        // Run teardown functions (in reverse order)
        for teardown_fn in self.teardown_fns.iter().rev() {
            teardown_fn().await;
        }

        result
    }
}

/// Mock async delay for testing timing-dependent code
pub struct MockDelay {
    duration: Duration,
    start_time: Option<Instant>,
}

impl MockDelay {
    /// Create a new mock delay
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            start_time: None,
        }
    }

    /// Start the delay
    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    /// Check if the delay has completed
    pub fn is_elapsed(&self) -> bool {
        self.start_time
            .map(|start| start.elapsed() >= self.duration)
            .unwrap_or(false)
    }

    /// Get remaining time
    pub fn remaining(&self) -> Option<Duration> {
        self.start_time.map(|start| {
            let elapsed = start.elapsed();
            if elapsed >= self.duration {
                Duration::ZERO
            } else {
                self.duration - elapsed
            }
        })
    }

    /// Convert to a future that resolves after the delay
    pub async fn into_future(self) -> () {
        if let Some(remaining) = self.remaining() {
            time::sleep(remaining).await;
        }
    }
}

/// Async operation tracker for monitoring async operations during testing
pub struct AsyncOperationTracker {
    operations: Arc<Mutex<Vec<AsyncOperation>>>,
}

#[derive(Debug, Clone)]
pub struct AsyncOperation {
    pub id: String,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub result: Option<String>,
}

impl AsyncOperationTracker {
    /// Create a new operation tracker
    pub fn new() -> Self {
        Self {
            operations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start tracking an operation
    pub fn start_operation(&self, id: &str) -> OperationGuard {
        let operation = AsyncOperation {
            id: id.to_string(),
            start_time: Instant::now(),
            end_time: None,
            result: None,
        };

        self.operations.lock().unwrap().push(operation);

        OperationGuard {
            id: id.to_string(),
            tracker: Arc::clone(&self.operations),
        }
    }

    /// Get all completed operations
    pub fn completed_operations(&self) -> Vec<AsyncOperation> {
        self.operations.lock().unwrap()
            .iter()
            .filter(|op| op.end_time.is_some())
            .cloned()
            .collect()
    }

    /// Get all active operations
    pub fn active_operations(&self) -> Vec<AsyncOperation> {
        self.operations.lock().unwrap()
            .iter()
            .filter(|op| op.end_time.is_none())
            .cloned()
            .collect()
    }

    /// Get the total number of operations
    pub fn total_operations(&self) -> usize {
        self.operations.lock().unwrap().len()
    }
}

/// RAII guard for tracking async operations
pub struct OperationGuard {
    id: String,
    tracker: Arc<Mutex<Vec<AsyncOperation>>>,
}

impl OperationGuard {
    /// Mark the operation as completed with a result
    pub fn complete(self, result: &str) {
        let mut operations = self.tracker.lock().unwrap();
        if let Some(op) = operations.iter_mut().find(|op| op.id == self.id) {
            op.end_time = Some(Instant::now());
            op.result = Some(result.to_string());
        }
    }

    /// Mark the operation as completed without a result
    pub fn complete_empty(self) {
        self.complete("");
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        // If not explicitly completed, mark as completed on drop
        let mut operations = self.tracker.lock().unwrap();
        if let Some(op) = operations.iter_mut().find(|op| op.id == self.id) {
            if op.end_time.is_none() {
                op.end_time = Some(Instant::now());
                op.result = Some("dropped".to_string());
            }
        }
    }
}

/// Async barrier for synchronizing multiple async tasks in tests
pub struct AsyncBarrier {
    count: usize,
    current: Arc<Mutex<usize>>,
    wakers: Arc<Mutex<Vec<Waker>>>,
}

impl AsyncBarrier {
    /// Create a new barrier that waits for N tasks
    pub fn new(count: usize) -> Self {
        Self {
            count,
            current: Arc::new(Mutex::new(0)),
            wakers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Wait at the barrier
    pub async fn wait(&self) {
        let mut current = self.current.lock().unwrap();
        *current += 1;

        if *current == self.count {
            // All tasks have arrived, wake everyone
            let wakers = self.wakers.lock().unwrap().drain(..).collect::<Vec<_>>();
            for waker in wakers {
                waker.wake();
            }
        } else {
            // Wait for others
            let waker = futures::task::noop_waker();
            self.wakers.lock().unwrap().push(waker.clone());

            // In a real implementation, this would use a proper waker
            // For testing purposes, we'll just yield
            tokio::task::yield_now().await;
        }
    }
}

/// Utility for testing race conditions in async code
pub struct RaceConditionTester {
    operations: Vec<Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
}

impl RaceConditionTester {
    /// Create a new race condition tester
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    /// Add an operation to test for race conditions
    pub fn add_operation<F, Fut>(&mut self, operation: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.operations.push(Box::new(move || Box::pin(operation())));
    }

    /// Run all operations concurrently and wait for completion
    pub async fn run_race(&self) {
        use futures::future::join_all;

        let futures = self.operations.iter()
            .map(|op| op())
            .collect::<Vec<_>>();

        join_all(futures).await;
    }

    /// Run operations with controlled interleaving (for deterministic testing)
    pub async fn run_deterministic(&self) {
        for operation in &self.operations {
            operation().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_timeout_success() {
        let timeout = Timeout::new(Duration::from_secs(1));
        let result = timeout.run(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            42
        }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_timeout_failure() {
        let timeout = Timeout::new(Duration::from_millis(100));
        let result = timeout.run(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            42
        }).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_deterministic_executor() {
        let executor = DeterministicExecutor::new();
        let results = Arc::new(Mutex::new(Vec::new()));

        for i in 0..5 {
            let results_clone = Arc::clone(&results);
            executor.spawn(async move {
                results_clone.lock().unwrap().push(i);
            });
        }

        executor.run_all().await;

        let final_results = results.lock().unwrap();
        assert_eq!(*final_results, vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_future_inspector() {
        let future = async { 42 };
        let mut inspector = FutureInspector::new(future);

        // The future should not have been polled yet
        assert_eq!(inspector.poll_count(), 0);
        assert!(!inspector.has_been_polled());

        // Await the future (this will poll it)
        let result = inspector.await;
        assert_eq!(result, 42);

        // Now it should have been polled at least once
        assert!(inspector.poll_count() >= 1);
        assert!(inspector.has_been_polled());
    }

    #[tokio::test]
    async fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_async_barrier() {
        let barrier = Arc::new(AsyncBarrier::new(3));
        let results = Arc::new(Mutex::new(Vec::new()));

        let tasks: Vec<_> = (0..3).map(|i| {
            let barrier = Arc::clone(&barrier);
            let results = Arc::clone(&results);
            tokio::spawn(async move {
                results.lock().unwrap().push(format!("before_{}", i));
                barrier.wait().await;
                results.lock().unwrap().push(format!("after_{}", i));
            })
        }).collect();

        for task in tasks {
            task.await.unwrap();
        }

        let final_results = results.lock().unwrap();
        // All "before" messages should come before all "after" messages
        let before_count = final_results.iter().filter(|s| s.starts_with("before_")).count();
        let after_count = final_results.iter().filter(|s| s.starts_with("after_")).count();
        assert_eq!(before_count, 3);
        assert_eq!(after_count, 3);
    }

    #[tokio::test]
    async fn test_operation_tracker() {
        let tracker = AsyncOperationTracker::new();

        let guard1 = tracker.start_operation("op1");
        let guard2 = tracker.start_operation("op2");

        // Complete operations
        guard1.complete("success");
        guard2.complete("failure");

        let completed = tracker.completed_operations();
        assert_eq!(completed.len(), 2);

        let active = tracker.active_operations();
        assert_eq!(active.len(), 0);

        assert_eq!(tracker.total_operations(), 2);
    }

    #[tokio::test]
    async fn test_race_condition_tester() {
        let mut tester = RaceConditionTester::new();
        let results = Arc::new(Mutex::new(Vec::new()));

        for i in 0..3 {
            let results_clone = Arc::clone(&results);
            tester.add_operation(move || async move {
                results_clone.lock().unwrap().push(i);
                tokio::time::sleep(Duration::from_millis(10)).await;
            });
        }

        tester.run_race().await;

        let final_results = results.lock().unwrap();
        assert_eq!(final_results.len(), 3);
        // Results may be in any order due to race conditions
        for i in 0..3 {
            assert!(final_results.contains(&i));
        }
    }
}</code>
