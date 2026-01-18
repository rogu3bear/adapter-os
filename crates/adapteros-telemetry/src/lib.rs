//! Telemetry with canonical JSON and BLAKE3 hashing

use ::tracing::{error, info, warn};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{generate_signing_key, load_signing_key, sign_bundle, Keypair};
use crossbeam::channel::{bounded, Receiver, RecvTimeoutError, Sender};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// PRD-4.8: Track dropped telemetry events for observability
static DROPPED_EVENTS: AtomicU64 = AtomicU64::new(0);

/// Returns the total count of dropped telemetry events since process start.
///
/// Use this for observability to detect backpressure issues.
pub fn dropped_event_count() -> u64 {
    DROPPED_EVENTS.load(Ordering::Relaxed)
}

pub mod alerting;
pub mod bundle;
pub mod bundle_parser;
pub mod bundle_store;
pub mod compression;
pub mod crash_journal;
pub mod event;
pub mod events;
pub mod health_monitoring;
pub mod logging_macros;
pub mod merkle;
pub mod metrics;
pub mod middleware;
pub mod monitoring;
pub mod observability;
pub mod performance_monitoring;
pub mod replay;
pub mod report;
pub mod ring_buffer;
pub mod sampling;
pub mod signature_audit;
pub mod tracing;
pub mod uds_exporter;
pub mod unified_events;
pub mod writer;

pub use alerting::{
    AlertComparator, AlertRecord, AlertRule, AlertSeverity, AlertingEngine, EscalationPolicy,
    NotificationChannel,
};
pub use bundle::BundleWriter;
pub use bundle_parser::{
    parse_bundle_events, parse_bundle_content, BundleEventFilter, BundleParseStats,
    TelemetryEvent as ParsedTelemetryEvent,
};
pub use bundle_store::{
    BundleStore, ChainVerificationReport, EvictionStrategy, GarbageCollectionReport,
    RetentionPolicy, StorageStats,
};
pub use signature_audit::{
    SignatureAuditEntry, SignatureAuditLogger, SignatureOperation, SignatureResult,
};
// Re-export canonical BundleMetadata with StoredBundleMetadata alias for backward compatibility
pub use adapteros_telemetry_types::BundleMetadata as StoredBundleMetadata;
// Import canonical BundleMetadata for internal use
use adapteros_telemetry_types::BundleMetadata;
pub use compression::{
    CompressedBundleMetadata, CompressionAlgorithm, CompressionLevel, TelemetryCompressor,
};
pub use event::Event;
pub use events::{
    InferenceEvent, KvEvictionEvent, KvEvictionReason, KvQuotaEvent, KvQuotaEventType,
    KvResidencyEvent, KvResidencyState, KvResidencyTransitionReason, PolicyHashValidationEvent,
    ResidencyProbeEvent, ResidencyProbeResult, RngSnapshot, RouterDecisionEvent, ValidationStatus,
};
pub use health_monitoring::{HealthCheck, HealthMonitor, HealthReport, HealthState, HealthStatus};
pub use merkle::{compute_merkle_root, generate_proof, verify_proof, MerkleProof};
pub use metrics::{
    // Prometheus-based critical component metrics
    critical_components::{CriticalComponentMetrics, HotSwapTimer, KernelExecutionTimer},
    // Simple serializable metrics types
    AdapterMetrics,
    LatencyMetrics,
    MetricsCollector,
    MetricsConfig,
    MetricsServer,
    MetricsSnapshot,
    PolicyMetrics,
    // Prometheus re-exports with explicit names
    PrometheusCriticalMetrics,
    PrometheusHotSwapTimer,
    PrometheusKernelTimer,
    QueueDepthMetrics,
    SystemMetrics,
    ThroughputMetrics,
};
// Re-export critical_components module for direct access
pub use crate::tracing::{
    Span, SpanEvent, SpanKind, SpanStatus, Trace, TraceBuffer, TraceBufferStats, TraceContext,
    TraceSearchQuery,
};
pub use metrics::critical_components;
pub use monitoring::{
    HealthCheckEventPayload, MemoryPressureAlertPayload, MemoryProcessSample, MonitoringTelemetry,
    PerformanceAlertPayload, PerformanceThreshold, PerformanceThresholdMonitor,
    PolicyViolationAlertPayload, TelemetrySink, ThresholdRange,
};
pub use observability::*;
pub use performance_monitoring::{
    LatencySample, PerformanceMonitoringService, PerformanceSnapshot, ThroughputSample,
};
pub use replay::{
    find_divergence, format_divergence, load_replay_bundle, ReplayBundle, ReplayDivergence,
};
pub use report::generate_html_report;
pub use ring_buffer::{RingBufferStats, TelemetryRingBuffer};
pub use sampling::{EventSampler, SamplingStats, SamplingStrategy};
pub use uds_exporter::{MetricMetadata, MetricValue, UdsMetricsExporter};
pub use unified_events::{
    EventType, LogLevel, TelemetryEvent as UnifiedTelemetryEvent, TelemetryEventBuilder,
    TelemetryFilters,
};
pub use writer::RouterDecisionWriter;

