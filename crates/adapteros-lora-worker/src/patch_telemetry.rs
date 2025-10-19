//! Telemetry and monitoring for patch operations
//!
//! Implements comprehensive telemetry for patch proposal system:
//! - Patch generation events
//! - Evidence retrieval metrics
//! - Policy validation results
//! - Security violation tracking
//! - Performance monitoring
//!
//! Aligns with Telemetry Ruleset #9 and security monitoring requirements.

use adapteros_core::{AosError, Result};
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Patch operation event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchEventType {
    EvidenceRetrieved,
    PatchGenerated,
    PatchValidated,
    PatchApplied,
    SecurityViolation,
    PerformanceThreshold,
    ErrorOccurred,
}

/// Patch operation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchEvent {
    pub event_type: PatchEventType,
    pub tenant_id: String,
    pub proposal_id: Option<String>,
    pub repo_id: Option<String>,
    pub timestamp: u64,
    pub duration_ms: Option<u64>,
    pub metadata: HashMap<String, String>,
    pub severity: EventSeverity,
}

/// Event severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Evidence retrieval metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceMetrics {
    pub query: String,
    pub sources_used: Vec<String>,
    pub spans_found: usize,
    pub retrieval_time_ms: u64,
    pub avg_relevance_score: f32,
    pub min_score_threshold: f32,
}

/// Patch generation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchGenerationMetrics {
    pub proposal_id: String,
    pub description: String,
    pub target_files: Vec<String>,
    pub evidence_count: usize,
    pub patch_count: usize,
    pub total_lines: usize,
    pub generation_time_ms: u64,
    pub confidence_score: f32,
}

/// Policy validation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetrics {
    pub proposal_id: String,
    pub is_valid: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub violation_count: usize,
    pub validation_time_ms: u64,
    pub confidence_score: f32,
    pub violations: Vec<ViolationMetric>,
}

/// Security violation metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationMetric {
    pub violation_type: String,
    pub severity: String,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
    pub description: String,
}

/// Performance threshold event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholdEvent {
    pub operation: String,
    pub threshold_ms: u64,
    pub actual_ms: u64,
    pub threshold_exceeded: bool,
    pub proposal_id: Option<String>,
}

/// Patch telemetry collector
pub struct PatchTelemetry {
    events: Vec<PatchEvent>,
    #[allow(dead_code)]
    metrics: HashMap<String, serde_json::Value>,
    performance_thresholds: HashMap<String, u64>,
    telemetry_writer: Option<TelemetryWriter>,
}

