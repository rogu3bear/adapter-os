use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};
use tracing;

/// Severity levels for threat assessments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ThreatSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl ThreatSeverity {
    fn escalate(self, other: ThreatSeverity) -> ThreatSeverity {
        use ThreatSeverity::*;
        match (self, other) {
            (Critical, _) | (_, Critical) => Critical,
            (High, _) | (_, High) => High,
            (Medium, _) | (_, Medium) => Medium,
            _ => Low,
        }
    }
}

/// Structured threat assessment output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatAssessment {
    pub risk_score: f32,
    pub severity: ThreatSeverity,
    pub matched_patterns: Vec<String>,
    pub anomalies: Vec<String>,
    pub evidence: Vec<serde_json::Value>,
}

impl ThreatAssessment {
    pub fn compliant() -> Self {
        Self {
            risk_score: 0.0,
            severity: ThreatSeverity::Low,
            matched_patterns: Vec::new(),
            anomalies: Vec::new(),
            evidence: Vec::new(),
        }
    }
}

/// Signal captured by the detection engine.
#[derive(Debug, Clone)]
pub struct ThreatSignal {
    pub event_type: String,
    pub value: f32,
    pub metadata: serde_json::Value,
    pub timestamp: SystemTime,
}

impl ThreatSignal {
    pub fn new(event_type: impl Into<String>, value: f32, metadata: serde_json::Value) -> Self {
        Self {
            event_type: event_type.into(),
            value,
            metadata,
            timestamp: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone)]
struct ThreatPattern {
    name: String,
    event_type: String,
    threshold: f32,
    window: usize,
    severity: ThreatSeverity,
}

/// Threat detection engine implementing anomaly detection and pattern matching.
#[derive(Debug)]
pub struct ThreatDetectionEngine {
    window: VecDeque<ThreatSignal>,
    window_capacity: usize,
    baselines: HashMap<String, f32>,
    patterns: Vec<ThreatPattern>,
}

impl ThreatDetectionEngine {
    /// Create a new detection engine.
    pub fn new(window_capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(window_capacity),
            window_capacity,
            baselines: HashMap::new(),
            patterns: Vec::new(),
        }
    }

    /// Register a new detection pattern.
    pub fn register_pattern(
        &mut self,
        name: impl Into<String>,
        event_type: impl Into<String>,
        threshold: f32,
        window: usize,
        severity: ThreatSeverity,
    ) {
        self.patterns.push(ThreatPattern {
            name: name.into(),
            event_type: event_type.into(),
            threshold,
            window,
            severity,
        });
    }

    /// Update baseline for an event type.
    pub fn update_baseline(&mut self, event_type: impl Into<String>, value: f32) {
        self.baselines.insert(event_type.into(), value.max(0.01));
    }

    /// Ingest a signal and return a threat assessment.
    pub fn ingest(&mut self, signal: ThreatSignal) -> ThreatAssessment {
        self.window.push_back(signal.clone());
        if self.window.len() > self.window_capacity {
            self.window.pop_front();
        }

        let mut assessment = ThreatAssessment::compliant();
        let (risk, anomalies) = self.detect_anomalies(&signal);
        assessment.risk_score = risk;
        assessment.anomalies = anomalies;

        // Check patterns in the sliding window.
        for pattern in &self.patterns {
            if pattern.event_type != signal.event_type {
                continue;
            }
            let recent: Vec<&ThreatSignal> = self
                .window
                .iter()
                .rev()
                .take(pattern.window)
                .filter(|s| s.event_type == pattern.event_type)
                .collect();
            let sum: f32 = recent.iter().map(|s| s.value).sum();
            if sum >= pattern.threshold {
                assessment.matched_patterns.push(pattern.name.clone());
                assessment.severity = assessment.severity.escalate(pattern.severity);
            }
        }

        if assessment.risk_score > 0.7 {
            assessment.severity = assessment.severity.escalate(ThreatSeverity::High);
        } else if assessment.risk_score > 0.4 {
            assessment.severity = assessment.severity.escalate(ThreatSeverity::Medium);
        }

        if !assessment.matched_patterns.is_empty()
            || !assessment.anomalies.is_empty()
            || assessment.severity >= ThreatSeverity::Medium
        {
            tracing::warn!(
                target: "security.threat",
                severity = ?assessment.severity,
                risk_score = assessment.risk_score,
                event_type = %signal.event_type,
                matched_patterns = ?assessment.matched_patterns,
                anomalies = ?assessment.anomalies,
                "threat detected"
            );
        }

        assessment.evidence.push(signal.metadata);
        assessment
    }

    fn detect_anomalies(&self, signal: &ThreatSignal) -> (f32, Vec<String>) {
        let baseline = self
            .baselines
            .get(&signal.event_type)
            .copied()
            .unwrap_or(1.0);
        let deviation = (signal.value - baseline).abs() / baseline.max(1e-6);
        let mut anomalies = Vec::new();
        let mut risk = (signal.value / baseline).min(3.0) / 3.0;

        if deviation > 1.5 {
            anomalies.push(format!(
                "{} deviation {:.2} exceeds baseline {:.2}",
                signal.event_type, deviation, baseline
            ));
            risk = risk.max(0.6);
        }

        // Temporal anomaly: multiple events within short period
        let burst_score = self
            .window
            .iter()
            .rev()
            .take(5)
            .filter(|s| s.event_type == signal.event_type)
            .count();
        if burst_score >= 4 {
            anomalies.push(format!(
                "{} burst detected ({} events)",
                signal.event_type, burst_score
            ));
            risk = 0.9;
        }

        (risk.min(1.0), anomalies)
    }

    /// Prune stale signals outside the retention duration.
    pub fn prune(&mut self, retention: Duration) {
        let cutoff = SystemTime::now()
            .checked_sub(retention)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        while let Some(front) = self.window.front() {
            if front.timestamp <= cutoff {
                self.window.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pattern_and_anomaly() {
        let mut engine = ThreatDetectionEngine::new(16);
        engine.update_baseline("egress", 10.0);
        engine.register_pattern("egress-spike", "egress", 50.0, 5, ThreatSeverity::High);

        for _ in 0..4 {
            engine.ingest(ThreatSignal::new(
                "egress",
                12.0,
                serde_json::json!({"count": 12}),
            ));
        }
        let assessment = engine.ingest(ThreatSignal::new(
            "egress",
            20.0,
            serde_json::json!({"count": 20}),
        ));

        assert!(assessment.risk_score > 0.4);
        assert!(assessment
            .matched_patterns
            .contains(&"egress-spike".to_string()));
        assert!(matches!(
            assessment.severity,
            ThreatSeverity::High | ThreatSeverity::Critical
        ));
    }

    #[test]
    fn pruning_removes_stale_events() {
        let mut engine = ThreatDetectionEngine::new(4);
        engine.ingest(ThreatSignal::new("auth", 1.0, serde_json::json!({})));
        engine.prune(Duration::from_secs(0));
        assert!(engine.window.is_empty());
    }
}
