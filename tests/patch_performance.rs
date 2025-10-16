//! Performance tests for patch proposal system
//!
//! Tests performance requirements and benchmarks:
//! - Evidence retrieval performance (< 100ms)
//! - Patch generation performance (< 2s)
//! - Policy validation performance
//! - Memory usage and resource limits
//! - Concurrent request handling
//!
//! Aligns with performance targets from code intelligence requirements.

use adapteros_lora_worker::{
    evidence::{
        EvidenceRequest, EvidenceRetriever, MockCodeIndex, MockDocIndex, MockFrameworkIndex,
        MockSymbolIndex, MockTestIndex,
    },
    patch_generator::{FilePatch, HunkType, PatchHunk},
    patch_generator::{MockLlmBackend, PatchGenerationRequest, PatchGenerator},
    patch_validator::{CodePolicy, PatchValidator},
};
use std::collections::HashMap;
use std::time::Instant;
use tokio;

/// Test evidence retrieval performance requirements
#[tokio::test]
async fn test_evidence_retrieval_performance() {
    let retriever = EvidenceRetriever::new(
        Box::new(MockSymbolIndex),
        Box::new(MockTestIndex),
        Box::new(MockDocIndex),
        Box::new(MockCodeIndex),
        Box::new(MockFrameworkIndex),
    );

    let request = EvidenceRequest {
        query: "performance test query".to_string(),
        target_files: vec!["src/test.rs".to_string()],
        repo_id: "test_repo".to_string(),
        commit_sha: None,
        max_results: 10,
        min_score: 0.5,
    };

    // Test single retrieval performance
    let start = Instant::now();
    let result = retriever.retrieve_patch_evidence(&request).await.unwrap();
    let duration = start.elapsed();

    // Should complete within 100ms (performance requirement)
    assert!(duration.as_millis() < 100);
    assert_eq!(result.spans.len(), 5);
    assert_eq!(result.sources_used.len(), 5);

    // Test multiple concurrent retrievals
    let start = Instant::now();
    let futures: Vec<_> = (0..10)
        .map(|_| retriever.retrieve_patch_evidence(&request))
        .collect();

    let results = futures::future::join_all(futures).await;
    let duration = start.elapsed();

    // All retrievals should complete within 500ms
    assert!(duration.as_millis() < 500);
    assert_eq!(results.len(), 10);

    for result in results {
        assert!(result.is_ok());
        let evidence_result = result.unwrap();
        assert_eq!(evidence_result.spans.len(), 5);
    }
}

/// Test patch generation performance requirements
#[tokio::test]
async fn test_patch_generation_performance() {
    let generator = PatchGenerator::new(
        Box::new(MockLlmBackend),
        adapteros_lora_worker::patch_generator::PatchParser::new(),
        adapteros_lora_worker::patch_generator::CitationExtractor::new(),
    );

    let request = PatchGenerationRequest {
        repo_id: "test_repo".to_string(),
        commit_sha: None,
        target_files: vec!["src/test.rs".to_string()],
        description: "Performance test patch generation".to_string(),
        evidence: vec![],
        context: HashMap::new(),
    };

    // Test single patch generation performance
    let start = Instant::now();
    let proposal = generator.generate_patch(request).await.unwrap();
    let duration = start.elapsed();

    // Should complete within 2s (performance requirement)
    assert!(duration.as_secs() < 2);
    assert!(!proposal.proposal_id.is_empty());
    assert!(proposal.confidence > 0.0);

    // Test multiple concurrent generations
    let start = Instant::now();
    let futures: Vec<_> = (0..5)
        .map(|i| {
            let req = PatchGenerationRequest {
                repo_id: format!("test_repo_{}", i),
                commit_sha: None,
                target_files: vec![format!("src/test_{}.rs", i)],
                description: format!("Performance test patch {}", i),
                evidence: vec![],
                context: HashMap::new(),
            };
            generator.generate_patch(req)
        })
        .collect();

    let results = futures::future::join_all(futures).await;
    let duration = start.elapsed();

    // All generations should complete within 10s
    assert!(duration.as_secs() < 10);
    assert_eq!(results.len(), 5);

    for result in results {
        assert!(result.is_ok());
        let proposal = result.unwrap();
        assert!(!proposal.proposal_id.is_empty());
    }
}

/// Test policy validation performance
#[tokio::test]
async fn test_policy_validation_performance() {
    let policy = CodePolicy::default();
    let validator = PatchValidator::new(policy);

    // Create a large patch for performance testing
    let large_patch = FilePatch {
        file_path: "src/large.rs".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 100,
            context_lines: vec![],
            modified_lines: vec!["fn large_function() {}".to_string(); 50],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 50,
        metadata: HashMap::new(),
    };

    // Test single validation performance
    let start = Instant::now();
    let result = validator.validate(&[large_patch.clone()]).await.unwrap();
    let duration = start.elapsed();

    // Should complete within 100ms
    assert!(duration.as_millis() < 100);
    assert!(result.is_valid);

    // Test multiple concurrent validations
    let start = Instant::now();
    let futures: Vec<_> = (0..20)
        .map(|_| validator.validate(&[large_patch.clone()]))
        .collect();

    let results = futures::future::join_all(futures).await;
    let duration = start.elapsed();

    // All validations should complete within 1s
    assert!(duration.as_millis() < 1000);
    assert_eq!(results.len(), 20);

    for result in results {
        assert!(result.is_ok());
        let validation_result = result.unwrap();
        assert!(validation_result.is_valid);
    }
}

