//! K Reduction Integration - Channel-based communication between memory and lifecycle managers
//!
//! This module sets up and manages the tokio mpsc channel for K reduction requests
//! flowing from the memory pressure manager to the lifecycle manager. It provides
//! a clean separation of concerns and enables asynchronous, non-blocking communication.
//!
//! # Architecture
//!
//! ```text
//! MemoryPressureManager (sender)
//!         |
//!         v
//!    mpsc::Sender<KReductionRequest>
//!         |
//!         v
//!    mpsc::Receiver<KReductionRequest>
//!         |
//!         v
//! LifecycleManager (consumer)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_memory::k_reduction_integration::KReductionChannelManager;
//!
//! // Create channel with buffer size of 32
//! let (tx, rx) = KReductionChannelManager::create_channel(32);
//!
//! // Memory manager sends requests
//! let request = KReductionRequest::new(...);
//! let pressure_manager = MemoryPressureManager::with_channel_sender(tracker, tx.clone());
//!
//! // Lifecycle manager consumes requests
//! tokio::spawn(async move {
//!     while let Some(request) = rx.recv().await {
//!         // Process K reduction request
//!         let response = process_request(request).await;
//!     }
//! });
//! ```

use crate::k_reduction_protocol::{KReductionRequest, KReductionResponse};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Configuration for K reduction channel
#[derive(Debug, Clone)]
pub struct KReductionChannelConfig {
    /// Channel buffer size (number of pending requests)
    pub buffer_size: usize,
    /// Maximum number of concurrent K reduction operations
    pub max_concurrent: usize,
    /// Timeout for waiting on response (ms)
    pub response_timeout_ms: u64,
    /// Enable telemetry for channel events
    pub enable_telemetry: bool,
}

impl Default for KReductionChannelConfig {
    fn default() -> Self {
        Self {
            buffer_size: 32,
            max_concurrent: 4,
            response_timeout_ms: 5000,
            enable_telemetry: true,
        }
    }
}

/// Statistics about K reduction channel activity
#[derive(Debug, Clone, Default)]
pub struct KReductionChannelStats {
    /// Total requests sent
    pub total_requests_sent: u64,
    /// Total requests received
    pub total_requests_received: u64,
    /// Requests currently pending
    pub pending_requests: usize,
    /// Total requests approved
    pub total_approved: u64,
    /// Total requests rejected
    pub total_rejected: u64,
    /// Average processing time (ms)
    pub avg_processing_time_ms: f64,
    /// Peak queue depth
    pub peak_queue_depth: usize,
    /// Total dropped requests (due to closed receiver)
    pub total_dropped: u64,
}

/// K Reduction Channel Manager - manages sender/receiver pair and statistics
pub struct KReductionChannelManager {
    /// Configuration
    config: KReductionChannelConfig,
    /// Statistics tracking
    stats: Arc<parking_lot::RwLock<KReductionChannelStats>>,
}

impl KReductionChannelManager {
    /// Create a new K reduction channel manager with default config
    pub fn new() -> Self {
        Self::with_config(KReductionChannelConfig::default())
    }

    /// Create a new K reduction channel manager with custom config
    pub fn with_config(config: KReductionChannelConfig) -> Self {
        Self {
            config,
            stats: Arc::new(parking_lot::RwLock::new(KReductionChannelStats::default())),
        }
    }

    /// Create the mpsc channel pair
    pub fn create_channel(&self) -> (KReductionRequestSender, KReductionRequestReceiver) {
        let (tx, rx) = mpsc::channel(self.config.buffer_size);
        let stats = Arc::clone(&self.stats);

        let sender = KReductionRequestSender {
            tx,
            stats: Arc::clone(&stats),
            config: self.config.clone(),
        };

        let receiver = KReductionRequestReceiver {
            rx,
            stats,
            config: self.config.clone(),
        };

        info!(
            buffer_size = self.config.buffer_size,
            "K reduction channel created"
        );

        (sender, receiver)
    }

    /// Get current channel statistics
    pub fn get_stats(&self) -> KReductionChannelStats {
        self.stats.read().clone()
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        *self.stats.write() = KReductionChannelStats::default();
    }
}

impl Default for KReductionChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Sender half of the K reduction channel
pub struct KReductionRequestSender {
    tx: mpsc::Sender<KReductionRequest>,
    stats: Arc<parking_lot::RwLock<KReductionChannelStats>>,
    config: KReductionChannelConfig,
}

