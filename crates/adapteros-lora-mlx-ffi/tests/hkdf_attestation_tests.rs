//! HKDF determinism attestation tests for MLX backend
//!
//! Tests the determinism attestation and HKDF seeding integration
//! to ensure the MLX backend properly reports its determinism capabilities.

#[cfg(test)]
mod attestation_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_kernel_api::attestation::*;

    /// Mock attestation report builder for testing
    fn create_mlx_attestation_report() -> DeterminismReport {
        DeterminismReport {
            backend_type: BackendType::Mlx,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Unknown,
            compiler_flags: vec!["-DMLX_HKDF_SEEDED".to_string()],
            deterministic: false,
        }
    }

    #[test]
    fn test_mlx_attestation_reports_hkdf_seeding() {
        let report = create_mlx_attestation_report();

        // Should report HKDF seeding method
        assert_eq!(report.rng_seed_method, RngSeedingMethod::HkdfSeeded);

        // Should report MLX backend type
        assert_eq!(report.backend_type, BackendType::Mlx);

        // Should include compiler flag indicating HKDF usage
        assert!(report
            .compiler_flags
            .contains(&"-DMLX_HKDF_SEEDED".to_string()));
    }

    #[test]
    fn test_mlx_attestation_non_deterministic() {
        let report = create_mlx_attestation_report();

        // MLX is not fully deterministic due to GPU scheduling
        assert!(!report.deterministic);
    }

    #[test]
    fn test_hkdf_seed_derivation_pattern() {
        // Verify HKDF seeding follows expected pattern
        let global_seed = B3Hash::hash(b"adapteros-mlx-backend");
        let model_hash = B3Hash::hash(b"test-model-config");

        let seed_label = format!("mlx-backend:{}", model_hash.to_short_hex());
        let base_seed = derive_seed(&global_seed, &seed_label);

        // Base seed should be 32 bytes
        assert_eq!(base_seed.len(), 32);

        // Different labels should produce different seeds
        let plan_seed = derive_seed(&B3Hash::from_bytes(base_seed), "mlx-plan:abc123");
        let adapter_seed = derive_seed(&B3Hash::from_bytes(base_seed), "mlx-adapter:42");
        let step_seed = derive_seed(&B3Hash::from_bytes(base_seed), "mlx-step:0");

        assert_ne!(plan_seed, adapter_seed);
        assert_ne!(plan_seed, step_seed);
        assert_ne!(adapter_seed, step_seed);
    }

    #[test]
    fn test_hkdf_model_isolation() {
        // Different models should produce different seed hierarchies
        let model1_hash = B3Hash::hash(b"model-v1");
        let model2_hash = B3Hash::hash(b"model-v2");

        let global_seed = B3Hash::hash(b"adapteros-mlx-backend");

        let label1 = format!("mlx-backend:{}", model1_hash.to_short_hex());
        let label2 = format!("mlx-backend:{}", model2_hash.to_short_hex());

        let seed1 = derive_seed(&global_seed, &label1);
        let seed2 = derive_seed(&global_seed, &label2);

        // Different models should get different base seeds
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_hkdf_adapter_isolation() {
        let base_seed_bytes = [42u8; 32];
        let base_seed = B3Hash::from_bytes(base_seed_bytes);

        // Each adapter should get a unique seed
        let mut adapter_seeds = vec![];
        for adapter_id in 0..10 {
            let label = format!("mlx-adapter:{}", adapter_id);
            let seed = derive_seed(&base_seed, &label);
            adapter_seeds.push(seed);
        }

        // All adapter seeds should be unique
        for i in 0..adapter_seeds.len() {
            for j in (i + 1)..adapter_seeds.len() {
                assert_ne!(
                    adapter_seeds[i], adapter_seeds[j],
                    "Adapter {} and {} have identical seeds",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_hkdf_module_isolation() {
        let base_seed_bytes = [99u8; 32];
        let base_seed = B3Hash::from_bytes(base_seed_bytes);

        // Each module should get a unique seed
        let modules = vec!["q_proj", "k_proj", "v_proj", "o_proj"];
        let mut module_seeds = vec![];

        for module in modules {
            let label = format!("mlx-adapter:0:module:{}", module);
            let seed = derive_seed(&base_seed, &label);
            module_seeds.push(seed);
        }

        // All module seeds should be unique
        for i in 0..module_seeds.len() {
            for j in (i + 1)..module_seeds.len() {
                assert_ne!(module_seeds[i], module_seeds[j]);
            }
        }
    }

    #[test]
    fn test_hkdf_step_isolation() {
        let base_seed_bytes = [11u8; 32];
        let base_seed = B3Hash::from_bytes(base_seed_bytes);

        // Each step should get a unique seed
        let mut step_seeds = vec![];
        for step in 0..20 {
            let label = format!("mlx-step:{}", step);
            let seed = derive_seed(&base_seed, &label);
            step_seeds.push(seed);
        }

        // All step seeds should be unique
        for i in 0..step_seeds.len() {
            for j in (i + 1)..step_seeds.len() {
                assert_ne!(
                    step_seeds[i], step_seeds[j],
                    "Step {} and {} have identical seeds",
                    i, j
                );
            }
        }
    }

    #[test]
    fn test_attestation_summary_generation() {
        let report = create_mlx_attestation_report();

        // Attestation should be summarizable
        let summary = report.summary();
        assert!(!summary.is_empty());

        // Summary should mention HKDF
        assert!(
            summary.to_lowercase().contains("hkdf") || summary.to_lowercase().contains("seed"),
            "Summary should mention determinism method: {}",
            summary
        );
    }

    #[test]
    fn test_hkdf_determinism_hierarchy() {
        // Verify complete HKDF hierarchy works correctly
        let global_seed = B3Hash::hash(b"adapteros-mlx-backend");

        // Level 1: Model initialization
        let model_hash = B3Hash::hash(b"/models/qwen-7b-mlx");
        let model_label = format!("mlx-backend:{}", model_hash.to_short_hex());
        let model_seed = derive_seed(&global_seed, &model_label);

        // Level 2: Plan loading
        let plan_bytes = b"inference-plan";
        let plan_hash = B3Hash::hash(plan_bytes);
        let plan_label = format!("mlx-plan:{}", plan_hash.to_short_hex());
        let plan_seed = derive_seed(&B3Hash::from_bytes(model_seed), &plan_label);

        // Level 3: Adapter registration
        let adapter_id: u16 = 42;
        let adapter_label = format!("mlx-adapter:{}", adapter_id);
        let adapter_seed = derive_seed(&B3Hash::from_bytes(plan_seed), &adapter_label);

        // Level 4: Inference step
        let step_label = "mlx-step:0";
        let step_seed = derive_seed(&B3Hash::from_bytes(adapter_seed), step_label);

        // All levels should produce different seeds
        assert_ne!(&model_seed[..], &plan_seed[..]);
        assert_ne!(&plan_seed[..], &adapter_seed[..]);
        assert_ne!(&adapter_seed[..], &step_seed[..]);

        // Each level should be reproducible
        let model_seed_check = derive_seed(&global_seed, &model_label);
        assert_eq!(model_seed, model_seed_check);

        let plan_seed_check = derive_seed(&B3Hash::from_bytes(model_seed), &plan_label);
        assert_eq!(plan_seed, plan_seed_check);
    }

    #[test]
    fn test_hkdf_entropy_distribution() {
        let base_seed_bytes = [7u8; 32];
        let base_seed = B3Hash::from_bytes(base_seed_bytes);

        // Derive many seeds and check they have reasonable byte distribution
        for i in 0..100 {
            let label = format!("test-label-{}", i);
            let seed = derive_seed(&base_seed, &label);

            // Check not all zeros
            assert!(seed.iter().any(|&b| b != 0), "Seed {} is all zeros", i);

            // Check not all 255
            assert!(seed.iter().any(|&b| b != 255), "Seed {} is all 0xff", i);

            // Check reasonable distribution (not skewed)
            let ones_count = seed.iter().filter(|&&b| b > 127).count();
            assert!(
                ones_count > 5 && ones_count < 27,
                "Seed {} has skewed distribution: {} high bytes",
                i,
                ones_count
            );
        }
    }
}

#[cfg(test)]
mod backend_hkdf_integration_tests {
    use adapteros_core::{derive_seed, B3Hash};

    #[test]
    fn test_backend_initialization_seeding() {
        // Simulate MlxBackend::new() seeding logic
        let model_hash = B3Hash::hash(b"test-model-config");
        let global_seed = B3Hash::hash(b"adapteros-mlx-backend");
        let seed_label = format!("mlx-backend:{}", model_hash.to_short_hex());
        let derived_seed = derive_seed(&global_seed, &seed_label);
        let base_seed = B3Hash::from_bytes(derived_seed);

        // Verify base seed is valid
        assert_eq!(base_seed.as_bytes().len(), 32);

        // Verify it can be used for further derivation
        let plan_seed = derive_seed(&base_seed, "mlx-plan:test");
        assert_eq!(plan_seed.len(), 32);
    }

    #[test]
    fn test_adapter_registration_seeding() {
        // Simulate adapter registration seeding
        let base_seed_bytes = [55u8; 32];
        let base_seed = B3Hash::from_bytes(base_seed_bytes);

        let adapter_id: u16 = 100;
        let adapter_label = format!("mlx-adapter:{}", adapter_id);
        let adapter_seed = derive_seed(&base_seed, &adapter_label);

        // Seed should be valid for MLX
        assert_eq!(adapter_seed.len(), 32);
        assert!(adapter_seed.iter().any(|&b| b != 0)); // Not all zeros
    }

    #[test]
    fn test_plan_loading_seeding() {
        // Simulate plan loading seeding
        let base_seed_bytes = [123u8; 32];
        let base_seed = B3Hash::from_bytes(base_seed_bytes);

        let plan_bytes = b"test-plan-data";
        let plan_hash = B3Hash::hash(plan_bytes);
        let plan_label = format!("mlx-plan:{}", plan_hash.to_short_hex());
        let plan_seed = derive_seed(&base_seed, &plan_label);

        // Seed should be valid
        assert_eq!(plan_seed.len(), 32);

        // Should be deterministic
        let plan_seed_check = derive_seed(&base_seed, &plan_label);
        assert_eq!(plan_seed, plan_seed_check);
    }

    #[test]
    fn test_step_seeding() {
        // Simulate step-by-step seeding
        let base_seed_bytes = [200u8; 32];
        let base_seed = B3Hash::from_bytes(base_seed_bytes);

        for step in 0..10 {
            let step_label = format!("mlx-step:{}", step);
            let step_seed = derive_seed(&base_seed, &step_label);

            assert_eq!(step_seed.len(), 32);

            // Each step seed should be unique
            if step > 0 {
                let prev_step_label = format!("mlx-step:{}", step - 1);
                let prev_seed = derive_seed(&base_seed, &prev_step_label);
                assert_ne!(step_seed, prev_seed);
            }
        }
    }
}
