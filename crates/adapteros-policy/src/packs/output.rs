//! Output Policy Pack
//!
//! Enforces LLM output format, safety, and citation requirements
//! for AdapterOS responses.
//!
//! Includes length enforcement with actual truncation (not just warnings).

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Preferred response length levels
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ResponseLength {
    /// Brief response (max 500 characters)
    Brief,
    /// Standard response (max 2000 characters)
    #[default]
    Standard,
    /// Detailed response (max 10000 characters)
    Detailed,
    /// Custom length limit
    Custom(usize),
}

impl ResponseLength {
    /// Get the character limit for this response length
    pub fn char_limit(&self) -> usize {
        match self {
            ResponseLength::Brief => 500,
            ResponseLength::Standard => 2000,
            ResponseLength::Detailed => 10000,
            ResponseLength::Custom(limit) => *limit,
        }
    }
}

/// Result of length enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LengthEnforcementResult {
    /// The (possibly truncated) content
    pub content: String,
    /// Whether truncation occurred
    pub was_truncated: bool,
    /// Original length before truncation
    pub original_length: usize,
    /// Final length after enforcement
    pub final_length: usize,
    /// Truncation indicator appended (if any)
    pub truncation_indicator: Option<String>,
}

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
    /// Procedural harm - instructions that enable wrongdoing
    ProceduralHarm,
    /// Hate speech targeting protected classes
    HateSpeech,
    /// Extremist or recruitment content
    ExtremistContent,
}

/// Category of harmful content
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HarmCategory {
    /// Procedural instructions for harmful activities
    Procedural,
    /// Informational content about harmful topics (may be educational)
    Informational,
    /// No harmful content detected
    None,
}

/// Procedural harm detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralHarmResult {
    /// Whether procedural harm was detected
    pub detected: bool,
    /// The category of harm
    pub category: HarmCategory,
    /// Matched patterns
    pub matched_patterns: Vec<String>,
    /// Risk level (0.0 to 1.0)
    pub risk_level: f64,
}

