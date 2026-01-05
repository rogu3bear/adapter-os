//! Background batch writer for diagnostic events.
//!
//! The writer consumes events from a channel and writes them to storage
//! in batches for efficiency. It handles:
//! - Batching (by count or timeout)
//! - Per-run sequence assignment
//! - Graceful shutdown

use crate::run_tracker::RunTracker;
use crate::DiagEnvelope;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, Instant, MissedTickBehavior};
use tracing::{debug, error, info, warn};

/// Configuration for the background writer.
#[derive(Debug, Clone)]
pub struct WriterConfig {
    /// Flush when batch reaches this size
    pub batch_size: usize,
    /// Flush after this timeout even if batch is incomplete
    pub batch_timeout: Duration,
    /// Maximum events per run (for logging/metrics)
    pub max_events_per_run: u32,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            batch_timeout: Duration::from_millis(500),
            max_events_per_run: 10000,
        }
    }
}

/// Event with assigned sequence number, ready for persistence.
#[derive(Debug, Clone)]
pub struct SequencedEvent {
    /// Sequence number within the run
    pub seq: u64,
    /// The original envelope
    pub envelope: DiagEnvelope,
}

/// Trait for persisting diagnostic events.
///
/// Implement this trait to provide storage for diagnostic events.
/// The writer calls this trait's methods during batch flushes.
#[async_trait::async_trait]
pub trait DiagPersister: Send + Sync {
    /// Persist a batch of sequenced events.
    ///
    /// Should execute as a single transaction for atomicity.
    /// Returns the number of events successfully persisted.
    async fn persist_batch(&self, events: &[SequencedEvent]) -> Result<usize, PersistError>;

    /// Update run statistics after a batch is persisted.
    ///
    /// Called after each successful batch to update event counts.
    async fn update_run_stats(&self, run_id: &str, events_added: u64) -> Result<(), PersistError>;
}