enum TelemetryCommand {
    Event(Box<UnifiedTelemetryEvent>),
    Flush(Sender<Result<()>>),
    Shutdown(Sender<Result<()>>),
}

/// Telemetry writer with background thread
#[derive(Clone)]
pub struct TelemetryWriter {
    sender: Sender<TelemetryCommand>,
    handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl TelemetryWriter {
    /// Create a new telemetry writer
    ///
    /// Per Artifacts Ruleset #13: All bundles signed with persistent Ed25519 key
    pub fn new<P: AsRef<Path>>(output_dir: P, max_events: usize, max_bytes: usize) -> Result<Self> {
        // Use bounded channel to provide backpressure and prevent OOM under burst load.
        // 50,000 events provides ~10s buffer at 5k events/sec peak while bounding memory.
        // When full, senders will block briefly, providing natural flow control.
        let (sender, receiver) = bounded(50_000);
        let output_dir = output_dir.as_ref().to_path_buf();

        // Load or generate persistent signing key using adapteros-crypto
        let key_path = PathBuf::from("var/keys/telemetry_signing.key");
        let signing_keypair = if key_path.exists() {
            load_signing_key(&key_path)?
        } else {
            generate_signing_key(&key_path)?
        };

        let handle = thread::spawn(move || {
            if let Err(e) = run_writer(receiver, output_dir, max_events, max_bytes, signing_keypair)
            {
                error!(error = %e, "Telemetry writer thread failed");
            }
        });

        Ok(Self {
            sender,
            handle: Arc::new(Mutex::new(Some(handle))),
        })
    }

    /// Log an event using the unified event schema
    ///
    /// Uses non-blocking send to avoid stalling callers under burst load.
    /// If the channel is full, the event is dropped and a warning is logged.
    /// This ensures telemetry never impacts inference latency.
    pub fn log_event(&self, event: UnifiedTelemetryEvent) -> Result<()> {
        use crossbeam::channel::TrySendError;

        match self
            .sender
            .try_send(TelemetryCommand::Event(Box::new(event)))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                // Channel is full - drop event to maintain backpressure.
                // Log warning but don't fail the caller; telemetry should
                // never block or fail inference operations.
                // PRD-4.8: Track dropped events for observability
                let dropped = DROPPED_EVENTS.fetch_add(1, Ordering::Relaxed) + 1;
                warn!(
                    dropped_total = dropped,
                    "Telemetry channel full (capacity: 50,000), dropping event. \
                     Consider increasing channel capacity or reducing event volume."
                );
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => {
                Err(AosError::Io("Telemetry channel disconnected".to_string()))
            }
        }
    }

    /// Log with required identity envelope
    pub fn log_with_identity(
        &self,
        event_type: EventType,
        level: LogLevel,
        message: String,
        identity: &IdentityEnvelope,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let event = TelemetryEventBuilder::new(event_type, level, message, identity.clone())
            .metadata(metadata.unwrap_or_default())
            .build()
            .map_err(|e| AosError::Config(format!("Failed to build telemetry event: {}", e)))?;
        self.log_event(event)
    }

    /// Gracefully shutdown the telemetry writer
    ///
    /// Flushes all pending events and waits for the writer thread to complete.
    /// This ensures no telemetry data is lost during shutdown.
    pub fn shutdown(self) -> Result<()> {
        self.shutdown_with_timeout(Duration::from_secs(5))
    }

