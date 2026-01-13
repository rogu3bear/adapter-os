//! Change detection for the RECTIFY phase of the AARA lifecycle

use super::types::{AffectedAdapter, ChangeAction, ChangeType, SourceChangeEvent};
use adapteros_core::{AosError, B3Hash, Result};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

/// Detector for source document changes
///
/// Monitors registered documents and detects when their content
/// has changed, triggering the rectification workflow.
pub struct ChangeDetector {
    /// Known document hashes (document_id -> hash)
    known_hashes: HashMap<String, String>,
}

impl ChangeDetector {
    /// Create a new change detector
    pub fn new() -> Self {
        Self {
            known_hashes: HashMap::new(),
        }
    }

    /// Register a document for monitoring
    pub fn register_document(&mut self, document_id: &str, content_hash: &str) {
        self.known_hashes
            .insert(document_id.to_string(), content_hash.to_string());
    }

    /// Check a document for changes
    ///
    /// Compares the current content hash with the stored hash.
    /// Returns Some(SourceChangeEvent) if the document has changed.
    pub fn check_document(
        &self,
        document_id: &str,
        path: &str,
        current_content: &[u8],
    ) -> Option<SourceChangeEvent> {
        let current_hash = B3Hash::hash(current_content).to_hex();

        let Some(old_hash) = self.known_hashes.get(document_id) else {
            // Document not registered - treat as new
            debug!(
                document_id = document_id,
                "Document not registered, skipping"
            );
            return None;
        };

        if *old_hash == current_hash {
            return None; // No change
        }

        let change = SourceChangeEvent::new(
            document_id,
            path,
            old_hash,
            &current_hash,
            ChangeType::Modified,
        );

        info!(
            document_id = document_id,
            path = path,
            old_hash = &old_hash[..16],
            new_hash = &current_hash[..16],
            "Document change detected"
        );

        Some(change)
    }

    /// Check if a document was deleted
    pub fn check_deleted(&self, document_id: &str, path: &str) -> Option<SourceChangeEvent> {
        let old_hash = self.known_hashes.get(document_id)?;

        let change = SourceChangeEvent::new(
            document_id,
            path,
            old_hash,
            "", // Empty hash for deleted
            ChangeType::Deleted,
        );

        warn!(document_id = document_id, path = path, "Document deleted");

        Some(change)
    }

    /// Check a directory for changes
    ///
    /// Scans all files in the directory and compares with known hashes.
    /// Returns a list of changes detected.
    pub fn scan_directory(&self, dir_path: &Path) -> Result<Vec<SourceChangeEvent>> {
        if !dir_path.exists() {
            return Err(AosError::not_found(format!(
                "Directory not found: {}",
                dir_path.display()
            )));
        }

        let mut changes = Vec::new();
        let mut seen_docs: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Walk the directory
        for entry in std::fs::read_dir(dir_path)
            .map_err(|e| AosError::io(format!("{}: {}", dir_path.to_string_lossy(), e)))?
        {
            let entry = entry
                .map_err(|e| AosError::io(format!("{}: {}", dir_path.to_string_lossy(), e)))?;
            let path = entry.path();

            if path.is_file() {
                let path_str = path.to_string_lossy().to_string();
                let doc_id = path_str.clone(); // Use path as document ID for simplicity

                seen_docs.insert(doc_id.clone());

                // Read file content
                let content = std::fs::read(&path)
                    .map_err(|e| AosError::io(format!("{}: {}", path_str, e)))?;

                if let Some(change) = self.check_document(&doc_id, &path_str, &content) {
                    changes.push(change);
                }
            }
        }

        // Check for deleted documents
        for (doc_id, _) in &self.known_hashes {
            if !seen_docs.contains(doc_id) {
                if let Some(change) = self.check_deleted(doc_id, doc_id) {
                    changes.push(change);
                }
            }
        }

        Ok(changes)
    }