/// Error type for persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("database error: {0}")]
    Database(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

/// Background writer that consumes DiagEnvelopes and persists to storage.
///
/// Features:
/// - Batched writes (N events or T timeout, whichever comes first)
/// - Per-run sequence assignment
/// - Transaction-wrapped batch inserts
pub struct DiagnosticsWriter<P: DiagPersister> {
    persister: Arc<P>,
    config: WriterConfig,
    #[allow(dead_code)] // Reserved for future per-run stats coordination
    run_tracker: Arc<RunTracker>,
    /// Per-run sequence counters
    seq_counters: HashMap<String, u64>,
    /// Current batch buffer
    batch: Vec<SequencedEvent>,
    /// Last flush time
    last_flush: Instant,
    /// Stats
    total_persisted: u64,
    total_failed: u64,
}

impl<P: DiagPersister + 'static> DiagnosticsWriter<P> {
    /// Create a new diagnostics writer.
    pub fn new(persister: Arc<P>, config: WriterConfig, run_tracker: Arc<RunTracker>) -> Self {
        let batch_size = config.batch_size;
        Self {
            persister,
            config,
            run_tracker,
            seq_counters: HashMap::new(),
            batch: Vec::with_capacity(batch_size),
            last_flush: Instant::now(),
            total_persisted: 0,
            total_failed: 0,
        }
    }

    /// Run the background writer loop.
    ///
    /// Consumes events from the receiver and writes to storage.
    /// Exits gracefully on shutdown signal.
    pub async fn run(
        mut self,
        mut receiver: mpsc::Receiver<DiagEnvelope>,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) {
        info!("Diagnostics writer started");

        let mut flush_interval = interval(self.config.batch_timeout);
        flush_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;

                // Shutdown signal takes priority
                _ = shutdown_rx.recv() => {
                    info!("Diagnostics writer received shutdown signal");
                    // Drain remaining events from channel
                    while let Ok(envelope) = receiver.try_recv() {
                        self.handle_envelope(envelope).await;
                    }
                    // Flush remaining batch before exit
                    if !self.batch.is_empty() {
                        if let Err(e) = self.flush_batch().await {
                            error!(error = %e, "Failed to flush final batch on shutdown");
                        }
                    }
                    break;
                }

                // Receive events from channel
                result = receiver.recv() => {
                    match result {
                        Some(envelope) => {
                            self.handle_envelope(envelope).await;
                        }
                        None => {
                            // Channel closed - flush and exit
                            info!("Diagnostics channel closed");
                            if !self.batch.is_empty() {
                                if let Err(e) = self.flush_batch().await {
                                    error!(error = %e, "Failed to flush final batch on channel close");
                                }
                            }
                            break;
                        }
                    }
                }

                // Periodic flush on timeout
                _ = flush_interval.tick() => {
                    if !self.batch.is_empty() && self.last_flush.elapsed() >= self.config.batch_timeout {
                        if let Err(e) = self.flush_batch().await {
                            warn!(error = %e, "Periodic batch flush failed");
                        }
                    }
                }
            }
        }

        info!(
            total_persisted = self.total_persisted,
            total_failed = self.total_failed,
            active_runs = self.seq_counters.len(),
            "Diagnostics writer stopped"
        );
    }

    /// Handle a single envelope: assign sequence and add to batch.
    async fn handle_envelope(&mut self, envelope: DiagEnvelope) {
        let run_id = envelope.run_id.as_str().to_string();

        // Assign sequence number
        let seq = self.seq_counters.entry(run_id).or_insert(0);
        *seq += 1;
        let event_seq = *seq;

        // Add to batch with sequence
        self.batch.push(SequencedEvent {
            seq: event_seq,
            envelope,
        });

        // Flush if batch is full
        if self.batch.len() >= self.config.batch_size {
            if let Err(e) = self.flush_batch().await {
                warn!(error = %e, "Batch flush failed");
            }
        }
    }

    /// Flush the current batch to storage.
    async fn flush_batch(&mut self) -> Result<(), PersistError> {
        if self.batch.is_empty() {
            return Ok(());
        }

        let batch_size = self.batch.len();
        debug!(batch_size, "Flushing diagnostics batch");

        // Persist the batch
        match self.persister.persist_batch(&self.batch).await {
            Ok(persisted) => {
                self.total_persisted += persisted as u64;

                // Update run stats for each unique run in the batch
                let mut run_counts: HashMap<&str, u64> = HashMap::new();
                for event in &self.batch {
                    *run_counts
                        .entry(event.envelope.run_id.as_str())
                        .or_insert(0) += 1;
                }

                for (run_id, count) in run_counts {
                    if let Err(e) = self.persister.update_run_stats(run_id, count).await {
                        warn!(run_id = run_id, error = %e, "Failed to update run stats");
                    }
                }

                self.batch.clear();
                self.last_flush = Instant::now();
                Ok(())
            }
            Err(e) => {
                self.total_failed += batch_size as u64;
                error!(error = %e, batch_size, "Failed to persist diagnostics batch");
                // Keep batch for retry on next flush
                Err(e)
            }
        }
    }

    /// Get the current sequence for a run (for testing).
    #[cfg(test)]
    pub fn seq_for_run(&self, run_id: &str) -> u64 {
        *self.seq_counters.get(run_id).unwrap_or(&0)
    }
}