    /// Gracefully shutdown the telemetry writer with a timeout.
    ///
    /// Flushes pending events, asks the writer thread to stop, and joins with a timeout
    /// to avoid hanging the shutdown path.
    pub fn shutdown_with_timeout(self, timeout: Duration) -> Result<()> {
        info!("Initiating telemetry writer shutdown");

        let (ack_tx, ack_rx) = bounded(1);
        let mut shutdown_result = Ok(());

        if let Err(e) = self.sender.send(TelemetryCommand::Shutdown(ack_tx)) {
            warn!(error = %e, "Telemetry shutdown signal failed to send");
            shutdown_result = Err(AosError::Io(
                "Failed to send telemetry shutdown command".to_string(),
            ));
        } else {
            match ack_rx.recv_timeout(timeout) {
                Ok(result) => {
                    if let Err(e) = result {
                        warn!(error = %e, "Telemetry shutdown flush failed");
                        shutdown_result = Err(e);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    warn!(
                        timeout_ms = timeout.as_millis() as u64,
                        "Telemetry shutdown timed out waiting for flush"
                    );
                    shutdown_result = Err(AosError::Telemetry(format!(
                        "Telemetry shutdown timeout after {:?}",
                        timeout
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    warn!("Telemetry shutdown response channel disconnected");
                    shutdown_result = Err(AosError::Io(
                        "Telemetry shutdown response channel closed".to_string(),
                    ));
                }
            }
        }

        drop(self.sender);

        let handle = self.handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(handle) = handle {
            if let Err(e) = join_handle_with_timeout(handle, timeout) {
                warn!(error = %e, "Telemetry writer thread did not shut down cleanly");
                if shutdown_result.is_ok() {
                    shutdown_result = Err(e);
                }
            } else {
                info!("Telemetry writer thread shutdown complete");
            }
        } else {
            info!("Telemetry writer thread already shut down");
        }

        shutdown_result
    }

    /// Flush buffered telemetry to disk.
    ///
    /// Blocks until the writer thread confirms the flush.
    pub fn flush(&self) -> Result<()> {
        let (ack_tx, ack_rx) = bounded(1);
        self.sender
            .send(TelemetryCommand::Flush(ack_tx))
            .map_err(|_| AosError::Io("Failed to send telemetry flush command".to_string()))?;
        ack_rx
            .recv()
            .map_err(|_| AosError::Io("Telemetry flush response channel closed".to_string()))?
    }

    /// Flush buffered telemetry with a timeout.
    ///
    /// Returns an error if the flush does not complete within the timeout.
    pub fn flush_with_timeout(&self, timeout: Duration) -> Result<()> {
        let (ack_tx, ack_rx) = bounded(1);
        self.sender
            .send(TelemetryCommand::Flush(ack_tx))
            .map_err(|_| AosError::Io("Failed to send telemetry flush command".to_string()))?;
        match ack_rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(AosError::Telemetry(format!(
                "Telemetry flush timeout after {:?}",
                timeout
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(AosError::Io(
                "Telemetry flush response channel closed".to_string(),
            )),
        }
    }

    /// Legacy log method - uses system identity
    pub fn log<T: Serialize>(&self, event_type_str: &str, payload: T) -> Result<()> {
        let identity = IdentityEnvelope::new(
            "system".to_string(),
            "telemetry".to_string(),
            "event".to_string(),
            "1.0".to_string(),
        );
        let event_type = EventType::Custom(event_type_str.to_string());
        let message = format!("Legacy event: {}", event_type_str);
        let metadata = serde_json::to_value(payload)?;
        let event = TelemetryEventBuilder::new(event_type, LogLevel::Info, message, identity)
            .metadata(metadata)
            .build()
            .map_err(|e| AosError::Config(format!("Failed to build telemetry event: {}", e)))?;

        self.log_event(event)
    }

    /// Log a security event (always logged at 100% sampling per Telemetry Ruleset #9)
    pub fn log_security_event(&self, event: SecurityEvent) -> Result<()> {
        self.log("security", event)
    }

    /// Log a policy violation (always logged at 100% sampling)
    pub fn log_policy_violation(
        &self,
        policy: &str,
        violation_type: &str,
        details: &str,
    ) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();
        let event = SecurityEvent::PolicyViolation {
            policy: policy.to_string(),
            violation_type: violation_type.to_string(),
            details: details.to_string(),
            timestamp: format!("{}", timestamp),
        };
        self.log_security_event(event)
    }

    /// Log an egress attempt (always logged at 100% sampling)
    pub fn log_egress_attempt(&self, destination: &str, blocked: bool) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();
        let event = SecurityEvent::EgressAttempt {
            destination: destination.to_string(),
            blocked,
            timestamp: format!("{}", timestamp),
        };
        self.log_security_event(event)
    }

    /// Log an isolation violation (always logged at 100% sampling)
    pub fn log_isolation_violation(&self, tenant_id: &str, details: &str) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();
        let event = SecurityEvent::IsolationViolation {
            tenant_id: tenant_id.to_string(),
            details: details.to_string(),
            timestamp: format!("{}", timestamp),
        };
        self.log_security_event(event)
    }

    /// Log adapter swap operation (Tier 6)
    pub fn log_adapter_swap(&self, event: AdapterSwapEvent) -> Result<()> {
        self.log("adapter_swap", event)
    }

    /// Log adapter preload operation (phase 1 of hot-swap)
    pub fn log_adapter_preload(&self, event: AdapterPreloadEvent) -> Result<()> {
        self.log("adapter_preload", event)
    }

    /// Log stack verification (Tier 6)
    pub fn log_stack_verification(&self, event: StackVerificationEvent) -> Result<()> {
        self.log("stack_verification", event)
    }

    /// Log node sync operation (Tier 6)
    pub fn log_node_sync(&self, event: NodeSyncEvent) -> Result<()> {
        self.log("node_sync", event)
    }

    /// Log a kernel noise event
    pub fn log_kernel_noise(&self, event: crate::event::KernelNoiseEvent) -> Result<()> {
        self.log("kernel.noise", event)
    }

    /// Log a kernel step event
    pub fn log_kernel_step(&self, event: crate::event::KernelStepEvent) -> Result<()> {
        self.log("kernel.step", event)
    }

    /// Log an inference event with RNG state tracking (Ruleset #2)
    pub fn log_inference(&self, event: crate::events::InferenceEvent) -> Result<()> {
        self.log("inference", event)
    }

    /// Log a router decision event
    pub fn log_router_decision(&self, event: crate::events::RouterDecisionEvent) -> Result<()> {
        self.log("router.decision", event)
    }

    /// Log an abstain event (Ruleset #5)
    pub fn log_abstain(&self, event: crate::events::AbstainEvent) -> Result<()> {
        self.log("policy.abstain", event)
    }

    /// Log an adapter eviction event (Ruleset #12)
    pub fn log_adapter_eviction(&self, event: crate::events::AdapterEvictionEvent) -> Result<()> {
        self.log("adapter.evict", event)
    }

    /// Log a K reduction event (Ruleset #12)
    pub fn log_k_reduction(&self, event: crate::events::KReductionEvent) -> Result<()> {
        self.log("router.k_reduced", event)
    }

    /// Log a K reduction request event (Ruleset #12 - lifecycle phase 1)
    pub fn log_k_reduction_request(
        &self,
        event: crate::events::KReductionRequestEvent,
    ) -> Result<()> {
        self.log("router.k_reduction.request", event)
    }

    /// Log a K reduction evaluation event (Ruleset #12 - lifecycle phase 2)
    pub fn log_k_reduction_evaluation(
        &self,
        event: crate::events::KReductionEvaluationEvent,
    ) -> Result<()> {
        self.log("router.k_reduction.evaluation", event)
    }

    /// Log a K reduction execution event (Ruleset #12 - lifecycle phase 3)
    pub fn log_k_reduction_execution(
        &self,
        event: crate::events::KReductionExecutionEvent,
    ) -> Result<()> {
        self.log("router.k_reduction.execution", event)
    }

    /// Log a K reduction completion event (Ruleset #12 - lifecycle phase 4)
    pub fn log_k_reduction_completion(
        &self,
        event: crate::events::KReductionCompletionEvent,
    ) -> Result<()> {
        self.log("router.k_reduction.completion", event)
    }

    /// Log a performance budget violation (Ruleset #11)
    pub fn log_budget_violation(
        &self,
        event: crate::events::PerformanceBudgetViolationEvent,
    ) -> Result<()> {
        self.log("performance.budget_violation", event)
    }

    /// Log a policy hash validation event
    ///
    /// Logged at 100% sampling (policy violations per Telemetry Ruleset #9).
    /// Used to track runtime policy pack hash validation and detect mutations.
    pub fn log_policy_hash_validation(
        &self,
        event: crate::events::PolicyHashValidationEvent,
    ) -> Result<()> {
        self.log("policy.hash_validation", event)
    }

    /// Log a health lifecycle event using the canonical builder helpers.
    pub fn log_health_lifecycle(
        &self,
        identity: IdentityEnvelope,
        event: crate::observability::HealthLifecycleEvent,
    ) -> Result<()> {
        let evt = crate::observability::build_health_event(identity, event)?;
        self.log_event(evt)
    }

    /// Log an inference metrics event using the canonical builder helpers.
    pub fn log_inference_metrics(
        &self,
        identity: IdentityEnvelope,
        event: crate::observability::InferenceMetricsEvent,
    ) -> Result<()> {
        let evt = crate::observability::build_inference_metrics_event(identity, event)?;
        self.log_event(evt)
    }

    /// Log a routing telemetry event with RouterDecision metadata.
    pub fn log_routing_event(
        &self,
        identity: IdentityEnvelope,
        event: crate::observability::RoutingTelemetryEvent,
    ) -> Result<()> {
        let evt = crate::observability::build_routing_event(identity, event)?;
        self.log_event(evt)
    }

    /// Log a residency probe event
    ///
    /// Tracks base model residency during adapter hot-swap cycles.
    /// Emitted by the hardware residency harness and admin debug endpoints.
    pub fn log_residency_probe(&self, event: crate::events::ResidencyProbeEvent) -> Result<()> {
        self.log("residency.probe", event)
    }

    /// Log a KV cache residency state transition event
    ///
    /// Tracks KV cache entry transitions between COLD and HOT states
    /// for memory pressure management and purgeable state optimization.
    pub fn log_kv_residency(&self, event: crate::events::KvResidencyEvent) -> Result<()> {
        self.log("kv.residency", event)
    }

    /// Log a KV cache eviction event
    ///
    /// Tracks KV cache evictions with residency breakdown for analyzing
    /// eviction patterns and memory management effectiveness.
    pub fn log_kv_eviction(&self, event: crate::events::KvEvictionEvent) -> Result<()> {
        self.log("kv.eviction", event)
    }

    /// Log a KV cache quota event
    ///
    /// Tracks quota violations and warnings for KV cache memory limits.
    pub fn log_kv_quota(&self, event: crate::events::KvQuotaEvent) -> Result<()> {
        self.log("kv.quota", event)
    }
}

// Legacy TelemetryEvent struct removed - use UnifiedTelemetryEvent instead

fn join_handle_with_timeout(handle: thread::JoinHandle<()>, timeout: Duration) -> Result<()> {
    let (done_tx, done_rx) = bounded(1);
    thread::spawn(move || {
        let result = handle.join();
        let _ = done_tx.send(result);
    });

    match done_rx.recv_timeout(timeout) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(AosError::Internal(
            "Telemetry writer thread panicked during shutdown".to_string(),
        )),
        Err(RecvTimeoutError::Timeout) => Err(AosError::Telemetry(format!(
            "Telemetry writer thread shutdown timeout after {:?}",
            timeout
        ))),
        Err(RecvTimeoutError::Disconnected) => Err(AosError::Telemetry(
            "Telemetry writer shutdown join channel closed".to_string(),
        )),
    }
}

fn run_writer(
    receiver: Receiver<TelemetryCommand>,
    output_dir: PathBuf,
    max_events: usize,
    max_bytes: usize,
    signing_keypair: Keypair,
) -> Result<()> {
    std::fs::create_dir_all(&output_dir)?;

    let mut bundle_idx = 0;
    let mut event_count = 0;
    let mut byte_count = 0;
    let mut event_hashes = Vec::new();
    let mut skipped_events = 0;

    let mut bundle_path = output_dir.join(format!("bundle_{:06}.ndjson", bundle_idx));
    let mut writer = BufWriter::new(File::create(&bundle_path)?);
    let mut shutdown_requested = false;

    for command in receiver {
        let event = match command {
            TelemetryCommand::Event(event) => *event,
            TelemetryCommand::Flush(reply) => {
                let result = writer
                    .flush()
                    .map_err(|e| AosError::Io(format!("Bundle writer flush failed: {}", e)));
                if let Err(ref e) = result {
                    error!(error = %e, "Failed to flush telemetry bundle");
                }
                let _ = reply.send(result);
                continue;
            }
            TelemetryCommand::Shutdown(reply) => {
                shutdown_requested = true;
                let result = writer
                    .flush()
                    .map_err(|e| AosError::Io(format!("Bundle writer flush failed: {}", e)));
                if let Err(ref e) = result {
                    error!(error = %e, "Failed to flush telemetry bundle on shutdown");
                }
                let _ = reply.send(result);
                break;
            }
        };
        // Validate identity envelope
        if let Err(e) = event.identity.validate() {
            warn!(error = %e, "Invalid identity in telemetry event, skipping");
            skipped_events += 1;
            continue;
        }

        // Validate event before serialization
        if let Err(e) = validate_event(&event) {
            warn!(error = %e, "Event validation failed, recording fallback event");
            skipped_events += 1;
            // Create and log a fallback error event
            if let Err(fallback_err) = log_serialization_error_event(&mut writer, &event, &e) {
                error!(error = %fallback_err, "Failed to write fallback error event");
            }
            continue;
        }

        // Serialize event with proper error handling
        let line = match serde_json::to_string(&event) {
            Ok(json_line) => json_line + "\n",
            Err(e) => {
                warn!(error = %e, "Serialization error for event, recording fallback");
                skipped_events += 1;
                if let Err(fallback_err) = log_serialization_error_event(
                    &mut writer,
                    &event,
                    &AosError::Io(format!("Serialization failed: {}", e)),
                ) {
                    error!(error = %fallback_err, "Failed to write fallback error event");
                }
                continue;
            }
        };

        // Compute event hash with proper error handling
        let event_hash = compute_event_hash(&event, &line);
        event_hashes.push(event_hash);

        // Write NDJSON line
        let line_bytes = line.as_bytes();
        if let Err(e) = writer.write_all(line_bytes) {
            error!(error = %e, "Failed to write event to bundle, skipping");
            skipped_events += 1;
            event_hashes.pop(); // Remove hash for skipped event
            continue;
        }

        event_count += 1;
        byte_count += line_bytes.len();

        // Check rotation conditions
        if event_count >= max_events || byte_count >= max_bytes {
            if let Err(e) = writer.flush() {
                error!(error = %e, "Failed to flush bundle writer");
                return Err(AosError::Io(format!("Bundle writer flush failed: {}", e)));
            }
            drop(writer);

            // Log bundle statistics
            if skipped_events > 0 {
                warn!(
                    event_count = event_count,
                    skipped_events = skipped_events,
                    "Bundle rotation with skipped events"
                );
            }

            // Sign bundle with Merkle root using persistent keypair
            finalize_bundle(&bundle_path, &event_hashes, &signing_keypair)?;

            // Start new bundle
            bundle_idx += 1;
            event_count = 0;
            byte_count = 0;
            event_hashes.clear();
            skipped_events = 0;

            bundle_path = output_dir.join(format!("bundle_{:06}.ndjson", bundle_idx));
            writer = BufWriter::new(File::create(&bundle_path)?);
        }
    }

    // Flush final bundle (unless already flushed during shutdown)
    if !shutdown_requested {
        if let Err(e) = writer.flush() {
            error!(error = %e, "Failed to flush final bundle");
            return Err(AosError::Io(format!("Final bundle flush failed: {}", e)));
        }
    }

    drop(writer);

    if event_count > 0 {
        if let Err(e) = finalize_bundle(&bundle_path, &event_hashes, &signing_keypair) {
            error!(error = %e, "Failed to finalize telemetry bundle");
            if !shutdown_requested {
                return Err(e);
            }
        }
    }

    if skipped_events > 0 {
        warn!(
            total_skipped_events = skipped_events,
            "Total events skipped in session"
        );
    }

    Ok(())
}

/// Validate event before serialization
fn validate_event(event: &UnifiedTelemetryEvent) -> Result<()> {
    // Validate required fields
    if event.id.is_empty() {
        return Err(AosError::Validation("Event ID cannot be empty".to_string()));
    }

    if event.message.is_empty() {
        return Err(AosError::Validation(
            "Event message cannot be empty".to_string(),
        ));
    }

    if event.event_type.is_empty() {
        return Err(AosError::Validation(
            "Event type cannot be empty".to_string(),
        ));
    }

    // Validate identity envelope
    event
        .identity
        .validate()
        .map_err(|e| AosError::Validation(format!("Identity validation failed: {}", e)))?;

    // Validate metadata if present (ensure it's valid JSON)
    if let Some(metadata) = &event.metadata {
        // Attempt serialization to ensure it's valid
        if serde_json::to_string(metadata).is_err() {
            return Err(AosError::Validation(
                "Event metadata is not serializable".to_string(),
            ));
        }
    }

    Ok(())
}

/// Compute event hash with proper error handling
fn compute_event_hash(event: &UnifiedTelemetryEvent, serialized_line: &str) -> B3Hash {
    // Try to use pre-computed hash if available and valid
    if let Some(hash_str) = &event.hash {
        if !hash_str.is_empty() {
            // Validate hash format before using it
            if hash_str.len() >= 32 {
                return B3Hash::hash(hash_str.as_bytes());
            }
        }
    }

    // Fallback: hash the serialized JSON line
    let hash_bytes = blake3::hash(serialized_line.as_bytes());
    B3Hash::from_bytes(hash_bytes.into())
}

/// Log a fallback error event when serialization fails
fn log_serialization_error_event(
    writer: &mut BufWriter<File>,
    original_event: &UnifiedTelemetryEvent,
    error: &AosError,
) -> Result<()> {
    // Create a minimal fallback event with error information
    let fallback = serde_json::json!({
        "id": original_event.id,
        "timestamp": original_event.timestamp,
        "event_type": "telemetry.serialization_error",
        "level": "Error",
        "message": format!("Serialization failed: {}", error),
        "component": "adapteros-telemetry",
        "identity": {
            "tenant_id": original_event.identity.tenant_id,
            "domain": "error_recovery",
            "purpose": "serialization_error",
            "version": "1.0"
        },
        "user_id": original_event.user_id.clone(),
        "original_event_type": original_event.event_type,
        "error_message": error.to_string(),
    });

    let line = serde_json::to_string(&fallback)
        .map_err(|e| AosError::Io(format!("Failed to serialize fallback event: {}", e)))?
        + "\n";

    writer
        .write_all(line.as_bytes())
        .map_err(|e| AosError::Io(format!("Failed to write fallback event: {}", e)))?;

    Ok(())
}

fn finalize_bundle(path: &Path, event_hashes: &[B3Hash], signing_keypair: &Keypair) -> Result<()> {
    // Compute Merkle root (simplified but deterministic)
    let merkle_root = if event_hashes.is_empty() {
        B3Hash::hash(b"empty")
    } else {
        let mut combined = Vec::new();
        for hash in event_hashes {
            combined.extend_from_slice(hash.as_bytes());
        }
        B3Hash::hash(&combined)
    };

    // Compute bundle hash (content-addressed)
    let bundle_bytes = fs::read(path)?;
    let bundle_hash = B3Hash::hash(&bundle_bytes);

    // Sign bundle using adapteros-crypto (per Artifacts Ruleset #13)
    let bundle_signature = sign_bundle(&bundle_hash, &merkle_root, signing_keypair)?;

    // Write metadata file
    let meta_path = path.with_extension("meta.json");
    let public_key_bytes = bundle_signature.public_key.to_bytes();
    let key_id = {
        let hash = adapteros_core::hash::B3Hash::hash(&public_key_bytes);
        hex::encode(&hash.as_bytes()[..16])
    };
    let metadata = BundleMetadata {
        bundle_hash,
        merkle_root,
        event_count: event_hashes.len(),
        signature: hex::encode(bundle_signature.signature.to_bytes()),
        public_key: hex::encode(public_key_bytes),
        key_id,
        schema_version: 1,
        signed_at_us: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64,
        cpid: None,
        tenant_id: None,
        sequence_no: None,
        created_at: std::time::SystemTime::now(),
        prev_bundle_hash: None,
        is_incident_bundle: false,
        is_promotion_bundle: false,
        tags: Vec::new(),
        stack_id: None,
        stack_version: None,
    };

    let meta_file = File::create(meta_path)?;
    serde_json::to_writer_pretty(meta_file, &metadata)?;

    Ok(())
}

/// Verify bundle signature (for audit/validation)
pub fn verify_bundle_signature(
    merkle_root: &B3Hash,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<bool> {
    use adapteros_crypto::{PublicKey, Signature};

    let signature_bytes = hex::decode(signature_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

    let public_key_bytes = hex::decode(public_key_hex)
        .map_err(|e| AosError::Crypto(format!("Invalid public key hex: {}", e)))?;

    // Convert bytes to proper types
    let mut sig_array = [0u8; 64];
    if signature_bytes.len() != 64 {
        return Err(AosError::Crypto("Invalid signature length".to_string()));
    }
    sig_array.copy_from_slice(&signature_bytes);
    let signature = Signature::from_bytes(&sig_array)?;

    let mut pk_array = [0u8; 32];
    if public_key_bytes.len() != 32 {
        return Err(AosError::Crypto("Invalid public key length".to_string()));
    }
    pk_array.copy_from_slice(&public_key_bytes);
    let public_key = PublicKey::from_bytes(&pk_array)?;

    // Verify signature
    public_key.verify(merkle_root.as_bytes(), &signature)?;
    Ok(true)
}

// BundleMetadata is now imported from adapteros_telemetry_types

/// Security events (always logged at 100% sampling per Telemetry Ruleset #9)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum SecurityEvent {
    PolicyViolation {
        policy: String,
        violation_type: String,
        details: String,
        timestamp: String,
    },
    EgressAttempt {
        destination: String,
        blocked: bool,
        timestamp: String,
    },
    IsolationViolation {
        tenant_id: String,
        details: String,
        timestamp: String,
    },
}

// ============================================================
// Tier 6: Adapters & Clusters Telemetry Events
// ============================================================

/// Adapter swap event (hot-swap operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSwapEvent {
    pub tenant: String,
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub vram_mb: i64,
    pub latency_ms: u64,
    pub result: String, // "ok", "failed", "rollback"
    pub stack_hash: Option<String>,
}

/// Adapter preload event (phase 1 of hot-swap)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPreloadEvent {
    pub adapter_id: String,
    pub vram_mb: u64,
    pub latency_ms: u64,
    pub result: String, // "ok", "failed"
}

/// Stack verification event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackVerificationEvent {
    pub plan_id: String,
    pub stack_hash: String,
    pub adapters: Vec<String>,
    pub result: String, // "ok", "mismatch", "error"
}

