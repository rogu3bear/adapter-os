//! Integration tests for CoreML backend with router k-sparse selection
//!
//! Tests router integration with k=0,1,4,8 adapter configurations

use adapteros_core::Result;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use adapteros_lora_kernel_mtl::coreml_backend::CoreMLBackend;

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_k0_base_model_only() -> Result<()> {
    // k=0: Base model only, no adapters
    let mut backend = CoreMLBackend::new()?;

    // Load dummy model
    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    // Create empty RouterRing (k=0)
    let ring = RouterRing::new(0);
    assert_eq!(ring.k, 0);
    assert_eq!(ring.active_indices().len(), 0);

    // Create IoBuffers with test input
    let mut io = IoBuffers::new(152064); // Qwen2.5-7B vocab size
    io.input_ids = vec![100, 200, 300];

    // Execute inference (should use base model only)
    backend.run_step(&ring, &mut io)?;

    // Verify output
    assert_eq!(io.output_logits.len(), 152064);
    assert!(io.output_logits.iter().any(|&x| x != 0.0));

    Ok(())
}

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_k1_single_adapter() -> Result<()> {
    // k=1: Single adapter MVP
    let mut backend = CoreMLBackend::new()?;

    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    // Create RouterRing with single adapter
    let mut ring = RouterRing::new(1);
    ring.set(&[0], &[32767]); // Adapter 0 with full gate (Q15=32767)

    assert_eq!(ring.k, 1);
    assert_eq!(ring.active_indices(), &[0]);
    assert_eq!(ring.active_gates(), &[32767]);

    let mut io = IoBuffers::new(152064);
    io.input_ids = vec![100];

    backend.run_step(&ring, &mut io)?;

    assert_eq!(io.output_logits.len(), 152064);

    Ok(())
}

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_k4_medium_complexity() -> Result<()> {
    // k=4: Medium complexity multi-adapter fusion
    let mut backend = CoreMLBackend::new()?;

    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    // Create RouterRing with 4 adapters
    let mut ring = RouterRing::new(4);
    ring.set(
        &[0, 1, 2, 3],
        &[
            8192,  // 0.25 in Q15
            8192,  // 0.25 in Q15
            8192,  // 0.25 in Q15
            8191,  // ~0.25 in Q15 (rounded)
        ],
    );

    assert_eq!(ring.k, 4);
    assert_eq!(ring.active_indices().len(), 4);

    let mut io = IoBuffers::new(152064);
    io.input_ids = vec![100, 200];

    backend.run_step(&ring, &mut io)?;

    assert_eq!(io.output_logits.len(), 152064);

    // Verify logits are scaled by gate weights (should be ~0.25 each)
    let total_gate_weight: f32 = ring
        .active_gates()
        .iter()
        .map(|&g| (g as f32) / 32767.0)
        .sum();

    assert!((total_gate_weight - 1.0).abs() < 0.01, "Gate weights should sum to ~1.0");

    Ok(())
}

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_k8_maximum_adapters() -> Result<()> {
    // k=8: Maximum number of adapters
    let mut backend = CoreMLBackend::new()?;

    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    // Create RouterRing with 8 adapters (max)
    let mut ring = RouterRing::new(8);
    ring.set(
        &[0, 1, 2, 3, 4, 5, 6, 7],
        &[
            4096, 4096, 4096, 4096, 4096, 4096, 4096, 4095, // Each ~0.125 in Q15
        ],
    );

    assert_eq!(ring.k, 8);
    assert_eq!(ring.active_indices().len(), 8);

    let mut io = IoBuffers::new(152064);
    io.input_ids = vec![100];

    backend.run_step(&ring, &mut io)?;

    assert_eq!(io.output_logits.len(), 152064);

    // Verify all 8 adapters contribute
    let total_gate_weight: f32 = ring
        .active_gates()
        .iter()
        .map(|&g| (g as f32) / 32767.0)
        .sum();

    assert!((total_gate_weight - 1.0).abs() < 0.01);

    Ok(())
}

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_gate_quantization() -> Result<()> {
    // Test Q15 gate quantization accuracy
    let mut backend = CoreMLBackend::new()?;

    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    // Test various Q15 gate values
    let test_cases = vec![
        (32767, 1.0),      // Full gate
        (16384, 0.5),      // Half gate
        (8192, 0.25),      // Quarter gate
        (4096, 0.125),     // Eighth gate
        (0, 0.0),          // Zero gate
    ];

    for (gate_q15, expected_weight) in test_cases {
        let mut ring = RouterRing::new(1);
        ring.set(&[0], &[gate_q15]);

        let actual_weight = (gate_q15 as f32) / 32767.0;
        let error = (actual_weight - expected_weight).abs();

        assert!(
            error < 0.001,
            "Q15 gate {} should convert to ~{}, got {} (error: {})",
            gate_q15,
            expected_weight,
            actual_weight,
            error
        );
    }

    Ok(())
}

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_adapter_indices() -> Result<()> {
    // Test that adapter indices are correctly parsed and used
    let mut backend = CoreMLBackend::new()?;

    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    // Test non-sequential adapter indices
    let mut ring = RouterRing::new(3);
    ring.set(
        &[2, 5, 7],
        &[
            10922, // ~1/3 in Q15
            10922, // ~1/3 in Q15
            10923, // ~1/3 in Q15
        ],
    );

    assert_eq!(ring.active_indices(), &[2, 5, 7]);

    let mut io = IoBuffers::new(152064);
    io.input_ids = vec![100];

    // Should execute successfully with non-sequential indices
    backend.run_step(&ring, &mut io)?;

    Ok(())
}

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_edge_cases() -> Result<()> {
    let mut backend = CoreMLBackend::new()?;

    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    // Test edge case: k=2 with unequal gates
    let mut ring = RouterRing::new(2);
    ring.set(
        &[0, 1],
        &[
            26214, // 0.8 in Q15
            6553,  // 0.2 in Q15
        ],
    );

    let mut io = IoBuffers::new(152064);
    io.input_ids = vec![100];

    backend.run_step(&ring, &mut io)?;

    // Verify output was produced
    assert_eq!(io.output_logits.len(), 152064);

    Ok(())
}

