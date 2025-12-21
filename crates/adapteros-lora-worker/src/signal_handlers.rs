//! Signal Handler Implementations
//!
//! Implements specific signal handlers for different signal types.
//! Each handler encapsulates the logic for processing a particular category
//! of signals during inference.
//!
//! Citation: docs/llm-interface-specification.md §5.3, §5.4, §5.5

use super::signal::*;
use async_trait::async_trait;
use adapteros_core::Result;
use tracing::{debug, info, warn};

/// Handler for adapter request signals (Specification §5.3.1)
///
/// Processes adapter routing requests from the LLM, adjusting adapter
/// selection based on query characteristics and hints.
///
/// Citation: docs/llm-interface-specification.md §5.3.1
pub struct AdapterRequestHandler {
    /// Routing preferences to apply
    preferred_adapters: Vec<String>,

    /// Required capabilities for next generation step
    required_capabilities: Vec<String>,
}

impl AdapterRequestHandler {
    pub fn new() -> Self {
        Self {
            preferred_adapters: Vec::new(),
            required_capabilities: Vec::new(),
        }
    }

    /// Get preferred adapters for routing
    pub fn get_preferred_adapters(&self) -> &[String] {
        &self.preferred_adapters
    }

    /// Get required capabilities
    pub fn get_required_capabilities(&self) -> &[String] {
        &self.required_capabilities
    }

    /// Clear routing preferences
    pub fn clear_preferences(&mut self) {
        self.preferred_adapters.clear();
        self.required_capabilities.clear();
    }
}

#[async_trait]
impl SignalHandler for AdapterRequestHandler {
    async fn handle_signal(&mut self, signal: &Signal) -> Result<()> {
        // Extract preferred adapters from payload
        if let Some(preferred) = signal.payload.get("preferredAdapters") {
            if let Some(adapters) = preferred.as_array() {
                self.preferred_adapters = adapters
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                info!("Adapter preferences updated: {:?}", self.preferred_adapters);
            }
        }

        // Extract required capabilities
        if let Some(capabilities) = signal.payload.get("requiredCapabilities") {
            if let Some(caps) = capabilities.as_array() {
                self.required_capabilities = caps
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                info!("Required capabilities: {:?}", self.required_capabilities);
            }
        }

        // Log routing request details
        if let Some(query) = signal.payload.get("query") {
            debug!("Adapter routing requested for query: {:?}", query);
        }

        if let Some(domain) = signal.payload.get("domain") {
            debug!("Target domain: {:?}", domain);
        }

        Ok(())
    }

    fn signal_types(&self) -> Vec<SignalType> {
        vec![SignalType::AdapterRequest]
    }
}

impl Default for AdapterRequestHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for adapter activation signals (Specification §5.3.2)
///
/// Logs adapter activation events for telemetry and profiling.
///
/// Citation: docs/llm-interface-specification.md §5.3.2
pub struct AdapterActivationHandler {
    /// Activation history for current inference
    activations: Vec<AdapterActivation>,
}

#[derive(Debug, Clone)]
pub struct AdapterActivation {
    pub adapter_id: String,
    pub token_position: usize,
    pub confidence: f32,
    pub timestamp: u128,
}

impl AdapterActivationHandler {
    pub fn new() -> Self {
        Self {
            activations: Vec::new(),
        }
    }

    /// Get activation history
    pub fn get_activations(&self) -> &[AdapterActivation] {
        &self.activations
    }

    /// Clear activation history
    pub fn clear(&mut self) {
        self.activations.clear();
    }
}

#[async_trait]
impl SignalHandler for AdapterActivationHandler {
    async fn handle_signal(&mut self, signal: &Signal) -> Result<()> {
        let adapter_id = signal
            .payload
            .get("adapterId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let token_position = signal
            .payload
            .get("tokenPosition")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let confidence = signal
            .payload
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;

        self.activations.push(AdapterActivation {
            adapter_id: adapter_id.clone(),
            token_position,
            confidence,
            timestamp: signal.timestamp,
        });

        debug!(
            "Adapter activated: {} at position {} (confidence: {:.3})",
            adapter_id, token_position, confidence
        );

        Ok(())
    }

    fn signal_types(&self) -> Vec<SignalType> {
        vec![SignalType::AdapterActivate]
    }
}

impl Default for AdapterActivationHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for evidence signals (Specification §5.4)
///
/// Processes evidence retrieval, citation, and insufficiency signals.
/// Enforces Evidence Ruleset #4 requirements.
///
/// Citation: docs/llm-interface-specification.md §5.4
pub struct EvidenceHandler {
    /// Evidence spans cited during generation
    cited_spans: Vec<String>,

