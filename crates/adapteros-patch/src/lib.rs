//! AdapterOS Patch Engine
//!
//! Deterministic, policy-compliant code patching with cryptographic verification.
//!
//! This crate provides:
//! - Secure patch validation against all 20 policy packs
//! - Cryptographic signature verification (Ed25519 + BLAKE3)
//! - Deterministic patch application with rollback support
//! - Full audit trail and compliance reporting
//!
//! # Citations
//! - CONTRIBUTING.md L123: Use `tracing` for logging
//! - CONTRIBUTING.md L122: Use `cargo fmt` for formatting
//! - CONTRIBUTING.md L121: Use `cargo clippy` for linting

pub mod patch;

// Re-export patch types
pub use patch::{Patch, PatchFile, PatchMetadata, PatchOperation};

// Re-export commonly used types
pub mod prelude {
    pub use crate::Patch;
}

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use tracing::{debug, info};

/// Summary of a single change within a patch operation
#[derive(Debug, Clone)]
pub struct ChangeSummary {
    /// Line number where the change occurred
    pub line_number: usize,
    /// Type of change (Replace, Insert, Delete, Move)
    pub change_type: String,
    /// Brief description of what changed
    pub description: String,
    /// Number of lines affected
    pub lines_affected: usize,
}

/// Result of applying a patch to source content
#[derive(Debug, Clone)]
pub struct PatchApplicationResult {
    /// The modified source content
    pub output: String,
    /// Summary of all changes with line numbers
    pub changes: Vec<ChangeSummary>,
    /// Total number of operations applied
    pub operations_applied: usize,
}

/// Engine for parsing, transforming, and applying patches to source code
pub struct PatchEngine {
    /// Whether to preserve original content for rollback
    preserve_original: bool,
    /// Cache of parsed content by file path
    content_cache: HashMap<String, String>,
}

impl Default for PatchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchEngine {
    /// Create a new patch engine
    pub fn new() -> Self {
        Self {
            preserve_original: true,
            content_cache: HashMap::new(),
        }
    }

    /// Create a patch engine with custom configuration
    pub fn with_config(preserve_original: bool) -> Self {
        Self {
            preserve_original,
            content_cache: HashMap::new(),
        }
    }

    /// Parse source content and cache it for transformation
    pub fn parse(&mut self, path: &str, content: &str) -> Result<()> {
        debug!(path = %path, content_len = content.len(), "Parsing source content");
        self.content_cache.insert(path.to_string(), content.to_string());
        Ok(())
    }

    /// Apply a patch to parsed content and generate output with change summaries
    pub fn apply_patch(&mut self, patch: &Patch) -> Result<HashMap<String, PatchApplicationResult>> {
        info!(patch_id = %patch.id, files = patch.files.len(), "Applying patch");

        let mut results = HashMap::new();

        for patch_file in &patch.files {
            let content = self.content_cache.get(&patch_file.path).cloned().unwrap_or_default();
            let result = self.apply_file_patch(&content, patch_file)?;

            // Update cache with modified content
            self.content_cache.insert(patch_file.path.clone(), result.output.clone());
            results.insert(patch_file.path.clone(), result);
        }

        Ok(results)
    }

