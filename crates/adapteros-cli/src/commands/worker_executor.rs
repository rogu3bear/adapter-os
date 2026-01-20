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

    // TODO: Implement actual task execution
    // For now, we return an empty proposal to complete the protocol flow

    let modifications = analyze_files(&assignment.scope.owned_files).await?;

    let rationale = format!(
        "Analyzed {} files for objective: {}",
        assignment.scope.owned_files.len(),
        assignment.objective
    );

    let mut proposal = TaskProposal {
        task_id: assignment.task_id,
        agent_id: agent_id.to_string(),
        modifications,
        rationale,
        confidence: 0.5, // Default confidence for stub
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
/// Performs basic static analysis on each file to identify potential
/// areas for modification based on the task objective.
async fn analyze_files(files: &[PathBuf]) -> Result<Vec<FileModification>> {
    let mut modifications = Vec::new();

    for file in files {
        // Read file content
        if let Ok(content) = tokio::fs::read_to_string(file).await {
            // Compute hash of original content
            let original_hash = blake3::hash(content.as_bytes());

            // Gather file statistics for analysis
            let stats = analyze_file_content(&content);

            // Create modification entry with analysis
            let explanation = format!(
                "File analysis: {} lines, {} TODOs, {} unimplemented!() calls. {}",
                stats.line_count,
                stats.todo_count,
                stats.unimplemented_count,
                if stats.todo_count > 0 || stats.unimplemented_count > 0 {
                    "Contains areas requiring attention."
                } else {
                    "No obvious issues detected."
                }
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

    Ok(modifications)
}

/// File content statistics
struct FileStats {
    line_count: usize,
    todo_count: usize,
    unimplemented_count: usize,
}

/// Analyze file content for basic statistics
fn analyze_file_content(content: &str) -> FileStats {
    let lines: Vec<&str> = content.lines().collect();

    let todo_count = lines
        .iter()
        .filter(|line| line.contains("TODO") || line.contains("FIXME"))
        .count();

    let unimplemented_count = lines
        .iter()
        .filter(|line| line.contains("unimplemented!") || line.contains("todo!"))
        .count();

    FileStats {
        line_count: lines.len(),
        todo_count,
        unimplemented_count,
    }
}

/// Calculate confidence score based on file analysis
#[allow(dead_code)]
fn calculate_confidence(modifications: &[FileModification]) -> f64 {
    if modifications.is_empty() {
        return 0.5;
    }

    // Higher confidence when we have concrete analysis
    let analyzed_count = modifications
        .iter()
        .filter(|m| m.explanation.is_some())
        .count();

    let base_confidence = 0.3 + (analyzed_count as f64 / modifications.len() as f64) * 0.4;
    base_confidence.min(0.8) // Cap at 0.8 for conservative estimates
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
