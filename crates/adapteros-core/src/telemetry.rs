//! Observability and incident hardening events.
//!
//! Provides canonical, structured payloads for compliance-sensitive alerts so
//! callers can emit a single JSON shape for logs, metrics, and telemetry
//! bundles. Each helper returns an `ObservabilityEvent` and also maps severity
//! into a tracing level plus `adapteros-types` log level for downstream sinks.

use adapteros_types::telemetry::LogLevel;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{event, Level};

/// Metric name for determinism violation counters.
pub const DETERMINISM_VIOLATION_METRIC: &str = "determinism_violation_total";

/// Observability severity used for both tracing and telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ObservabilitySeverity {
    Info,
    Warning,
    Alert,
}

impl ObservabilitySeverity {
    fn as_tracing_level(&self) -> Level {
        match self {
            Self::Info => Level::INFO,
            Self::Warning => Level::WARN,
            Self::Alert => Level::ERROR,
        }
    }

    /// Map to canonical telemetry log level.
    pub fn as_log_level(&self) -> LogLevel {
        match self {
            Self::Info => LogLevel::Info,
            Self::Warning => LogLevel::Warn,
            Self::Alert => LogLevel::Critical,
        }
    }
}

/// Canonical observability event kinds that must always emit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ObservabilityEventKind {
    ReceiptMismatch,
    DualWriteDivergence,
    DeterminismViolation,
    StrictModeFailure,
    AuditExportTamper,
}

/// Specific determinism violation categories for metrics and alerting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeterminismViolationKind {
    RouterDecisionMismatch,
    ReplayMismatch,
    EvidenceTamper,
    RngDivergence,
    OutputDrift,
    Unknown,
}

/// Optional metric payload describing a determinism violation counter update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeterminismViolationMetric {
    /// Prometheus/UDS metric name.
    pub counter: &'static str,
    /// Violation label for the counter.
    pub violation: DeterminismViolationKind,
    /// Whether strict determinism mode was active.
    pub strict_mode: bool,
}

/// Structured payloads for each observability event type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "detail_type", rename_all = "snake_case")]
pub enum ObservabilityDetail {
    ReceiptMismatch {
        expected_receipt: String,
        received_receipt: String,
        scope: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        evidence_hash: Option<String>,
    },
    DualWriteDivergence {
        table: String,
        key: String,
        primary_checksum: String,
        secondary_checksum: String,
        attempt: u32,
    },
    DeterminismViolation {
        violation: DeterminismViolationKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        divergence_at: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        manifest_hash: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seed_hash: Option<String>,
        strict_mode: bool,
    },
    StrictModeFailure {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<String>,
        fallback_used: bool,
    },
    AuditExportTamper {
        bundle_id: String,
        expected_hash: String,
        observed_hash: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        export_path: Option<String>,
    },
}

/// Canonical observability event envelope for structured logs and telemetry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityEvent {
    pub kind: ObservabilityEventKind,
    pub severity: ObservabilitySeverity,
    pub message: String,
    pub component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub timestamp_us: u64,
    #[serde(flatten)]
    pub detail: ObservabilityDetail,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<DeterminismViolationMetric>,
    /// Lightweight labels to support metrics/tag enrichment.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

impl ObservabilityEvent {
    fn now_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or_default()
    }

    /// Emit the event to structured tracing. Callers should still forward to
    /// telemetry sinks if available; this is the minimal "make it loud" path.
    pub fn emit_tracing(&self) {
        let level = self.severity.as_tracing_level();
        event!(
            level,
            event_kind = ?self.kind,
            severity = ?self.severity,
            component = %self.component,
            tenant_id = self.tenant_id.as_deref().unwrap_or(""),
            request_id = self.request_id.as_deref().unwrap_or(""),
            correlation_id = self.correlation_id.as_deref().unwrap_or(""),
            metric = self.metric.as_ref().map(|m| m.counter).unwrap_or(""),
            labels = ?self.labels,
            detail = ?self.detail,
            "observability.event: {}",
            self.message
        );
    }

    fn base(
        kind: ObservabilityEventKind,
        severity: ObservabilitySeverity,
        component: impl Into<String>,
        message: impl Into<String>,
        detail: ObservabilityDetail,
    ) -> Self {
        Self {
            kind,
            severity,
            message: message.into(),
            component: component.into(),
            tenant_id: None,
            request_id: None,
            correlation_id: None,
            timestamp_us: Self::now_us(),
            detail,
            metric: None,
            labels: BTreeMap::new(),
        }
    }

    fn with_common_ids(
        mut self,
        tenant_id: Option<String>,
        request_id: Option<String>,
        correlation_id: Option<String>,
    ) -> Self {
        self.tenant_id = tenant_id;
        self.request_id = request_id;
        self.correlation_id = correlation_id;
        self
    }
}

/// Emit a receipt mismatch alert. Must be called whenever signature/receipt
/// validation fails (tamper, wrong key, or replayed receipt).
pub fn receipt_mismatch_event(
    expected_receipt: impl Into<String>,
    received_receipt: impl Into<String>,
    scope: impl Into<String>,
    evidence_hash: Option<String>,
    tenant_id: Option<String>,
    request_id: Option<String>,
) -> ObservabilityEvent {
    ObservabilityEvent::base(
        ObservabilityEventKind::ReceiptMismatch,
        ObservabilitySeverity::Alert,
        "audit",
        "Receipt mismatch detected",
        ObservabilityDetail::ReceiptMismatch {
            expected_receipt: expected_receipt.into(),
            received_receipt: received_receipt.into(),
            scope: scope.into(),
            evidence_hash,
        },
    )
    .with_common_ids(tenant_id, request_id, None)
}

