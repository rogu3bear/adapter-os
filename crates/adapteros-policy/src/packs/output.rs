//! Output Policy Pack
//!
//! Enforces LLM output format, safety, and citation requirements
//! for AdapterOS responses.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Output policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Required output format
    pub format: OutputFormat,
    /// Whether trace is required
    pub require_trace: bool,
    /// Forbidden topic classes
    pub forbidden_topics: Vec<String>,
    /// Required citation fields
    pub required_citation_fields: Vec<String>,
    /// Maximum response length
    pub max_response_length: usize,
    /// Minimum confidence threshold
    pub min_confidence_threshold: f64,
}

/// Output formats
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OutputFormat {
    /// JSON format
    Json,
    /// Plain text format
    PlainText,
    /// Markdown format
    Markdown,
    /// Structured format
    Structured,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Json,
            require_trace: true,
            forbidden_topics: vec![
                "tenant_crossing".to_string(),
                "export_control_bypass".to_string(),
                "security_vulnerability".to_string(),
                "unauthorized_access".to_string(),
            ],
            required_citation_fields: vec![
                "evidence_refs".to_string(),
                "router_summary".to_string(),
                "confidence_score".to_string(),
                "trace_id".to_string(),
            ],
            max_response_length: 10000,
            min_confidence_threshold: 0.7,
        }
    }
}

/// LLM output structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmOutput {
    pub output_id: String,
    pub content: String,
    pub format: OutputFormat,
    pub trace: Option<OutputTrace>,
    pub confidence_score: f64,
    pub citations: Vec<Citation>,
    pub metadata: OutputMetadata,
    pub timestamp: u64,
}

/// Output trace information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputTrace {
    pub trace_id: String,
    pub evidence_refs: Vec<String>,
    pub router_summary: RouterSummary,
    pub execution_time_ms: u64,
    pub memory_usage_mb: u64,
    pub adapter_ids: Vec<String>,
}

/// Router summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterSummary {
    pub top_adapters: Vec<AdapterInfo>,
    pub gates: Vec<f64>,
    pub entropy: f64,
    pub k_value: u32,
}

/// Adapter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub adapter_id: String,
    pub adapter_name: String,
    pub gate_value: f64,
    pub rank: u32,
}

/// Citation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub citation_id: String,
    pub source: String,
    pub page_number: Option<u32>,
    pub confidence: f64,
    pub relevance_score: f64,
}

/// Output metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputMetadata {
    pub model_id: String,
    pub session_id: String,
    pub tenant_id: String,
    pub request_id: String,
    pub custom_fields: HashMap<String, serde_json::Value>,
}

/// Content safety check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyCheckResult {
    pub is_safe: bool,
    pub violations: Vec<SafetyViolation>,
    pub warnings: Vec<String>,
    pub risk_score: f64,
}

/// Safety violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyViolation {
    pub violation_type: SafetyViolationType,
    pub severity: SafetySeverity,
    pub description: String,
    pub position: Option<usize>,
}

/// Types of safety violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SafetyViolationType {
    /// Forbidden topic detected
    ForbiddenTopic,
    /// Inappropriate content
    InappropriateContent,
    /// Security risk
    SecurityRisk,
    /// Privacy violation
    PrivacyViolation,
}

/// Safety severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SafetySeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Output policy implementation
pub struct OutputPolicy {
    config: OutputConfig,
}

impl OutputPolicy {
    /// Create new output policy
    pub fn new(config: OutputConfig) -> Self {
        Self { config }
    }

    /// Validate output format
    pub fn validate_format(&self, output: &LlmOutput) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        if output.format != self.config.format {
            errors.push(format!(
                "Output format {:?} does not match required format {:?}",
                output.format, self.config.format
            ));
        }

        // Check JSON format
        if matches!(self.config.format, OutputFormat::Json)
            && serde_json::from_str::<serde_json::Value>(&output.content).is_err()
        {
            errors.push("Output content is not valid JSON".to_string());
        }

