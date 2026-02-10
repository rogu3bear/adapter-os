//! Review Trigger Detection for Human-in-the-Loop Protocol
//!
//! Detects when inference should pause for human review based on:
//! - Explicit `<review>` tags in model output
//! - Uncertainty signals (low confidence, hedging language)
//! - Complexity thresholds (long reasoning chains, nested logic)
//!
//! Integrates with `reasoning_router::StreamInspector` for token-level detection
//! and `inference_pause` for pause/resume coordination.

use std::collections::HashSet;

use tracing::{debug, info};

use crate::inference_pause::{pause_for_code_review, InferencePauseToken};
use adapteros_api_types::review::ReviewContext;

/// Configuration for review trigger detection.
#[derive(Debug, Clone)]
pub struct ReviewTriggerConfig {
    /// Enable explicit tag detection (e.g., `<review>`, `<human-review>`)
    pub detect_explicit_tags: bool,

    /// Enable uncertainty signal detection
    pub detect_uncertainty: bool,

    /// Enable complexity threshold detection
    pub detect_complexity: bool,

    /// Minimum characters before complexity check kicks in
    pub complexity_char_threshold: usize,

    /// Number of nested reasoning markers that triggers review
    pub nested_depth_threshold: usize,

    /// Custom review tags to detect (in addition to defaults)
    pub custom_tags: Vec<String>,
}

impl Default for ReviewTriggerConfig {
    fn default() -> Self {
        Self {
            detect_explicit_tags: true,
            detect_uncertainty: true,
            detect_complexity: true,
            complexity_char_threshold: 2000,
            nested_depth_threshold: 3,
            custom_tags: Vec::new(),
        }
    }
}

/// Types of triggers that can pause inference for review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewTriggerKind {
    /// Explicit tag in output (e.g., `<review>`)
    ExplicitTag { tag: String },

    /// Uncertainty signals detected (hedging, low confidence)
    UncertaintySignal { signals: Vec<String> },

    /// Complexity threshold exceeded
    ComplexityThreshold {
        char_count: usize,
        nested_depth: usize,
    },

    /// Custom trigger from external source
    External { source: String },
}

/// A detected review trigger.
#[derive(Debug, Clone)]
pub struct ReviewTrigger {
    pub kind: ReviewTriggerKind,
    pub token_index: usize,
    pub context_preview: String,
    pub confidence: f32,
}

/// Streaming detector for review triggers.
#[derive(Debug)]
pub struct ReviewTriggerDetector {
    config: ReviewTriggerConfig,
    buffer: String,
    token_count: usize,
    nested_depth: usize,
    detected_triggers: Vec<ReviewTrigger>,

    // Pre-compiled patterns
    explicit_tags: HashSet<String>,
    uncertainty_phrases: Vec<&'static str>,
}

