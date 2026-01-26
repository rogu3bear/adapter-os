//! Diagnostics service for managing event emission and tracking.
//!
//! The DiagnosticsService provides:
//! - Non-blocking event emission via bounded channel
//! - Per-run event and drop tracking
//! - Level-based event filtering
//! - Global metrics

use super::{DiagEnvelope, DiagError, DiagEvent, DiagRunId};
use crate::diagnostics::run_tracker::{RunSummary, RunTracker};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

/// Diagnostic verbosity level for filtering.
///
/// Higher levels include all events from lower levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum DiagLevel {
    /// Diagnostics disabled
    #[default]
    Off = 0,
    /// Only error events (StageFailed)
    Errors = 1,
    /// Stage enter/complete/failed events
    Stages = 2,
    /// Stages + router decision events
    Router = 3,
    /// All events including token-level
    Tokens = 4,
}

impl DiagLevel {
    /// Create from a string (for config integration).
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "off" => Self::Off,
            "errors" => Self::Errors,
            "stages" => Self::Stages,
            "router" => Self::Router,
            "tokens" => Self::Tokens,
            _ => Self::Off,
        }
    }

    /// Check if this level is enabled (not Off).
    pub fn is_enabled(self) -> bool {
        self != Self::Off
    }
}

/// Configuration for DiagnosticsService.
#[derive(Debug, Clone)]
pub struct DiagnosticsConfig {
    /// Whether diagnostics are enabled
    pub enabled: bool,
    /// Verbosity level
    pub level: DiagLevel,
    /// Bounded channel capacity
    pub channel_capacity: usize,
    /// Maximum events to persist per run
    pub max_events_per_run: u32,
    /// Batch size for writes
    pub batch_size: usize,
    /// Batch timeout in milliseconds
    pub batch_timeout_ms: u64,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: DiagLevel::Off,
            channel_capacity: 1000,
            max_events_per_run: 10000,
            batch_size: 100,
            batch_timeout_ms: 500,
        }
    }
}

/// Global diagnostics metrics snapshot.
#[derive(Debug, Clone, Default)]
pub struct DiagMetrics {
    /// Total events emitted globally
    pub global_emits: u64,
    /// Total events dropped globally
    pub global_drops: u64,
    /// Currently active runs
    pub active_runs: usize,
}

/// Central diagnostics service for event emission and tracking.
///
/// The service owns the sender side of a bounded mpsc channel.
/// Events are emitted via `try_send` (non-blocking).
/// A background task consumes events and writes to SQLite.
#[derive(Clone)]
pub struct DiagnosticsService {
    /// Channel sender for emitting events (cloneable)
    sender: mpsc::Sender<DiagEnvelope>,
    /// Global drop counter
    global_drops: Arc<AtomicU64>,
    /// Global emit counter
    global_emits: Arc<AtomicU64>,
    /// Per-run tracker
    run_tracker: Arc<RunTracker>,
    /// Configuration snapshot
    config: DiagnosticsConfig,
    /// Whether the service is effectively enabled
    is_enabled: bool,
}

impl DiagnosticsService {
    /// Create a new DiagnosticsService and its receiver.
    ///
    /// Returns `(service, receiver)` where:
    /// - `service` can be cloned and distributed to emit events
    /// - `receiver` should be passed to the background writer task
    pub fn new(config: DiagnosticsConfig) -> (Self, mpsc::Receiver<DiagEnvelope>) {
        let (sender, receiver) = mpsc::channel(config.channel_capacity);
        let is_enabled = config.enabled && config.level.is_enabled();

        let service = Self {
            sender,
            global_drops: Arc::new(AtomicU64::new(0)),
            global_emits: Arc::new(AtomicU64::new(0)),
            run_tracker: Arc::new(RunTracker::new()),
            config,
            is_enabled,
        };

        (service, receiver)
    }

    /// Create a disabled (no-op) service.
    ///
    /// Events are silently discarded. Use when diagnostics are disabled.
    pub fn disabled() -> Self {
        // Create a channel that we immediately drop the receiver for
        let (sender, _receiver) = mpsc::channel(1);
        Self {
            sender,
            global_drops: Arc::new(AtomicU64::new(0)),
            global_emits: Arc::new(AtomicU64::new(0)),
            run_tracker: Arc::new(RunTracker::new()),
            config: DiagnosticsConfig::default(),
            is_enabled: false,
        }
    }

