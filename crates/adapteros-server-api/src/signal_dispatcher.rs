//! Robust Signal Dispatcher
//!
//! Handles signal collection from multiple workers with:
//! - Exponential backoff for connection retries
//! - Circuit breaker pattern for fault tolerance
//! - Multi-worker signal aggregation
//! - Signal authentication and validation
//! - Graceful shutdown support

use adapteros_core::{AosError, Result};
use adapteros_crypto::signature::PublicKey;
use adapteros_lora_worker::signal::{Signal, SignalType};
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

/// Signal processing configuration
#[derive(Debug, Clone)]
pub struct SignalConfig {
    pub auth_required: bool,
    pub channel_capacity: usize,
    pub retry_delay_secs: u64,
    pub max_retry_delay_secs: u64,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_reset_secs: u64,
    pub connection_timeout_secs: u64,
    pub multi_worker_enabled: bool,
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Failing, reject requests
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker implementation
#[derive(Debug)]
struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    last_failure_time: Option<std::time::Instant>,
    config: SignalConfig,
}

impl CircuitBreaker {
    fn new(config: SignalConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            last_failure_time: None,
            config,
        }
    }

    fn should_attempt(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    let elapsed = last_failure.elapsed();
                    if elapsed >= Duration::from_secs(self.config.circuit_breaker_reset_secs) {
                        self.state = CircuitBreakerState::HalfOpen;
                        debug!("Circuit breaker transitioning to half-open");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitBreakerState::Closed;
        debug!("Circuit breaker reset to closed on success");
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(std::time::Instant::now());

        if self.failure_count >= self.config.circuit_breaker_threshold {
            self.state = CircuitBreakerState::Open;
            warn!(
                "Circuit breaker opened after {} failures",
                self.failure_count
            );
        }
    }
}

/// Worker connection state
#[derive(Debug)]
struct WorkerConnection {
    uds_path: std::path::PathBuf,
    circuit_breaker: CircuitBreaker,
    last_connected: Option<std::time::Instant>,
    retry_delay: Duration,
}

/// Signal dispatcher configuration
#[derive(Debug, Clone)]
pub struct SignalConfig {
    pub auth_required: bool,
    pub channel_capacity: usize,
    pub retry_delay_secs: u64,
    pub max_retry_delay_secs: u64,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_reset_secs: u64,
    pub connection_timeout_secs: u64,
    pub multi_worker_enabled: bool,
}

/// Signal dispatcher metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDispatcherMetrics {
    pub total_signals_received: u64,
    pub total_signals_validated: u64,
    pub total_signals_rejected: u64,
    pub worker_connections_active: usize,
    pub circuit_breakers_open: usize,
    pub last_signal_timestamp: Option<u128>,
}

/// Robust signal dispatcher
pub struct SignalDispatcher {
    config: SignalConfig,
    discovery_tx: broadcast::Sender<Signal>,
    contact_tx: broadcast::Sender<Signal>,
    workers: Arc<RwLock<HashMap<String, WorkerConnection>>>,
    public_key: Option<PublicKey>,
    telemetry: Option<TelemetryWriter>,
    metrics: Arc<RwLock<SignalDispatcherMetrics>>,
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: mpsc::Receiver<()>,
}

