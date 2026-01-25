//! Tests for worker_executor module
//!
//! This module tests the task execution interface for agent workers:
//! - execute_task: Core task execution logic
//! - analyze_files: File analysis and modification proposal generation
//! - analyze_file_content: Static analysis of file content
//! - calculate_confidence: Confidence scoring for proposals

use std::path::PathBuf;

use adapteros_agent_spawn::protocol::{
    ModificationType, TaskAssignment, TaskConstraints, TaskProposal, TaskScope,
};

// Re-export the worker_executor module functions for testing
// Note: We test through the public interface and verify golden outputs

/// Helper to get the path to test fixtures
fn fixtures_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).join("tests/data/worker_executor_fixtures")
}

/// Create a basic task assignment for testing
fn create_test_assignment(files: Vec<PathBuf>, objective: &str) -> TaskAssignment {
    TaskAssignment {
        task_id: [1u8; 32],
        sequence: 0,
        objective: objective.to_string(),
        scope: TaskScope {
            owned_files: files,
            context_files: vec![],
            root_dir: PathBuf::from("."),
            ast_nodes: None,
            semantic_scope: None,
        },
        constraints: TaskConstraints::default(),
        context: serde_json::Value::Object(Default::default()),
    }
}

/// Create a task assignment with custom constraints
fn create_constrained_assignment(
    files: Vec<PathBuf>,
    objective: &str,
    max_modifications: Option<u32>,
    min_confidence: Option<f32>,
) -> TaskAssignment {
    TaskAssignment {
        task_id: [2u8; 32],
        sequence: 1,
        objective: objective.to_string(),
        scope: TaskScope {
            owned_files: files,
            context_files: vec![],
            root_dir: PathBuf::from("."),
            ast_nodes: None,
            semantic_scope: None,
        },
        constraints: TaskConstraints {
            max_modifications,
            max_lines_per_file: None,
            excluded_patterns: vec![],
            require_rationale: true,
            min_confidence,
        },
        context: serde_json::Value::Object(Default::default()),
    }
}

mod execute_task_tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_task_empty_scope() {
        // Test execution with no files
        let assignment = create_test_assignment(vec![], "Refactor the codebase");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-01").await;

        assert!(
            result.is_ok(),
            "execute_task should succeed with empty scope"
        );
        let proposal = result.unwrap();

        assert_eq!(proposal.agent_id, "agent-01");
        assert_eq!(proposal.task_id, [1u8; 32]);
        assert!(proposal.modifications.is_empty());
        assert!(proposal.rationale.contains("0 files"));
    }

    #[tokio::test]
    async fn test_execute_task_with_real_files() {
        let fixtures = fixtures_path();
        let sample_file = fixtures.join("sample_rust_code.rs");

        // Skip test if fixture doesn't exist
        if !sample_file.exists() {
            eprintln!("Skipping test: fixture file not found at {:?}", sample_file);
            return;
        }

        let assignment =
            create_test_assignment(vec![sample_file.clone()], "Add error handling to functions");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-02").await;

        assert!(result.is_ok());
        let proposal = result.unwrap();

        assert_eq!(proposal.agent_id, "agent-02");
        assert_eq!(proposal.modifications.len(), 1);
        assert_eq!(proposal.modifications[0].file_path, sample_file);
        assert_eq!(
            proposal.modifications[0].modification_type,
            ModificationType::Modify
        );

        // Verify the file was analyzed
        let explanation = proposal.modifications[0].explanation.as_ref().unwrap();
        assert!(explanation.contains("lines"));
        assert!(explanation.contains("TODOs"));
    }

    #[tokio::test]
    async fn test_execute_task_multiple_files() {
        let fixtures = fixtures_path();
        let files = vec![
            fixtures.join("sample_rust_code.rs"),
            fixtures.join("clean_code.rs"),
        ];

        // Check if fixtures exist
        let existing_files: Vec<PathBuf> = files.into_iter().filter(|f| f.exists()).collect();

        if existing_files.is_empty() {
            eprintln!("Skipping test: no fixture files found");
            return;
        }

        let assignment = create_test_assignment(existing_files.clone(), "Review code quality");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-03").await;

        assert!(result.is_ok());
        let proposal = result.unwrap();

        assert_eq!(proposal.modifications.len(), existing_files.len());

        // Verify rationale mentions the correct file count
        assert!(proposal
            .rationale
            .contains(&format!("{} files", existing_files.len())));
    }

    #[tokio::test]
    async fn test_execute_task_nonexistent_file() {
        let nonexistent = PathBuf::from("/nonexistent/path/to/file.rs");
        let assignment = create_test_assignment(vec![nonexistent], "Process file");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-04").await;

        // Should succeed but with no modifications for unreadable files
        assert!(result.is_ok());
        let proposal = result.unwrap();
        assert!(proposal.modifications.is_empty());
    }

    #[tokio::test]
    async fn test_execute_task_proposal_hash_integrity() {
        let assignment = create_test_assignment(vec![], "Test hash computation");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-05").await;

        assert!(result.is_ok());
        let proposal = result.unwrap();

        // Content hash should be non-zero
        assert_ne!(proposal.content_hash, [0u8; 32]);

        // Recomputing should give same hash
        let recomputed = proposal.compute_hash();
        assert_eq!(proposal.content_hash, recomputed);
    }

    #[tokio::test]
    async fn test_execute_task_deterministic_output() {
        let assignment = create_test_assignment(vec![], "Determinism test");

        let result1 =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-det")
                .await
                .unwrap();
        let result2 =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-det")
                .await
                .unwrap();

        // Agent ID and task ID should match
        assert_eq!(result1.agent_id, result2.agent_id);
        assert_eq!(result1.task_id, result2.task_id);

        // Modifications should be the same (empty in this case)
        assert_eq!(result1.modifications.len(), result2.modifications.len());

        // Rationale should be the same
        assert_eq!(result1.rationale, result2.rationale);
    }
}

