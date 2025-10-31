#![cfg(all(test, feature = "extended-tests"))]

//! Deterministic testing for MPLoRA multi-path execution
//!
//! Tests orthogonal constraints and shared downsample functionality
//! Reference: https://openreview.net/pdf?id=jqz6Msm3AF

use adapteros_core::Result;
use adapteros_lora_kernel_api::{MploraConfig, RouterRing};
use adapteros_lora_router::{OrthogonalConstraints, Router, RouterWeights};
use adapteros_manifest::RouterCfg;
use adapteros_policy::{MploraConfig as PolicyMploraConfig, MploraPolicy};
use std::collections::HashMap;

/// Test orthogonal constraints determinism
#[test]
fn test_orthogonal_constraints_determinism() -> Result<()> {
    let mut constraints = OrthogonalConstraints::new(0.7, 0.1, 10);

    // Test 1: Empty history should have zero penalty
    let adapter_indices = vec![0, 1, 2];
    let gates = vec![16383, 16383, 16383]; // Q15 values
    let penalty = constraints.compute_penalty(&adapter_indices, &gates);
    assert_eq!(penalty, 0.0);

    // Test 2: Add identical activation should increase penalty
    constraints.update_history(&adapter_indices, &gates);
    let penalty = constraints.compute_penalty(&adapter_indices, &gates);
    assert!(penalty > 0.0);

    // Test 3: Different activation should have lower penalty
    let different_indices = vec![3, 4, 5];
    let different_gates = vec![8191, 8191, 8191]; // Different Q15 values
    let different_penalty = constraints.compute_penalty(&different_indices, &different_gates);
    assert!(different_penalty < penalty);

    // Test 4: Deterministic diversity score
    let diversity1 = constraints.diversity_score();
    let diversity2 = constraints.diversity_score();
    assert_eq!(diversity1, diversity2);

    Ok(())
}

/// Test router with MPLoRA features
#[test]
fn test_router_mplora_determinism() -> Result<()> {
    let weights = RouterWeights::new_with_mplora(
        0.3, 0.25, 0.2, 0.15, 0.1, // Original weights
        0.05, 0.03, 0.02, // MPLoRA weights
    );

    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    // Enable orthogonal constraints
    router.set_orthogonal_constraints(true, 0.7, 0.1, 10);
    router.set_compression_ratio(0.8);
    router.set_shared_downsample(true);

    // Test deterministic routing
    let features = vec![0.0; 22]; // 22-dimensional feature vector
    let priors = vec![0.1, 0.2, 0.3, 0.4, 0.5]; // 5 adapters

    let decision1 = router.route(&features, &priors);
    let decision2 = router.route(&features, &priors);

    // Should be identical
    assert_eq!(decision1.indices, decision2.indices);
    assert_eq!(decision1.gates_q15, decision2.gates_q15);

    // Test diversity score consistency
    let diversity1 = router.diversity_score();
    let diversity2 = router.diversity_score();
    assert_eq!(diversity1, diversity2);

    Ok(())
}

/// Test MPLoRA configuration validation
#[test]
fn test_mplora_config_validation() -> Result<()> {
    let policy = MploraPolicy::default();

    // Valid configuration
    let valid_config = RouterCfg {
        k_sparse: 3,
        gate_quant: "q15".to_string(),
        entropy_floor: 0.02,
        tau: 1.0,
        sample_tokens_full: 128,
        warmup: false,
        algorithm: "weighted".to_string(),
        orthogonal_penalty: 0.1,
        shared_downsample: false,
        compression_ratio: 0.8,
        multi_path_enabled: false,
        diversity_threshold: 0.05,
        orthogonal_constraints: false,
    };

    assert!(policy.validate_router_config(&valid_config).is_ok());

    // Invalid compression ratio
    let mut invalid_config = valid_config.clone();
    invalid_config.compression_ratio = 0.3; // Below minimum
    assert!(policy.validate_router_config(&invalid_config).is_err());

    // Invalid diversity threshold
    let mut invalid_config = valid_config.clone();
    invalid_config.diversity_threshold = 0.005; // Below minimum
    assert!(policy.validate_router_config(&invalid_config).is_err());

    Ok(())
}