    /// Check if diagnostics are enabled.
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Get the configured diagnostic level.
    pub fn level(&self) -> DiagLevel {
        self.config.level
    }

    /// Get the configuration.
    pub fn config(&self) -> &DiagnosticsConfig {
        &self.config
    }

    /// Emit a diagnostic event (non-blocking).
    ///
    /// If the channel is full, the event is dropped and counters are incremented.
    /// This method NEVER blocks the calling thread.
    ///
    /// # Level Filtering
    ///
    /// Events are filtered based on the configured level before sending.
    pub fn emit(&self, envelope: DiagEnvelope) -> Result<(), DiagError> {
        // Short-circuit if disabled
        if !self.is_enabled {
            return Ok(());
        }

        // Apply level filtering
        if !self.should_emit(&envelope) {
            return Ok(());
        }

        // Clone run_id before moving envelope
        let run_id = envelope.run_id.as_str().to_string();

        // Check per-run limit
        if self.run_tracker.event_count(&run_id) >= self.config.max_events_per_run as u64 {
            self.run_tracker.increment_drops(&run_id);
            self.global_drops.fetch_add(1, Ordering::Relaxed);
            return Ok(()); // Silently drop, not an error
        }

        // Attempt non-blocking send
        match self.sender.try_send(envelope) {
            Ok(()) => {
                self.global_emits.fetch_add(1, Ordering::Relaxed);
                self.run_tracker.increment_events(&run_id);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Channel full - increment drop counters
                self.global_drops.fetch_add(1, Ordering::Relaxed);
                self.run_tracker.increment_drops(&run_id);
                warn!(run_id = %run_id, "Diagnostic event dropped: channel full");
                Ok(()) // Not an error - intentional drop
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(DiagError::ChannelClosed),
        }
    }

    /// Start a new diagnostic run.
    ///
    /// Initializes per-run tracking. Call at the beginning of each request.
    pub fn start_run(&self, run_id: &DiagRunId) {
        if self.is_enabled {
            self.run_tracker.start_run(run_id.as_str());
        }
    }

    /// End a diagnostic run and get the summary.
    ///
    /// Returns the number of events emitted and dropped for this run.
    pub fn end_run(&self, run_id: &DiagRunId) -> RunSummary {
        if self.is_enabled {
            self.run_tracker.end_run(run_id.as_str())
        } else {
            RunSummary::default()
        }
    }

    /// Get global metrics snapshot.
    pub fn metrics(&self) -> DiagMetrics {
        DiagMetrics {
            global_emits: self.global_emits.load(Ordering::Relaxed),
            global_drops: self.global_drops.load(Ordering::Relaxed),
            active_runs: self.run_tracker.active_run_count(),
        }
    }

    /// Get the run tracker (for background writer).
    pub fn run_tracker(&self) -> Arc<RunTracker> {
        Arc::clone(&self.run_tracker)
    }

    /// Check if an event should be emitted based on level filtering.
    fn should_emit(&self, envelope: &DiagEnvelope) -> bool {
        match self.config.level {
            DiagLevel::Off => false,
            DiagLevel::Errors => self.is_error_event(&envelope.payload),
            DiagLevel::Stages => self.is_stage_event(&envelope.payload),
            DiagLevel::Router => {
                self.is_stage_event(&envelope.payload) || self.is_router_event(&envelope.payload)
            }
            DiagLevel::Tokens => true, // All events
        }
    }

    /// Check if event is an error event.
    fn is_error_event(&self, event: &DiagEvent) -> bool {
        matches!(event, DiagEvent::StageFailed { .. })
    }

    /// Check if event is a stage event (includes errors).
    fn is_stage_event(&self, event: &DiagEvent) -> bool {
        matches!(
            event,
            DiagEvent::StageEnter { .. }
                | DiagEvent::StageComplete { .. }
                | DiagEvent::StageFailed { .. }
        )
    }

    /// Check if event is a router event.
    fn is_router_event(&self, event: &DiagEvent) -> bool {
        matches!(
            event,
            DiagEvent::RouterDecisionMade { .. }
                | DiagEvent::AdapterResolved { .. }
                | DiagEvent::PolicyCheckResult { .. }
                // Fine-grained router decision events
                | DiagEvent::RoutingStart { .. }
                | DiagEvent::GateComputed { .. }
                | DiagEvent::KsparseSelected { .. }
                | DiagEvent::TieBreakApplied { .. }
                | DiagEvent::RoutingEnd { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagSeverity, DiagStage};
    use crate::tracing::TraceContext;
    use adapteros_core::B3Hash;

    fn make_envelope(event: DiagEvent) -> DiagEnvelope {
        let trace_ctx = TraceContext::new_root();
        let run_id = DiagRunId::from_trace_context(&trace_ctx);
        DiagEnvelope::new(
            &trace_ctx,
            "tenant-123",
            run_id,
            DiagSeverity::Info,
            1000,
            event,
        )
    }

    #[test]
    fn test_disabled_service() {
        let service = DiagnosticsService::disabled();
        assert!(!service.is_enabled());

        let envelope = make_envelope(DiagEvent::StageEnter {
            stage: DiagStage::RequestValidation,
        });

        // Should not error even when disabled
        assert!(service.emit(envelope).is_ok());
    }

    #[tokio::test]
    async fn test_enabled_service() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Tokens,
            channel_capacity: 10,
            ..Default::default()
        };

        let (service, mut receiver) = DiagnosticsService::new(config);
        assert!(service.is_enabled());

        let run_id = DiagRunId::new_random();
        service.start_run(&run_id);

        let trace_ctx = TraceContext::new_root();
        let envelope = DiagEnvelope::new(
            &trace_ctx,
            "tenant-123",
            run_id.clone(),
            DiagSeverity::Info,
            1000,
            DiagEvent::StageEnter {
                stage: DiagStage::RequestValidation,
            },
        );

        service.emit(envelope).unwrap();

        let received = receiver.recv().await;
        assert!(received.is_some());

        let summary = service.end_run(&run_id);
        assert_eq!(summary.events_emitted, 1);
        assert_eq!(summary.events_dropped, 0);
    }

    #[test]
    fn test_level_filtering_errors() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Errors,
            channel_capacity: 10,
            ..Default::default()
        };

