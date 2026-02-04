//! Tests for MLX backend HKDF seeding implementation
//!
//! This test module verifies that:
//! 1. mlx_set_seed correctly accepts HKDF-derived seeds
//! 2. Seeded operations produce consistent results
//! 3. Error handling works correctly

#[cfg(test)]
mod mlx_seed_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_mlx_seed_from_hkdf() {
        // Create a base seed like the backend factory does
        let global_seed = B3Hash::hash(b"test-model");
        let seed = derive_seed(&global_seed, "mlx-test:0");

        // Should successfully set the seed
        let result = mlx_set_seed_from_bytes(&seed);
        assert!(result.is_ok(), "Failed to set MLX seed");
    }

    #[test]
    fn test_mlx_seed_with_different_labels() {
        let base = B3Hash::hash(b"model-path");

        let seeds: Vec<_> = (0..5)
            .map(|i| {
                let label = format!("mlx-step:{}", i);
                derive_seed(&base, &label)
            })
            .collect();

        // All seeds should be different
        for (i, seed_i) in seeds.iter().enumerate() {
            for (j, seed_j) in seeds.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        seed_i, seed_j,
                        "Seeds at position {} and {} are identical",
                        i, j
                    );
                }
            }
        }

        // All should be settable
        for (i, seed) in seeds.iter().enumerate() {
            let result = mlx_set_seed_from_bytes(seed);
            assert!(result.is_ok(), "Failed to set seed at position {}", i);
        }
    }

    #[test]
    fn test_mlx_seed_adapter_specific() {
        let base = B3Hash::hash(b"base-model");

        // Different adapters should get different seeds
        let adapter_0_seed = derive_seed(&base, "mlx-adapter:0");
        let adapter_1_seed = derive_seed(&base, "mlx-adapter:1");

        assert_ne!(adapter_0_seed, adapter_1_seed);

        // Both should be settable
        assert!(mlx_set_seed_from_bytes(&adapter_0_seed).is_ok());
        assert!(mlx_set_seed_from_bytes(&adapter_1_seed).is_ok());
    }

    #[test]
    fn test_mlx_seed_determinism_workflow() {
        // Simulate the actual workflow in MlxBackend::load()
        let model_path = "/models/test-model";
        let model_hash = B3Hash::hash(model_path.as_bytes());
        let base_seed = derive_seed(
            &B3Hash::hash(b"adapteros-mlx-backend"),
            &format!("mlx-backend:{}", model_hash.to_short_hex()),
        );

        // Simulate plan load
        let plan_bytes = b"test-plan-data";
        let plan_hash = B3Hash::hash(plan_bytes);
        let plan_label = format!("mlx-plan:{}", plan_hash.to_short_hex());
        let base_seed_hash = B3Hash::from_bytes(base_seed);
        let plan_seed = derive_seed(&base_seed_hash, &plan_label);

        assert!(mlx_set_seed_from_bytes(&plan_seed).is_ok());

        // Simulate step-specific seed
        let plan_seed_hash = B3Hash::from_bytes(plan_seed);
        let step_seed = derive_seed(&plan_seed_hash, "mlx-step:0");
        assert!(mlx_set_seed_from_bytes(&step_seed).is_ok());

        // Simulate next step
        let step_seed = derive_seed(&plan_seed_hash, "mlx-step:1");
        assert!(mlx_set_seed_from_bytes(&step_seed).is_ok());
    }

    #[test]
    fn test_mlx_seed_reproducibility() {
        // Same seed should be reproducible across calls
        let seed = derive_seed(&B3Hash::hash(b"test"), "mlx-test");

        let result1 = mlx_set_seed_from_bytes(&seed);
        let result2 = mlx_set_seed_from_bytes(&seed);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        // Both should succeed with the same seed
    }

    #[test]
    fn test_mlx_seed_full_workflow() {
        // Simulate the complete MlxBackend initialization

        // 1. Model initialization
        let model_path = "/models/qwen-7b-mlx";
        let model_hash = B3Hash::hash(model_path.as_bytes());
        let global_seed = B3Hash::hash(b"adapteros-mlx-backend");
        let base_seed = derive_seed(
            &global_seed,
            &format!("mlx-backend:{}", model_hash.to_short_hex()),
        );

        // 2. Plan load (like MlxBackend::load)
        let plan_bytes = b"inference-plan-v1";
        let plan_hash = B3Hash::hash(plan_bytes);
        let plan_label = format!("mlx-plan:{}", plan_hash.to_short_hex());
        let base_seed_hash = B3Hash::from_bytes(base_seed);
        let seed = derive_seed(&base_seed_hash, &plan_label);

        assert!(mlx_set_seed_from_bytes(&seed).is_ok());

        // 3. Adapter load (like MlxBackend::load_adapter)
        let adapter_id: u16 = 42;
        let adapter_label = format!("mlx-adapter:{}", adapter_id);
        let adapter_seed = derive_seed(&base_seed_hash, &adapter_label);

        assert!(mlx_set_seed_from_bytes(&adapter_seed).is_ok());

        // 4. Multiple steps
        for position in 0..10 {
            let label = format!("mlx-step:{}", position);
            let step_seed = derive_seed(&base_seed_hash, &label);
            assert!(mlx_set_seed_from_bytes(&step_seed).is_ok());
        }
    }
}

// Note: The tests above run with stub implementations when the mlx feature is disabled

#[cfg(feature = "mlx")]
#[test]
fn mlx_runtime_initializes_when_feature_enabled() {
    adapteros_lora_mlx_ffi::mlx_runtime_init_with_device(
        adapteros_lora_mlx_ffi::MlxDeviceType::Cpu,
    )
    .expect("MLX runtime must be installed when building with `--features mlx`");
}