mod file_analysis_tests {
    use super::*;

    #[tokio::test]
    async fn test_analyze_file_with_todos() {
        let fixtures = fixtures_path();
        let sample_file = fixtures.join("sample_rust_code.rs");

        if !sample_file.exists() {
            eprintln!("Skipping test: fixture file not found");
            return;
        }

        let assignment = create_test_assignment(vec![sample_file], "Analyze TODOs in the codebase");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-06")
                .await
                .unwrap();

        // Should find TODOs in sample_rust_code.rs
        assert!(!result.modifications.is_empty());
        let explanation = result.modifications[0].explanation.as_ref().unwrap();

        // The sample file has TODO and FIXME comments, plus unimplemented!/todo! macros
        assert!(
            explanation.contains("TODOs") || explanation.contains("areas requiring attention"),
            "Should detect TODOs/FIXMEs in the file: {}",
            explanation
        );
    }

    #[tokio::test]
    async fn test_analyze_clean_file() {
        let fixtures = fixtures_path();
        let clean_file = fixtures.join("clean_code.rs");

        if !clean_file.exists() {
            eprintln!("Skipping test: fixture file not found");
            return;
        }

        let assignment = create_test_assignment(vec![clean_file], "Check code quality");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-07")
                .await
                .unwrap();

        // Should find no issues in clean_code.rs
        assert!(!result.modifications.is_empty());
        let explanation = result.modifications[0].explanation.as_ref().unwrap();

        assert!(
            explanation.contains("No obvious issues") || explanation.contains("0 TODOs"),
            "Clean file should report no issues: {}",
            explanation
        );
    }

    #[tokio::test]
    async fn test_analyze_empty_file() {
        let fixtures = fixtures_path();
        let empty_file = fixtures.join("empty_file.rs");

        if !empty_file.exists() {
            eprintln!("Skipping test: fixture file not found");
            return;
        }

        let assignment = create_test_assignment(vec![empty_file], "Analyze empty file");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-08")
                .await
                .unwrap();

        // Should handle empty file gracefully
        assert!(!result.modifications.is_empty());
        let explanation = result.modifications[0].explanation.as_ref().unwrap();

        // Empty file should have 0 lines
        assert!(
            explanation.contains("0 lines") || explanation.contains("No obvious issues"),
            "Empty file analysis: {}",
            explanation
        );
    }

    #[tokio::test]
    async fn test_file_modification_structure() {
        let fixtures = fixtures_path();
        let sample_file = fixtures.join("sample_rust_code.rs");

        if !sample_file.exists() {
            eprintln!("Skipping test: fixture file not found");
            return;
        }

        let assignment = create_test_assignment(vec![sample_file.clone()], "Check modifications");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-09")
                .await
                .unwrap();

        let modification = &result.modifications[0];

        // Verify FileModification structure
        assert_eq!(modification.file_path, sample_file);
        assert_eq!(modification.modification_type, ModificationType::Modify);
        assert!(modification.original_content_hash.is_some());
        assert!(modification.explanation.is_some());

        // Current stub doesn't set new_content or diff
        assert!(modification.new_content.is_none());
        assert!(modification.diff.is_none());
        assert!(modification.line_range.is_none());
    }
}