/// Protected class categories for hate speech detection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProtectedClass {
    Race,
    Religion,
    Gender,
    Sexuality,
    Disability,
    Nationality,
    Age,
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

    /// Enforce length constraints on content
    /// Truncates at sentence boundary when possible, appends indicator if truncated
    pub fn enforce_length(
        &self,
        content: &str,
        preferred_length: Option<ResponseLength>,
    ) -> LengthEnforcementResult {
        let limit = preferred_length
            .map(|l| l.char_limit())
            .unwrap_or(self.config.max_response_length);

        let original_length = content.len();

        if original_length <= limit {
            return LengthEnforcementResult {
                content: content.to_string(),
                was_truncated: false,
                original_length,
                final_length: original_length,
                truncation_indicator: None,
            };
        }

        // Try to truncate at sentence boundary
        let truncation_indicator = " [...]".to_string();
        let target_length = limit.saturating_sub(truncation_indicator.len());

        // Find the best truncation point (end of sentence)
        let truncation_point = Self::find_sentence_boundary(content, target_length);

        let truncated = format!("{}{}", &content[..truncation_point], truncation_indicator);
        let final_length = truncated.len();

        LengthEnforcementResult {
            content: truncated,
            was_truncated: true,
            original_length,
            final_length,
            truncation_indicator: Some(truncation_indicator),
        }
    }

    /// Find the best sentence boundary for truncation
    fn find_sentence_boundary(content: &str, max_pos: usize) -> usize {
        if max_pos >= content.len() {
            return content.len();
        }

        // Look for sentence-ending punctuation before max_pos
        let search_range = &content[..max_pos];

        // Find the last sentence boundary (. ! ?)
        let sentence_endings = [". ", "! ", "? ", ".\n", "!\n", "?\n"];

        let mut best_pos = 0;
        for ending in sentence_endings {
            if let Some(pos) = search_range.rfind(ending) {
                let end_pos = pos + ending.len() - 1; // Include the punctuation but not the space
                if end_pos > best_pos {
                    best_pos = end_pos;
                }
            }
        }

        // If no sentence boundary found, try to find word boundary
        if best_pos == 0 {
            if let Some(pos) = search_range.rfind(' ') {
                best_pos = pos;
            } else {
                // Last resort: just use the max_pos
                best_pos = max_pos;
            }
        }

        best_pos
    }

    /// Count sentences in content
    pub fn count_sentences(content: &str) -> usize {
        content
            .chars()
            .filter(|c| *c == '.' || *c == '!' || *c == '?')
            .count()
    }

    /// Count paragraphs in content
    pub fn count_paragraphs(content: &str) -> usize {
        content
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count()
    }

    /// Count words in content
    pub fn count_words(content: &str) -> usize {
        content.split_whitespace().count()
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

        // Check for procedural harm (instructions that enable wrongdoing)
        let procedural_harm = self.check_procedural_harm(content);
        if procedural_harm.detected {
            for pattern in &procedural_harm.matched_patterns {
                violations.push(SafetyViolation {
                    violation_type: SafetyViolationType::ProceduralHarm,
                    severity: SafetySeverity::Critical,
                    description: format!(
                        "Procedural harm detected: instructions for '{}'",
                        pattern
                    ),
                    position: None,
                });
            }
            risk_score += procedural_harm.risk_level;
        }

        // Check for hate speech targeting protected classes
        let hate_speech = self.check_hate_speech(content);
        if !hate_speech.is_empty() {
            for (class, description) in &hate_speech {
                violations.push(SafetyViolation {
                    violation_type: SafetyViolationType::HateSpeech,
                    severity: SafetySeverity::Critical,
                    description: format!("Hate speech targeting {:?}: {}", class, description),
                    position: None,
                });
                risk_score += 0.4;
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

    /// Check for procedural harm - instructions that enable wrongdoing
    ///
    /// Detects patterns like "how to hack", "steps to build a weapon", etc.
    /// Returns a result indicating whether procedural harm was detected.
    pub fn check_procedural_harm(&self, content: &str) -> ProceduralHarmResult {
        let content_lower = content.to_lowercase();
        let mut matched_patterns = Vec::new();
        let mut risk_level: f64 = 0.0;

        // Procedural patterns that indicate harmful instructions
        let procedural_patterns = [
            ("how to hack", "hacking"),
            ("how to exploit", "exploitation"),
            ("how to attack", "cyber attack"),
            ("how to bypass", "security bypass"),
            ("steps to build a weapon", "weapon construction"),
            ("steps to build a bomb", "explosive construction"),
            ("steps to make a bomb", "explosive construction"),
            ("instructions for making", "harmful manufacturing"),
            ("how to synthesize", "chemical synthesis"),
            ("how to manufacture drugs", "drug manufacturing"),
            ("how to create malware", "malware creation"),
            ("how to write a virus", "virus creation"),
            ("how to steal", "theft instructions"),
            ("how to break into", "break-in instructions"),
            ("how to pick a lock", "lock picking"),
            ("how to evade detection", "evasion techniques"),
            ("how to launder money", "money laundering"),
            ("how to commit fraud", "fraud instructions"),
            ("how to forge", "forgery instructions"),
        ];

        for (pattern, category) in procedural_patterns {
            if content_lower.contains(pattern) {
                matched_patterns.push(category.to_string());
                risk_level += 0.5;
            }
        }

        // Additional high-risk patterns
        let high_risk_indicators = [
            "step by step",
            "step-by-step",
            "detailed instructions",
            "complete guide to",
            "tutorial on how to",
        ];

        let dangerous_topics = [
            "weapon",
            "bomb",
            "explosive",
            "poison",
            "drug synthesis",
            "hack",
            "malware",
            "ransomware",
            "phishing",
        ];

        // Check for combination of instructional language + dangerous topic
        let has_instructional = high_risk_indicators
            .iter()
            .any(|ind| content_lower.contains(ind));
        let has_dangerous = dangerous_topics
            .iter()
            .any(|topic| content_lower.contains(topic));

        if has_instructional && has_dangerous {
            matched_patterns.push("instructional_dangerous_combination".to_string());
            risk_level += 0.4;
        }

        let detected = !matched_patterns.is_empty();
        let category = if detected {
            HarmCategory::Procedural
        } else {
            HarmCategory::None
        };

        ProceduralHarmResult {
            detected,
            category,
            matched_patterns,
            risk_level: risk_level.min(1.0),
        }
    }

    /// Check for hate speech targeting protected classes
    ///
    /// Detects dehumanizing language, slurs, and targeted attacks against
    /// protected groups (race, religion, gender, sexuality, disability, nationality).
    pub fn check_hate_speech(&self, content: &str) -> Vec<(ProtectedClass, String)> {
        let content_lower = content.to_lowercase();
        let mut detections = Vec::new();

        // Dehumanization patterns
        let dehumanization_terms = [
            "vermin",
            "subhuman",
            "plague",
            "infestation",
            "cockroaches",
            "parasites",
            "filth",
            "scum",
        ];

        // Protected class indicators
        let race_indicators = ["race", "racial", "ethnic"];
        let religion_indicators = ["muslim", "jewish", "christian", "hindu", "religious"];
        let gender_indicators = ["women", "men", "female", "male", "transgender", "trans"];
        let sexuality_indicators = ["gay", "lesbian", "homosexual", "lgbtq", "queer"];
        let disability_indicators = ["disabled", "handicapped"];
        let nationality_indicators = ["immigrant", "foreigner", "refugee", "migrant"];

        // Check for dehumanization combined with protected class
        for dehumanization in dehumanization_terms {
            if content_lower.contains(dehumanization) {
                if race_indicators.iter().any(|i| content_lower.contains(i)) {
                    detections.push((
                        ProtectedClass::Race,
                        format!("Dehumanizing language '{}' targeting race", dehumanization),
                    ));
                }
                if religion_indicators
                    .iter()
                    .any(|i| content_lower.contains(i))
                {
                    detections.push((
                        ProtectedClass::Religion,
                        format!(
                            "Dehumanizing language '{}' targeting religion",
                            dehumanization
                        ),
                    ));
                }
                if gender_indicators.iter().any(|i| content_lower.contains(i)) {
                    detections.push((
                        ProtectedClass::Gender,
                        format!(
                            "Dehumanizing language '{}' targeting gender",
                            dehumanization
                        ),
                    ));
                }
                if sexuality_indicators
                    .iter()
                    .any(|i| content_lower.contains(i))
                {
                    detections.push((
                        ProtectedClass::Sexuality,
                        format!(
                            "Dehumanizing language '{}' targeting sexuality",
                            dehumanization
                        ),
                    ));
                }
                if disability_indicators
                    .iter()
                    .any(|i| content_lower.contains(i))
                {
                    detections.push((
                        ProtectedClass::Disability,
                        format!(
                            "Dehumanizing language '{}' targeting disability",
                            dehumanization
                        ),
                    ));
                }
                if nationality_indicators
                    .iter()
                    .any(|i| content_lower.contains(i))
                {
                    detections.push((
                        ProtectedClass::Nationality,
                        format!(
                            "Dehumanizing language '{}' targeting nationality",
                            dehumanization
                        ),
                    ));
                }
            }
        }

        // Check for explicit calls for violence against protected groups
        let violence_indicators = [
            "kill all",
            "exterminate",
            "eliminate all",
            "get rid of all",
            "purge",
        ];

        for violence in violence_indicators {
            if content_lower.contains(violence) {
                if race_indicators.iter().any(|i| content_lower.contains(i)) {
                    detections.push((
                        ProtectedClass::Race,
                        format!("Violence incitement '{}' targeting race", violence),
                    ));
                }
                if religion_indicators
                    .iter()
                    .any(|i| content_lower.contains(i))
                {
                    detections.push((
                        ProtectedClass::Religion,
                        format!("Violence incitement '{}' targeting religion", violence),
                    ));
                }
            }
        }

        detections
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

    #[test]
    fn test_enforce_length_no_truncation() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        let short_content = "This is a short response.";
        let result = policy.enforce_length(short_content, None);

        assert!(!result.was_truncated);
        assert_eq!(result.content, short_content);
        assert!(result.truncation_indicator.is_none());
    }

    #[test]
    fn test_enforce_length_with_truncation() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        // Create content longer than Brief limit (500 chars)
        let long_content = "This is a test sentence. ".repeat(50); // ~1250 chars
        let result = policy.enforce_length(&long_content, Some(ResponseLength::Brief));

        assert!(result.was_truncated);
        assert!(result.final_length <= 500);
        assert!(result.content.ends_with("[...]"));
        assert!(result.truncation_indicator.is_some());
    }

    #[test]
    fn test_enforce_length_sentence_boundary() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        let content = "First sentence. Second sentence. Third sentence. Fourth sentence.";
        let result = policy.enforce_length(content, Some(ResponseLength::Custom(40)));

        // Should truncate at a sentence boundary
        assert!(result.was_truncated);
        assert!(result.content.contains("First sentence."));
    }

    #[test]
    fn test_response_length_limits() {
        assert_eq!(ResponseLength::Brief.char_limit(), 500);
        assert_eq!(ResponseLength::Standard.char_limit(), 2000);
        assert_eq!(ResponseLength::Detailed.char_limit(), 10000);
        assert_eq!(ResponseLength::Custom(1500).char_limit(), 1500);
    }

    #[test]
    fn test_count_sentences() {
        let content = "First sentence. Second sentence! Third sentence? Fourth.";
        assert_eq!(OutputPolicy::count_sentences(content), 4);
    }

    #[test]
    fn test_count_paragraphs() {
        let content = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
        assert_eq!(OutputPolicy::count_paragraphs(content), 3);
    }

    #[test]
    fn test_count_words() {
        let content = "This is a test with seven words.";
        assert_eq!(OutputPolicy::count_words(content), 7);
    }

    #[test]
    fn test_procedural_harm_detection() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        // Should detect procedural harm
        let result = policy.check_procedural_harm("Here's how to hack into a system");
        assert!(result.detected);
        assert_eq!(result.category, HarmCategory::Procedural);
        assert!(!result.matched_patterns.is_empty());

        // Should detect weapon instructions
        let result = policy.check_procedural_harm("Steps to build a bomb at home");
        assert!(result.detected);
        assert!(result
            .matched_patterns
            .iter()
            .any(|p| p.contains("explosive")));

        // Should detect combination of instructional + dangerous
        let result =
            policy.check_procedural_harm("Here's a step by step guide to creating malware");
        assert!(result.detected);

        // Should NOT flag benign content
        let result = policy.check_procedural_harm("How to cook a delicious meal");
        assert!(!result.detected);
        assert_eq!(result.category, HarmCategory::None);
    }

    #[test]
    fn test_hate_speech_detection() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        // Should detect hate speech with dehumanization + protected class
        let result = policy.check_hate_speech("Those immigrants are vermin");
        assert!(!result.is_empty());
        assert!(result
            .iter()
            .any(|(class, _)| *class == ProtectedClass::Nationality));

        // Should detect religious targeting
        let result = policy.check_hate_speech("Muslim people are parasites");
        assert!(!result.is_empty());
        assert!(result
            .iter()
            .any(|(class, _)| *class == ProtectedClass::Religion));

        // Should detect violence incitement
        let result = policy.check_hate_speech("We should exterminate all religious people");
        assert!(!result.is_empty());

        // Should NOT flag neutral content
        let result = policy.check_hate_speech("The conference discussed immigration policy");
        assert!(result.is_empty());
    }

    #[test]
    fn test_content_safety_includes_new_checks() {
        let config = OutputConfig::default();
        let policy = OutputPolicy::new(config);

        // Should detect procedural harm in safety check
        let result = policy.check_content_safety("Learn how to hack into bank systems");
        assert!(!result.is_safe);
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, SafetyViolationType::ProceduralHarm)));

        // Should detect hate speech in safety check
        let result =
            policy.check_content_safety("Those immigrants are vermin and should be purged");
        assert!(!result.is_safe);
        assert!(result
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, SafetyViolationType::HateSpeech)));
    }
}