impl Default for PatchTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchTelemetry {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            metrics: HashMap::new(),
            performance_thresholds: Self::default_thresholds(),
            telemetry_writer: None,
        }
    }

    /// Create with telemetry writer integration
    pub fn new_with_writer(telemetry_writer: TelemetryWriter) -> Self {
        Self {
            events: Vec::new(),
            metrics: HashMap::new(),
            performance_thresholds: Self::default_thresholds(),
            telemetry_writer: Some(telemetry_writer),
        }
    }

    /// Forward patch event to telemetry writer
    fn forward_to_telemetry_writer(&self, event: &PatchEvent) {
        if let Some(ref writer) = self.telemetry_writer {
            let event_type = format!("patch_{:?}", event.event_type);
            let payload = serde_json::to_value(event).unwrap_or_default();

            if let Err(e) = writer.log(&event_type, payload) {
                warn!("Failed to forward patch event to telemetry writer: {}", e);
            }
        }
    }

    /// Default performance thresholds
    fn default_thresholds() -> HashMap<String, u64> {
        let mut thresholds = HashMap::new();
        thresholds.insert("evidence_retrieval".to_string(), 100); // 100ms
        thresholds.insert("patch_generation".to_string(), 2000); // 2s
        thresholds.insert("patch_validation".to_string(), 100); // 100ms
        thresholds.insert("patch_apply".to_string(), 5000); // 5s
        thresholds
    }

    /// Log evidence retrieval event
    pub fn log_evidence_retrieval(
        &mut self,
        tenant_id: &str,
        metrics: EvidenceMetrics,
        proposal_id: Option<&str>,
    ) {
        let event = PatchEvent {
            event_type: PatchEventType::EvidenceRetrieved,
            tenant_id: tenant_id.to_string(),
            proposal_id: proposal_id.map(|s| s.to_string()),
            repo_id: None,
            timestamp: Self::current_timestamp(),
            duration_ms: Some(metrics.retrieval_time_ms),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("query".to_string(), metrics.query);
                meta.insert(
                    "sources_count".to_string(),
                    metrics.sources_used.len().to_string(),
                );
                meta.insert("spans_found".to_string(), metrics.spans_found.to_string());
                meta.insert(
                    "avg_score".to_string(),
                    format!("{:.3}", metrics.avg_relevance_score),
                );
                meta.insert(
                    "min_threshold".to_string(),
                    format!("{:.3}", metrics.min_score_threshold),
                );
                meta
            },
            severity: EventSeverity::Info,
        };

        self.events.push(event);
        self.check_performance_threshold(
            "evidence_retrieval",
            metrics.retrieval_time_ms,
            proposal_id,
        );

        info!(
            "Evidence retrieved: {} spans in {}ms",
            metrics.spans_found, metrics.retrieval_time_ms
        );
    }

    /// Log patch generation event
    pub fn log_patch_generation(&mut self, tenant_id: &str, metrics: PatchGenerationMetrics) {
        let event = PatchEvent {
            event_type: PatchEventType::PatchGenerated,
            tenant_id: tenant_id.to_string(),
            proposal_id: Some(metrics.proposal_id.clone()),
            repo_id: None,
            timestamp: Self::current_timestamp(),
            duration_ms: Some(metrics.generation_time_ms),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("description".to_string(), metrics.description);
                meta.insert("target_files".to_string(), metrics.target_files.join(","));
                meta.insert(
                    "evidence_count".to_string(),
                    metrics.evidence_count.to_string(),
                );
                meta.insert("patch_count".to_string(), metrics.patch_count.to_string());
                meta.insert("total_lines".to_string(), metrics.total_lines.to_string());
                meta.insert(
                    "confidence".to_string(),
                    format!("{:.3}", metrics.confidence_score),
                );
                meta
            },
            severity: EventSeverity::Info,
        };

        self.events.push(event.clone());
        self.check_performance_threshold(
            "patch_generation",
            metrics.generation_time_ms,
            Some(&metrics.proposal_id),
        );

        // Forward to telemetry writer if available
        self.forward_to_telemetry_writer(&event);

        info!(
            "Patch generated: {} files, {} lines, confidence {:.3} in {}ms",
            metrics.patch_count,
            metrics.total_lines,
            metrics.confidence_score,
            metrics.generation_time_ms
        );
    }

    /// Log patch validation event
    pub fn log_patch_validation(&mut self, tenant_id: &str, metrics: ValidationMetrics) {
        let severity = if metrics.is_valid {
            if metrics.warning_count > 0 {
                EventSeverity::Warning
            } else {
                EventSeverity::Info
            }
        } else {
            EventSeverity::Error
        };

        let event = PatchEvent {
            event_type: PatchEventType::PatchValidated,
            tenant_id: tenant_id.to_string(),
            proposal_id: Some(metrics.proposal_id.clone()),
            repo_id: None,
            timestamp: Self::current_timestamp(),
            duration_ms: Some(metrics.validation_time_ms),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("is_valid".to_string(), metrics.is_valid.to_string());
                meta.insert("error_count".to_string(), metrics.error_count.to_string());
                meta.insert(
                    "warning_count".to_string(),
                    metrics.warning_count.to_string(),
                );
                meta.insert(
                    "violation_count".to_string(),
                    metrics.violation_count.to_string(),
                );
                meta.insert(
                    "confidence".to_string(),
                    format!("{:.3}", metrics.confidence_score),
                );
                meta
            },
            severity,
        };

        self.events.push(event.clone());
        self.check_performance_threshold(
            "patch_validation",
            metrics.validation_time_ms,
            Some(&metrics.proposal_id),
        );

        // Forward to telemetry writer if available
        self.forward_to_telemetry_writer(&event);

        // Log individual violations
        for violation in metrics.violations {
            self.log_security_violation(tenant_id, violation, Some(&metrics.proposal_id));
        }

        if metrics.is_valid {
            info!(
                "Patch validation passed: {} warnings in {}ms",
                metrics.warning_count, metrics.validation_time_ms
            );
        } else {
            error!(
                "Patch validation failed: {} errors, {} violations in {}ms",
                metrics.error_count, metrics.violation_count, metrics.validation_time_ms
            );
        }
    }

    /// Log security violation event
    pub fn log_security_violation(
        &mut self,
        tenant_id: &str,
        violation: ViolationMetric,
        proposal_id: Option<&str>,
    ) {
        let severity = match violation.severity.as_str() {
            "critical" => EventSeverity::Critical,
            "high" => EventSeverity::Error,
            "medium" => EventSeverity::Warning,
            _ => EventSeverity::Info,
        };

        let description = violation.description.clone();
        let event = PatchEvent {
            event_type: PatchEventType::SecurityViolation,
            tenant_id: tenant_id.to_string(),
            proposal_id: proposal_id.map(|s| s.to_string()),
            repo_id: None,
            timestamp: Self::current_timestamp(),
            duration_ms: None,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("violation_type".to_string(), violation.violation_type);
                meta.insert("description".to_string(), description.clone());
                if let Some(file_path) = violation.file_path {
                    meta.insert("file_path".to_string(), file_path);
                }
                if let Some(line_number) = violation.line_number {
                    meta.insert("line_number".to_string(), line_number.to_string());
                }
                meta
            },
            severity: severity.clone(),
        };

        self.events.push(event.clone());

        // Forward to telemetry writer if available
        self.forward_to_telemetry_writer(&event);

        match severity {
            EventSeverity::Critical => error!("Critical security violation: {}", description),
            EventSeverity::Error => error!("Security violation: {}", description),
            EventSeverity::Warning => warn!("Security warning: {}", description),
            EventSeverity::Info => info!("Security info: {}", description),
        }
    }

    /// Log performance threshold event
    pub fn log_performance_threshold(&mut self, tenant_id: &str, event: PerformanceThresholdEvent) {
        let severity = if event.threshold_exceeded {
            EventSeverity::Warning
        } else {
            EventSeverity::Info
        };

        let operation = event.operation.clone();
        let actual_ms = event.actual_ms;
        let threshold_ms = event.threshold_ms;
        let threshold_exceeded = event.threshold_exceeded;

        let event_type = PatchEventType::PerformanceThreshold;
        let patch_event = PatchEvent {
            event_type,
            tenant_id: tenant_id.to_string(),
            proposal_id: event.proposal_id,
            repo_id: None,
            timestamp: Self::current_timestamp(),
            duration_ms: Some(actual_ms),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("operation".to_string(), operation.clone());
                meta.insert("threshold_ms".to_string(), threshold_ms.to_string());
                meta.insert("actual_ms".to_string(), actual_ms.to_string());
                meta.insert("exceeded".to_string(), threshold_exceeded.to_string());
                meta
            },
            severity,
        };

        self.events.push(patch_event);

        if threshold_exceeded {
            warn!(
                "Performance threshold exceeded: {} took {}ms > {}ms",
                operation, actual_ms, threshold_ms
            );
        } else {
            debug!(
                "Performance within threshold: {} took {}ms <= {}ms",
                operation, actual_ms, threshold_ms
            );
        }
    }

    /// Log error event
    pub fn log_error(
        &mut self,
        tenant_id: &str,
        error: &AosError,
        proposal_id: Option<&str>,
        operation: &str,
    ) {
        let event = PatchEvent {
            event_type: PatchEventType::ErrorOccurred,
            tenant_id: tenant_id.to_string(),
            proposal_id: proposal_id.map(|s| s.to_string()),
            repo_id: None,
            timestamp: Self::current_timestamp(),
            duration_ms: None,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("error_type".to_string(), format!("{:?}", error));
                meta.insert("error_message".to_string(), error.to_string());
                meta.insert("operation".to_string(), operation.to_string());
                meta
            },
            severity: EventSeverity::Error,
        };

        self.events.push(event);
        error!("Patch operation error in {}: {}", operation, error);
    }

    /// Check performance threshold and log if exceeded
    fn check_performance_threshold(
        &mut self,
        operation: &str,
        duration_ms: u64,
        proposal_id: Option<&str>,
    ) {
        if let Some(&threshold_ms) = self.performance_thresholds.get(operation) {
            let exceeded = duration_ms > threshold_ms;
            let event = PerformanceThresholdEvent {
                operation: operation.to_string(),
                threshold_ms,
                actual_ms: duration_ms,
                threshold_exceeded: exceeded,
                proposal_id: proposal_id.map(|s| s.to_string()),
            };
            self.log_performance_threshold("default_tenant", event);
        }
    }

    /// Get current timestamp
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs()
    }

    /// Get all events
    pub fn get_events(&self) -> &[PatchEvent] {
        &self.events
    }

    /// Get events by type
    pub fn get_events_by_type(&self, event_type: &PatchEventType) -> Vec<&PatchEvent> {
        self.events
            .iter()
            .filter(|e| std::mem::discriminant(&e.event_type) == std::mem::discriminant(event_type))
            .collect()
    }

    /// Get events by severity
    pub fn get_events_by_severity(&self, severity: &EventSeverity) -> Vec<&PatchEvent> {
        self.events
            .iter()
            .filter(|e| std::mem::discriminant(&e.severity) == std::mem::discriminant(severity))
            .collect()
    }

    /// Get performance metrics
    pub fn get_performance_metrics(&self) -> HashMap<String, f64> {
        let mut metrics = HashMap::new();

        // Calculate average durations by operation type
        let mut operation_durations: HashMap<String, Vec<u64>> = HashMap::new();

        for event in &self.events {
            if let Some(duration_ms) = event.duration_ms {
                let operation = match event.event_type {
                    PatchEventType::EvidenceRetrieved => "evidence_retrieval",
                    PatchEventType::PatchGenerated => "patch_generation",
                    PatchEventType::PatchValidated => "patch_validation",
                    PatchEventType::PatchApplied => "patch_apply",
                    _ => continue,
                };

                operation_durations
                    .entry(operation.to_string())
                    .or_default()
                    .push(duration_ms);
            }
        }

        for (operation, durations) in operation_durations {
            let avg_duration = durations.iter().sum::<u64>() as f64 / durations.len() as f64;
            metrics.insert(format!("{}_avg_ms", operation), avg_duration);

            let max_duration = *durations
                .iter()
                .max()
                .expect("Test durations should have max value")
                as f64;
            metrics.insert(format!("{}_max_ms", operation), max_duration);
        }

        metrics
    }

    /// Clear all events (for testing)
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Export events as JSON
    pub fn export_events(&self) -> Result<String> {
        serde_json::to_string(&self.events)
            .map_err(|e| AosError::Worker(format!("Failed to export events: {}", e)))
    }
}