mod proposal_structure_tests {
    use super::*;

    #[tokio::test]
    async fn test_proposal_serialization() {
        let assignment = create_test_assignment(vec![], "Serialization test");

        let proposal =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-10")
                .await
                .unwrap();

        // Verify proposal can be serialized/deserialized
        let json = serde_json::to_string(&proposal).expect("Should serialize proposal");
        let deserialized: TaskProposal =
            serde_json::from_str(&json).expect("Should deserialize proposal");

        assert_eq!(deserialized.agent_id, proposal.agent_id);
        assert_eq!(deserialized.task_id, proposal.task_id);
        assert_eq!(deserialized.rationale, proposal.rationale);
        assert_eq!(deserialized.confidence, proposal.confidence);
    }

    #[tokio::test]
    async fn test_proposal_confidence_bounds() {
        let assignment = create_test_assignment(vec![], "Confidence bounds test");

        let proposal =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-11")
                .await
                .unwrap();

        // Confidence should be in valid range [0.0, 1.0]
        assert!(proposal.confidence >= 0.0);
        assert!(proposal.confidence <= 1.0);
    }

    #[tokio::test]
    async fn test_proposal_timestamp() {
        let before = chrono::Utc::now();

        let assignment = create_test_assignment(vec![], "Timestamp test");

        let proposal =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-12")
                .await
                .unwrap();

        let after = chrono::Utc::now();

        // Timestamp should be between before and after
        assert!(proposal.created_at >= before);
        assert!(proposal.created_at <= after);
    }

    #[tokio::test]
    async fn test_proposal_dependencies_empty_by_default() {
        let assignment = create_test_assignment(vec![], "Dependencies test");

        let proposal =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-13")
                .await
                .unwrap();

        // Stub implementation should have no dependencies
        assert!(proposal.depends_on.is_empty());
        assert!(proposal.conflicts_with.is_empty());
    }
}

mod golden_tests {
    use super::*;

    /// Golden test: Verify expected analysis output for sample_rust_code.rs
    #[tokio::test]
    async fn golden_sample_rust_analysis() {
        let fixtures = fixtures_path();
        let sample_file = fixtures.join("sample_rust_code.rs");

        if !sample_file.exists() {
            eprintln!("Skipping golden test: fixture file not found");
            return;
        }

        let assignment =
            create_test_assignment(vec![sample_file], "Identify areas needing attention");

        let proposal =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "golden-agent")
                .await
                .unwrap();

        // Golden expectations for sample_rust_code.rs:
        // - Has TODO comments (lines 15, 26)
        // - Has FIXME comment (line 27)
        // - Has unimplemented!() (line 37)
        // - Has todo!() (line 42)
        // - Approximately 52 lines

        let explanation = proposal.modifications[0].explanation.as_ref().unwrap();

        // Should detect multiple TODOs/FIXMEs
        assert!(
            explanation.contains("TODO") || explanation.contains("areas requiring attention"),
            "Golden test: should detect TODOs"
        );

        // Should detect unimplemented!/todo! calls
        assert!(
            explanation.contains("unimplemented")
                || explanation.contains("areas requiring attention"),
            "Golden test: should detect unimplemented calls"
        );
    }

    /// Golden test: Verify clean file produces expected output
    #[tokio::test]
    async fn golden_clean_code_analysis() {
        let fixtures = fixtures_path();
        let clean_file = fixtures.join("clean_code.rs");

        if !clean_file.exists() {
            eprintln!("Skipping golden test: fixture file not found");
            return;
        }

        let assignment = create_test_assignment(vec![clean_file], "Verify code cleanliness");

        let proposal =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "golden-agent")
                .await
                .unwrap();

        let explanation = proposal.modifications[0].explanation.as_ref().unwrap();

        // Golden expectations for clean_code.rs:
        // - No TODOs
        // - No FIXMEs
        // - No unimplemented!/todo! macros
        // - Should report "No obvious issues"

        assert!(
            explanation.contains("0 TODOs") || explanation.contains("No obvious issues"),
            "Golden test: clean file should report no issues: {}",
            explanation
        );
        assert!(
            explanation.contains("0 unimplemented") || explanation.contains("No obvious issues"),
            "Golden test: clean file should have no unimplemented: {}",
            explanation
        );
    }

    /// Golden test: Empty file handling
    #[tokio::test]
    async fn golden_empty_file_analysis() {
        let fixtures = fixtures_path();
        let empty_file = fixtures.join("empty_file.rs");

        if !empty_file.exists() {
            eprintln!("Skipping golden test: fixture file not found");
            return;
        }

        let assignment = create_test_assignment(vec![empty_file], "Analyze empty source");

        let proposal =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "golden-agent")
                .await
                .unwrap();

        let explanation = proposal.modifications[0].explanation.as_ref().unwrap();

        // Golden expectations for empty file:
        // - 0 lines
        // - 0 TODOs
        // - 0 unimplemented calls
        // - "No obvious issues"

        assert!(
            explanation.contains("0 lines"),
            "Golden test: empty file should have 0 lines: {}",
            explanation
        );
    }
}

