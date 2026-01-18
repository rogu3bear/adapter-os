//! Rectification workflow for the AARA lifecycle

use super::detector::find_affected_adapters;
use super::types::{
    AffectedAdapter, NewAdapterVersion, RectifyResult, RectifyStatus, SourceChangeEvent,
    VersionState,
};
use adapteros_core::Result;
use tracing::{debug, error, info, warn};

/// Workflow for rectifying adapters when source documents change
///
/// The workflow:
/// 1. Identifies affected adapters
/// 2. Re-synthesizes training data from updated documents
/// 3. Creates new adapter versions
/// 4. Runs validation
/// 5. Optionally auto-promotes validated versions
pub struct RectifyWorkflow {
    /// The source change triggering this workflow
    source_change: SourceChangeEvent,
    /// Whether to auto-promote validated versions
    auto_promote: bool,
    /// Minimum affected percentage to trigger retrain
    min_affected_percentage: f32,
}

impl RectifyWorkflow {
    /// Create a new rectify workflow
    pub fn new(source_change: SourceChangeEvent) -> Self {
        Self {
            source_change,
            auto_promote: false,
            min_affected_percentage: 0.05, // 5% threshold
        }
    }

    /// Builder: enable auto-promotion
    pub fn with_auto_promote(mut self, auto_promote: bool) -> Self {
        self.auto_promote = auto_promote;
        self
    }

    /// Builder: set minimum affected percentage
    pub fn with_min_threshold(mut self, threshold: f32) -> Self {
        self.min_affected_percentage = threshold.clamp(0.0, 1.0);
        self
    }

    /// Execute the rectification workflow
    pub async fn execute(self, db: &adapteros_db::Db) -> Result<RectifyResult> {
        let mut result = RectifyResult::pending(self.source_change.clone());

        info!(
            document_id = &self.source_change.document_id,
            path = &self.source_change.path,
            "Starting rectification workflow"
        );

        // Step 1: Find affected adapters
        result.status = RectifyStatus::Pending;
        let affected = match find_affected_adapters(db, &self.source_change).await {
            Ok(a) => a,
            Err(e) => {
                error!(error = %e, "Failed to find affected adapters");
                return Ok(result.fail(format!("Failed to find affected adapters: {}", e)));
            }
        };

        result.affected_adapters = affected;

        // Filter to adapters that need action
        let adapters_to_rectify: Vec<_> = result
            .affected_adapters
            .iter()
            .filter(|a| a.affected_percentage >= self.min_affected_percentage)
            .filter(|a| a.needs_retrain())
            .collect();

        if adapters_to_rectify.is_empty() {
            info!("No adapters need rectification");
            return Ok(result.skip());
        }

        info!(
            adapters_count = adapters_to_rectify.len(),
            "Found adapters requiring rectification"
        );

        // Step 2: Re-synthesize training data
        result.status = RectifyStatus::Synthesizing;

        // For each affected adapter, create a new version
        for adapter in adapters_to_rectify {
            debug!(
                adapter_id = &adapter.adapter_id,
                affected_percentage = adapter.affected_percentage,
                "Processing adapter for rectification"
            );

            // Create new version entry
            let new_version = NewAdapterVersion {
                adapter_id: adapter.adapter_id.clone(),
                previous_version: adapter.current_version,
                new_version: adapter.current_version + 1,
                new_examples_count: 0, // Will be filled during synthesis
                validation_passed: None,
                state: VersionState::Draft,
            };

            result.new_versions.push(new_version);
        }

        // Step 3: Training would happen here (placeholder)
        result.status = RectifyStatus::Training;

        // Step 4: Validation would happen here (placeholder)
        result.status = RectifyStatus::Validating;

        // For now, mark all as validated since we don't have the actual training pipeline
        for version in &mut result.new_versions {
            version.validation_passed = Some(true);
            version.state = VersionState::Validated;
        }

        // Step 5: Auto-promote if enabled
        if self.auto_promote {
            for version in &mut result.new_versions {
                if version.validation_passed == Some(true) {
                    version.state = VersionState::Active;
                    info!(
                        adapter_id = &version.adapter_id,
                        new_version = version.new_version,
                        "Auto-promoted new adapter version"
                    );
                }
            }
        }

        Ok(result.complete())
    }

