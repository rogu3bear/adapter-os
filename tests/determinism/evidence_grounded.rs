<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
//! Evidence-grounded response verification tests for AdapterOS determinism
//!
//! Verifies that all responses are properly backed by evidence from code,
//! tests, documentation, and other verifiable sources.

use super::utils::*;

/// Test basic evidence-grounded response verification
#[test]
fn test_evidence_grounded_response_verification() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence for a response
    let evidence = vec![
        "function: calculate_total".to_string(),
        "file: src/math.rs".to_string(),
        "test: test_calculate_total".to_string(),
    ];

    verifier.add_evidence("response1", evidence);

    // Verify response that references the evidence
    let response = "The calculate_total function in src/math.rs computes the sum correctly as shown in test_calculate_total.";
    verifier.verify_evidence_grounding("response1", response).unwrap();
}

/// Test evidence-grounded response with missing evidence
#[test]
fn test_evidence_grounded_missing_evidence() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence
    let evidence = vec!["function: add_numbers".to_string()];
    verifier.add_evidence("response1", evidence);

    // Response that doesn't reference the evidence should fail
    let response = "The multiply_numbers function works well.";
    assert!(verifier.verify_evidence_grounding("response1", response).is_err());
}

/// Test multiple evidence sources
#[test]
fn test_multiple_evidence_sources() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence from multiple sources
    let evidence = vec![
        "function: tokenize_text".to_string(),
        "file: src/nlp/tokenizer.rs".to_string(),
        "documentation: docs/nlp/tokenization.md".to_string(),
        "test: test_tokenizer_unicode".to_string(),
        "benchmark: tokenizer_performance".to_string(),
    ];

    verifier.add_evidence("nlp_response", evidence);

    // Response that references multiple evidence sources
    let response = "The tokenize_text function in src/nlp/tokenizer.rs handles Unicode correctly as documented in docs/nlp/tokenization.md and verified by test_tokenizer_unicode and benchmark_tokenizer_performance.";
    verifier.verify_evidence_grounding("nlp_response", response).unwrap();
}

/// Test evidence grounding with code snippets
#[test]
fn test_evidence_grounding_code_snippets() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence including code snippets
    let evidence = vec![
        "function: validate_input".to_string(),
        "code: if input.len() > MAX_SIZE { return Err(ValidationError::TooLarge); }".to_string(),
        "file: src/validation.rs".to_string(),
    ];

    verifier.add_evidence("validation_response", evidence);

    // Response that references the code
    let response = "The validate_input function in src/validation.rs checks if input.len() > MAX_SIZE and returns ValidationError::TooLarge.";
    verifier.verify_evidence_grounding("validation_response", response).unwrap();
}

/// Test evidence grounding with test cases
#[test]
fn test_evidence_grounding_test_cases() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add test case evidence
    let evidence = vec![
        "test: test_matrix_multiplication".to_string(),
        "test_case: multiply_2x2_matrices".to_string(),
        "expected_result: [[7, 10], [15, 22]]".to_string(),
    ];

    verifier.add_evidence("matrix_response", evidence);

    // Response that references test results
    let response = "Matrix multiplication works correctly as shown in test_matrix_multiplication, specifically the multiply_2x2_matrices test case produces [[7, 10], [15, 22]].";
    verifier.verify_evidence_grounding("matrix_response", response).unwrap();
}

/// Test evidence grounding with documentation references
#[test]
fn test_evidence_grounding_documentation() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add documentation evidence
    let evidence = vec![
        "documentation: docs/api/determinism.md".to_string(),
        "section: HKDF Seeding".to_string(),
        "requirement: All RNG must derive from global seed".to_string(),
    ];

    verifier.add_evidence("determinism_response", evidence);

    // Response that references documentation
    let response = "According to docs/api/determinism.md section HKDF Seeding, all RNG must derive from global seed.";
    verifier.verify_evidence_grounding("determinism_response", response).unwrap();
}

/// Test evidence grounding with performance benchmarks
#[test]
fn test_evidence_grounding_performance() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add performance evidence
    let evidence = vec![
        "benchmark: inference_latency".to_string(),
        "result: 95th percentile 50ms".to_string(),
        "hardware: M2 MacBook Pro".to_string(),
    ];

    verifier.add_evidence("performance_response", evidence);

    // Response that references performance data
    let response = "The inference latency benchmark shows 95th percentile of 50ms on M2 MacBook Pro hardware.";
    verifier.verify_evidence_grounding("performance_response", response).unwrap();
}

/// Test evidence grounding with security analysis
#[test]
fn test_evidence_grounding_security() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add security evidence
    let evidence = vec![
        "security_audit: buffer_overflow_check".to_string(),
        "vulnerability: CVE-2023-XXXX".to_string(),
        "mitigation: bounds checking".to_string(),
    ];

    verifier.add_evidence("security_response", evidence);

    // Response that references security analysis
    let response = "The buffer_overflow_check security audit confirms that CVE-2023-XXXX is mitigated through bounds checking.";
    verifier.verify_evidence_grounding("security_response", response).unwrap();
}