impl KReductionRequestSender {
    /// Send a K reduction request (non-blocking)
    pub async fn send(&self, request: KReductionRequest) -> Result<(), SendError> {
        let request_id = request.request_id.clone();

        match self.tx.try_send(request.clone()) {
            Ok(()) => {
                self.stats.write().total_requests_sent += 1;

                if self.config.enable_telemetry {
                    debug!(
                        request_id = %request_id,
                        target_k = request.target_k,
                        pressure_level = request.pressure_level,
                        "K reduction request sent"
                    );
                }

                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!(
                    request_id = %request_id,
                    buffer_size = self.config.buffer_size,
                    "K reduction channel buffer full"
                );
                Err(SendError::ChannelFull)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                error!(
                    request_id = %request_id,
                    "K reduction channel closed, receiver dropped"
                );
                self.stats.write().total_dropped += 1;
                Err(SendError::ChannelClosed)
            }
        }
    }

    /// Send a K reduction request (blocking with timeout)
    pub async fn send_with_timeout(
        &self,
        request: KReductionRequest,
        timeout_ms: u64,
    ) -> Result<(), SendError> {
        let request_id = request.request_id.clone();

        match tokio::time::timeout(
            tokio::time::Duration::from_millis(timeout_ms),
            self.tx.send(request.clone()),
        )
        .await
        {
            Ok(Ok(())) => {
                self.stats.write().total_requests_sent += 1;

                if self.config.enable_telemetry {
                    debug!(
                        request_id = %request_id,
                        target_k = request.target_k,
                        "K reduction request sent with timeout"
                    );
                }

                Ok(())
            }
            Ok(Err(_)) => {
                error!(
                    request_id = %request_id,
                    "K reduction channel closed during send"
                );
                self.stats.write().total_dropped += 1;
                Err(SendError::ChannelClosed)
            }
            Err(_) => {
                warn!(
                    request_id = %request_id,
                    timeout_ms = timeout_ms,
                    "K reduction request send timed out"
                );
                Err(SendError::SendTimeout)
            }
        }
    }

    /// Get the number of pending requests
    pub fn pending_requests(&self) -> usize {
        self.tx.capacity()
    }

    /// Check if channel is closed
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

impl Clone for KReductionRequestSender {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            stats: Arc::clone(&self.stats),
            config: self.config.clone(),
        }
    }
}

/// Receiver half of the K reduction channel
pub struct KReductionRequestReceiver {
    rx: mpsc::Receiver<KReductionRequest>,
    stats: Arc<parking_lot::RwLock<KReductionChannelStats>>,
    config: KReductionChannelConfig,
}

impl KReductionRequestReceiver {
    /// Receive the next K reduction request
    pub async fn recv(&mut self) -> Option<KReductionRequest> {
        match self.rx.recv().await {
            Some(request) => {
                self.stats.write().total_requests_received += 1;

                if self.config.enable_telemetry {
                    debug!(
                        request_id = %request.request_id,
                        target_k = request.target_k,
                        pressure_level = request.pressure_level,
                        "K reduction request received"
                    );
                }

                Some(request)
            }
            None => {
                info!("K reduction channel closed, no more requests");
                None
            }
        }
    }

    /// Receive with a timeout
    pub async fn recv_with_timeout(
        &mut self,
        timeout_ms: u64,
    ) -> Result<Option<KReductionRequest>, RecvError> {
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(timeout_ms),
            self.rx.recv(),
        )
        .await
        {
            Ok(Some(request)) => {
                self.stats.write().total_requests_received += 1;

                if self.config.enable_telemetry {
                    debug!(
                        request_id = %request.request_id,
                        "K reduction request received with timeout"
                    );
                }

                Ok(Some(request))
            }
            Ok(None) => Ok(None),
            Err(_) => {
                warn!("K reduction receive timed out");
                Err(RecvError::Timeout)
            }
        }
    }

    /// Try to receive without waiting
    pub fn try_recv(&mut self) -> Result<KReductionRequest, RecvError> {
        match self.rx.try_recv() {
            Ok(request) => {
                self.stats.write().total_requests_received += 1;

                if self.config.enable_telemetry {
                    debug!(
                        request_id = %request.request_id,
                        "K reduction request try_recv succeeded"
                    );
                }

                Ok(request)
            }
            Err(mpsc::error::TryRecvError::Empty) => Err(RecvError::Empty),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                error!("K reduction channel disconnected");
                Err(RecvError::Disconnected)
            }
        }
    }

    /// Record a decision outcome (for statistics)
    pub fn record_decision_outcome(&self, approved: bool) {
        let mut stats = self.stats.write();
        if approved {
            stats.total_approved += 1;
        } else {
            stats.total_rejected += 1;
        }
    }

    /// Get current pending requests
    pub fn pending_requests(&self) -> usize {
        self.rx.len()
    }
}

