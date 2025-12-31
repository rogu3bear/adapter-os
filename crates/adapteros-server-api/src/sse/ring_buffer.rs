//! Thread-safe ring buffer for SSE event replay
//!
//! Provides O(1) insertion with drop-oldest semantics when full,
//! and O(n) replay from a specific event ID for client reconnection.

use super::types::SseEvent;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

/// Thread-safe ring buffer for SSE event storage and replay
///
/// # Features
///
/// - **O(1) insertion** with automatic eviction when capacity is reached
/// - **Drop-oldest semantics** to bound memory usage
/// - **O(n) replay** from a specific event ID for reconnecting clients
/// - **Thread-safe** using `RwLock` for concurrent access
/// - **Monotonic ID generation** using atomic counters
///
/// # Example
///
/// ```ignore
/// let buffer = SseRingBuffer::new(1000);
///
/// // Generate monotonic IDs and store events
/// let id = buffer.next_id();
/// let event = SseEvent::new(id, "metrics", json_data);
/// buffer.push(event).await;
///
/// // Replay events for reconnecting client
/// let missed = buffer.replay_from(last_client_id).await;
/// ```
pub struct SseRingBuffer {
    /// Event storage (VecDeque for efficient front removal)
    events: RwLock<VecDeque<SseEvent>>,

    /// Maximum capacity before oldest events are dropped
    capacity: usize,

    /// Current sequence counter for monotonic ID generation
    sequence: AtomicU64,

    /// Count of events dropped due to overflow
    dropped_count: AtomicU64,

    /// Lowest ID currently in the buffer (for gap detection)
    lowest_id: AtomicU64,
}

impl SseRingBuffer {
    /// Create a new ring buffer with the specified capacity
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events to store. When exceeded,
    ///   the oldest events are dropped.
    pub fn new(capacity: usize) -> Self {
        Self {
            events: RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
            sequence: AtomicU64::new(0),
            dropped_count: AtomicU64::new(0),
            lowest_id: AtomicU64::new(0),
        }
    }

    /// Generate the next monotonic event ID
    ///
    /// IDs are guaranteed to be unique and strictly increasing within
    /// this buffer instance. Uses `SeqCst` ordering for deterministic
    /// behavior across threads.
    pub fn next_id(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// Get the current sequence value without incrementing
    pub fn current_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Store an event in the buffer
    ///
    /// If the buffer is at capacity, the oldest event is dropped
    /// and the dropped count is incremented.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to store
    pub async fn push(&self, event: SseEvent) {
        let mut events = self.events.write().await;

        // Drop oldest if at capacity
        if events.len() >= self.capacity {
            if let Some(dropped) = events.pop_front() {
                self.dropped_count.fetch_add(1, Ordering::Relaxed);
                // Update lowest_id to reflect the new oldest event
                if let Some(oldest) = events.front() {
                    self.lowest_id.store(oldest.id, Ordering::Relaxed);
                }
                tracing::trace!(
                    dropped_id = dropped.id,
                    "SSE ring buffer dropped oldest event"
                );
            }
        }

        // Update lowest_id if buffer was empty
        if events.is_empty() {
            self.lowest_id.store(event.id, Ordering::Relaxed);
        }

        events.push_back(event);
    }

    /// Replay events starting after the given ID
    ///
    /// Returns all events where `event.id > last_event_id`, in order.
    /// Used when a client reconnects with a `Last-Event-ID` header.
    ///
    /// # Arguments
    ///
    /// * `last_event_id` - The last event ID the client received.
    ///   Events with IDs greater than this will be returned.
    ///
    /// # Returns
    ///
    /// A vector of events to replay, in chronological order.
    /// May be empty if no events exist after the given ID.
    pub async fn replay_from(&self, last_event_id: u64) -> Vec<SseEvent> {
        let events = self.events.read().await;
        events
            .iter()
            .filter(|e| e.id > last_event_id)
            .cloned()
            .collect()
    }

    /// Get all events currently in the buffer
    ///
    /// Useful for debugging or initial client connection.
    pub async fn get_all(&self) -> Vec<SseEvent> {
        let events = self.events.read().await;
        events.iter().cloned().collect()
    }

    /// Get the number of events currently in the buffer
    pub async fn len(&self) -> usize {
        let events = self.events.read().await;
        events.len()
    }

    /// Check if the buffer is empty
    pub async fn is_empty(&self) -> bool {
        let events = self.events.read().await;
        events.is_empty()
    }

    /// Get buffer statistics
    pub fn stats(&self) -> BufferStats {
        BufferStats {
            capacity: self.capacity,
            current_sequence: self.sequence.load(Ordering::Relaxed),
            dropped_count: self.dropped_count.load(Ordering::Relaxed),
            lowest_id: self.lowest_id.load(Ordering::Relaxed),
        }
    }

    /// Check if there's a gap between the client's last ID and our buffer
    ///
    /// Returns `true` if the client has missed events that are no longer
    /// available in the buffer (they were dropped due to overflow).
    pub fn has_gap(&self, last_event_id: u64) -> bool {
        let lowest = self.lowest_id.load(Ordering::Relaxed);
        // Gap exists if the client's last ID is older than our oldest stored event
        // and events have been generated since then
        last_event_id < lowest && lowest > 0
    }

    /// Clear all events from the buffer
    ///
    /// Resets dropped count but preserves the sequence counter
    /// to maintain monotonic ID guarantees.
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
        self.dropped_count.store(0, Ordering::Relaxed);
        self.lowest_id.store(0, Ordering::Relaxed);
    }
}