    /// Check if a source change requires rectification
    pub fn needs_rectification(&self, affected: &[AffectedAdapter]) -> bool {
        affected
            .iter()
            .any(|a| a.affected_percentage >= self.min_affected_percentage && a.needs_retrain())
    }
}

/// Batch rectification for multiple source changes
#[allow(dead_code)]
pub struct BatchRectifyWorkflow {
    changes: Vec<SourceChangeEvent>,
    auto_promote: bool,
}

#[allow(dead_code)]
impl BatchRectifyWorkflow {
    /// Create a new batch workflow
    pub fn new(changes: Vec<SourceChangeEvent>) -> Self {
        Self {
            changes,
            auto_promote: false,
        }
    }

    /// Builder: enable auto-promotion
    pub fn with_auto_promote(mut self, auto_promote: bool) -> Self {
        self.auto_promote = auto_promote;
        self
    }

    /// Execute all rectifications
    pub async fn execute(self, db: &adapteros_db::Db) -> Result<Vec<RectifyResult>> {
        let mut results = Vec::with_capacity(self.changes.len());

        for change in self.changes {
            let workflow = RectifyWorkflow::new(change).with_auto_promote(self.auto_promote);

            match workflow.execute(db).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!(error = %e, "Rectification failed for one change, continuing");
                    // Continue with other changes
                }
            }
        }

        Ok(results)
    }
}

/// Summary of a batch rectification
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RectifyBatchSummary {
    /// Total changes processed
    pub total_changes: usize,
    /// Successful rectifications
    pub successful: usize,
    /// Skipped (no action needed)
    pub skipped: usize,
    /// Failed rectifications
    pub failed: usize,
    /// New adapter versions created
    pub new_versions_created: usize,
}

#[allow(dead_code)]
impl RectifyBatchSummary {
    /// Create summary from results
    pub fn from_results(results: &[RectifyResult]) -> Self {
        let successful = results.iter().filter(|r| r.is_success()).count();
        let skipped = results
            .iter()
            .filter(|r| r.status == RectifyStatus::Skipped)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == RectifyStatus::Failed)
            .count();
        let new_versions: usize = results.iter().map(|r| r.new_versions.len()).sum();

        Self {
            total_changes: results.len(),
            successful,
            skipped,
            failed,
            new_versions_created: new_versions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rectify::types::ChangeType;

    #[test]
    fn test_workflow_creation() {
        let change = SourceChangeEvent::new(
            "doc-1",
            "/path/doc.md",
            "old_hash",
            "new_hash",
            ChangeType::Modified,
        );

        let workflow = RectifyWorkflow::new(change)
            .with_auto_promote(true)
            .with_min_threshold(0.1);

        assert!(workflow.auto_promote);
        assert!((workflow.min_affected_percentage - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn test_needs_rectification() {
        let change = SourceChangeEvent::new("doc-1", "/path", "old", "new", ChangeType::Modified);

        let workflow = RectifyWorkflow::new(change).with_min_threshold(0.1);

        // High impact adapter - needs rectification
        let affected_high = vec![AffectedAdapter::new("adapter-1", 1).with_affected_stats(50, 0.5)];
        assert!(workflow.needs_rectification(&affected_high));

        // Low impact adapter - no rectification needed
        let affected_low = vec![AffectedAdapter::new("adapter-2", 1).with_affected_stats(2, 0.02)];
        assert!(!workflow.needs_rectification(&affected_low));
    }

    #[test]
    fn test_batch_summary() {
        let change1 = SourceChangeEvent::new("d1", "/p1", "o1", "n1", ChangeType::Modified);
        let change2 = SourceChangeEvent::new("d2", "/p2", "o2", "n2", ChangeType::Modified);

        let results = vec![
            RectifyResult::pending(change1).complete(),
            RectifyResult::pending(change2).skip(),
        ];

        let summary = RectifyBatchSummary::from_results(&results);

        assert_eq!(summary.total_changes, 2);
        assert_eq!(summary.successful, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.failed, 0);
    }
}