mod edge_cases {
    use super::*;

    #[tokio::test]
    async fn test_special_characters_in_agent_id() {
        let assignment = create_test_assignment(vec![], "Test special chars");

        // Test with various agent IDs
        let special_ids = vec![
            "agent-with-dashes",
            "agent_with_underscores",
            "agent.with.dots",
            "agent:colon",
            "agent/slash",
            "agent@at",
        ];

        for agent_id in special_ids {
            let result =
                adapteros_cli::commands::worker_executor::execute_task(&assignment, agent_id).await;
            assert!(result.is_ok(), "Should handle agent ID: {}", agent_id);
            assert_eq!(result.unwrap().agent_id, agent_id);
        }
    }

    #[tokio::test]
    async fn test_unicode_in_objective() {
        let assignment =
            create_test_assignment(vec![], "Refactor code with unicode: cafe, , emoji");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-unicode")
                .await;

        assert!(result.is_ok());
        let proposal = result.unwrap();
        assert!(proposal.rationale.contains("unicode"));
    }

    #[tokio::test]
    async fn test_very_long_objective() {
        let long_objective = "a".repeat(10000);
        let assignment = create_test_assignment(vec![], &long_objective);

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-long").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_empty_objective() {
        let assignment = create_test_assignment(vec![], "");

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-empty")
                .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_binary_task_id() {
        // Test with various task ID patterns
        let mut assignment = create_test_assignment(vec![], "Binary ID test");
        assignment.task_id = [0xff; 32]; // All 1s

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-bin").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().task_id, [0xff; 32]);
    }

    #[tokio::test]
    async fn test_context_json_handling() {
        let mut assignment = create_test_assignment(vec![], "Context test");
        assignment.context = serde_json::json!({
            "project": "test-project",
            "nested": {
                "key": "value",
                "array": [1, 2, 3]
            },
            "null_field": null,
            "bool_field": true
        });

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-ctx").await;

        assert!(result.is_ok());
    }
}

mod constraint_tests {
    use super::*;

    #[tokio::test]
    async fn test_assignment_with_constraints() {
        let fixtures = fixtures_path();
        let files: Vec<PathBuf> = vec![
            fixtures.join("sample_rust_code.rs"),
            fixtures.join("clean_code.rs"),
        ]
        .into_iter()
        .filter(|f| f.exists())
        .collect();

        if files.is_empty() {
            eprintln!("Skipping test: no fixture files found");
            return;
        }

        let assignment =
            create_constrained_assignment(files, "Limited modifications", Some(5), Some(0.3));

        let result = adapteros_cli::commands::worker_executor::execute_task(
            &assignment,
            "agent-constrained",
        )
        .await;

        assert!(result.is_ok());
        let proposal = result.unwrap();

        // Note: Current stub doesn't enforce constraints, but the assignment structure is valid
        assert!(proposal.modifications.len() <= 5);
    }

    #[tokio::test]
    async fn test_excluded_patterns_in_constraints() {
        let mut assignment = create_test_assignment(vec![], "Exclusion test");
        assignment.constraints.excluded_patterns = vec![
            "*.test.rs".to_string(),
            "*_test.rs".to_string(),
            "tests/**".to_string(),
        ];

        let result =
            adapteros_cli::commands::worker_executor::execute_task(&assignment, "agent-exclude")
                .await;

        assert!(result.is_ok());
    }
}