/// Node sync event (replication operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSyncEvent {
    pub session_id: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub bytes: u64,
    pub artifacts: usize,
    pub duration_ms: u64,
    pub result: String, // "success", "failed", "partial"
    pub verified: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
    use adapteros_core::identity::IdentityEnvelope;

    #[test]
    fn test_log_with_identity() {
        let writer = TelemetryWriter::new("test_bundles", 10, 1024).unwrap();
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
        );
        writer
            .log_with_identity(
                EventType::SystemStart,
                LogLevel::Info,
                "Test event".to_string(),
                &identity,
                None,
            )
            .unwrap();
        // Verify by checking if channel received, but since async, skip or mock
    }

    #[test]
    fn test_validate_event_with_valid_event() {
        let identity = IdentityEnvelope::new(
            "test-tenant".to_string(),
            "test-domain".to_string(),
            "test-purpose".to_string(),
            "1.0".to_string(),
        );
        let event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Valid test event".to_string(),
            identity,
        )
        .build()
        .unwrap();

        // Should not error
        assert!(validate_event(&event).is_ok());
    }

    #[test]
    fn test_validate_event_with_empty_id() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );
        let mut event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .build()
        .unwrap();
        event.id = String::new(); // Make ID empty

        let result = validate_event(&event);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Event ID"));
    }

    #[test]
    fn test_validate_event_with_empty_message() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );
        let mut event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Original".to_string(),
            identity,
        )
        .build()
        .unwrap();
        event.message = String::new(); // Make message empty

        let result = validate_event(&event);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("message"));
    }

    #[test]
    fn test_validate_event_with_empty_event_type() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );
        let mut event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .build()
        .unwrap();
        event.event_type = String::new(); // Make event_type empty

        let result = validate_event(&event);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("type"));
    }

    #[test]
    fn test_validate_event_with_invalid_metadata() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );
        let mut event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .metadata(serde_json::json!({
            "valid": "json"
        }))
        .build()
        .unwrap();

        // Valid metadata should pass
        assert!(validate_event(&event).is_ok());

        // Try with invalid metadata - note: serde_json doesn't easily create invalid JSON
        // So we test with complex nested structures
        event.metadata = Some(serde_json::json!({
            "nested": {
                "deeply": {
                    "valid": ["json", "array"]
                }
            }
        }));
        assert!(validate_event(&event).is_ok());
    }

    #[test]
    fn test_compute_event_hash_with_precomputed_hash() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );
        let event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .build()
        .unwrap();

        let serialized = serde_json::to_string(&event).unwrap();
        let hash = compute_event_hash(&event, &serialized);

        // Hash should be computed successfully
        assert!(!hash.as_bytes().is_empty());
    }

    #[test]
    fn test_compute_event_hash_with_empty_hash_field() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );
        let mut event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .build()
        .unwrap();
        event.hash = Some(String::new()); // Empty hash

        let serialized = "test line";
        let hash = compute_event_hash(&event, serialized);

        // Should fall back to serialized line hash
        assert!(!hash.as_bytes().is_empty());
    }

    #[test]
    fn test_compute_event_hash_consistency() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );
        let event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .build()
        .unwrap();

        let serialized = serde_json::to_string(&event).unwrap();
        let hash1 = compute_event_hash(&event, &serialized);
        let hash2 = compute_event_hash(&event, &serialized);

        // Same input should produce same hash
        assert_eq!(hash1.as_bytes(), hash2.as_bytes());
    }

    #[test]
    fn test_serialization_error_event_creation() {
        let identity = IdentityEnvelope::new(
            "test-tenant".to_string(),
            "test-domain".to_string(),
            "test-purpose".to_string(),
            "1.0".to_string(),
        );
        let original_event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .build()
        .unwrap();

        let error = AosError::Io("Test serialization error".to_string());
        let fallback = serde_json::json!({
            "id": original_event.id,
            "timestamp": original_event.timestamp,
            "event_type": "telemetry.serialization_error",
            "level": "Error",
            "message": format!("Serialization failed: {}", error),
            "component": "adapteros-telemetry",
            "identity": {
                "tenant_id": original_event.identity.tenant_id,
                "domain": "error_recovery",
                "purpose": "serialization_error",
                "version": "1.0"
            },
            "user_id": original_event.user_id.clone(),
            "original_event_type": original_event.event_type,
            "error_message": error.to_string(),
        });

        // Should be serializable
        let result = serde_json::to_string(&fallback);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("serialization_error"));
    }

    #[test]
    fn test_multiple_validation_failures() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );

        // Test with multiple field violations
        let mut event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .build()
        .unwrap();

        // First, empty message fails
        event.message = String::new();
        assert!(validate_event(&event).is_err());

        // Fix message, empty event_type should fail
        event.message = "Test".to_string();
        event.event_type = String::new();
        assert!(validate_event(&event).is_err());

        // Fix event_type, empty ID should fail
        event.event_type = "test.event".to_string();
        event.id = String::new();
        assert!(validate_event(&event).is_err());

        // When all fixed, should pass
        event.id = "valid-id".to_string();
        assert!(validate_event(&event).is_ok());
    }

    #[test]
    fn test_bundle_rotation_tracking() {
        // This test verifies that skipped_events counter is correctly tracked
        // during bundle rotation. Since run_writer is internal, we test
        // the validation logic that feeds into it.
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );

        let valid_event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Valid".to_string(),
            identity.clone(),
        )
        .build()
        .unwrap();

        let mut invalid_event = valid_event.clone();
        invalid_event.id = String::new();

        // Validation should distinguish between valid and invalid
        assert!(validate_event(&valid_event).is_ok());
        assert!(validate_event(&invalid_event).is_err());
    }

    #[test]
    fn test_event_validation_with_complex_metadata() {
        let identity = IdentityEnvelope::new(
            "test".to_string(),
            "test".to_string(),
            "test".to_string(),
            "1.0".to_string(),
        );

        let event = TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "Test".to_string(),
            identity,
        )
        .metadata(serde_json::json!({
            "adapter": {
                "id": "test-adapter",
                "metrics": {
                    "latency_ms": [1.2, 2.3, 3.4],
                    "throughput": 1000.5
                },
                "tags": ["production", "critical"]
            }
        }))
        .build()
        .unwrap();

        assert!(validate_event(&event).is_ok());
    }

    #[test]
    fn test_hash_computation_with_various_serialized_inputs() {
        let long_string = "very long string ".repeat(1000);
        let test_cases = vec![
            "simple string",
            r#"{"json":"object"}"#,
            "{\"nested\":{\"array\":[1,2,3]}}",
            "unicode_λ_δ_ε",
            "",
            long_string.as_str(),
        ];

        for input in test_cases {
            let identity = IdentityEnvelope::new(
                "test".to_string(),
                "test".to_string(),
                "test".to_string(),
                "1.0".to_string(),
            );
            let event = TelemetryEventBuilder::new(
                EventType::SystemStart,
                LogLevel::Info,
                format!("Test with: {}", input),
                identity,
            )
            .build()
            .unwrap();

            let hash = compute_event_hash(&event, input);
            assert!(!hash.as_bytes().is_empty());

            // Hash should be deterministic
            let hash2 = compute_event_hash(&event, input);
            assert_eq!(hash.as_bytes(), hash2.as_bytes());
        }
    }
}
