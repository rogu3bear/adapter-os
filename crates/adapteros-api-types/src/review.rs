//! Review Protocol Types
//!
//! Types for the human-in-the-loop review protocol. Enables inference to pause
//! when external review is needed, and resume when review is provided.
//!
//! # Flow
//!
//! 1. Inference detects need for review (complexity, explicit tag, uncertainty)
//! 2. Emits `ReviewRequest` and pauses (state = `InferenceState::Paused`)
//! 3. Human copies context to external reviewer (e.g., Claude Code)
//! 4. Human submits `ReviewResponse` via API
//! 5. Inference resumes with review incorporated
//!
//! # Fail Closed Semantics
//!
//! If `REVIEW_NEEDED` is emitted but no `REVIEW_RESPONSE` received:
//! - Inference stays paused indefinitely (configurable timeout)
//! - Cannot proceed without review

use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use chrono::Utc;

use crate::schema_version;

// =============================================================================
// Inference State Machine
// =============================================================================

/// State of an inference request in the pause/resume lifecycle
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum InferenceState {
    /// Inference is actively generating tokens
    #[default]
    Running,
    /// Inference is paused, waiting for external input
    Paused(PauseReason),
    /// Inference completed successfully
    Complete,
    /// Inference failed with an error
    Failed,
    /// Inference was cancelled by user
    Cancelled,
}

// =============================================================================
// Pause Reason
// =============================================================================

/// Why an inference was paused
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PauseReason {
    /// Type of pause
    pub kind: PauseKind,
    /// Unique identifier for this pause (for correlating response)
    pub pause_id: String,
    /// Context to provide to reviewer
    pub context: ReviewContext,
    /// When the pause was initiated (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Types of pause reasons
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PauseKind {
    /// Needs external code review
    ReviewNeeded,
    /// Policy enforcement requires approval
    PolicyApproval,
    /// Resource wait (memory, GPU)
    ResourceWait,
    /// Explicit user-requested pause
    UserRequested,
}

// =============================================================================
// Review Request/Response
// =============================================================================

/// Context provided to reviewer when review is needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReviewContext {
    /// Code or content to be reviewed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Specific question for the reviewer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// What aspects to focus on
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<ReviewScope>,
    /// Additional context (file paths, dependencies, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Aspects to focus review on
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReviewScope {
    /// Logic correctness
    Logic,
    /// Edge cases and boundary conditions
    EdgeCases,
    /// Security vulnerabilities
    Security,
    /// Performance implications
    Performance,
    /// Code style and conventions
    Style,
    /// API design
    ApiDesign,
    /// Test coverage
    Testing,
    /// Documentation
    Documentation,
}

/// Request to submit a review for a paused inference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SubmitReviewRequest {
    /// ID of the paused inference to resume
    pub pause_id: String,
    /// The review content
    pub review: Review,
    /// Who provided the review
    #[serde(default = "default_reviewer")]
    pub reviewer: String,
}

fn default_reviewer() -> String {
    "human".to_string()
}

/// The actual review content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct Review {
    /// Overall assessment
    pub assessment: ReviewAssessment,
    /// List of issues found
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ReviewIssue>,
    /// Suggested improvements
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    /// Free-form comments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
    /// Confidence in the review (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

/// Overall assessment of the reviewed code/content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReviewAssessment {
    /// Approved, no changes needed
    Approved,
    /// Approved with minor suggestions
    ApprovedWithSuggestions,
    /// Changes required before proceeding
    NeedsChanges,
    /// Rejected, should not proceed
    Rejected,
    /// Unable to assess, need more information
    Inconclusive,
}

/// A specific issue found during review
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReviewIssue {
    /// Severity of the issue
    pub severity: IssueSeverity,
    /// Category of the issue
    pub category: ReviewScope,
    /// Description of the issue
    pub description: String,
    /// Line number or location (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Suggested fix
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
}

/// Severity levels for review issues
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    /// Informational note
    Info,
    /// Minor issue, nice to fix
    Low,
    /// Moderate issue, should fix
    Medium,
    /// Serious issue, must fix
    High,
    /// Critical issue, blocks approval
    Critical,
}

// =============================================================================
// API Responses
// =============================================================================

/// Response when querying inference state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct InferenceStateResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Inference request ID
    pub inference_id: String,
    /// Current state
    pub state: InferenceState,
    /// If paused, when it was paused
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<String>,
    /// If paused, how long it's been paused (seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_duration_secs: Option<u64>,
}