    /// Update the known hash for a document
    pub fn update_hash(&mut self, document_id: &str, new_hash: &str) {
        self.known_hashes
            .insert(document_id.to_string(), new_hash.to_string());
    }

    /// Remove a document from monitoring
    pub fn unregister_document(&mut self, document_id: &str) {
        self.known_hashes.remove(document_id);
    }

    /// Get the number of monitored documents
    pub fn document_count(&self) -> usize {
        self.known_hashes.len()
    }
}

impl Default for ChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Find adapters affected by a source change
///
/// This looks up the training lineage to find which adapters
/// were trained on data from the changed document.
pub async fn find_affected_adapters(
    db: &adapteros_db::Db,
    change: &SourceChangeEvent,
) -> Result<Vec<AffectedAdapter>> {
    use sqlx::Row;

    // Query training dataset rows that reference this document
    // Note: adapters table doesn't have a version column, so we use 1 as default
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT
            tl.adapter_id,
            a.name as adapter_name,
            COUNT(*) as example_count
        FROM training_dataset_rows tdr
        JOIN adapter_training_lineage tl ON tdr.dataset_version_id = tl.dataset_version_id
        JOIN adapters a ON tl.adapter_id = a.id
        WHERE tdr.source_file = ? OR tdr.content_hash_b3 = ?
        GROUP BY tl.adapter_id, a.name
        "#,
    )
    .bind(&change.path)
    .bind(&change.old_hash_b3)
    .fetch_all(db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    let mut affected = Vec::new();

    for row in rows {
        let adapter_id: String = row.get("adapter_id");
        let adapter_name: Option<String> = row.try_get("adapter_name").ok();
        let example_count: i64 = row.get("example_count");

        // Get total examples for this adapter to calculate percentage
        let total_result = sqlx::query(
            r#"
            SELECT COUNT(*) as total
            FROM training_dataset_rows tdr
            JOIN adapter_training_lineage tl ON tdr.dataset_version_id = tl.dataset_version_id
            WHERE tl.adapter_id = ?
            "#,
        )
        .bind(&adapter_id)
        .fetch_one(db.pool())
        .await
        .ok();

        let total: i64 = total_result
            .map(|r| r.get("total"))
            .unwrap_or(example_count);

        let percentage = if total > 0 {
            example_count as f32 / total as f32
        } else {
            0.0
        };

        // Use 1 as default version since adapters table doesn't track versions
        let mut adapter = AffectedAdapter::new(&adapter_id, 1)
            .with_affected_stats(example_count as usize, percentage);

        adapter.adapter_name = adapter_name;

        // If document was deleted, recommend deprecation
        if change.is_deletion() && percentage > 0.5 {
            adapter.recommended_action = ChangeAction::Deprecate;
        }

        affected.push(adapter);
    }

    info!(
        document_id = &change.document_id,
        affected_count = affected.len(),
        "Found affected adapters"
    );

    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_detector() {
        let mut detector = ChangeDetector::new();

        // Register a document
        detector.register_document("doc-1", "abc123");

        // Same content - no change
        let content = b"test content";
        let hash = B3Hash::hash(content).to_hex();
        detector.update_hash("doc-1", &hash);

        let result = detector.check_document("doc-1", "/path/doc.txt", content);
        assert!(result.is_none());

        // Different content - change detected
        let new_content = b"modified content";
        let result = detector.check_document("doc-1", "/path/doc.txt", new_content);
        assert!(result.is_some());

        let change = result.unwrap();
        assert_eq!(change.document_id, "doc-1");
        assert!(change.is_modification());
    }

    #[test]
    fn test_deleted_detection() {
        let mut detector = ChangeDetector::new();
        detector.register_document("doc-1", "abc123");

        let change = detector.check_deleted("doc-1", "/path/doc.txt");
        assert!(change.is_some());
        assert!(change.unwrap().is_deletion());
    }
}