/// Telemetry event builder for convenience
pub struct TelemetryEventBuilder {
    tenant_id: String,
    proposal_id: Option<String>,
    metadata: HashMap<String, String>,
}

impl TelemetryEventBuilder {
    pub fn new(tenant_id: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            proposal_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_proposal_id(mut self, proposal_id: &str) -> Self {
        self.proposal_id = Some(proposal_id.to_string());
        self
    }

    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    pub fn build_evidence_event(self, metrics: EvidenceMetrics) -> PatchEvent {
        PatchEvent {
            event_type: PatchEventType::EvidenceRetrieved,
            tenant_id: self.tenant_id,
            proposal_id: self.proposal_id,
            repo_id: None,
            timestamp: PatchTelemetry::current_timestamp(),
            duration_ms: Some(metrics.retrieval_time_ms),
            metadata: {
                let mut meta = self.metadata;
                meta.insert("query".to_string(), metrics.query);
                meta.insert(
                    "sources_count".to_string(),
                    metrics.sources_used.len().to_string(),
                );
                meta.insert("spans_found".to_string(), metrics.spans_found.to_string());
                meta.insert(
                    "avg_score".to_string(),
                    format!("{:.3}", metrics.avg_relevance_score),
                );
                meta
            },
            severity: EventSeverity::Info,
        }
    }

    pub fn build_generation_event(self, metrics: PatchGenerationMetrics) -> PatchEvent {
        PatchEvent {
            event_type: PatchEventType::PatchGenerated,
            tenant_id: self.tenant_id,
            proposal_id: Some(metrics.proposal_id.clone()),
            repo_id: None,
            timestamp: PatchTelemetry::current_timestamp(),
            duration_ms: Some(metrics.generation_time_ms),
            metadata: {
                let mut meta = self.metadata;
                meta.insert("description".to_string(), metrics.description);
                meta.insert("target_files".to_string(), metrics.target_files.join(","));
                meta.insert(
                    "evidence_count".to_string(),
                    metrics.evidence_count.to_string(),
                );
                meta.insert("patch_count".to_string(), metrics.patch_count.to_string());
                meta.insert(
                    "confidence".to_string(),
                    format!("{:.3}", metrics.confidence_score),
                );
                meta
            },
            severity: EventSeverity::Info,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_creation() {
        let telemetry = PatchTelemetry::new();
        assert!(telemetry.get_events().is_empty());
    }

    #[test]
    fn test_evidence_retrieval_logging() {
        let mut telemetry = PatchTelemetry::new();

        let metrics = EvidenceMetrics {
            query: "test query".to_string(),
            sources_used: vec!["symbol".to_string(), "test".to_string()],
            spans_found: 5,
            retrieval_time_ms: 50,
            avg_relevance_score: 0.8,
            min_score_threshold: 0.5,
        };

        telemetry.log_evidence_retrieval("test_tenant", metrics, Some("proposal_123"));

        let events = telemetry.get_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            PatchEventType::EvidenceRetrieved
        ));
        assert_eq!(events[0].tenant_id, "test_tenant");
        assert_eq!(events[0].proposal_id, Some("proposal_123".to_string()));
    }

    #[test]
    fn test_patch_generation_logging() {
        let mut telemetry = PatchTelemetry::new();

        let metrics = PatchGenerationMetrics {
            proposal_id: "proposal_123".to_string(),
            description: "Test patch".to_string(),
            target_files: vec!["src/test.rs".to_string()],
            evidence_count: 3,
            patch_count: 1,
            total_lines: 10,
            generation_time_ms: 1500,
            confidence_score: 0.9,
        };

        telemetry.log_patch_generation("test_tenant", metrics);

        let events = telemetry.get_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            PatchEventType::PatchGenerated
        ));
        assert_eq!(events[0].tenant_id, "test_tenant");
        assert_eq!(events[0].proposal_id, Some("proposal_123".to_string()));
    }

    #[test]
    fn test_validation_logging() {
        let mut telemetry = PatchTelemetry::new();

        let metrics = ValidationMetrics {
            proposal_id: "proposal_123".to_string(),
            is_valid: true,
            error_count: 0,
            warning_count: 1,
            violation_count: 0,
            validation_time_ms: 75,
            confidence_score: 0.9,
            violations: vec![],
        };

        telemetry.log_patch_validation("test_tenant", metrics);

        let events = telemetry.get_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            PatchEventType::PatchValidated
        ));
        assert_eq!(events[0].tenant_id, "test_tenant");
        assert!(matches!(events[0].severity, EventSeverity::Warning));
    }

    #[test]
    fn test_security_violation_logging() {
        let mut telemetry = PatchTelemetry::new();

        let violation = ViolationMetric {
            violation_type: "secret_detected".to_string(),
            severity: "critical".to_string(),
            file_path: Some("src/config.rs".to_string()),
            line_number: Some(10),
            description: "API key detected".to_string(),
        };

        telemetry.log_security_violation("test_tenant", violation, Some("proposal_123"));

        let events = telemetry.get_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            PatchEventType::SecurityViolation
        ));
        assert_eq!(events[0].tenant_id, "test_tenant");
        assert!(matches!(events[0].severity, EventSeverity::Critical));
    }

    #[test]
    fn test_performance_threshold_logging() {
        let mut telemetry = PatchTelemetry::new();

        let event = PerformanceThresholdEvent {
            operation: "patch_generation".to_string(),
            threshold_ms: 2000,
            actual_ms: 2500,
            threshold_exceeded: true,
            proposal_id: Some("proposal_123".to_string()),
        };

        telemetry.log_performance_threshold("test_tenant", event);

        let events = telemetry.get_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            PatchEventType::PerformanceThreshold
        ));
        assert_eq!(events[0].tenant_id, "test_tenant");
        assert!(matches!(events[0].severity, EventSeverity::Warning));
    }

    #[test]
    fn test_error_logging() {
        let mut telemetry = PatchTelemetry::new();

        let error = AosError::Worker("Test error".to_string());
        telemetry.log_error(
            "test_tenant",
            &error,
            Some("proposal_123"),
            "patch_generation",
        );

        let events = telemetry.get_events();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            PatchEventType::ErrorOccurred
        ));
        assert_eq!(events[0].tenant_id, "test_tenant");
        assert!(matches!(events[0].severity, EventSeverity::Error));
    }

    #[test]
    fn test_performance_metrics() {
        let mut telemetry = PatchTelemetry::new();

        // Log some events with durations
        let evidence_metrics = EvidenceMetrics {
            query: "test".to_string(),
            sources_used: vec![],
            spans_found: 1,
            retrieval_time_ms: 50,
            avg_relevance_score: 0.8,
            min_score_threshold: 0.5,
        };
        telemetry.log_evidence_retrieval("test_tenant", evidence_metrics, None);

        let generation_metrics = PatchGenerationMetrics {
            proposal_id: "proposal_123".to_string(),
            description: "test".to_string(),
            target_files: vec![],
            evidence_count: 1,
            patch_count: 1,
            total_lines: 1,
            generation_time_ms: 1500,
            confidence_score: 0.9,
        };
        telemetry.log_patch_generation("test_tenant", generation_metrics);

        let metrics = telemetry.get_performance_metrics();
        assert!(metrics.contains_key("evidence_retrieval_avg_ms"));
        assert!(metrics.contains_key("patch_generation_avg_ms"));
        assert_eq!(metrics["evidence_retrieval_avg_ms"], 50.0);
        assert_eq!(metrics["patch_generation_avg_ms"], 1500.0);
    }

    #[test]
    fn test_event_builder() {
        let builder = TelemetryEventBuilder::new("test_tenant")
            .with_proposal_id("proposal_123")
            .with_metadata("key", "value");

        let metrics = EvidenceMetrics {
            query: "test".to_string(),
            sources_used: vec![],
            spans_found: 1,
            retrieval_time_ms: 50,
            avg_relevance_score: 0.8,
            min_score_threshold: 0.5,
        };

        let event = builder.build_evidence_event(metrics);
        assert_eq!(event.tenant_id, "test_tenant");
        assert_eq!(event.proposal_id, Some("proposal_123".to_string()));
        assert_eq!(event.metadata["key"], "value");
    }
}