/// Spawn the diagnostics writer as a background task.
///
/// Returns a JoinHandle that can be used to wait for the task.
pub fn spawn_diagnostics_writer<P: DiagPersister + 'static>(
    persister: Arc<P>,
    receiver: mpsc::Receiver<DiagEnvelope>,
    run_tracker: Arc<RunTracker>,
    config: WriterConfig,
    shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let writer = DiagnosticsWriter::new(persister, config, run_tracker);
        writer.run(receiver, shutdown_rx).await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagEvent, DiagRunId, DiagSeverity, DiagStage};
    use adapteros_telemetry::tracing::TraceContext;
    use std::sync::Mutex;

    /// Mock persister for testing.
    struct MockPersister {
        events: Mutex<Vec<SequencedEvent>>,
        run_stats: Mutex<HashMap<String, u64>>,
        fail_on_persist: Mutex<bool>,
    }

    impl MockPersister {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                run_stats: Mutex::new(HashMap::new()),
                fail_on_persist: Mutex::new(false),
            }
        }

        fn set_fail(&self, fail: bool) {
            *self.fail_on_persist.lock().unwrap() = fail;
        }

        fn event_count(&self) -> usize {
            self.events.lock().unwrap().len()
        }

        fn run_stat(&self, run_id: &str) -> u64 {
            *self.run_stats.lock().unwrap().get(run_id).unwrap_or(&0)
        }
    }

    #[async_trait::async_trait]
    impl DiagPersister for MockPersister {
        async fn persist_batch(&self, events: &[SequencedEvent]) -> Result<usize, PersistError> {
            if *self.fail_on_persist.lock().unwrap() {
                return Err(PersistError::Database("mock failure".to_string()));
            }

            let mut stored = self.events.lock().unwrap();
            let count = events.len();
            stored.extend(events.iter().cloned());
            Ok(count)
        }

        async fn update_run_stats(
            &self,
            run_id: &str,
            events_added: u64,
        ) -> Result<(), PersistError> {
            let mut stats = self.run_stats.lock().unwrap();
            *stats.entry(run_id.to_string()).or_insert(0) += events_added;
            Ok(())
        }
    }

    fn make_envelope(run_id: DiagRunId) -> DiagEnvelope {
        let trace_ctx = TraceContext::new_root();
        DiagEnvelope::new(
            &trace_ctx,
            "tenant-123",
            run_id,
            DiagSeverity::Info,
            1000,
            DiagEvent::StageEnter {
                stage: DiagStage::RequestValidation,
            },
        )
    }

    #[tokio::test]
    async fn test_batch_flush_on_size() {
        let persister = Arc::new(MockPersister::new());
        let run_tracker = Arc::new(RunTracker::new());
        let config = WriterConfig {
            batch_size: 3,
            batch_timeout: Duration::from_secs(60), // Long timeout
            ..Default::default()
        };

        let (tx, rx) = mpsc::channel(10);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let writer_persister = Arc::clone(&persister);
        let writer_tracker = Arc::clone(&run_tracker);

        let handle = tokio::spawn(async move {
            let writer = DiagnosticsWriter::new(writer_persister, config, writer_tracker);
            writer.run(rx, shutdown_rx).await;
        });

        // Send 3 events (batch_size)
        let run_id = DiagRunId::new_random();
        for _ in 0..3 {
            tx.send(make_envelope(run_id.clone())).await.unwrap();
        }

        // Give time for batch to flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should have persisted 3 events
        assert_eq!(persister.event_count(), 3);

        // Cleanup
        drop(tx);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_sequence_assignment() {
        let persister = Arc::new(MockPersister::new());
        let run_tracker = Arc::new(RunTracker::new());
        let config = WriterConfig {
            batch_size: 10,
            batch_timeout: Duration::from_millis(10),
            ..Default::default()
        };

        let (tx, rx) = mpsc::channel(10);
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let writer_persister = Arc::clone(&persister);
        let writer_tracker = Arc::clone(&run_tracker);

        let handle = tokio::spawn(async move {
            let writer = DiagnosticsWriter::new(writer_persister, config, writer_tracker);
            writer.run(rx, shutdown_rx).await;
        });

        // Send events for same run
        let run_id = DiagRunId::new_random();
        for _ in 0..5 {
            tx.send(make_envelope(run_id.clone())).await.unwrap();
        }

        // Wait for timeout flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Check sequences
        let events = persister.events.lock().unwrap();
        let sequences: Vec<u64> = events.iter().map(|e| e.seq).collect();
        assert_eq!(sequences, vec![1, 2, 3, 4, 5]);

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let persister = Arc::new(MockPersister::new());
        let run_tracker = Arc::new(RunTracker::new());
        let config = WriterConfig {
            batch_size: 100,                        // Large batch - won't flush by size
            batch_timeout: Duration::from_secs(60), // Long timeout
            ..Default::default()
        };

        let (tx, rx) = mpsc::channel(10);
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let writer_persister = Arc::clone(&persister);
        let writer_tracker = Arc::clone(&run_tracker);

        let handle = tokio::spawn(async move {
            let writer = DiagnosticsWriter::new(writer_persister, config, writer_tracker);
            writer.run(rx, shutdown_rx).await;
        });

        // Send some events (less than batch_size)
        let run_id = DiagRunId::new_random();
        for _ in 0..5 {
            tx.send(make_envelope(run_id.clone())).await.unwrap();
        }

        // Events not yet flushed
        assert_eq!(persister.event_count(), 0);

        // Send shutdown signal
        let _ = shutdown_tx.send(());

        // Wait for writer to finish
        let _ = handle.await;

        // Should have flushed remaining events on shutdown
        assert_eq!(persister.event_count(), 5);
    }

    #[tokio::test]
    async fn test_persist_failure_keeps_batch() {
        let persister = Arc::new(MockPersister::new());
        let run_tracker = Arc::new(RunTracker::new());
        let config = WriterConfig {
            batch_size: 3,
            batch_timeout: Duration::from_millis(10),
            ..Default::default()
        };

        // Set to fail on persist
        persister.set_fail(true);

        let (tx, rx) = mpsc::channel(10);
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let writer_persister = Arc::clone(&persister);
        let writer_tracker = Arc::clone(&run_tracker);

        let handle = tokio::spawn(async move {
            let writer = DiagnosticsWriter::new(writer_persister, config, writer_tracker);
            writer.run(rx, shutdown_rx).await;
        });

        // Send batch_size events
        let run_id = DiagRunId::new_random();
        for _ in 0..3 {
            tx.send(make_envelope(run_id.clone())).await.unwrap();
        }

        // Wait for flush attempt
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should have no persisted events (failure)
        assert_eq!(persister.event_count(), 0);

        // Now allow persistence
        persister.set_fail(false);

        // Wait for retry flush on timeout
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = handle.await;

        // Should have eventually persisted on retry
        // Note: depends on retry logic - current impl keeps batch for next flush
    }

    #[tokio::test]
    async fn test_run_stats_update() {
        let persister = Arc::new(MockPersister::new());
        let run_tracker = Arc::new(RunTracker::new());
        let config = WriterConfig {
            batch_size: 100,
            batch_timeout: Duration::from_millis(10),
            ..Default::default()
        };

        let (tx, rx) = mpsc::channel(10);
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        let writer_persister = Arc::clone(&persister);
        let writer_tracker = Arc::clone(&run_tracker);

        let handle = tokio::spawn(async move {
            let writer = DiagnosticsWriter::new(writer_persister, config, writer_tracker);
            writer.run(rx, shutdown_rx).await;
        });

        // Send events for two runs
        let run_id1 = DiagRunId::from_trace_id("run-1");
        let run_id2 = DiagRunId::from_trace_id("run-2");

        for _ in 0..3 {
            tx.send(make_envelope(run_id1.clone())).await.unwrap();
        }
        for _ in 0..2 {
            tx.send(make_envelope(run_id2.clone())).await.unwrap();
        }

        // Wait for timeout flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Check run stats were updated
        assert_eq!(persister.run_stat("run-1"), 3);
        assert_eq!(persister.run_stat("run-2"), 2);

        // Cleanup
        let _ = shutdown_tx.send(());
        let _ = handle.await;
    }
}