/// Statistics about the ring buffer state
#[derive(Debug, Clone, Copy)]
pub struct BufferStats {
    /// Maximum capacity of the buffer
    pub capacity: usize,

    /// Current sequence number (next ID to be assigned)
    pub current_sequence: u64,

    /// Total number of events dropped due to overflow
    pub dropped_count: u64,

    /// Lowest event ID currently in the buffer
    pub lowest_id: u64,
}

impl BufferStats {
    /// Calculate the number of events currently stored
    ///
    /// This is an approximation based on sequence and dropped counts.
    pub fn estimated_size(&self) -> u64 {
        self.current_sequence.saturating_sub(self.dropped_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monotonic_id_generation() {
        let buffer = SseRingBuffer::new(100);

        let id1 = buffer.next_id();
        let id2 = buffer.next_id();
        let id3 = buffer.next_id();

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);
        assert!(id1 < id2);
        assert!(id2 < id3);
    }

    #[tokio::test]
    async fn test_push_and_replay() {
        let buffer = SseRingBuffer::new(100);

        // Push some events
        for i in 0..5 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", format!(r#"{{"seq": {}}}"#, i));
            buffer.push(event).await;
        }

        assert_eq!(buffer.len().await, 5);

        // Replay from ID 2 should return events 3, 4
        let replay = buffer.replay_from(2).await;
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].id, 3);
        assert_eq!(replay[1].id, 4);
    }

    #[tokio::test]
    async fn test_ring_buffer_overflow() {
        let buffer = SseRingBuffer::new(5);

        // Push 10 events into buffer of size 5
        for i in 0..10 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", format!("{}", i));
            buffer.push(event).await;
        }

        // Should only have last 5 events
        let all = buffer.get_all().await;
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].id, 5);
        assert_eq!(all[4].id, 9);

        // Stats should show 5 dropped
        let stats = buffer.stats();
        assert_eq!(stats.dropped_count, 5);
        assert_eq!(stats.current_sequence, 10);
    }

    #[tokio::test]
    async fn test_gap_detection() {
        let buffer = SseRingBuffer::new(5);

        // Push 10 events (IDs 0-9)
        for _ in 0..10 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", "{}");
            buffer.push(event).await;
        }

        // Client with last_id=2 has a gap (events 3,4 were dropped)
        assert!(buffer.has_gap(2));

        // Client with last_id=7 has no gap (events 8,9 still available)
        assert!(!buffer.has_gap(7));

        // Client with last_id=9 has no gap
        assert!(!buffer.has_gap(9));
    }

    #[tokio::test]
    async fn test_replay_with_gap() {
        let buffer = SseRingBuffer::new(3);

        // Push 6 events (IDs 0-5), buffer keeps only 3-5
        for _ in 0..6 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", "{}");
            buffer.push(event).await;
        }

        // Client with last_id=1 gets events 3,4,5 (2 was dropped)
        let replay = buffer.replay_from(1).await;
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[0].id, 3);

        // Gap exists
        assert!(buffer.has_gap(1));
    }

    #[tokio::test]
    async fn test_clear() {
        let buffer = SseRingBuffer::new(10);

        for _ in 0..5 {
            let id = buffer.next_id();
            let event = SseEvent::new(id, "test", "{}");
            buffer.push(event).await;
        }

        assert_eq!(buffer.len().await, 5);

        buffer.clear().await;

        assert!(buffer.is_empty().await);
        assert_eq!(buffer.stats().dropped_count, 0);

        // Sequence counter should be preserved
        let next_id = buffer.next_id();
        assert_eq!(next_id, 5);
    }

    #[tokio::test]
    async fn test_empty_replay() {
        let buffer = SseRingBuffer::new(10);

        let replay = buffer.replay_from(0).await;
        assert!(replay.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        use std::sync::Arc;

        let buffer = Arc::new(SseRingBuffer::new(1000));
        let mut handles = vec![];

        // Spawn 10 tasks, each pushing 100 events
        for _ in 0..10 {
            let buf = Arc::clone(&buffer);
            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    let id = buf.next_id();
                    let event = SseEvent::new(id, "test", "{}");
                    buf.push(event).await;
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // All 1000 events should be stored (capacity is 1000)
        assert_eq!(buffer.len().await, 1000);

        // Sequence should be 1000
        assert_eq!(buffer.stats().current_sequence, 1000);

        // No events should be dropped
        assert_eq!(buffer.stats().dropped_count, 0);
    }
}