/// Test MPLoRA policy compliance
#[test]
fn test_adapteros_policy_compliance() -> Result<()> {
    let policy = MploraPolicy {
        orthogonal_constraints_required: true,
        similarity_threshold_max: 0.8,
        diversity_threshold_min: 0.1,
        ..Default::default()
    };

    // Valid adapter selection
    let adapter_indices = vec![0, 1, 2];
    let gates = vec![16383, 16383, 16383]; // Q15 values
    let similarity_scores = vec![0.3, 0.4, 0.5];

    assert!(policy
        .check_orthogonal_compliance(&adapter_indices, &gates, &similarity_scores)
        .is_ok());

    // High similarity violation
    let high_similarity_scores = vec![0.9, 0.8, 0.7];
    assert!(policy
        .check_orthogonal_compliance(&adapter_indices, &gates, &high_similarity_scores)
        .is_err());

    Ok(())
}

/// Test deterministic multi-path execution
#[test]
fn test_deterministic_multipath_execution() -> Result<()> {
    let mut constraints = OrthogonalConstraints::new(0.7, 0.1, 10);

    // Simulate multi-path execution
    let paths = vec![
        (vec![0, 1], vec![16383, 16383]),
        (vec![2, 3], vec![8191, 8191]),
        (vec![4, 5], vec![4095, 4095]),
    ];

    let mut results = Vec::new();

    // Execute paths multiple times
    for _ in 0..5 {
        let mut path_results = Vec::new();

        for (indices, gates) in &paths {
            let penalty = constraints.compute_penalty(indices, gates);
            constraints.update_history(indices, gates);
            path_results.push((indices.clone(), gates.clone(), penalty));
        }

        results.push(path_results);
    }

    // All executions should be identical
    for i in 1..results.len() {
        assert_eq!(results[0], results[i]);
    }

    Ok(())
}

/// Test shared downsample determinism
#[test]
fn test_shared_downsample_determinism() -> Result<()> {
    let config = MploraConfig {
        shared_downsample: true,
        compression_ratio: 0.8,
        orthogonal_constraints: true,
        similarity_threshold: 0.7,
        penalty_weight: 0.1,
        history_window: 10,
    };

    // Test configuration consistency
    let config1 = config.clone();
    let config2 = config.clone();
    assert_eq!(config1.shared_downsample, config2.shared_downsample);
    assert_eq!(config1.compression_ratio, config2.compression_ratio);
    assert_eq!(
        config1.orthogonal_constraints,
        config2.orthogonal_constraints
    );

    // Test serialization determinism
    let serialized1 = serde_json::to_string(&config)?;
    let serialized2 = serde_json::to_string(&config)?;
    assert_eq!(serialized1, serialized2);

    // Test deserialization consistency
    let deserialized1: MploraConfig = serde_json::from_str(&serialized1)?;
    let deserialized2: MploraConfig = serde_json::from_str(&serialized2)?;
    assert_eq!(
        deserialized1.shared_downsample,
        deserialized2.shared_downsample
    );
    assert_eq!(
        deserialized1.compression_ratio,
        deserialized2.compression_ratio
    );

    Ok(())
}

/// Test router ring buffer with MPLoRA
#[test]
fn test_router_ring_mplora() -> Result<()> {
    let mut ring = RouterRing::new(3);

    // Test deterministic ring operations
    let indices1 = vec![0, 1, 2];
    let gates1 = vec![16383, 16383, 16383];

    let indices2 = vec![3, 4, 5];
    let gates2 = vec![8191, 8191, 8191];

    // Set first configuration
    ring.set(&indices1, &gates1);
    assert_eq!(ring.indices, indices1);
    assert_eq!(ring.gates_q15, gates1);

    // Set second configuration
    ring.set(&indices2, &gates2);
    assert_eq!(ring.indices, indices2);
    assert_eq!(ring.gates_q15, gates2);

    // Test position tracking
    assert_eq!(ring.position, 0); // Should remain 0 for deterministic testing

    Ok(())
}