    /// Evidence insufficiency events
    insufficiency_count: usize,

    /// Minimum evidence spans required (from policy)
    min_spans_required: usize,
}

impl EvidenceHandler {
    pub fn new(min_spans_required: usize) -> Self {
        Self {
            cited_spans: Vec::new(),
            insufficiency_count: 0,
            min_spans_required,
        }
    }

    /// Get cited evidence spans
    pub fn get_cited_spans(&self) -> &[String] {
        &self.cited_spans
    }

    /// Check if sufficient evidence was cited
    pub fn has_sufficient_evidence(&self) -> bool {
        self.cited_spans.len() >= self.min_spans_required
    }

    /// Get insufficiency event count
    pub fn get_insufficiency_count(&self) -> usize {
        self.insufficiency_count
    }

    /// Clear evidence state
    pub fn clear(&mut self) {
        self.cited_spans.clear();
        self.insufficiency_count = 0;
    }
}

#[async_trait]
impl SignalHandler for EvidenceHandler {
    async fn handle_signal(&mut self, signal: &Signal) -> Result<()> {
        match signal.signal_type {
            SignalType::EvidenceCite => {
                // Handle evidence citation (Specification §5.4.1)
                let span_id = signal
                    .payload
                    .get("spanId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                self.cited_spans.push(span_id.clone());

                let citation_type = signal
                    .payload
                    .get("citationType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("direct");

                info!("Evidence cited: {} (type: {})", span_id, citation_type);
            }

            SignalType::EvidenceInsufficient => {
                // Handle insufficient evidence (Specification §5.4.2)
                self.insufficiency_count += 1;

                let retrieved = signal
                    .payload
                    .get("retrievedSpans")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let required = signal
                    .payload
                    .get("requiredSpans")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(self.min_spans_required as u64);

                let confidence = signal
                    .payload
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                warn!(
                    "Evidence insufficient: retrieved={}, required={}, confidence={:.3}",
                    retrieved, required, confidence
                );

                if let Some(reason) = signal.payload.get("reason") {
                    debug!("Insufficiency reason: {:?}", reason);
                }
            }

            SignalType::EvidenceRequired => {
                debug!("Evidence required for current query");
            }

            _ => {}
        }

        Ok(())
    }

    fn signal_types(&self) -> Vec<SignalType> {
        vec![
            SignalType::EvidenceCite,
            SignalType::EvidenceInsufficient,
            SignalType::EvidenceRequired,
        ]
    }
}

/// Handler for policy signals (Specification §5.5)
///
/// Processes policy checks, violations, and refusal intent signals.
/// Enforces Policy Ruleset requirements.
///
/// Citation: docs/llm-interface-specification.md §5.5
pub struct PolicyHandler {
    /// Policy violations recorded during inference
    violations: Vec<PolicyViolation>,

    /// Refusal intents signaled
    refusal_signaled: bool,

    /// Last refusal reason
    refusal_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PolicyViolation {
    pub policy_type: String,
    pub severity: String,
    pub message: String,
    pub timestamp: u128,
}

impl PolicyHandler {
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
            refusal_signaled: false,
            refusal_reason: None,
        }
    }

    /// Check if refusal was signaled
    pub fn should_refuse(&self) -> bool {
        self.refusal_signaled
    }

    /// Get refusal reason if available
    pub fn get_refusal_reason(&self) -> Option<&str> {
        self.refusal_reason.as_deref()
    }

    /// Get policy violations
    pub fn get_violations(&self) -> &[PolicyViolation] {
        &self.violations
    }

    /// Clear policy state
    pub fn clear(&mut self) {
        self.violations.clear();
        self.refusal_signaled = false;
        self.refusal_reason = None;
    }
}

#[async_trait]
impl SignalHandler for PolicyHandler {
    async fn handle_signal(&mut self, signal: &Signal) -> Result<()> {
        match signal.signal_type {
            SignalType::RefusalIntent => {
                // Handle refusal intent (Specification §5.5.1)
                self.refusal_signaled = true;

                let reason = signal
                    .payload
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unspecified");

                self.refusal_reason = Some(reason.to_string());

                let confidence = signal
                    .payload
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                warn!(
                    "Refusal intent signaled: reason={}, confidence={:.3}",
                    reason, confidence
                );

                // Log missing fields if provided
                if let Some(missing) = signal.payload.get("missingFields") {
                    if let Some(fields) = missing.as_array() {
                        let field_names: Vec<_> =
                            fields.iter().filter_map(|v| v.as_str()).collect();
                        debug!("Missing fields: {:?}", field_names);
                    }
                }
            }

            SignalType::PolicyViolation => {
                let policy_type = signal
                    .payload
                    .get("policyType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let severity = signal
                    .payload
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("error")
                    .to_string();

                let message = signal
                    .payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                self.violations.push(PolicyViolation {
                    policy_type: policy_type.clone(),
                    severity: severity.clone(),
                    message: message.clone(),
                    timestamp: signal.timestamp,
                });

                warn!(
                    "Policy violation: {} (severity: {}): {}",
                    policy_type, severity, message
                );
            }

            SignalType::PolicyCheck => {
                debug!("Policy check requested");
            }

            _ => {}
        }

        Ok(())
    }

    fn signal_types(&self) -> Vec<SignalType> {
        vec![
            SignalType::PolicyCheck,
            SignalType::PolicyViolation,
            SignalType::RefusalIntent,
        ]
    }
}

impl Default for PolicyHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for memory pressure signals (Specification §8.2)
///
/// Responds to memory pressure events by coordinating with lifecycle
/// management and adapter eviction.
///
/// Citation: docs/llm-interface-specification.md §8.2
pub struct MemoryPressureHandler {
    /// Memory pressure events recorded
    pressure_events: Vec<MemoryPressureEvent>,
}

#[derive(Debug, Clone)]
pub struct MemoryPressureEvent {
    pub level: String,
    pub recommendation: Option<String>,
    pub timestamp: u128,
}

impl MemoryPressureHandler {
    pub fn new() -> Self {
        Self {
            pressure_events: Vec::new(),
        }
    }

