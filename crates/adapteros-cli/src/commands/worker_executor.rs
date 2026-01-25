//! Worker task executor
//!
//! Executes task assignments received from the orchestrator and generates
//! code modification proposals.

use adapteros_agent_spawn::protocol::{
    FileModification, ModificationType, TaskAssignment, TaskProposal,
};
use adapteros_core::Result;
use std::path::PathBuf;
use tracing::{debug, info};

/// Execute a task assignment and generate a proposal
///
/// This is the core logic that runs inside each agent worker process.
/// It analyzes the assigned code files and generates modification proposals.
///
/// # Current Implementation
///
/// This is currently a stub that returns an empty proposal. A full implementation
/// would:
/// 1. Parse and analyze the assigned files
/// 2. Use the LoRA worker to generate code suggestions
/// 3. Create file modifications with diffs
/// 4. Calculate confidence scores
/// 5. Detect conflicts with other agents' scopes
pub async fn execute_task(assignment: &TaskAssignment, agent_id: &str) -> Result<TaskProposal> {
    info!(
        agent_id = %agent_id,
        task_id = %hex::encode(assignment.task_id),
        file_count = assignment.scope.owned_files.len(),
        "Executing task assignment"
    );

    debug!(
        objective = %assignment.objective,
        "Task objective"
    );

    // Analyze files and generate modification proposals
    let modifications = analyze_files(&assignment.scope.owned_files).await?;

    // Calculate confidence based on analysis quality
    let confidence = calculate_confidence(&modifications);

    // Generate detailed rationale
    let issues_found = modifications.iter().filter(|m| m.diff.is_some()).count();
    let rationale = if issues_found > 0 {
        format!(
            "Analyzed {} files for objective: {}. Found {} actionable issues with suggested fixes.",
            assignment.scope.owned_files.len(),
            assignment.objective,
            issues_found
        )
    } else {
        format!(
            "Analyzed {} files for objective: {}. No actionable issues detected.",
            assignment.scope.owned_files.len(),
            assignment.objective
        )
    };

    let mut proposal = TaskProposal {
        task_id: assignment.task_id,
        agent_id: agent_id.to_string(),
        modifications,
        rationale,
        confidence,
        depends_on: vec![],
        conflicts_with: vec![],
        content_hash: [0u8; 32],
        created_at: chrono::Utc::now(),
    };

    // Compute the content hash
    proposal.content_hash = proposal.compute_hash();

    info!(
        agent_id = %agent_id,
        modification_count = proposal.modifications.len(),
        confidence = %proposal.confidence,
        "Task execution complete"
    );

    Ok(proposal)
}

/// Analyze files and generate modification proposals
///
/// Performs static analysis on each file to identify areas requiring attention
/// and generates suggested modifications with diffs.
async fn analyze_files(files: &[PathBuf]) -> Result<Vec<FileModification>> {
    let mut modifications = Vec::new();

    for file in files {
        // Read file content
        if let Ok(content) = tokio::fs::read_to_string(file).await {
            // Compute hash of original content
            let original_hash = blake3::hash(content.as_bytes());

            // Gather file statistics and issues for analysis
            let analysis = analyze_file_content_detailed(&content);

            // Generate modifications for each identified issue
            for issue in &analysis.issues {
                let diff = generate_issue_diff(&content, issue);

                let modification = FileModification {
                    file_path: file.clone(),
                    modification_type: ModificationType::Modify,
                    original_content_hash: Some(*original_hash.as_bytes()),
                    new_content: None,
                    diff: Some(diff),
                    line_range: Some((issue.line_number as u32, issue.line_number as u32)),
                    explanation: Some(issue.description.clone()),
                };

                modifications.push(modification);
            }

            // If no specific issues found, add a summary entry
            if analysis.issues.is_empty() {
                let explanation = format!(
                    "File analysis: {} lines. No actionable issues detected.",
                    analysis.line_count
                );

                let modification = FileModification {
                    file_path: file.clone(),
                    modification_type: ModificationType::Modify,
                    original_content_hash: Some(*original_hash.as_bytes()),
                    new_content: None,
                    diff: None,
                    line_range: None,
                    explanation: Some(explanation),
                };

                modifications.push(modification);
            }
        }
    }

    Ok(modifications)
}

/// Detailed issue found during analysis
#[derive(Debug, Clone)]
struct FileIssue {
    line_number: usize,
    issue_type: IssueType,
    description: String,
    original_line: String,
    suggested_replacement: Option<String>,
}

#[derive(Debug, Clone)]
enum IssueType {
    Todo,
    Fixme,
    Unimplemented,
    TodoMacro,
}

/// Detailed file analysis results
struct FileAnalysis {
    line_count: usize,
    issues: Vec<FileIssue>,
}

