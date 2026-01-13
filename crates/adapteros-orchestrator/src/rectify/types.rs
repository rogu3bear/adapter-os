//! Types for the RECTIFY phase of the AARA lifecycle

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Event indicating a source document has changed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceChangeEvent {
    /// Document that changed
    pub document_id: String,
    /// Original path
    pub path: String,
    /// Previous content hash
    pub old_hash_b3: String,
    /// New content hash
    pub new_hash_b3: String,
    /// When the change was detected
    pub detected_at: DateTime<Utc>,
    /// Type of change
    pub change_type: ChangeType,
    /// Size change in bytes (positive = grew, negative = shrunk)
    pub size_delta: i64,
}

impl SourceChangeEvent {
    /// Create a new change event
    pub fn new(
        document_id: impl Into<String>,
        path: impl Into<String>,
        old_hash: impl Into<String>,
        new_hash: impl Into<String>,
        change_type: ChangeType,
    ) -> Self {
        Self {
            document_id: document_id.into(),
            path: path.into(),
            old_hash_b3: old_hash.into(),
            new_hash_b3: new_hash.into(),
            detected_at: Utc::now(),
            change_type,
            size_delta: 0,
        }
    }

    /// Builder: set size delta
    pub fn with_size_delta(mut self, delta: i64) -> Self {
        self.size_delta = delta;
        self
    }

    /// Check if the document was deleted
    pub fn is_deletion(&self) -> bool {
        matches!(self.change_type, ChangeType::Deleted)
    }

    /// Check if this is a content modification
    pub fn is_modification(&self) -> bool {
        matches!(self.change_type, ChangeType::Modified)
    }
}

/// Type of change detected in source document
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// Document content was modified
    Modified,
    /// Document was deleted
    Deleted,
    /// Document was renamed/moved
    Renamed,
    /// New document added (won't have affected adapters)
    Added,
}

/// An adapter affected by source changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedAdapter {
    /// Adapter ID
    pub adapter_id: String,
    /// Adapter name
    pub adapter_name: Option<String>,
    /// Current version
    pub current_version: i64,
    /// Number of training examples from the changed document
    pub affected_examples: usize,
    /// Percentage of adapter's training data affected
    pub affected_percentage: f32,
    /// Recommended action
    pub recommended_action: ChangeAction,
}

impl AffectedAdapter {
    /// Create a new affected adapter
    pub fn new(adapter_id: impl Into<String>, current_version: i64) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            adapter_name: None,
            current_version,
            affected_examples: 0,
            affected_percentage: 0.0,
            recommended_action: ChangeAction::None,
        }
    }

    /// Builder: set affected stats
    pub fn with_affected_stats(mut self, examples: usize, percentage: f32) -> Self {
        self.affected_examples = examples;
        self.affected_percentage = percentage;
        self.recommended_action = Self::compute_action(percentage);
        self
    }

    fn compute_action(percentage: f32) -> ChangeAction {
        if percentage >= 0.5 {
            ChangeAction::FullRetrain
        } else if percentage >= 0.1 {
            ChangeAction::PartialRetrain
        } else if percentage > 0.0 {
            ChangeAction::Optional
        } else {
            ChangeAction::None
        }
    }

    /// Check if retraining is recommended
    pub fn needs_retrain(&self) -> bool {
        matches!(
            self.recommended_action,
            ChangeAction::FullRetrain | ChangeAction::PartialRetrain
        )
    }
}

/// Recommended action for an affected adapter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    /// No action needed (changes are minimal)
    None,
    /// Optional retrain (small changes)
    Optional,
    /// Partial retrain recommended (moderate changes)
    PartialRetrain,
    /// Full retrain recommended (major changes)
    FullRetrain,
    /// Adapter should be deprecated (source deleted)
    Deprecate,
}

impl std::fmt::Display for ChangeAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeAction::None => write!(f, "none"),
            ChangeAction::Optional => write!(f, "optional"),
            ChangeAction::PartialRetrain => write!(f, "partial_retrain"),
            ChangeAction::FullRetrain => write!(f, "full_retrain"),
            ChangeAction::Deprecate => write!(f, "deprecate"),
        }
    }
}

