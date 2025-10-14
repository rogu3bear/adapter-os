//! Deterministic channel implementation
//!
//! Provides sequence-tagged message passing for deterministic execution.
//! All messages are tagged with a sequence number to ensure reproducible ordering.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use thiserror::Error;

/// Global channel sequence counter for cross-channel ordering
static GLOBAL_CHANNEL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Error types for deterministic channels
#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("Channel closed")]
    Closed,
    #[error("Channel is full (capacity: {capacity})")]
    Full { capacity: usize },
}

/// Result type for channel operations
pub type Result<T> = std::result::Result<T, ChannelError>;

/// Sequence-tagged message for deterministic ordering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencedMessage<T> {
    /// Global sequence number for ordering
    pub sequence: u64,
    /// The actual message payload
    pub payload: T,
    /// Optional agent ID for multi-agent scenarios
    pub agent_id: Option<String>,
}

/// Sender half of deterministic channel
#[derive(Debug, Clone)]
pub struct DeterministicSender<T> {
    queue: Arc<Mutex<VecDeque<SequencedMessage<T>>>>,
    capacity: usize,
    closed: Arc<AtomicU64>,
    agent_id: Option<String>,
}

impl<T> DeterministicSender<T> {
    /// Send a message with automatic sequencing
    pub fn send(&self, payload: T) -> Result<()> {
        if self.closed.load(Ordering::Relaxed) != 0 {
            return Err(ChannelError::Closed);
        }

        let mut queue = self.queue.lock();

        if queue.len() >= self.capacity {
            return Err(ChannelError::Full {
                capacity: self.capacity,
            });
        }

        // Assign global sequence number
        let sequence = GLOBAL_CHANNEL_SEQUENCE.fetch_add(1, Ordering::SeqCst);

        let message = SequencedMessage {
            sequence,
            payload,
            agent_id: self.agent_id.clone(),
        };

        queue.push_back(message);
        Ok(())
    }

    /// Check if the channel is closed
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed) != 0
    }

    /// Close the sender
    pub fn close(&self) {
        self.closed.store(1, Ordering::Relaxed);
    }
}

/// Receiver half of deterministic channel
#[derive(Debug)]
pub struct DeterministicReceiver<T> {
    queue: Arc<Mutex<VecDeque<SequencedMessage<T>>>>,
    capacity: usize,
    closed: Arc<AtomicU64>,
}

impl<T> DeterministicReceiver<T> {
    /// Try to receive a message (non-blocking)
    pub fn try_recv(&self) -> Result<SequencedMessage<T>> {
        let mut queue = self.queue.lock();

        if let Some(message) = queue.pop_front() {
            Ok(message)
        } else if self.closed.load(Ordering::Relaxed) != 0 {
            Err(ChannelError::Closed)
        } else {
            // Would block - in deterministic context, this should be handled by executor
            Err(ChannelError::Closed)
        }
    }

    /// Receive a message (blocking in standard async context)
    /// In deterministic executor, this should be polled deterministically
    pub async fn recv(&self) -> Result<SequencedMessage<T>> {
        loop {
            match self.try_recv() {
                Ok(msg) => return Ok(msg),
                Err(ChannelError::Closed) if self.is_closed() => {
                    return Err(ChannelError::Closed);
                }
                Err(_) => {
                    // In deterministic context, yield to executor
                    tokio::task::yield_now().await;
                }
            }
        }
    }

    /// Check if the channel is closed
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed) != 0
    }

    /// Close the receiver
    pub fn close(&self) {
        self.closed.store(1, Ordering::Relaxed);
    }

    /// Get number of pending messages
    pub fn len(&self) -> usize {
        self.queue.lock().len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }
}

impl<T> Clone for DeterministicReceiver<T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            capacity: self.capacity,
            closed: self.closed.clone(),
        }
    }
}

/// Create a deterministic channel with bounded capacity
pub fn channel<T>(capacity: usize) -> (DeterministicSender<T>, DeterministicReceiver<T>) {
    channel_with_agent(capacity, None)
}

/// Create a deterministic channel with agent ID
pub fn channel_with_agent<T>(
    capacity: usize,
    agent_id: Option<String>,
) -> (DeterministicSender<T>, DeterministicReceiver<T>) {
    let queue = Arc::new(Mutex::new(VecDeque::with_capacity(capacity)));
    let closed = Arc::new(AtomicU64::new(0));

    let sender = DeterministicSender {
        queue: queue.clone(),
        capacity,
        closed: closed.clone(),
        agent_id,
    };

    let receiver = DeterministicReceiver {
        queue,
        capacity,
        closed,
    };

    (sender, receiver)
}

/// Reset global channel sequence (for testing)
#[cfg(test)]
pub fn reset_global_sequence() {
    GLOBAL_CHANNEL_SEQUENCE.store(0, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_basic() {
        reset_global_sequence();

        let (tx, rx) = channel::<i32>(10);

        tx.send(42).unwrap();
        tx.send(43).unwrap();

        let msg1 = rx.try_recv().unwrap();
        assert_eq!(msg1.payload, 42);
        assert_eq!(msg1.sequence, 0);

        let msg2 = rx.try_recv().unwrap();
        assert_eq!(msg2.payload, 43);
        assert_eq!(msg2.sequence, 1);
    }

    #[test]
    fn test_channel_capacity() {
        reset_global_sequence();

        let (tx, _rx) = channel::<i32>(2);

        tx.send(1).unwrap();
        tx.send(2).unwrap();

        // Should fail due to capacity
        let result = tx.send(3);
        assert!(matches!(result, Err(ChannelError::Full { .. })));
    }

    #[test]
    fn test_channel_close() {
        reset_global_sequence();

        let (tx, _rx) = channel::<i32>(10);

        tx.close();
        assert!(tx.is_closed());

        // Should fail after close
        let result = tx.send(1);
        assert!(matches!(result, Err(ChannelError::Closed)));
    }

    #[test]
    fn test_sequencing_across_channels() {
        reset_global_sequence();

        let (tx1, rx1) = channel::<i32>(10);
        let (tx2, rx2) = channel::<i32>(10);

        tx1.send(100).unwrap();
        tx2.send(200).unwrap();
        tx1.send(101).unwrap();

        let msg1 = rx1.try_recv().unwrap();
        let msg2 = rx2.try_recv().unwrap();
        let msg3 = rx1.try_recv().unwrap();

        assert_eq!(msg1.sequence, 0);
        assert_eq!(msg2.sequence, 1);
        assert_eq!(msg3.sequence, 2);
    }

    #[test]
    fn test_agent_id() {
        reset_global_sequence();

        let (tx, rx) = channel_with_agent::<i32>(10, Some("agent-1".to_string()));

        tx.send(42).unwrap();

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.agent_id.as_deref(), Some("agent-1"));
    }
}