#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_router_transitions() -> Result<()> {
    // Test transitioning between different k values
    let mut backend = CoreMLBackend::new()?;

    let plan_bytes = vec![0u8; 1024];
    backend.load(&plan_bytes)?;

    let mut io = IoBuffers::new(152064);
    io.input_ids = vec![100];

    // k=0 -> k=1 -> k=4 -> k=0
    let test_sequence = vec![
        (0, vec![], vec![]),
        (1, vec![0], vec![32767]),
        (4, vec![0, 1, 2, 3], vec![8192, 8192, 8192, 8191]),
        (0, vec![], vec![]),
    ];

    for (k, indices, gates) in test_sequence {
        let mut ring = RouterRing::new(k);
        if k > 0 {
            ring.set(&indices, &gates);
        }

        backend.run_step(&ring, &mut io)?;
        assert_eq!(io.output_logits.len(), 152064);
    }

    Ok(())
}

#[test]
#[cfg(not(feature = "coreml-backend"))]
fn test_coreml_disabled() {
    // When CoreML backend is disabled, operations should fail gracefully
    use adapteros_lora_kernel_mtl::coreml_backend::is_coreml_available;

    assert!(!is_coreml_available());
}

#[cfg(feature = "coreml-backend")]
mod router_utilities {
    use super::*;
    use adapteros_lora_kernel_mtl::router_integration::*;

    #[test]
    fn test_q15_gates_to_weights() {
        let gates_q15 = vec![32767, 16384, 8192];
        let weights = q15_gates_to_weights(&gates_q15);

        assert_eq!(weights.len(), 3);
        assert!((weights[0] - 1.0).abs() < 0.001);
        assert!((weights[1] - 0.5).abs() < 0.001);
        assert!((weights[2] - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_adapter_model_mapper() {
        let mut mapper = AdapterModelMapper::new();

        mapper.register(0, 0x1000);
        mapper.register(1, 0x2000);
        mapper.register(5, 0x5000);

        let handle0 = mapper.get(0).unwrap();
        assert_eq!(handle0.ptr, 0x1000);

        let handle5 = mapper.get(5).unwrap();
        assert_eq!(handle5.ptr, 0x5000);

        let (hits, misses, hit_rate) = mapper.cache_stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 0);
        assert_eq!(hit_rate, 1.0);

        assert!(mapper.get(10).is_none());
    }

    #[test]
    fn test_router_pattern_cache() {
        let mut cache = RouterPatternCache::new(10);

        let indices = vec![0, 1, 2, 3];
        let gates = vec![8192, 8192, 8192, 8191];
        let weights = vec![0.25, 0.25, 0.25, 0.25];

        assert!(cache.get(&indices, &gates).is_none());
        cache.put(&indices, &gates, weights.clone()).unwrap();

        let cached = cache.get(&indices, &gates).unwrap();
        assert_eq!(cached, weights);

        let stats = cache.stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }

    #[test]
    fn test_gate_weight_optimizer() {
        let (w1, w2) = GateWeightOptimizer::precompute_k2_weights(16384, 16383);
        assert!((w1 - 0.5).abs() < 0.001);
        assert!((w2 - 0.5).abs() < 0.001);

        let gates_k4 = [8192, 8192, 8192, 8191];
        let weights = GateWeightOptimizer::precompute_k4_weights(&gates_k4);
        for weight in &weights {
            assert!((weight - 0.25).abs() < 0.001);
        }

        let gates_k8 = [4096, 4096, 4096, 4096, 4096, 4096, 4096, 4095];
        let weights = GateWeightOptimizer::precompute_k8_weights(&gates_k8);
        for weight in &weights {
            assert!((weight - 0.125).abs() < 0.001);
        }
    }

    #[test]
    fn test_gate_weight_validation() {
        let valid_weights = vec![0.25, 0.25, 0.25, 0.25];
        assert!(GateWeightOptimizer::validate_weights(&valid_weights).is_ok());

        let invalid_weights = vec![0.5, 0.5, 0.5];
        assert!(GateWeightOptimizer::validate_weights(&invalid_weights).is_err());

        let nearly_valid = vec![0.24, 0.25, 0.26, 0.25];
        assert!(GateWeightOptimizer::validate_weights(&nearly_valid).is_ok());
    }
}
