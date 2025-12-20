//! Request tracking with cancellation support for inference operations
//!
//! This module provides RequestTracker which maintains active inference requests
//! and allows cancellation via atomic boolean flags.

use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Request tracker for managing active inference requests and supporting cancellation
///
/// Each request is identified by a request_id string and associated with a cancellation
/// token (Arc<AtomicBool>). When a request is cancelled, the flag is set to true,
/// and inference loops can check this flag periodically to abort early.
///
/// # Example
/// ```
/// use adapteros_server_api::request_tracker::RequestTracker;
///
/// let tracker = RequestTracker::new();
/// let request_id = "req-123".to_string();
///
/// // Register request
/// let cancel_token = tracker.register(request_id.clone());
///
/// // Check if cancelled during inference
/// if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
///     // Abort inference
/// }
///
/// // Complete request when done
/// tracker.complete(&request_id);
/// ```
pub struct RequestTracker {
    /// Active requests mapped to their cancellation tokens
    active: DashMap<String, Arc<AtomicBool>>,
}

impl RequestTracker {
    /// Create a new request tracker
    pub fn new() -> Self {
        Self {
            active: DashMap::new(),
        }
    }

    /// Register a new active request
    ///
    /// Returns a cancellation token that can be checked during inference.
    /// If a request with this ID already exists, returns the existing token.
    pub fn register(&self, request_id: String) -> Arc<AtomicBool> {
        self.active
            .entry(request_id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    /// Cancel an active request
    ///
    /// Sets the cancellation flag to true. Returns true if the request was found
    /// and cancelled, false if the request was not active.
    pub fn cancel(&self, request_id: &str) -> bool {
        if let Some(token) = self.active.get(request_id) {
            token.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Complete a request (remove from active set)
    ///
    /// Should be called when inference finishes (either successfully or with error).
    /// Returns true if the request was active and removed, false otherwise.
    pub fn complete(&self, request_id: &str) -> bool {
        self.active.remove(request_id).is_some()
    }

    /// Get the cancellation token for an active request
    ///
    /// Returns None if the request is not active.
    pub fn get_token(&self, request_id: &str) -> Option<Arc<AtomicBool>> {
        self.active.get(request_id).map(|r| r.clone())
    }

    /// Get count of active requests
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Check if a request is cancelled
    pub fn is_cancelled(&self, request_id: &str) -> bool {
        self.active
            .get(request_id)
            .map(|token| token.load(Ordering::Acquire))
            .unwrap_or(false)
    }
}

impl Default for RequestTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_complete() {
        let tracker = RequestTracker::new();
        let request_id = "req-123".to_string();

        let token = tracker.register(request_id.clone());
        assert_eq!(tracker.active_count(), 1);
        assert!(!token.load(Ordering::Relaxed));

        assert!(tracker.complete(&request_id));
        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_cancel() {
        let tracker = RequestTracker::new();
        let request_id = "req-456".to_string();

        let token = tracker.register(request_id.clone());
        assert!(!token.load(Ordering::Relaxed));

        assert!(tracker.cancel(&request_id));
        assert!(token.load(Ordering::Relaxed));
        assert!(tracker.is_cancelled(&request_id));
    }

    #[test]
    fn test_cancel_nonexistent() {
        let tracker = RequestTracker::new();
        assert!(!tracker.cancel("nonexistent"));
    }

    #[test]
    fn test_get_token() {
        let tracker = RequestTracker::new();
        let request_id = "req-789".to_string();

        assert!(tracker.get_token(&request_id).is_none());

        tracker.register(request_id.clone());
        assert!(tracker.get_token(&request_id).is_some());
    }

    #[test]
    fn test_duplicate_register() {
        let tracker = RequestTracker::new();
        let request_id = "req-duplicate".to_string();

        let token1 = tracker.register(request_id.clone());
        let token2 = tracker.register(request_id.clone());

        assert_eq!(tracker.active_count(), 1);

        // Both tokens should point to the same atomic bool
        token1.store(true, Ordering::Release);
        assert!(token2.load(Ordering::Acquire));
    }
}