/// Status of a rectification operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RectifyStatus {
    /// Rectification pending
    Pending,
    /// Re-synthesis in progress
    Synthesizing,
    /// Training new version
    Training,
    /// Validation in progress
    Validating,
    /// Completed successfully
    Completed,
    /// Failed
    Failed,
    /// Skipped (no action needed)
    Skipped,
}

/// Result of a rectification operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RectifyResult {
    /// Source change that triggered this
    pub source_change: SourceChangeEvent,
    /// Adapters that were affected
    pub affected_adapters: Vec<AffectedAdapter>,
    /// New adapter versions created (if any)
    pub new_versions: Vec<NewAdapterVersion>,
    /// Overall status
    pub status: RectifyStatus,
    /// Error message if failed
    pub error: Option<String>,
    /// When rectification started
    pub started_at: DateTime<Utc>,
    /// When rectification completed
    pub completed_at: Option<DateTime<Utc>>,
}

impl RectifyResult {
    /// Create a new pending result
    pub fn pending(source_change: SourceChangeEvent) -> Self {
        Self {
            source_change,
            affected_adapters: Vec::new(),
            new_versions: Vec::new(),
            status: RectifyStatus::Pending,
            error: None,
            started_at: Utc::now(),
            completed_at: None,
        }
    }

    /// Mark as failed
    pub fn fail(mut self, error: impl Into<String>) -> Self {
        self.status = RectifyStatus::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
        self
    }

    /// Mark as completed
    pub fn complete(mut self) -> Self {
        self.status = RectifyStatus::Completed;
        self.completed_at = Some(Utc::now());
        self
    }

    /// Mark as skipped
    pub fn skip(mut self) -> Self {
        self.status = RectifyStatus::Skipped;
        self.completed_at = Some(Utc::now());
        self
    }

    /// Check if completed successfully
    pub fn is_success(&self) -> bool {
        self.status == RectifyStatus::Completed
    }
}

/// A new adapter version created during rectification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAdapterVersion {
    /// Adapter ID
    pub adapter_id: String,
    /// Previous version
    pub previous_version: i64,
    /// New version
    pub new_version: i64,
    /// New training examples count
    pub new_examples_count: usize,
    /// Whether validation passed
    pub validation_passed: Option<bool>,
    /// Version state (draft until promoted)
    pub state: VersionState,
}

/// State of an adapter version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionState {
    /// Draft - needs validation/promotion
    Draft,
    /// Validated - ready for promotion
    Validated,
    /// Active - currently in use
    Active,
    /// Superseded - replaced by newer version
    Superseded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_change_event() {
        let event = SourceChangeEvent::new(
            "doc-1",
            "/path/to/doc.md",
            "old_hash",
            "new_hash",
            ChangeType::Modified,
        )
        .with_size_delta(100);

        assert_eq!(event.document_id, "doc-1");
        assert!(event.is_modification());
        assert!(!event.is_deletion());
        assert_eq!(event.size_delta, 100);
    }

    #[test]
    fn test_affected_adapter_action() {
        // High impact - full retrain
        let adapter = AffectedAdapter::new("adapter-1", 1).with_affected_stats(100, 0.6);
        assert_eq!(adapter.recommended_action, ChangeAction::FullRetrain);
        assert!(adapter.needs_retrain());

        // Medium impact - partial retrain
        let adapter = AffectedAdapter::new("adapter-2", 1).with_affected_stats(50, 0.25);
        assert_eq!(adapter.recommended_action, ChangeAction::PartialRetrain);
        assert!(adapter.needs_retrain());

        // Low impact - optional
        let adapter = AffectedAdapter::new("adapter-3", 1).with_affected_stats(5, 0.05);
        assert_eq!(adapter.recommended_action, ChangeAction::Optional);
        assert!(!adapter.needs_retrain());
    }

    #[test]
    fn test_rectify_result_lifecycle() {
        let event = SourceChangeEvent::new("doc-1", "/path", "old", "new", ChangeType::Modified);

        let result = RectifyResult::pending(event);
        assert_eq!(result.status, RectifyStatus::Pending);
        assert!(result.completed_at.is_none());

        let result = result.complete();
        assert_eq!(result.status, RectifyStatus::Completed);
        assert!(result.completed_at.is_some());
        assert!(result.is_success());
    }
}