/// Test memory usage and resource limits
#[tokio::test]
async fn test_memory_usage_limits() {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    // Test concurrent request limiting
    let max_concurrent = 5;
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    let generator = PatchGenerator::new(
        Box::new(MockLlmBackend),
        adapteros_lora_worker::patch_generator::PatchParser::new(),
        adapteros_lora_worker::patch_generator::CitationExtractor::new(),
    );

    // Create more requests than the limit
    let request_count = 10;
    let futures: Vec<_> = (0..request_count)
        .map(|i| {
            let semaphore = semaphore.clone();
            let generator = &generator;
            async move {
                let _permit = semaphore.acquire().await.unwrap();
                let req = PatchGenerationRequest {
                    repo_id: format!("test_repo_{}", i),
                    commit_sha: None,
                    target_files: vec![format!("src/test_{}.rs", i)],
                    description: format!("Memory test patch {}", i),
                    evidence: vec![],
                    context: HashMap::new(),
                };
                generator.generate_patch(req).await
            }
        })
        .collect();

    let start = Instant::now();
    let results = futures::future::join_all(futures).await;
    let duration = start.elapsed();

    // All requests should complete successfully
    assert_eq!(results.len(), request_count);
    for result in results {
        assert!(result.is_ok());
    }

    // Should not take too long (semaphore limits concurrency)
    assert!(duration.as_secs() < 30);
}

/// Test patch size impact on performance
#[tokio::test]
async fn test_patch_size_performance_impact() {
    let generator = PatchGenerator::new(
        Box::new(MockLlmBackend),
        adapteros_lora_worker::patch_generator::PatchParser::new(),
        adapteros_lora_worker::patch_generator::CitationExtractor::new(),
    );

    // Test small patch
    let small_request = PatchGenerationRequest {
        repo_id: "test_repo".to_string(),
        commit_sha: None,
        target_files: vec!["src/small.rs".to_string()],
        description: "Small patch".to_string(),
        evidence: vec![],
        context: HashMap::new(),
    };

    let start = Instant::now();
    let small_proposal = generator.generate_patch(small_request).await.unwrap();
    let small_duration = start.elapsed();

    // Test large patch
    let large_request = PatchGenerationRequest {
        repo_id: "test_repo".to_string(),
        commit_sha: None,
        target_files: vec!["src/large.rs".to_string()],
        description: "Large patch with many changes".to_string(),
        evidence: vec![],
        context: HashMap::new(),
    };

    let start = Instant::now();
    let large_proposal = generator.generate_patch(large_request).await.unwrap();
    let large_duration = start.elapsed();

    // Both should complete within performance limits
    assert!(small_duration.as_secs() < 2);
    assert!(large_duration.as_secs() < 2);

    // Large patch might take slightly longer but should still be reasonable
    assert!(large_duration <= small_duration * 2);

    assert!(!small_proposal.proposal_id.is_empty());
    assert!(!large_proposal.proposal_id.is_empty());
}

/// Test evidence quality impact on performance
#[tokio::test]
async fn test_evidence_quality_performance() {
    use adapteros_lora_worker::evidence::{EvidenceSpan, EvidenceType};

    let generator = PatchGenerator::new(
        Box::new(MockLlmBackend),
        adapteros_lora_worker::patch_generator::PatchParser::new(),
        adapteros_lora_worker::patch_generator::CitationExtractor::new(),
    );

    // Test with no evidence
    let no_evidence_request = PatchGenerationRequest {
        repo_id: "test_repo".to_string(),
        commit_sha: None,
        target_files: vec!["src/test.rs".to_string()],
        description: "No evidence patch".to_string(),
        evidence: vec![],
        context: HashMap::new(),
    };

    let start = Instant::now();
    let no_evidence_proposal = generator.generate_patch(no_evidence_request).await.unwrap();
    let no_evidence_duration = start.elapsed();

    // Test with high-quality evidence
    let high_quality_evidence = vec![
        EvidenceSpan {
            doc_id: "high_quality".to_string(),
            rev: "v1".to_string(),
            span_hash: "hash1".to_string(),
            score: 0.9,
            evidence_type: EvidenceType::Symbol,
            file_path: "src/test.rs".to_string(),
            start_line: 10,
            end_line: 15,
            content: "High quality evidence".to_string(),
            metadata: HashMap::new(),
        },
        EvidenceSpan {
            doc_id: "high_quality_2".to_string(),
            rev: "v1".to_string(),
            span_hash: "hash2".to_string(),
            score: 0.8,
            evidence_type: EvidenceType::Test,
            file_path: "tests/test.rs".to_string(),
            start_line: 20,
            end_line: 25,
            content: "High quality test evidence".to_string(),
            metadata: HashMap::new(),
        },
    ];

    let high_quality_request = PatchGenerationRequest {
        repo_id: "test_repo".to_string(),
        commit_sha: None,
        target_files: vec!["src/test.rs".to_string()],
        description: "High quality evidence patch".to_string(),
        evidence: high_quality_evidence,
        context: HashMap::new(),
    };

    let start = Instant::now();
    let high_quality_proposal = generator
        .generate_patch(high_quality_request)
        .await
        .unwrap();
    let high_quality_duration = start.elapsed();

    // Both should complete within performance limits
    assert!(no_evidence_duration.as_secs() < 2);
    assert!(high_quality_duration.as_secs() < 2);

    // High quality evidence should result in higher confidence
    assert!(high_quality_proposal.confidence > no_evidence_proposal.confidence);
    assert_eq!(high_quality_proposal.citations.len(), 2);
    assert_eq!(no_evidence_proposal.citations.len(), 0);
}