impl SignalDispatcher {
    /// Create a new signal dispatcher
    pub fn new(
        config: SignalConfig,
        discovery_tx: broadcast::Sender<Signal>,
        contact_tx: broadcast::Sender<Signal>,
        public_key: Option<PublicKey>,
        telemetry: Option<TelemetryWriter>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Self {
            config,
            discovery_tx,
            contact_tx,
            workers: Arc::new(RwLock::new(HashMap::new())),
            public_key,
            telemetry,
            metrics: Arc::new(RwLock::new(SignalDispatcherMetrics {
                total_signals_received: 0,
                total_signals_validated: 0,
                total_signals_rejected: 0,
                worker_connections_active: 0,
                circuit_breakers_open: 0,
                last_signal_timestamp: None,
            })),
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Add a worker to monitor
    pub async fn add_worker(&self, worker_id: String, uds_path: std::path::PathBuf) {
        let mut workers = self.workers.write().await;
        workers.insert(
            worker_id.clone(),
            WorkerConnection {
                uds_path,
                circuit_breaker: CircuitBreaker::new(self.config.clone()),
                last_connected: None,
                retry_delay: Duration::from_secs(self.config.retry_delay_secs),
            },
        );
        info!("Added worker {} to signal dispatcher", worker_id);
    }

    /// Start the signal dispatcher
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting signal dispatcher");

        if self.config.multi_worker_enabled {
            self.start_multi_worker_mode().await;
        } else {
            self.start_single_worker_mode().await;
        }

        Ok(())
    }

    /// Start in multi-worker mode (aggregate from all workers)
    async fn start_multi_worker_mode(&mut self) {
        let workers = self.workers.clone();
        let config = self.config.clone();
        let discovery_tx = self.discovery_tx.clone();
        let contact_tx = self.contact_tx.clone();
        let public_key = self.public_key.clone();
        let telemetry = self.telemetry.clone();
        let metrics = self.metrics.clone();
        let mut shutdown_rx = self.shutdown_rx;

        tokio::spawn(async move {
            info!("Signal dispatcher running in multi-worker mode");

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("Signal dispatcher received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        // Check all workers periodically
                        let worker_ids: Vec<String> = {
                            let workers = workers.read().await;
                            workers.keys().cloned().collect()
                        };

                        for worker_id in worker_ids {
                            let worker_info = {
                                let workers = workers.read().await;
                                workers.get(&worker_id).cloned()
                            };

                            if let Some(worker) = worker_info {
                                Self::connect_to_worker(
                                    worker_id.clone(),
                                    worker,
                                    config.clone(),
                                    discovery_tx.clone(),
                                    contact_tx.clone(),
                                    public_key.clone(),
                                    telemetry.clone(),
                                    metrics.clone(),
                                    workers.clone(),
                                ).await;
                            }
                        }
                    }
                }
            }
        });
    }

    /// Start in single-worker mode (connect to first available worker)
    async fn start_single_worker_mode(&mut self) {
        let workers = self.workers.clone();
        let config = self.config.clone();
        let discovery_tx = self.discovery_tx.clone();
        let contact_tx = self.contact_tx.clone();
        let public_key = self.public_key.clone();
        let telemetry = self.telemetry.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            info!("Signal dispatcher running in single-worker mode");

            loop {
                let worker_info = {
                    let workers = workers.read().await;
                    workers.values().next().cloned()
                };

                if let Some(worker) = worker_info {
                    Self::connect_to_worker(
                        "default".to_string(),
                        worker,
                        config.clone(),
                        discovery_tx.clone(),
                        contact_tx.clone(),
                        public_key.clone(),
                        telemetry.clone(),
                        metrics.clone(),
                        workers.clone(),
                    ).await;
                } else {
                    warn!("No workers available, waiting...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });
    }

    /// Connect to a specific worker
    async fn connect_to_worker(
        worker_id: String,
        mut worker: WorkerConnection,
        config: SignalConfig,
        discovery_tx: broadcast::Sender<Signal>,
        contact_tx: broadcast::Sender<Signal>,
        public_key: Option<PublicKey>,
        telemetry: Option<TelemetryWriter>,
        metrics: Arc<RwLock<SignalDispatcherMetrics>>,
        workers: Arc<RwLock<HashMap<String, WorkerConnection>>>,
    ) {
        if !worker.circuit_breaker.should_attempt() {
            return;
        }

        let connection_timeout = Duration::from_secs(config.connection_timeout_secs);

        match timeout(connection_timeout, tokio::net::UnixStream::connect(&worker.uds_path)).await {
            Ok(Ok(stream)) => {
                debug!("Connected to worker {} at {:?}", worker_id, worker.uds_path);
                worker.circuit_breaker.record_success();
                worker.last_connected = Some(std::time::Instant::now());

                // Update worker state
                {
                    let mut workers_write = workers.write().await;
                    if let Some(w) = workers_write.get_mut(&worker_id) {
                        w.circuit_breaker = worker.circuit_breaker;
                        w.last_connected = worker.last_connected;
                        w.retry_delay = Duration::from_secs(config.retry_delay_secs); // Reset backoff
                    }
                }

                // Handle the connection
                Self::handle_worker_connection(
                    worker_id,
                    stream,
                    config,
                    discovery_tx,
                    contact_tx,
                    public_key,
                    telemetry,
                    metrics,
                ).await;

            }
            Ok(Err(e)) => {
                warn!("Failed to connect to worker {}: {}", worker_id, e);
                worker.circuit_breaker.record_failure();

                // Exponential backoff
                let max_delay = Duration::from_secs(config.max_retry_delay_secs);
                worker.retry_delay = std::cmp::min(worker.retry_delay * 2, max_delay);

                // Update worker state
                {
                    let mut workers_write = workers.write().await;
                    if let Some(w) = workers_write.get_mut(&worker_id) {
                        w.circuit_breaker = worker.circuit_breaker;
                        w.retry_delay = worker.retry_delay;
                    }
                }

                tokio::time::sleep(worker.retry_delay).await;
            }
            Err(_) => {
                warn!("Connection timeout to worker {}", worker_id);
                worker.circuit_breaker.record_failure();
                tokio::time::sleep(worker.retry_delay).await;
            }
        }
    }

    /// Handle connection to a worker
    async fn handle_worker_connection(
        worker_id: String,
        stream: tokio::net::UnixStream,
        config: SignalConfig,
        discovery_tx: broadcast::Sender<Signal>,
        contact_tx: broadcast::Sender<Signal>,
        public_key: Option<PublicKey>,
        telemetry: Option<TelemetryWriter>,
        metrics: Arc<RwLock<SignalDispatcherMetrics>>,
    ) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        match stream.write_all(b"GET /signals HTTP/1.1\r\nHost: worker\r\n\r\n").await {
            Ok(()) => {}
            Err(e) => {
                warn!("Failed to send signals request to worker {}: {}", worker_id, e);
                return;
            }
        }

        let mut reader = BufReader::new(stream);
        let mut line = String::new();

        // Skip HTTP headers
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    debug!("Worker {} connection closed", worker_id);
                    return;
                }
                Ok(_) => {
                    if line.trim().is_empty() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Error reading headers from worker {}: {}", worker_id, e);
                    return;
                }
            }
        }

        // Read SSE events
        let mut event_type = String::new();
        let mut event_data = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    debug!("Worker {} signal stream ended", worker_id);
                    return;
                }
                Ok(_) => {
                    let l = line.trim();
                    if l.is_empty() {
                        if !event_type.is_empty() && !event_data.is_empty() {
                            Self::process_signal_event(
                                &worker_id,
                                &event_data,
                                &config,
                                &discovery_tx,
                                &contact_tx,
                                &public_key,
                                &telemetry,
                                &metrics,
                            ).await;
                        }
                        event_type.clear();
                        event_data.clear();
                    } else if let Some(et) = l.strip_prefix("event:") {
                        event_type = et.trim().to_string();
                    } else if let Some(data) = l.strip_prefix("data:") {
                        if !event_data.is_empty() {
                            event_data.push('\n');
                        }
                        event_data.push_str(data.trim());
                    }
                }
                Err(e) => {
                    warn!("Error reading signal data from worker {}: {}", worker_id, e);
                    return;
                }
            }
        }
    }

    /// Process a signal event from a worker
    async fn process_signal_event(
        worker_id: &str,
        event_data: &str,
        config: &SignalConfig,
        discovery_tx: &broadcast::Sender<Signal>,
        contact_tx: &broadcast::Sender<Signal>,
        public_key: &Option<PublicKey>,
        telemetry: &Option<TelemetryWriter>,
        metrics: &Arc<RwLock<SignalDispatcherMetrics>>,
    ) {
        // Update metrics
        {
            let mut metrics_write = metrics.write().await;
            metrics_write.total_signals_received += 1;
        }

        // Parse signal
        let signal: Signal = match serde_json::from_str(event_data) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to parse signal from worker {}: {}", worker_id, e);
                Self::update_rejected_metrics(metrics).await;
                return;
            }
        };

        // Validate signal if authentication required
        if config.auth_required {
            if let Some(ref pk) = public_key {
                match signal.verify_signature(pk) {
                    Ok(true) => {
                        debug!("Signal authentication successful for {:?}", signal.signal_type);
                    }
                    Ok(false) => {
                        warn!("Signal authentication failed for {:?}", signal.signal_type);
                        Self::update_rejected_metrics(metrics).await;
                        return;
                    }
                    Err(e) => {
                        warn!("Signal verification error: {}", e);
                        Self::update_rejected_metrics(metrics).await;
                        return;
                    }
                }
            } else {
                warn!("Signal authentication required but no public key configured");
                Self::update_rejected_metrics(metrics).await;
                return;
            }
        }

        // Update validated metrics
        {
            let mut metrics_write = metrics.write().await;
            metrics_write.total_signals_validated += 1;
            metrics_write.last_signal_timestamp = Some(signal.timestamp);
        }

        // Route signal to appropriate channel
        let result = match signal.signal_type {
            SignalType::RepoScanStarted
            | SignalType::RepoScanProgress
            | SignalType::SymbolIndexed
            | SignalType::FrameworkDetected
            | SignalType::TestMapUpdated
            | SignalType::RepoScanCompleted => {
                discovery_tx.send(signal)
            }
            SignalType::ContactDiscovered
            | SignalType::ContactUpdated
            | SignalType::ContactInteraction => {
                contact_tx.send(signal)
            }
            _ => {
                debug!("Ignoring non-streaming signal type: {:?}", signal.signal_type);
                return;
            }
        };

        if let Err(e) = result {
            warn!("Failed to broadcast signal: {}", e);
        }

        // Log to telemetry if available
        if let Some(ref tw) = telemetry {
            let _ = tw.log("signal.received", serde_json::json!({
                "worker_id": worker_id,
                "signal_type": signal.signal_type,
                "timestamp": signal.timestamp,
            }));
        }
    }

    /// Update rejected signal metrics
    async fn update_rejected_metrics(metrics: &Arc<RwLock<SignalDispatcherMetrics>>) {
        let mut metrics_write = metrics.write().await;
        metrics_write.total_signals_rejected += 1;
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> SignalDispatcherMetrics {
        self.metrics.read().await.clone()
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down signal dispatcher");
        self.shutdown_tx.send(()).await
            .map_err(|_| AosError::Internal("Failed to send shutdown signal".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::signature::Keypair;
    use adapteros_lora_worker::signal::SignalBuilder;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn test_signal_signing_and_verification() {
        let keypair = Keypair::generate();
        let public_key = keypair.public_key();

        let mut signal = SignalBuilder::new(SignalType::ContactDiscovered)
            .priority(adapteros_lora_worker::signal::SignalPriority::Normal)
            .with_field("name", serde_json::json!("test_contact"))
            .with_field("category", serde_json::json!("user"))
            .with_field("tenant_id", serde_json::json!("test_tenant"))
            .build();

        // Sign the signal
        signal.sign(&keypair).expect("Signal signing should succeed");

        // Verify the signature
        let is_valid = signal.verify_signature(&public_key).expect("Signature verification should succeed");
        assert!(is_valid, "Signal signature should be valid");

        // Tamper with the signal and verify it fails
        signal.payload.insert("name".to_string(), serde_json::json!("tampered"));
        let is_valid_tampered = signal.verify_signature(&public_key).expect("Signature verification should succeed");
        assert!(!is_valid_tampered, "Tampered signal signature should be invalid");
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = SignalConfig {
            auth_required: false,
            channel_capacity: 10,
            retry_delay_secs: 1,
            max_retry_delay_secs: 10,
            circuit_breaker_threshold: 3,
            circuit_breaker_reset_secs: 5,
            connection_timeout_secs: 1,
            multi_worker_enabled: false,
        };

        let mut breaker = CircuitBreaker::new(config);

        // Initially closed
        assert!(breaker.should_attempt());

        // Record failures
        for _ in 0..3 {
            breaker.record_failure();
        }

        // Should be open after threshold failures
        assert!(!breaker.should_attempt());

        // Record success
        breaker.record_success();

        // Should be closed again
        assert!(breaker.should_attempt());
    }

    #[tokio::test]
    async fn test_signal_dispatcher_creation() {
        let config = SignalConfig {
            auth_required: false,
            channel_capacity: 10,
            retry_delay_secs: 1,
            max_retry_delay_secs: 5,
            circuit_breaker_threshold: 3,
            circuit_breaker_reset_secs: 5,
            connection_timeout_secs: 1,
            multi_worker_enabled: false,
        };

        let (discovery_tx, _) = broadcast::channel(10);
        let (contact_tx, _) = broadcast::channel(10);

        let dispatcher = SignalDispatcher::new(
            config,
            discovery_tx,
            contact_tx,
            None,
            None,
        );

        let metrics = dispatcher.get_metrics().await;
        assert_eq!(metrics.total_signals_received, 0);
        assert_eq!(metrics.total_signals_validated, 0);
        assert_eq!(metrics.total_signals_rejected, 0);
    }

    #[test]
    fn test_signal_canonical_representation() {
        let signal = SignalBuilder::new(SignalType::ContactDiscovered)
            .priority(adapteros_lora_worker::signal::SignalPriority::Normal)
            .with_field("name", serde_json::json!("test"))
            .with_field("tenant_id", serde_json::json!("tenant"))
            .build();

        let canonical = signal.canonical_representation();
        assert!(!canonical.is_empty());

        // Canonical representation should be deterministic
        let canonical2 = signal.canonical_representation();
        assert_eq!(canonical, canonical2);
    }
}