    /// Apply patch operations to a single file's content
    fn apply_file_patch(&self, content: &str, patch_file: &PatchFile) -> Result<PatchApplicationResult> {
        let mut current_content = content.to_string();
        let mut changes = Vec::new();
        let mut operations_applied = 0;

        for operation in &patch_file.operations {
            match operation {
                PatchOperation::Replace { old_string, new_string } => {
                    if let Some(pos) = current_content.find(old_string) {
                        let line_number = current_content[..pos].matches('\n').count() + 1;
                        let old_lines = old_string.matches('\n').count() + 1;
                        let new_lines = new_string.matches('\n').count() + 1;

                        current_content = current_content.replacen(old_string, new_string, 1);

                        changes.push(ChangeSummary {
                            line_number,
                            change_type: "Replace".to_string(),
                            description: format!(
                                "Replaced {} line(s) with {} line(s)",
                                old_lines, new_lines
                            ),
                            lines_affected: old_lines.max(new_lines),
                        });
                        operations_applied += 1;
                    } else {
                        return Err(AosError::Verification(format!(
                            "Replace target not found in {}: '{}'",
                            patch_file.path,
                            if old_string.len() > 50 {
                                format!("{}...", &old_string[..50])
                            } else {
                                old_string.clone()
                            }
                        )));
                    }
                }

                PatchOperation::Insert { position, content: insert_content } => {
                    if *position > current_content.len() {
                        return Err(AosError::Verification(format!(
                            "Insert position {} exceeds content length {} in {}",
                            position,
                            current_content.len(),
                            patch_file.path
                        )));
                    }

                    let line_number = current_content[..*position].matches('\n').count() + 1;
                    let lines_inserted = insert_content.matches('\n').count() + 1;

                    current_content.insert_str(*position, insert_content);

                    changes.push(ChangeSummary {
                        line_number,
                        change_type: "Insert".to_string(),
                        description: format!("Inserted {} line(s)", lines_inserted),
                        lines_affected: lines_inserted,
                    });
                    operations_applied += 1;
                }

                PatchOperation::Delete { start, end } => {
                    if *start > current_content.len() || *end > current_content.len() || start > end {
                        return Err(AosError::Verification(format!(
                            "Invalid delete range {}..{} for content length {} in {}",
                            start,
                            end,
                            current_content.len(),
                            patch_file.path
                        )));
                    }

                    let line_number = current_content[..*start].matches('\n').count() + 1;
                    let lines_deleted = current_content[*start..*end].matches('\n').count() + 1;

                    current_content.replace_range(*start..*end, "");

                    changes.push(ChangeSummary {
                        line_number,
                        change_type: "Delete".to_string(),
                        description: format!("Deleted {} line(s)", lines_deleted),
                        lines_affected: lines_deleted,
                    });
                    operations_applied += 1;
                }

                PatchOperation::Move { from_start, from_end, to_position } => {
                    if *from_start > current_content.len()
                        || *from_end > current_content.len()
                        || from_start > from_end
                    {
                        return Err(AosError::Verification(format!(
                            "Invalid move source range {}..{} in {}",
                            from_start, from_end, patch_file.path
                        )));
                    }

                    let moved_content = current_content[*from_start..*from_end].to_string();
                    let from_line = current_content[..*from_start].matches('\n').count() + 1;
                    let lines_moved = moved_content.matches('\n').count() + 1;

                    // Remove from original position
                    current_content.replace_range(*from_start..*from_end, "");

                    // Adjust to_position if it was after the deleted range
                    let adjusted_pos = if *to_position > *from_end {
                        to_position - (from_end - from_start)
                    } else {
                        *to_position
                    };

                    if adjusted_pos > current_content.len() {
                        return Err(AosError::Verification(format!(
                            "Move destination {} exceeds content length {} in {}",
                            adjusted_pos,
                            current_content.len(),
                            patch_file.path
                        )));
                    }

                    let to_line = current_content[..adjusted_pos].matches('\n').count() + 1;
                    current_content.insert_str(adjusted_pos, &moved_content);

                    changes.push(ChangeSummary {
                        line_number: from_line,
                        change_type: "Move".to_string(),
                        description: format!(
                            "Moved {} line(s) from line {} to line {}",
                            lines_moved, from_line, to_line
                        ),
                        lines_affected: lines_moved,
                    });
                    operations_applied += 1;
                }
            }
        }

        debug!(
            path = %patch_file.path,
            operations = operations_applied,
            "File patch applied"
        );

        Ok(PatchApplicationResult {
            output: current_content,
            changes,
            operations_applied,
        })
    }

    /// Generate a patch from differences between two content strings
    pub fn generate_patch(
        &self,
        path: &str,
        original: &str,
        modified: &str,
        metadata: PatchMetadata,
    ) -> Result<Patch> {
        let mut operations = Vec::new();

        // Simple diff: if content differs, create a full replace operation
        if original != modified {
            operations.push(PatchOperation::Replace {
                old_string: original.to_string(),
                new_string: modified.to_string(),
            });
        }

        let patch_id = format!(
            "patch-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::now_v7()
        );

        info!(
            patch_id = %patch_id,
            path = %path,
            operations = operations.len(),
            "Generated patch"
        );

        Ok(Patch {
            id: patch_id,
            metadata,
            files: vec![PatchFile {
                path: path.to_string(),
                operations,
            }],
            signature: None,
            public_key: None,
        })
    }

    /// Get the current cached content for a file
    pub fn get_content(&self, path: &str) -> Option<&String> {
        self.content_cache.get(path)
    }

    /// Clear the content cache
    pub fn clear_cache(&mut self) {
        self.content_cache.clear();
    }

    /// Check if original content preservation is enabled
    pub fn preserves_original(&self) -> bool {
        self.preserve_original
    }
}

/// Result of patch application
#[derive(Debug, Clone)]
pub struct PatchResult {
    /// Unique patch application ID
    pub patch_id: String,
    /// Application timestamp
    pub applied_at: chrono::DateTime<chrono::Utc>,
    /// Files that were modified
    pub modified_files: Vec<String>,
    /// Whether rollback is available
    pub rollback_available: bool,
}

impl Default for PatchResult {
    fn default() -> Self {
        Self {
            patch_id: "placeholder".to_string(),
            applied_at: chrono::Utc::now(),
            modified_files: Vec::new(),
            rollback_available: false,
        }
    }
}