/// Test comprehensive MPLoRA integration
#[test]
fn test_mplora_integration() -> Result<()> {
    // Create MPLoRA router
    let weights = RouterWeights::new_with_mplora(
        0.3, 0.25, 0.2, 0.15, 0.1, // Original weights
        0.05, 0.03, 0.02, // MPLoRA weights
    );

    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);
    router.set_orthogonal_constraints(true, 0.7, 0.1, 10);
    router.set_compression_ratio(0.8);
    router.set_shared_downsample(true);

    // Create MPLoRA policy
    let policy = MploraPolicy {
        orthogonal_constraints_required: true,
        shared_downsample_required: true,
        compression_ratio_min: 0.5,
        compression_ratio_max: 1.0,
        diversity_threshold_min: 0.01,
        similarity_threshold_max: 0.9,
        penalty_weight_min: 0.01,
        penalty_weight_max: 0.5,
        history_window_min: 5,
        history_window_max: 100,
    };

    // Create router configuration
    let router_config = RouterCfg {
        k_sparse: 3,
        gate_quant: "q15".to_string(),
        entropy_floor: 0.02,
        tau: 1.0,
        sample_tokens_full: 128,
        warmup: false,
        algorithm: "weighted".to_string(),
        orthogonal_penalty: 0.1,
        shared_downsample: true,
        compression_ratio: 0.8,
        multi_path_enabled: true,
        diversity_threshold: 0.05,
        orthogonal_constraints: true,
    };

    // Validate configuration
    assert!(policy.validate_router_config(&router_config).is_ok());

    // Test routing
    let features = vec![0.0; 22]; // 22-dimensional feature vector
    let priors = vec![0.1, 0.2, 0.3, 0.4, 0.5]; // 5 adapters

    let decision = router.route(&features, &priors);

    // Validate decision
    assert_eq!(decision.indices.len(), 3);
    assert_eq!(decision.gates_q15.len(), 3);

    // Test diversity score
    let diversity = router.diversity_score();
    assert!(diversity >= 0.0 && diversity <= 1.0);

    // Test orthogonal constraints
    let constraints = OrthogonalConstraints::new(0.7, 0.1, 10);
    assert!(constraints
        .check_constraints(&decision.indices, &decision.gates_q15)
        .is_ok());

    Ok(())
}

/// Test MPLoRA feature vector extension
#[test]
fn test_mplora_feature_vector_extension() -> Result<()> {
    use adapteros_lora_router::CodeFeatures;

    let features = CodeFeatures::new();

    // Test original feature vector (22 dimensions)
    let original_vector = features.to_vector();
    assert_eq!(original_vector.len(), 22);

    // Test extended feature vector (25 dimensions)
    let extended_vector = features.to_vector_extended();
    assert_eq!(extended_vector.len(), 25);

    // Extended vector should contain original vector
    assert_eq!(original_vector, extended_vector[..22]);

    // Test MPLoRA-specific dimensions
    assert_eq!(extended_vector[22], 0.0); // orthogonal_penalty
    assert_eq!(extended_vector[23], 0.0); // adapter_diversity
    assert_eq!(extended_vector[24], 0.0); // path_similarity

    Ok(())
}

/// Test MPLoRA policy pack integration
#[test]
fn test_adapteros_policy_pack_integration() -> Result<()> {
    use adapteros_policy::PolicyId;

    // Test MPLoRA policy ID
    let adapteros_policy_id = PolicyId::Mplora;
    assert_eq!(adapteros_policy_id.name(), "MPLoRA");
    assert_eq!(
        adapteros_policy_id.description(),
        "Orthogonal multi-path LoRA constraints enforcement with shared downsample validation"
    );
    assert_eq!(
        adapteros_policy_id.enforcement_point(),
        "adapteros-router, adapteros-kernel-mtl"
    );
    assert!(adapteros_policy_id.is_implemented());

    // Test policy pack count
    let all_policies = PolicyId::all();
    assert_eq!(all_policies.len(), 22); // Should include MPLoRA

    // Test MPLoRA is in the list
    assert!(all_policies.contains(&PolicyId::Mplora));

    Ok(())
}

/// Test deterministic MPLoRA execution across multiple runs
#[test]
fn test_mplora_deterministic_execution() -> Result<()> {
    let weights = RouterWeights::new_with_mplora(
        0.3, 0.25, 0.2, 0.15, 0.1, // Original weights
        0.05, 0.03, 0.02, // MPLoRA weights
    );

    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);
    router.set_orthogonal_constraints(true, 0.7, 0.1, 10);
    router.set_compression_ratio(0.8);
    router.set_shared_downsample(true);

    let features = vec![0.0; 22]; // 22-dimensional feature vector
    let priors = vec![0.1, 0.2, 0.3, 0.4, 0.5]; // 5 adapters

    // Run multiple times
    let mut results = Vec::new();
    for _ in 0..10 {
        let decision = router.route(&features, &priors);
        results.push((decision.indices.clone(), decision.gates_q15.clone()));
    }

    // All results should be identical
    for i in 1..results.len() {
        assert_eq!(results[0], results[i]);
    }

    Ok(())
}