    /// Get pressure events
    pub fn get_pressure_events(&self) -> &[MemoryPressureEvent] {
        &self.pressure_events
    }

    /// Check if critical pressure was observed
    pub fn has_critical_pressure(&self) -> bool {
        self.pressure_events.iter().any(|e| e.level == "critical")
    }

    /// Clear pressure events
    pub fn clear(&mut self) {
        self.pressure_events.clear();
    }
}

#[async_trait]
impl SignalHandler for MemoryPressureHandler {
    async fn handle_signal(&mut self, signal: &Signal) -> Result<()> {
        let level = signal
            .payload
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let recommendation = signal
            .payload
            .get("recommendation")
            .and_then(|v| v.as_str())
            .map(String::from);

        self.pressure_events.push(MemoryPressureEvent {
            level: level.clone(),
            recommendation: recommendation.clone(),
            timestamp: signal.timestamp,
        });

        match level.as_str() {
            "critical" => {
                warn!("Critical memory pressure detected");
                if let Some(rec) = recommendation {
                    warn!("Recommendation: {}", rec);
                }
            }
            "high" => {
                warn!("High memory pressure");
            }
            _ => {
                debug!("Memory pressure: {}", level);
            }
        }

        Ok(())
    }

    fn signal_types(&self) -> Vec<SignalType> {
        vec![SignalType::MemoryPressure]
    }
}

impl Default for MemoryPressureHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_adapter_request_handler() {
        let mut handler = AdapterRequestHandler::new();

        let signal = SignalBuilder::new(SignalType::AdapterRequest)
            .with_field(
                "preferredAdapters",
                serde_json::json!(["adapter-1", "adapter-2"]),
            )
            .build();

        handler.handle_signal(&signal).await.expect("Test signal handling should succeed");

        assert_eq!(handler.get_preferred_adapters().len(), 2);
        assert_eq!(handler.get_preferred_adapters()[0], "adapter-1");
    }

    #[tokio::test]
    async fn test_evidence_handler() {
        let mut handler = EvidenceHandler::new(1);

        let cite_signal = SignalBuilder::new(SignalType::EvidenceCite)
            .with_field("spanId", "doc_123:span_456".into())
            .with_field("citationType", "direct".into())
            .build();

        handler.handle_signal(&cite_signal).await.expect("Test signal handling should succeed");

        assert_eq!(handler.get_cited_spans().len(), 1);
        assert!(handler.has_sufficient_evidence());
    }

    #[tokio::test]
    async fn test_policy_handler() {
        let mut handler = PolicyHandler::new();

        let refusal_signal = SignalBuilder::new(SignalType::RefusalIntent)
            .priority(SignalPriority::High)
            .with_field("reason", "insufficient_evidence".into())
            .with_field("confidence", 0.35.into())
            .build();

        handler.handle_signal(&refusal_signal).await.expect("Test signal handling should succeed");

        assert!(handler.should_refuse());
        assert_eq!(handler.get_refusal_reason(), Some("insufficient_evidence"));
    }
}