impl ReviewTriggerDetector {
    /// Default explicit tags that signal review needed.
    const DEFAULT_TAGS: &'static [&'static str] = &[
        "<review>",
        "</review>",
        "<human-review>",
        "<needs-review>",
        "<pause>",
        "<verify>",
        "REVIEW_NEEDED",
        "HUMAN_CHECK",
    ];

    /// Phrases indicating uncertainty.
    const UNCERTAINTY_PHRASES: &'static [&'static str] = &[
        "I'm not sure",
        "I'm uncertain",
        "this might be wrong",
        "please verify",
        "double-check this",
        "I could be mistaken",
        "this needs verification",
        "uncertain about",
        "low confidence",
        "might not be correct",
        "should be reviewed",
        "not entirely sure",
        "may need adjustment",
    ];

    /// Create a new detector with the given configuration.
    pub fn new(config: ReviewTriggerConfig) -> Self {
        let mut explicit_tags: HashSet<String> = Self::DEFAULT_TAGS
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        for tag in &config.custom_tags {
            explicit_tags.insert(tag.to_lowercase());
        }

        Self {
            config,
            buffer: String::new(),
            token_count: 0,
            nested_depth: 0,
            detected_triggers: Vec::new(),
            explicit_tags,
            uncertainty_phrases: Self::UNCERTAINTY_PHRASES.to_vec(),
        }
    }

    /// Process a streamed token, returning any triggered review.
    pub fn on_token(&mut self, token: &str) -> Option<ReviewTrigger> {
        self.buffer.push_str(token);
        self.token_count += 1;

        // Track nesting depth
        self.update_nesting(token);

        // Check for explicit tags first (most specific)
        if self.config.detect_explicit_tags {
            if let Some(trigger) = self.check_explicit_tag(token) {
                return Some(trigger);
            }
        }

        // Check complexity threshold
        if self.config.detect_complexity {
            if let Some(trigger) = self.check_complexity() {
                return Some(trigger);
            }
        }

        // Check uncertainty signals (only on sentence boundaries)
        if self.config.detect_uncertainty && self.is_sentence_boundary(token) {
            if let Some(trigger) = self.check_uncertainty() {
                return Some(trigger);
            }
        }

        None
    }

    /// Check if the token contains an explicit review tag.
    fn check_explicit_tag(&mut self, token: &str) -> Option<ReviewTrigger> {
        let lower = token.to_lowercase();

        for tag in &self.explicit_tags {
            if lower.contains(tag) {
                let trigger = ReviewTrigger {
                    kind: ReviewTriggerKind::ExplicitTag { tag: tag.clone() },
                    token_index: self.token_count,
                    context_preview: self.context_preview(),
                    confidence: 1.0, // Explicit tags are definitive
                };

                info!(
                    tag = %tag,
                    token_index = self.token_count,
                    "Explicit review tag detected"
                );

                self.detected_triggers.push(trigger.clone());
                return Some(trigger);
            }
        }

        None
    }

    /// Check if complexity threshold is exceeded.
    fn check_complexity(&mut self) -> Option<ReviewTrigger> {
        let char_count = self.buffer.len();
        let exceeds_chars = char_count >= self.config.complexity_char_threshold;
        let exceeds_depth = self.nested_depth >= self.config.nested_depth_threshold;

        if exceeds_chars || exceeds_depth {
            // Only trigger once per threshold crossing
            let already_triggered = self
                .detected_triggers
                .iter()
                .any(|t| matches!(t.kind, ReviewTriggerKind::ComplexityThreshold { .. }));

            if !already_triggered {
                let trigger = ReviewTrigger {
                    kind: ReviewTriggerKind::ComplexityThreshold {
                        char_count,
                        nested_depth: self.nested_depth,
                    },
                    token_index: self.token_count,
                    context_preview: self.context_preview(),
                    confidence: 0.85,
                };

                info!(
                    char_count,
                    nested_depth = self.nested_depth,
                    "Complexity threshold exceeded, triggering review"
                );

                self.detected_triggers.push(trigger.clone());
                return Some(trigger);
            }
        }

        None
    }

    /// Check for uncertainty signals in the buffer.
    fn check_uncertainty(&mut self) -> Option<ReviewTrigger> {
        let lower = self.buffer.to_lowercase();
        let mut found_signals = Vec::new();

        for phrase in &self.uncertainty_phrases {
            if lower.contains(&phrase.to_lowercase()) {
                found_signals.push((*phrase).to_string());
            }
        }

        if found_signals.len() >= 2 {
            // Require multiple signals to reduce false positives
            let trigger = ReviewTrigger {
                kind: ReviewTriggerKind::UncertaintySignal {
                    signals: found_signals.clone(),
                },
                token_index: self.token_count,
                context_preview: self.context_preview(),
                confidence: 0.7 + (found_signals.len() as f32 * 0.05).min(0.25),
            };

            debug!(
                signals = ?found_signals,
                "Uncertainty signals detected"
            );

            self.detected_triggers.push(trigger.clone());
            return Some(trigger);
        }

        None
    }

    /// Update nesting depth based on reasoning markers.
    fn update_nesting(&mut self, token: &str) {
        // Track common nesting markers
        let opens = ["<thinking>", "<reasoning>", "<step>", "```", "{"];
        let closes = ["</thinking>", "</reasoning>", "</step>", "```", "}"];

        for open in opens {
            if token.contains(open) {
                self.nested_depth += 1;
            }
        }

        for close in closes {
            if token.contains(close) && self.nested_depth > 0 {
                self.nested_depth = self.nested_depth.saturating_sub(1);
            }
        }
    }

    /// Check if token represents a sentence boundary.
    fn is_sentence_boundary(&self, token: &str) -> bool {
        token.contains('.') || token.contains('!') || token.contains('?') || token.contains('\n')
    }

    /// Get a preview of the current context.
    fn context_preview(&self) -> String {
        let max_len = 200;
        if self.buffer.len() <= max_len {
            self.buffer.clone()
        } else {
            let mut start = self.buffer.len() - max_len;
            // Ensure we start at a UTF-8 character boundary
            while start < self.buffer.len() && !self.buffer.is_char_boundary(start) {
                start += 1;
            }
            format!("...{}", &self.buffer[start..])
        }
    }

    /// Get all triggers detected so far.
    pub fn triggers(&self) -> &[ReviewTrigger] {
        &self.detected_triggers
    }

    /// Reset the detector state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.token_count = 0;
        self.nested_depth = 0;
        self.detected_triggers.clear();
    }

    /// Create a pause token from a trigger for the pause registry.
    pub fn create_pause_token(
        &self,
        trigger: &ReviewTrigger,
        inference_id: &str,
    ) -> (InferencePauseToken, ReviewContext) {
        let question = match &trigger.kind {
            ReviewTriggerKind::ExplicitTag { tag } => {
                format!("Model requested review via explicit tag: {}", tag)
            }
            ReviewTriggerKind::UncertaintySignal { signals } => {
                format!("Model expressed uncertainty: {}", signals.join(", "))
            }
            ReviewTriggerKind::ComplexityThreshold {
                char_count,
                nested_depth,
            } => {
                format!(
                    "Reasoning complexity threshold exceeded (chars: {}, depth: {})",
                    char_count, nested_depth
                )
            }
            ReviewTriggerKind::External { source } => {
                format!("External review trigger from: {}", source)
            }
        };

        pause_for_code_review(inference_id, &trigger.context_preview, &question)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_explicit_review_tag() {
        let mut detector = ReviewTriggerDetector::new(ReviewTriggerConfig::default());

        // No trigger on normal tokens
        assert!(detector.on_token("Hello ").is_none());
        assert!(detector.on_token("world").is_none());

        // Trigger on explicit tag
        let trigger = detector.on_token(" <review> ").unwrap();
        assert!(matches!(
            trigger.kind,
            ReviewTriggerKind::ExplicitTag { .. }
        ));
        assert_eq!(trigger.confidence, 1.0);
    }

    #[test]
    fn detects_pause_tag() {
        let mut detector = ReviewTriggerDetector::new(ReviewTriggerConfig::default());

        let trigger = detector.on_token("REVIEW_NEEDED").unwrap();
        assert!(matches!(
            trigger.kind,
            ReviewTriggerKind::ExplicitTag { .. }
        ));
    }

    #[test]
    fn detects_uncertainty_signals() {
        let config = ReviewTriggerConfig {
            detect_uncertainty: true,
            ..Default::default()
        };
        let mut detector = ReviewTriggerDetector::new(config);

        // Single uncertainty phrase - not enough
        detector.on_token("I'm not sure about this");
        assert!(detector.on_token(".").is_none());

        // Reset and try with multiple
        detector.reset();
        detector.on_token("I'm not sure about this. Please verify the result");
        let trigger = detector.on_token(".").unwrap();
        assert!(matches!(
            trigger.kind,
            ReviewTriggerKind::UncertaintySignal { .. }
        ));
    }

    #[test]
    fn detects_complexity_threshold() {
        let config = ReviewTriggerConfig {
            complexity_char_threshold: 50, // Low threshold for testing
            detect_complexity: true,
            ..Default::default()
        };
        let mut detector = ReviewTriggerDetector::new(config);

        // Build up to threshold
        detector.on_token(&"a".repeat(40));
        assert!(detector.on_token(&"b".repeat(15)).is_some());
    }

    #[test]
    fn custom_tags_work() {
        let config = ReviewTriggerConfig {
            custom_tags: vec!["<my-review>".to_string()],
            ..Default::default()
        };
        let mut detector = ReviewTriggerDetector::new(config);

        let trigger = detector.on_token("<my-review>").unwrap();
        assert!(matches!(
            trigger.kind,
            ReviewTriggerKind::ExplicitTag { tag } if tag == "<my-review>"
        ));
    }

    #[test]
    fn tracks_nesting_depth() {
        let config = ReviewTriggerConfig {
            nested_depth_threshold: 3, // Trigger at depth 3
            detect_complexity: true,
            complexity_char_threshold: 1000000, // Disable char threshold
            ..Default::default()
        };
        let mut detector = ReviewTriggerDetector::new(config);

        // depth=1 after this
        assert!(detector.on_token("<thinking>").is_none());
        assert!(detector.triggers().is_empty());

        // depth=2 after this, still below threshold
        assert!(detector.on_token("<reasoning>").is_none());

        // depth=3 after this, triggers at threshold
        let trigger = detector.on_token("<step>").unwrap();
        assert!(matches!(
            trigger.kind,
            ReviewTriggerKind::ComplexityThreshold {
                nested_depth: 3,
                ..
            }
        ));
    }

    #[test]
    fn context_preview_handles_multibyte_utf8() {
        let config = ReviewTriggerConfig::default();
        let mut detector = ReviewTriggerDetector::new(config);

        // Feed multilingual text with multi-byte UTF-8 characters
        // Chinese, Thai, Arabic, etc. are multi-byte
        let multilingual =
            "批准 eBook铊 Markus桀 低声 Hương打败批评นั่ง糊争 personality الوطنية спин ができる";
        // Repeat to exceed 200 bytes
        for _ in 0..10 {
            detector.on_token(multilingual);
        }

        // This should NOT panic - the bug was slicing at non-char boundary
        let preview = detector.context_preview();
        assert!(preview.starts_with("..."));
        // Verify it's valid UTF-8 (would panic on invalid)
        assert!(!preview.is_empty());
    }
}
