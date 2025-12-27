//! Async telemetry writer with bounded channel for router decisions
//!
//! This module provides a non-blocking telemetry writer specifically for router decision events.
//! It uses a bounded channel to prevent memory exhaustion and drops events on overflow,
//! tracking drop counters for observability.

use crate::events::RouterDecisionEvent;
use adapteros_core::{AosError, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Capacity of the bounded channel for router decision events
const DEFAULT_CHANNEL_CAPACITY: usize = 1000;

/// Async writer for router decision telemetry events
///
/// This writer uses a bounded channel to prevent blocking the router hot path.
/// Events are dropped on overflow, with counters tracked for monitoring.
#[derive(Clone)]
pub struct RouterDecisionWriter {
    sender: mpsc::Sender<RouterDecisionEvent>,
    drop_counter: Arc<AtomicU64>,
    total_counter: Arc<AtomicU64>,
}

impl RouterDecisionWriter {
    /// Create a new router decision writer
    ///
    /// Returns the writer and a receiver for consuming events asynchronously.
    pub fn new() -> (Self, mpsc::Receiver<RouterDecisionEvent>) {
        Self::with_capacity(DEFAULT_CHANNEL_CAPACITY)
    }

    /// Create a new router decision writer with custom capacity
    pub fn with_capacity(capacity: usize) -> (Self, mpsc::Receiver<RouterDecisionEvent>) {
        let (sender, receiver) = mpsc::channel(capacity);
        let drop_counter = Arc::new(AtomicU64::new(0));
        let total_counter = Arc::new(AtomicU64::new(0));

        let writer = Self {
            sender,
            drop_counter,
            total_counter,
        };

        (writer, receiver)
    }

    /// Emit a router decision event (non-blocking)
    ///
    /// Returns immediately, dropping the event if the channel is full.
    /// Increments drop counter on overflow.
    pub fn emit(&self, event: RouterDecisionEvent) -> Result<()> {
        self.total_counter.fetch_add(1, Ordering::Relaxed);

        match self.sender.try_send(event) {
            Ok(_) => {
                debug!("Router decision event emitted");
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                let dropped = self.drop_counter.fetch_add(1, Ordering::Relaxed) + 1;
                warn!(
                    dropped_events = dropped,
                    "Router decision event dropped: channel full"
                );
                Err(AosError::Io(
                    "Router decision channel full, event dropped".to_string(),
                ))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(AosError::Io("Router decision channel closed".to_string()))
            }
        }
    }

    /// Get the number of dropped events
    pub fn dropped_count(&self) -> u64 {
        self.drop_counter.load(Ordering::Relaxed)
    }

    /// Get the total number of events attempted
    pub fn total_count(&self) -> u64 {
        self.total_counter.load(Ordering::Relaxed)
    }

    /// Get the current drop rate (0.0 to 1.0)
    pub fn drop_rate(&self) -> f64 {
        let total = self.total_count();
        if total == 0 {
            return 0.0;
        }
        self.dropped_count() as f64 / total as f64
    }
}

impl Default for RouterDecisionWriter {
    fn default() -> Self {
        Self::new().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::RouterCandidate;

    fn create_test_event(step: usize) -> RouterDecisionEvent {
        RouterDecisionEvent {
            step,
            input_token_id: Some(42),
            candidate_adapters: vec![RouterCandidate {
                adapter_idx: 0,
                raw_score: 1.5,
                gate_q15: 16384,
            }],
            entropy: 0.8,
            tau: 1.0,
            entropy_floor: 0.02,
            stack_hash: None,
            stack_id: None,
            stack_version: None,
            model_type: adapteros_types::routing::RouterModelType::Dense,
            active_experts: None,
        }
    }

    #[tokio::test]
    async fn test_writer_basic() {
        let (writer, mut receiver) = RouterDecisionWriter::new();

        // Emit an event
        let event = create_test_event(0);
        writer.emit(event.clone()).unwrap();

        // Receive the event
        let received = receiver.recv().await.unwrap();
        assert_eq!(received.step, 0);
        assert_eq!(writer.total_count(), 1);
        assert_eq!(writer.dropped_count(), 0);
    }

    #[tokio::test]
    async fn test_writer_overflow() {
        let (writer, mut receiver) = RouterDecisionWriter::with_capacity(2);

        // Fill the channel
        writer.emit(create_test_event(0)).unwrap();
        writer.emit(create_test_event(1)).unwrap();

        // This should fail (channel full)
        let result = writer.emit(create_test_event(2));
        assert!(result.is_err());
        assert_eq!(writer.total_count(), 3);
        assert_eq!(writer.dropped_count(), 1);

        // Drain the channel
        receiver.recv().await.unwrap();
        receiver.recv().await.unwrap();

        // Now we can emit again
        writer.emit(create_test_event(3)).unwrap();
        let received = receiver.recv().await.unwrap();
        assert_eq!(received.step, 3);
    }

    #[tokio::test]
    async fn test_drop_rate() {
        let (writer, _receiver) = RouterDecisionWriter::with_capacity(1);

        // Fill the channel
        writer.emit(create_test_event(0)).unwrap();

        // Try to emit more (will drop)
        let _ = writer.emit(create_test_event(1));
        let _ = writer.emit(create_test_event(2));

        assert_eq!(writer.total_count(), 3);
        assert_eq!(writer.dropped_count(), 2);
        assert!((writer.drop_rate() - 0.666).abs() < 0.01);
    }
}