        Ok(errors)
    }

    /// Validate trace requirements
    pub fn validate_trace(&self, output: &LlmOutput) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        if self.config.require_trace {
            if let Some(trace) = &output.trace {
                // Check required trace fields
                if trace.trace_id.is_empty() {
                    errors.push("Trace ID is required but empty".to_string());
                }

                if trace.evidence_refs.is_empty() {
                    errors.push("Evidence references are required but empty".to_string());
                }

                if trace.adapter_ids.is_empty() {
                    errors.push("Adapter IDs are required but empty".to_string());
                }

                // Check router summary
                if trace.router_summary.top_adapters.is_empty() {
                    errors.push("Router summary top adapters are required but empty".to_string());
                }

                if trace.router_summary.gates.is_empty() {
                    errors.push("Router summary gates are required but empty".to_string());
                }
            } else {
                errors.push("Trace is required but not provided".to_string());
            }
        }

        Ok(errors)
    }

    /// Perform content safety check
    pub fn check_content_safety(&self, content: &str) -> SafetyCheckResult {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        let mut risk_score: f64 = 0.0;

        // Check for forbidden topics
        for forbidden_topic in &self.config.forbidden_topics {
            if content
                .to_lowercase()
                .contains(&forbidden_topic.to_lowercase())
            {
                violations.push(SafetyViolation {
                    violation_type: SafetyViolationType::ForbiddenTopic,
                    severity: SafetySeverity::High,
                    description: format!("Forbidden topic '{}' detected", forbidden_topic),
                    position: content.to_lowercase().find(&forbidden_topic.to_lowercase()),
                });
                risk_score += 0.3;
            }
        }

        // Check for security risks
        let security_keywords = vec![
            "password",
            "secret",
            "key",
            "token",
            "credential",
            "exploit",
            "vulnerability",
            "backdoor",
            "malware",
        ];

        for keyword in security_keywords {
            if content.to_lowercase().contains(keyword) {
                warnings.push(format!("Security-related keyword '{}' detected", keyword));
                risk_score += 0.1;
            }
        }

        // Check for inappropriate content
        let inappropriate_keywords = vec![
            "hate",
            "discrimination",
            "violence",
            "harassment",
            "illegal",
            "unethical",
            "harmful",
        ];

        for keyword in inappropriate_keywords {
            if content.to_lowercase().contains(keyword) {
                violations.push(SafetyViolation {
                    violation_type: SafetyViolationType::InappropriateContent,
                    severity: SafetySeverity::Medium,
                    description: format!("Inappropriate content keyword '{}' detected", keyword),
                    position: content.to_lowercase().find(keyword),
                });
                risk_score += 0.2;
            }
        }

        // Check response length
        if content.len() > self.config.max_response_length {
            warnings.push(format!(
                "Response length {} exceeds maximum {}",
                content.len(),
                self.config.max_response_length
            ));
        }

        SafetyCheckResult {
            is_safe: violations.is_empty(),
            violations,
            warnings,
            risk_score: risk_score.min(1.0),
        }
    }

    /// Validate citations
    pub fn validate_citations(&self, output: &LlmOutput) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        // Check required citation fields
        for required_field in &self.config.required_citation_fields {
            match required_field.as_str() {
                "evidence_refs" => {
                    if let Some(trace) = &output.trace {
                        if trace.evidence_refs.is_empty() {
                            errors.push("Evidence references are required but empty".to_string());
                        }
                    } else {
                        errors.push("Trace is required for evidence references".to_string());
                    }
                }
                "router_summary" => {
                    if let Some(trace) = &output.trace {
                        if trace.router_summary.top_adapters.is_empty() {
                            errors.push("Router summary is required but empty".to_string());
                        }
                    } else {
                        errors.push("Trace is required for router summary".to_string());
                    }
                }
                "confidence_score" => {
                    if output.confidence_score < self.config.min_confidence_threshold {
                        errors.push(format!(
                            "Confidence score {:.4} below minimum {:.4}",
                            output.confidence_score, self.config.min_confidence_threshold
                        ));
                    }
                }
                "trace_id" => {
                    if let Some(trace) = &output.trace {
                        if trace.trace_id.is_empty() {
                            errors.push("Trace ID is required but empty".to_string());
                        }
                    } else {
                        errors.push("Trace is required for trace ID".to_string());
                    }
                }
                _ => {}
            }
        }

        // Validate individual citations
        for citation in &output.citations {
            if citation.citation_id.is_empty() {
                errors.push("Citation ID is required but empty".to_string());
            }

            if citation.source.is_empty() {
                errors.push("Citation source is required but empty".to_string());
            }

            if citation.confidence < 0.0 || citation.confidence > 1.0 {
                errors.push(format!(
                    "Citation confidence {:.4} must be between 0 and 1",
                    citation.confidence
                ));
            }
        }

        Ok(errors)
    }

    /// Validate output configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.max_response_length == 0 {
            return Err(AosError::PolicyViolation(
                "Maximum response length must be greater than 0".to_string(),
            ));
        }

        if self.config.min_confidence_threshold < 0.0 || self.config.min_confidence_threshold > 1.0
        {
            return Err(AosError::PolicyViolation(
                "Minimum confidence threshold must be between 0 and 1".to_string(),
            ));
        }

        if self.config.required_citation_fields.is_empty() {
            return Err(AosError::PolicyViolation(
                "At least one citation field must be required".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for output policy enforcement
#[derive(Debug)]
pub struct OutputContext {
    pub outputs: Vec<LlmOutput>,
    pub tenant_id: String,
    pub session_id: String,
    pub operation: OutputOperation,
}

/// Types of output operations
#[derive(Debug)]
pub enum OutputOperation {
    /// Output generation
    Generation,
    /// Output validation
    Validation,
    /// Output filtering
    Filtering,
    /// Output formatting
    Formatting,
    /// Output safety check
    SafetyCheck,
}

impl PolicyContext for OutputContext {
    fn context_type(&self) -> &str {
        "output"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for OutputPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Output
    }

    fn name(&self) -> &'static str {
        "Output"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let output_ctx = ctx
            .as_any()
            .downcast_ref::<OutputContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid output context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Validate each output
        for output in &output_ctx.outputs {
            // Validate format
            match self.validate_format(output) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::Medium,
                            message: format!("Output format validation failed: {}", error),
                            details: Some(format!("Output ID: {}", output.output_id)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::Medium,
                        message: "Output format validation error".to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }

            // Validate trace
            match self.validate_trace(output) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::High,
                            message: format!("Trace validation failed: {}", error),
                            details: Some(format!("Output ID: {}", output.output_id)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: "Trace validation error".to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }

            // Check content safety
            let safety_result = self.check_content_safety(&output.content);
            if !safety_result.is_safe {
                for violation in safety_result.violations {
                    let severity = match violation.severity {
                        SafetySeverity::Low => Severity::Low,
                        SafetySeverity::Medium => Severity::Medium,
                        SafetySeverity::High => Severity::High,
                        SafetySeverity::Critical => Severity::Critical,
                    };

                    violations.push(Violation {
                        severity,
                        message: format!("Content safety violation: {}", violation.description),
                        details: Some(format!(
                            "Output ID: {}, Risk score: {:.4}",
                            output.output_id, safety_result.risk_score
                        )),
                    });
                }
            }

            for warning in safety_result.warnings {
                warnings.push(format!("Output {}: {}", output.output_id, warning));
            }

            // Validate citations
            match self.validate_citations(output) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::Medium,
                            message: format!("Citation validation failed: {}", error),
                            details: Some(format!("Output ID: {}", output.output_id)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::Medium,
                        message: "Citation validation error".to_string(),
                        details: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(Audit {
            policy_id: PolicyId::Output,
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_config_default() {
        let config = OutputConfig::default();
        assert!(matches!(config.format, OutputFormat::Json));
        assert!(config.require_trace);
        assert!(!config.forbidden_topics.is_empty());
        assert!(!config.required_citation_fields.is_empty());
    }

    #[test]
    fn test_output_policy_creation() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Output);
    }

    #[test]
    fn test_content_safety_check() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        let content = "This is a test about tenant_crossing and security vulnerabilities";
        let result = policy.check_content_safety(content);

        assert!(!result.is_safe);
        assert!(!result.violations.is_empty());
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, SafetyViolationType::ForbiddenTopic)));
    }

    #[test]
    fn test_output_format_validation() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        let output = LlmOutput {
            output_id: "test_output".to_string(),
            content: "invalid json".to_string(),
            format: OutputFormat::Json,
            trace: None,
            confidence_score: 0.8,
            citations: vec![],
            metadata: OutputMetadata {
                model_id: "test_model".to_string(),
                session_id: "test_session".to_string(),
                tenant_id: "test_tenant".to_string(),
                request_id: "test_request".to_string(),
                custom_fields: HashMap::new(),
            },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let errors = policy.validate_format(&output).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("not valid JSON")));
    }

    #[test]
    fn test_trace_validation() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        let output = LlmOutput {
            output_id: "test_output".to_string(),
            content: "{}".to_string(),
            format: OutputFormat::Json,
            trace: None, // Should fail
            confidence_score: 0.8,
            citations: vec![],
            metadata: OutputMetadata {
                model_id: "test_model".to_string(),
                session_id: "test_session".to_string(),
                tenant_id: "test_tenant".to_string(),
                request_id: "test_request".to_string(),
                custom_fields: HashMap::new(),
            },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let errors = policy.validate_trace(&output).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("Trace is required")));
    }

    #[test]
    fn test_output_config_validation() {
        let mut config = OutputConfig::default();
        config.max_response_length = 0; // Invalid
        let policy = OutputPolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