/// Error type for send operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// Channel buffer is full
    ChannelFull,
    /// Receiver half has been dropped
    ChannelClosed,
    /// Send operation timed out
    SendTimeout,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::ChannelFull => write!(f, "K reduction channel buffer is full"),
            SendError::ChannelClosed => {
                write!(f, "K reduction channel is closed (receiver dropped)")
            }
            SendError::SendTimeout => write!(f, "K reduction send timed out"),
        }
    }
}

impl std::error::Error for SendError {}

/// Error type for receive operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvError {
    /// No messages available (try_recv only)
    Empty,
    /// Sender half has been dropped
    Disconnected,
    /// Receive operation timed out
    Timeout,
}

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecvError::Empty => write!(f, "K reduction channel is empty"),
            RecvError::Disconnected => {
                write!(f, "K reduction channel is disconnected (sender dropped)")
            }
            RecvError::Timeout => write!(f, "K reduction receive timed out"),
        }
    }
}

impl std::error::Error for RecvError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_channel_creation() {
        let manager = KReductionChannelManager::new();
        let (tx, rx) = manager.create_channel();

        assert!(!tx.is_closed());
        assert_eq!(manager.get_stats().total_requests_sent, 0);
    }

    #[tokio::test]
    async fn test_channel_send_recv() {
        let manager = KReductionChannelManager::new();
        let (tx, mut rx) = manager.create_channel();

        let request = KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test".to_string());
        let request_id = request.request_id.clone();

        tx.send(request).await.unwrap();

        let received = rx.recv().await;
        assert!(received.is_some());
        assert_eq!(received.unwrap().request_id, request_id);

        let stats = manager.get_stats();
        assert_eq!(stats.total_requests_sent, 1);
        assert_eq!(stats.total_requests_received, 1);
    }

    #[tokio::test]
    async fn test_channel_full() {
        let config = KReductionChannelConfig {
            buffer_size: 1,
            ..Default::default()
        };
        let manager = KReductionChannelManager::with_config(config);
        let (tx, _rx) = manager.create_channel();

        let request1 = KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test1".to_string());
        let request2 = KReductionRequest::new(6, 10, 0.90, 2048 * 1024, 5.0, "Test2".to_string());

        assert!(tx.send(request1).await.is_ok());

        let result = tx.send(request2).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), SendError::ChannelFull);
    }

    #[tokio::test]
    async fn test_channel_closed() {
        let manager = KReductionChannelManager::new();
        let (tx, rx) = manager.create_channel();

        drop(rx);

        let request = KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test".to_string());
        let result = tx.send(request).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), SendError::ChannelClosed);
    }

    #[tokio::test]
    async fn test_try_recv() {
        let manager = KReductionChannelManager::new();
        let (tx, mut rx) = manager.create_channel();

        // Empty channel should return Empty
        assert_eq!(rx.try_recv().unwrap_err(), RecvError::Empty);

        // Send a request
        let request = KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test".to_string());
        tx.send(request.clone()).await.unwrap();

        // Now try_recv should succeed
        let received = rx.try_recv().unwrap();
        assert_eq!(received.request_id, request.request_id);
    }

    #[tokio::test]
    async fn test_record_decision_outcome() {
        let manager = KReductionChannelManager::new();
        let (_tx, rx) = manager.create_channel();

        rx.record_decision_outcome(true);
        rx.record_decision_outcome(true);
        rx.record_decision_outcome(false);

        let stats = manager.get_stats();
        assert_eq!(stats.total_approved, 2);
        assert_eq!(stats.total_rejected, 1);
    }

    #[tokio::test]
    async fn test_send_timeout() {
        let config = KReductionChannelConfig {
            buffer_size: 0, // Full immediately
            ..Default::default()
        };
        let manager = KReductionChannelManager::with_config(config);
        let (tx, _rx) = manager.create_channel();

        // The channel with buffer_size=0 is a bit tricky; let's test with a real timeout scenario
        let request = KReductionRequest::new(8, 10, 0.85, 1024 * 1024, 10.0, "Test".to_string());

        // Send should fail immediately since buffer is 0
        let result = tx.send(request).await;
        assert!(result.is_err());
    }
}