/// Response after submitting a review
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SubmitReviewResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// Whether the review was accepted
    pub accepted: bool,
    /// New state of the inference
    pub new_state: InferenceState,
    /// Message explaining the result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response listing paused inferences
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ListPausedResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// List of paused inference IDs with their pause reasons
    pub paused: Vec<PausedInferenceInfo>,
    /// Total count
    pub total: usize,
}

/// Info about a paused inference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PausedInferenceInfo {
    /// Inference request ID
    pub inference_id: String,
    /// Pause ID for submitting review
    pub pause_id: String,
    /// Why it's paused
    pub kind: PauseKind,
    /// When it was paused
    pub paused_at: String,
    /// Duration in seconds
    pub duration_secs: u64,
    /// Preview of the context (truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_preview: Option<String>,
}

// =============================================================================
// Helpers
// =============================================================================

impl PauseReason {
    /// Create a new review-needed pause
    pub fn review_needed(pause_id: impl Into<String>, context: ReviewContext) -> Self {
        Self {
            kind: PauseKind::ReviewNeeded,
            pause_id: pause_id.into(),
            context,
            created_at: None,
        }
    }

    /// Create with timestamp
    #[cfg(feature = "server")]
    pub fn with_timestamp(mut self) -> Self {
        self.created_at = Some(Utc::now().to_rfc3339());
        self
    }
}

impl ReviewContext {
    /// Create a code review context
    pub fn code_review(code: impl Into<String>, question: impl Into<String>) -> Self {
        Self {
            code: Some(code.into()),
            question: Some(question.into()),
            scope: vec![
                ReviewScope::Logic,
                ReviewScope::EdgeCases,
                ReviewScope::Security,
            ],
            metadata: None,
        }
    }

    /// Add specific scopes
    pub fn with_scope(mut self, scope: Vec<ReviewScope>) -> Self {
        self.scope = scope;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl Review {
    /// Create an approved review
    pub fn approved(comments: Option<String>) -> Self {
        Self {
            assessment: ReviewAssessment::Approved,
            issues: vec![],
            suggestions: vec![],
            comments,
            confidence: Some(1.0),
        }
    }

    /// Create a review that needs changes
    pub fn needs_changes(issues: Vec<ReviewIssue>, suggestions: Vec<String>) -> Self {
        Self {
            assessment: ReviewAssessment::NeedsChanges,
            issues,
            suggestions,
            comments: None,
            confidence: None,
        }
    }
}

// =============================================================================
// Review Context Export (for CLI integration)
// =============================================================================

/// Exported review context structure for external reviewers (e.g., Claude Code).
///
/// Used by:
/// - Server: `GET /v1/reviews/{pause_id}/context` endpoint
/// - CLI: `aosctl review export` command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ReviewContextExport {
    /// Pause ID for resume correlation
    pub pause_id: String,
    /// Inference request ID
    pub inference_id: String,
    /// Pause kind (e.g., "ReviewNeeded", "PolicyApproval")
    pub kind: String,
    /// When the inference was paused (RFC3339 timestamp)
    pub paused_at: String,
    /// How long inference has been paused (seconds)
    pub duration_secs: u64,
    /// Code or content to review
    pub code: Option<String>,
    /// Question for the reviewer
    pub question: Option<String>,
    /// Review scopes (e.g., "Logic", "Security")
    pub scope: Vec<String>,
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
    /// Instructions for the external reviewer
    pub instructions: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pause_reason_serialization() {
        let reason = PauseReason::review_needed(
            "review_123",
            ReviewContext::code_review("fn foo() {}", "Is this correct?"),
        );

        let json = serde_json::to_string_pretty(&reason).unwrap();
        assert!(json.contains("review_123"));
        assert!(json.contains("fn foo()"));

        let parsed: PauseReason = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pause_id, "review_123");
    }

    #[test]
    fn test_review_response_serialization() {
        let review = Review::needs_changes(
            vec![ReviewIssue {
                severity: IssueSeverity::High,
                category: ReviewScope::Security,
                description: "SQL injection vulnerability".into(),
                location: Some("line 42".into()),
                suggested_fix: Some("Use parameterized query".into()),
            }],
            vec!["Consider adding input validation".into()],
        );

        let json = serde_json::to_string_pretty(&review).unwrap();
        assert!(json.contains("needs_changes"));
        assert!(json.contains("SQL injection"));
    }
}