/// Test error handling performance
#[tokio::test]
async fn test_error_handling_performance() {
    let validator = PatchValidator::new(CodePolicy::default());

    // Create patch with multiple violations
    let violation_patch = FilePatch {
        file_path: ".env".to_string(),
        hunks: vec![PatchHunk {
            start_line: 1,
            end_line: 10,
            context_lines: vec![],
            modified_lines: vec![
                "api_key = \"sk-1234567890abcdef\"".to_string(),
                "eval(user_input)".to_string(),
                "use external_crate::function;".to_string(),
            ],
            hunk_type: HunkType::Addition,
        }],
        total_lines: 3,
        metadata: HashMap::new(),
    };

    // Test validation performance with violations
    let start = Instant::now();
    let result = validator.validate(&[violation_patch]).await.unwrap();
    let duration = start.elapsed();

    // Should complete quickly even with violations
    assert!(duration.as_millis() < 100);
    assert!(!result.is_valid);
    assert!(result.violations.len() >= 3);

    // Test multiple concurrent validations with violations
    let start = Instant::now();
    let futures: Vec<_> = (0..10)
        .map(|_| validator.validate(&[violation_patch.clone()]))
        .collect();

    let results = futures::future::join_all(futures).await;
    let duration = start.elapsed();

    // All validations should complete within 500ms
    assert!(duration.as_millis() < 500);
    assert_eq!(results.len(), 10);

    for result in results {
        assert!(result.is_ok());
        let validation_result = result.unwrap();
        assert!(!validation_result.is_valid);
    }
}

/// Test performance under load
#[tokio::test]
async fn test_performance_under_load() {
    let retriever = EvidenceRetriever::new(
        Box::new(MockSymbolIndex),
        Box::new(MockTestIndex),
        Box::new(MockDocIndex),
        Box::new(MockCodeIndex),
        Box::new(MockFrameworkIndex),
    );

    let generator = PatchGenerator::new(
        Box::new(MockLlmBackend),
        adapteros_lora_worker::patch_generator::PatchParser::new(),
        adapteros_lora_worker::patch_generator::CitationExtractor::new(),
    );

    let validator = PatchValidator::new(CodePolicy::default());

    // Simulate high load with many concurrent operations
    let load_count = 50;
    let start = Instant::now();

    let futures: Vec<_> = (0..load_count)
        .map(|i| {
            let retriever = &retriever;
            let generator = &generator;
            let validator = &validator;

            async move {
                // 1. Retrieve evidence
                let evidence_request = EvidenceRequest {
                    query: format!("Load test query {}", i),
                    target_files: vec![format!("src/test_{}.rs", i)],
                    repo_id: format!("test_repo_{}", i),
                    commit_sha: None,
                    max_results: 5,
                    min_score: 0.5,
                };
                let evidence_result = retriever
                    .retrieve_patch_evidence(&evidence_request)
                    .await
                    .unwrap();

                // 2. Generate patch
                let patch_request = PatchGenerationRequest {
                    repo_id: format!("test_repo_{}", i),
                    commit_sha: None,
                    target_files: vec![format!("src/test_{}.rs", i)],
                    description: format!("Load test patch {}", i),
                    evidence: evidence_result.spans,
                    context: HashMap::new(),
                };
                let proposal = generator.generate_patch(patch_request).await.unwrap();

                // 3. Validate patch
                let validation_result = validator.validate(&proposal.patches).await.unwrap();

                (proposal, validation_result)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;
    let duration = start.elapsed();

    // All operations should complete within reasonable time
    assert!(duration.as_secs() < 60);
    assert_eq!(results.len(), load_count);

    // Verify all operations succeeded
    for (proposal, validation_result) in results {
        assert!(!proposal.proposal_id.is_empty());
        assert!(validation_result.is_valid);
        assert!(proposal.confidence > 0.0);
    }

    println!(
        "Load test completed: {} operations in {:?}",
        load_count, duration
    );
}