/// Emit a dual-write divergence alert. Fires whenever SQL and KV disagree.
pub fn dual_write_divergence_event(
    table: impl Into<String>,
    key: impl Into<String>,
    primary_checksum: impl Into<String>,
    secondary_checksum: impl Into<String>,
    attempt: u32,
    tenant_id: Option<String>,
) -> ObservabilityEvent {
    ObservabilityEvent::base(
        ObservabilityEventKind::DualWriteDivergence,
        ObservabilitySeverity::Alert,
        "storage",
        "Dual-write divergence detected",
        ObservabilityDetail::DualWriteDivergence {
            table: table.into(),
            key: key.into(),
            primary_checksum: primary_checksum.into(),
            secondary_checksum: secondary_checksum.into(),
            attempt,
        },
    )
    .with_common_ids(tenant_id, None, None)
}

/// Emit a determinism violation metric + log. Severity escalates to alert when
/// strict mode is active.
pub fn determinism_violation_event(
    violation: DeterminismViolationKind,
    divergence_at: Option<u64>,
    manifest_hash: Option<String>,
    seed_hash: Option<String>,
    strict_mode: bool,
    tenant_id: Option<String>,
    request_id: Option<String>,
) -> ObservabilityEvent {
    let severity = if strict_mode {
        ObservabilitySeverity::Alert
    } else {
        ObservabilitySeverity::Warning
    };

    let mut labels = BTreeMap::new();
    labels.insert("violation".to_string(), format!("{:?}", violation));
    if strict_mode {
        labels.insert("strict_mode".to_string(), "true".to_string());
    }

    ObservabilityEvent {
        labels,
        metric: Some(DeterminismViolationMetric {
            counter: DETERMINISM_VIOLATION_METRIC,
            violation: violation.clone(),
            strict_mode,
        }),
        ..ObservabilityEvent::base(
            ObservabilityEventKind::DeterminismViolation,
            severity,
            "determinism",
            "Determinism violation detected",
            ObservabilityDetail::DeterminismViolation {
                violation,
                divergence_at,
                manifest_hash,
                seed_hash,
                strict_mode,
            },
        )
        .with_common_ids(tenant_id, request_id, None)
    }
}

/// Emit when strict mode fails closed (policy, routing, or sampler guard).
pub fn strict_mode_failure_event(
    reason: impl Into<String>,
    policy: Option<String>,
    fallback_used: bool,
    tenant_id: Option<String>,
    request_id: Option<String>,
) -> ObservabilityEvent {
    ObservabilityEvent::base(
        ObservabilityEventKind::StrictModeFailure,
        ObservabilitySeverity::Alert,
        "determinism",
        "Strict mode failure",
        ObservabilityDetail::StrictModeFailure {
            reason: reason.into(),
            policy,
            fallback_used,
        },
    )
    .with_common_ids(tenant_id, request_id, None)
}

/// Emit when audit export integrity checks detect tampering.
pub fn audit_export_tamper_event(
    bundle_id: impl Into<String>,
    expected_hash: impl Into<String>,
    observed_hash: impl Into<String>,
    export_path: Option<String>,
    tenant_id: Option<String>,
) -> ObservabilityEvent {
    ObservabilityEvent::base(
        ObservabilityEventKind::AuditExportTamper,
        ObservabilitySeverity::Alert,
        "audit",
        "Audit export tamper detected",
        ObservabilityDetail::AuditExportTamper {
            bundle_id: bundle_id.into(),
            expected_hash: expected_hash.into(),
            observed_hash: observed_hash.into(),
            export_path,
        },
    )
    .with_common_ids(tenant_id, None, None)
}

/// Convenience helper: emit to tracing immediately.
pub fn emit_observability_event(event: &ObservabilityEvent) {
    event.emit_tracing();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_violation_carries_metric_and_labels() {
        let event = determinism_violation_event(
            DeterminismViolationKind::RngDivergence,
            Some(42),
            Some("manifest".into()),
            None,
            true,
            Some("tenant-a".into()),
            Some("req-1".into()),
        );

        assert_eq!(
            event.metric.as_ref().unwrap().counter,
            DETERMINISM_VIOLATION_METRIC
        );
        assert_eq!(event.severity, ObservabilitySeverity::Alert);
        assert_eq!(event.labels.get("violation").unwrap(), "RngDivergence");
        assert_eq!(event.tenant_id.as_deref(), Some("tenant-a"));
    }

    #[test]
    fn dual_write_divergence_is_alert() {
        let event =
            dual_write_divergence_event("adapters", "adapter-123", "sql:abcd", "kv:efgh", 2, None);

        assert_eq!(event.kind, ObservabilityEventKind::DualWriteDivergence);
        assert_eq!(event.severity, ObservabilitySeverity::Alert);
        match event.detail {
            ObservabilityDetail::DualWriteDivergence { attempt, .. } => {
                assert_eq!(attempt, 2);
            }
            _ => panic!("wrong detail"),
        }
    }

    #[test]
    fn tamper_event_includes_hashes() {
        let event = audit_export_tamper_event(
            "bundle-1",
            "expected-hash",
            "observed-hash",
            Some("/tmp/export.json".into()),
            Some("tenant-b".into()),
        );

        assert_eq!(event.severity, ObservabilitySeverity::Alert);
        match event.detail {
            ObservabilityDetail::AuditExportTamper {
                expected_hash,
                observed_hash,
                ..
            } => {
                assert_eq!(expected_hash, "expected-hash");
                assert_eq!(observed_hash, "observed-hash");
            }
            _ => panic!("wrong detail"),
        }
    }
}