/// Analyze file content and extract actionable issues
fn analyze_file_content_detailed(content: &str) -> FileAnalysis {
    let lines: Vec<&str> = content.lines().collect();
    let mut issues = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let line_number = idx + 1;
        let trimmed = line.trim();

        // Check for TODO comments
        if trimmed.contains("// TODO") || trimmed.contains("/* TODO") {
            issues.push(FileIssue {
                line_number,
                issue_type: IssueType::Todo,
                description: format!(
                    "Line {}: Contains TODO that needs implementation",
                    line_number
                ),
                original_line: line.to_string(),
                suggested_replacement: None, // Would need LLM for actual implementation
            });
        }

        // Check for FIXME comments
        if trimmed.contains("// FIXME") || trimmed.contains("/* FIXME") {
            issues.push(FileIssue {
                line_number,
                issue_type: IssueType::Fixme,
                description: format!("Line {}: Contains FIXME that needs attention", line_number),
                original_line: line.to_string(),
                suggested_replacement: None,
            });
        }

        // Check for unimplemented!() macro
        if trimmed.contains("unimplemented!()") {
            issues.push(FileIssue {
                line_number,
                issue_type: IssueType::Unimplemented,
                description: format!(
                    "Line {}: Contains unimplemented!() - will panic at runtime",
                    line_number
                ),
                original_line: line.to_string(),
                suggested_replacement: Some(
                    line.replace("unimplemented!()", "todo!(\"Implement this\")"),
                ),
            });
        }

        // Check for todo!() macro without message
        if trimmed.contains("todo!()") && !trimmed.contains("todo!(\"") {
            issues.push(FileIssue {
                line_number,
                issue_type: IssueType::TodoMacro,
                description: format!("Line {}: Contains todo!() without description", line_number),
                original_line: line.to_string(),
                suggested_replacement: Some(
                    line.replace("todo!()", "todo!(\"Needs implementation\")"),
                ),
            });
        }
    }

    FileAnalysis {
        line_count: lines.len(),
        issues,
    }
}

/// Generate a unified diff for an issue
fn generate_issue_diff(content: &str, issue: &FileIssue) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = issue.line_number.saturating_sub(1);

    if line_idx >= lines.len() {
        return String::new();
    }

    let context_start = line_idx.saturating_sub(2);
    let context_end = (line_idx + 3).min(lines.len());

    let mut diff = format!(
        "@@ -{},{} +{},{} @@\n",
        context_start + 1,
        context_end - context_start,
        context_start + 1,
        context_end - context_start
    );

    for (offset, line) in lines[context_start..context_end].iter().enumerate() {
        let i = context_start + offset;
        if i == line_idx {
            diff.push_str(&format!("-{}\n", line));
            if let Some(ref replacement) = issue.suggested_replacement {
                diff.push_str(&format!("+{}\n", replacement));
            } else {
                // Keep original but mark for attention
                diff.push_str(&format!("+{} // AGENT: needs implementation\n", line));
            }
        } else {
            diff.push_str(&format!(" {}\n", line));
        }
    }

    diff
}

/// Calculate confidence score based on file analysis
///
/// Higher confidence when modifications have concrete diffs vs just analysis.
fn calculate_confidence(modifications: &[FileModification]) -> f32 {
    if modifications.is_empty() {
        return 0.5;
    }

    // Count modifications with actual diffs (more actionable)
    let with_diff = modifications.iter().filter(|m| m.diff.is_some()).count();
    let with_explanation = modifications
        .iter()
        .filter(|m| m.explanation.is_some())
        .count();

    // Higher weight for modifications with diffs
    let diff_ratio = with_diff as f32 / modifications.len() as f32;
    let explanation_ratio = with_explanation as f32 / modifications.len() as f32;

    let confidence = 0.3 + (diff_ratio * 0.4) + (explanation_ratio * 0.2);
    confidence.clamp(0.1, 0.9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_agent_spawn::protocol::TaskScope;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_execute_task_stub() {
        let assignment = TaskAssignment {
            task_id: [1u8; 32],
            sequence: 0,
            objective: "Test objective".to_string(),
            scope: TaskScope {
                owned_files: vec![],
                context_files: vec![],
                root_dir: PathBuf::from("."),
                ast_nodes: None,
                semantic_scope: None,
            },
            constraints: Default::default(),
            context: serde_json::Value::Object(Default::default()),
        };

        let result = execute_task(&assignment, "test-agent").await;
        assert!(result.is_ok());

        let proposal = result.unwrap();
        assert_eq!(proposal.agent_id, "test-agent");
        assert_eq!(proposal.task_id, [1u8; 32]);
    }
}