        let (service, _receiver) = DiagnosticsService::new(config);

        // StageEnter should be filtered out
        let enter = make_envelope(DiagEvent::StageEnter {
            stage: DiagStage::RequestValidation,
        });
        assert!(!service.should_emit(&enter));

        // StageFailed should pass
        let failed = make_envelope(DiagEvent::StageFailed {
            stage: DiagStage::RequestValidation,
            error_code: "E1001".to_string(),
            error_hash: B3Hash::hash(b"error"),
        });
        assert!(service.should_emit(&failed));
    }

    #[test]
    fn test_level_filtering_stages() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Stages,
            channel_capacity: 10,
            ..Default::default()
        };

        let (service, _receiver) = DiagnosticsService::new(config);

        // StageEnter should pass
        let enter = make_envelope(DiagEvent::StageEnter {
            stage: DiagStage::RequestValidation,
        });
        assert!(service.should_emit(&enter));

        // StageComplete should pass
        let complete = make_envelope(DiagEvent::StageComplete {
            stage: DiagStage::RequestValidation,
            duration_us: 1000,
        });
        assert!(service.should_emit(&complete));

        // RouterDecisionMade should be filtered out
        let router = make_envelope(DiagEvent::RouterDecisionMade {
            candidate_count: 5,
            selected_count: 2,
            decision_chain_hash: B3Hash::hash(b"chain"),
        });
        assert!(!service.should_emit(&router));
    }

    #[test]
    fn test_level_filtering_router() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Router,
            channel_capacity: 10,
            ..Default::default()
        };

        let (service, _receiver) = DiagnosticsService::new(config);

        // Stage events should pass
        let enter = make_envelope(DiagEvent::StageEnter {
            stage: DiagStage::RequestValidation,
        });
        assert!(service.should_emit(&enter));

        // Router events should pass
        let router = make_envelope(DiagEvent::RouterDecisionMade {
            candidate_count: 5,
            selected_count: 2,
            decision_chain_hash: B3Hash::hash(b"chain"),
        });
        assert!(service.should_emit(&router));

        // InferenceTiming should be filtered out
        let timing = make_envelope(DiagEvent::InferenceTiming {
            ttft_us: 1000,
            total_us: 5000,
            token_count: 100,
        });
        assert!(!service.should_emit(&timing));
    }

    #[test]
    fn test_level_filtering_tokens() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Tokens,
            channel_capacity: 10,
            ..Default::default()
        };

        let (service, _receiver) = DiagnosticsService::new(config);

        // All events should pass
        let timing = make_envelope(DiagEvent::InferenceTiming {
            ttft_us: 1000,
            total_us: 5000,
            token_count: 100,
        });
        assert!(service.should_emit(&timing));

        let custom = make_envelope(DiagEvent::Custom {
            name: "test".to_string(),
            data_hash: B3Hash::hash(b"data"),
        });
        assert!(service.should_emit(&custom));
    }

    #[tokio::test]
    async fn test_channel_full_drops() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Tokens,
            channel_capacity: 1, // Very small channel
            ..Default::default()
        };

        let (service, _receiver) = DiagnosticsService::new(config);

        let run_id = DiagRunId::new_random();
        service.start_run(&run_id);

        // Fill the channel
        for i in 0..5 {
            let trace_ctx = TraceContext::new_root();
            let envelope = DiagEnvelope::new(
                &trace_ctx,
                "tenant-123",
                run_id.clone(),
                DiagSeverity::Info,
                i * 1000,
                DiagEvent::StageEnter {
                    stage: DiagStage::RequestValidation,
                },
            );
            // Should not error
            assert!(service.emit(envelope).is_ok());
        }

        let metrics = service.metrics();
        // At least some should have been dropped (channel size 1, sent 5)
        assert!(metrics.global_drops > 0);
    }

    #[test]
    fn test_max_events_per_run() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Tokens,
            channel_capacity: 100,
            max_events_per_run: 3, // Very small limit
            ..Default::default()
        };

        let (service, _receiver) = DiagnosticsService::new(config);

        let run_id = DiagRunId::new_random();
        service.start_run(&run_id);

        // Send more than max_events_per_run
        for i in 0..10 {
            let trace_ctx = TraceContext::new_root();
            let envelope = DiagEnvelope::new(
                &trace_ctx,
                "tenant-123",
                run_id.clone(),
                DiagSeverity::Info,
                i * 1000,
                DiagEvent::StageEnter {
                    stage: DiagStage::RequestValidation,
                },
            );
            service.emit(envelope).unwrap();
        }

        let summary = service.end_run(&run_id);
        // Should only have emitted max_events_per_run
        assert_eq!(summary.events_emitted, 3);
        // Rest should be dropped
        assert_eq!(summary.events_dropped, 7);
    }

    #[test]
    fn test_metrics() {
        let config = DiagnosticsConfig {
            enabled: true,
            level: DiagLevel::Tokens,
            channel_capacity: 100,
            ..Default::default()
        };

        let (service, _receiver) = DiagnosticsService::new(config);

        let run_id1 = DiagRunId::new_random();
        let run_id2 = DiagRunId::new_random();

        service.start_run(&run_id1);
        service.start_run(&run_id2);

        let metrics = service.metrics();
        assert_eq!(metrics.active_runs, 2);
        assert_eq!(metrics.global_emits, 0);
        assert_eq!(metrics.global_drops, 0);

        service.end_run(&run_id1);

        let metrics = service.metrics();
        assert_eq!(metrics.active_runs, 1);
    }

    #[test]
    fn test_diag_level_from_str() {
        assert_eq!(DiagLevel::from_str_lossy("off"), DiagLevel::Off);
        assert_eq!(DiagLevel::from_str_lossy("OFF"), DiagLevel::Off);
        assert_eq!(DiagLevel::from_str_lossy("errors"), DiagLevel::Errors);
        assert_eq!(DiagLevel::from_str_lossy("STAGES"), DiagLevel::Stages);
        assert_eq!(DiagLevel::from_str_lossy("Router"), DiagLevel::Router);
        assert_eq!(DiagLevel::from_str_lossy("tokens"), DiagLevel::Tokens);
        assert_eq!(DiagLevel::from_str_lossy("invalid"), DiagLevel::Off);
    }
}