/// Test evidence grounding with multiple responses
#[test]
fn test_evidence_grounding_multiple_responses() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence for different responses
    verifier.add_evidence("response1", vec!["function: func_a".to_string()]);
    verifier.add_evidence("response2", vec!["function: func_b".to_string()]);
    verifier.add_evidence("response3", vec!["function: func_c".to_string()]);

    // Verify each response independently
    verifier.verify_evidence_grounding("response1", "func_a works correctly.").unwrap();
    verifier.verify_evidence_grounding("response2", "func_b is implemented.").unwrap();
    verifier.verify_evidence_grounding("response3", "func_c has good performance.").unwrap();
}

/// Test evidence grounding with overlapping evidence
#[test]
fn test_evidence_grounding_overlapping() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add overlapping evidence
    verifier.add_evidence("shared_evidence", vec![
        "function: shared_func".to_string(),
        "file: shared.rs".to_string(),
    ]);

    // Multiple responses can reference the same evidence
    verifier.verify_evidence_grounding("shared_evidence", "shared_func in shared.rs is reliable.").unwrap();
    verifier.verify_evidence_grounding("shared_evidence", "The shared.rs file contains shared_func.").unwrap();
}

/// Test evidence grounding failure cases
#[test]
fn test_evidence_grounding_failures() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Empty evidence
    verifier.add_evidence("empty_response", vec![]);
    assert!(verifier.verify_evidence_grounding("empty_response", "Some response.").is_err());

    // Response with no evidence reference
    verifier.add_evidence("no_match_response", vec!["evidence_a".to_string()]);
    assert!(verifier.verify_evidence_grounding("no_match_response", "This mentions nothing about evidence_a.").is_err());

    // Non-existent response ID
    assert!(verifier.verify_evidence_grounding("nonexistent", "Some response.").is_err());
}

/// Test evidence grounding with complex responses
#[test]
fn test_evidence_grounding_complex_responses() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add comprehensive evidence
    let evidence = vec![
        "architecture: microservices".to_string(),
        "component: api_gateway".to_string(),
        "component: user_service".to_string(),
        "component: order_service".to_string(),
        "protocol: grpc".to_string(),
        "database: postgresql".to_string(),
        "cache: redis".to_string(),
        "monitoring: prometheus".to_string(),
    ];

    verifier.add_evidence("architecture_response", evidence);

    // Complex response referencing multiple evidence pieces
    let response = "The microservices architecture uses an api_gateway to route requests to user_service and order_service, communicating via grpc protocol. Data is stored in postgresql with redis caching, and monitoring is handled by prometheus.";
    verifier.verify_evidence_grounding("architecture_response", response).unwrap();
}

/// Test evidence grounding performance
#[test]
fn test_evidence_grounding_performance() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add many evidence entries
    for i in 0..100 {
        let evidence = vec![format!("evidence_{}", i)];
        verifier.add_evidence(&format!("response_{}", i), evidence);
    }

    let start = std::time::Instant::now();

    // Verify many responses
    for i in 0..100 {
        let response = format!("This response references evidence_{}.", i);
        verifier.verify_evidence_grounding(&format!("response_{}", i), &response).unwrap();
    }

    let duration = start.elapsed();

    // Should be reasonably fast (< 100ms for 100 verifications)
    assert!(duration < std::time::Duration::from_millis(100),
            "Evidence grounding verification should be performant: {:?}", duration);
}

/// Test evidence grounding with special characters
#[test]
fn test_evidence_grounding_special_characters() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence with special characters
    let evidence = vec![
        "function: calculate_hash_256".to_string(),
        "file: src/crypto/sha256.rs".to_string(),
        "algorithm: SHA-256".to_string(),
    ];

    verifier.add_evidence("crypto_response", evidence);

    // Response with special characters
    let response = "The calculate_hash_256 function in src/crypto/sha256.rs implements SHA-256 algorithm correctly.";
    verifier.verify_evidence_grounding("crypto_response", response).unwrap();
}

/// Test evidence grounding case sensitivity
#[test]
fn test_evidence_grounding_case_sensitivity() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence with mixed case
    let evidence = vec!["Function: CalculateTotal".to_string()];
    verifier.add_evidence("case_response", evidence);

    // Response with different case should still match (case-insensitive)
    let response = "function: calculatetotal works well.";
    verifier.verify_evidence_grounding("case_response", response).unwrap();
}

/// Test evidence grounding with partial matches
#[test]
fn test_evidence_grounding_partial_matches() {
    let mut verifier = EvidenceGroundedVerifier::new();

    // Add evidence
    let evidence = vec!["long_function_name_that_does_something".to_string()];
    verifier.add_evidence("partial_response", evidence);

    // Response with partial match should work
    let response = "The long_function_name handles the task properly.";
    verifier.verify_evidence_grounding("partial_response", response).unwrap();
}