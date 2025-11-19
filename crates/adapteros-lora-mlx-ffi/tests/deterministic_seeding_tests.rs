//! Deterministic seeding tests for MLX FFI backend
//!
//! Tests RNG seeding consistency and HKDF integration.
//! These tests verify the seeding interface works correctly.

#[cfg(test)]
mod seeding_basic_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_seed_with_hkdf_bytes() {
        let base_hash = B3Hash::hash(b"test-model");
        let seed = derive_seed(&base_hash, "mlx-backend:0");

        assert_eq!(seed.len(), 32); // HKDF produces 32 bytes

        let result = mlx_set_seed_from_bytes(&seed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_seed_multiple_times() {
        let base_hash = B3Hash::hash(b"test-model");

        for i in 0..10 {
            let seed = derive_seed(&base_hash, &format!("mlx-step:{}", i));
            let result = mlx_set_seed_from_bytes(&seed);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_seed_consistency() {
        let base_hash = B3Hash::hash(b"consistent-model");

        // Same label should produce same seed
        let seed1 = derive_seed(&base_hash, "mlx-backend:0");
        let seed2 = derive_seed(&base_hash, "mlx-backend:0");

        assert_eq!(seed1, seed2);

        // Both should set successfully
        assert!(mlx_set_seed_from_bytes(&seed1).is_ok());
        assert!(mlx_set_seed_from_bytes(&seed2).is_ok());
    }

    #[test]
    fn test_seed_uniqueness() {
        let base_hash = B3Hash::hash(b"unique-model");

        let seed1 = derive_seed(&base_hash, "mlx-backend:0");
        let seed2 = derive_seed(&base_hash, "mlx-backend:1");
        let seed3 = derive_seed(&base_hash, "mlx-adapter:0");

        // All seeds should be different
        assert_ne!(seed1, seed2);
        assert_ne!(seed1, seed3);
        assert_ne!(seed2, seed3);
    }
}

#[cfg(test)]
mod seeding_domain_separation_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_seed_backend_domain() {
        let model_hash = B3Hash::hash(b"model-path");
        let seed = derive_seed(&model_hash, "mlx-backend:init");

        assert!(mlx_set_seed_from_bytes(&seed).is_ok());
    }

    #[test]
    fn test_seed_adapter_domain() {
        let base_hash = B3Hash::hash(b"model");

        for adapter_id in 0..5 {
            let seed = derive_seed(&base_hash, &format!("mlx-adapter:{}", adapter_id));
            assert!(mlx_set_seed_from_bytes(&seed).is_ok());
        }
    }

    #[test]
    fn test_seed_step_domain() {
        let base_hash = B3Hash::hash(b"model");

        for step in 0..10 {
            let seed = derive_seed(&base_hash, &format!("mlx-step:{}", step));
            assert!(mlx_set_seed_from_bytes(&seed).is_ok());
        }
    }

    #[test]
    fn test_seed_plan_domain() {
        let base_hash = B3Hash::hash(b"model");
        let plan_data = b"inference-plan";
        let plan_hash = B3Hash::hash(plan_data);

        let seed = derive_seed(&base_hash, &format!("mlx-plan:{}", plan_hash.to_short_hex()));

        assert!(mlx_set_seed_from_bytes(&seed).is_ok());
    }
}

#[cfg(test)]
mod seeding_workflow_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_full_initialization_workflow() {
        // Step 1: Model initialization
        let model_path = "/models/test-model-mlx";
        let model_hash = B3Hash::hash(model_path.as_bytes());

        // Step 2: Backend initialization seed
        let global_seed = B3Hash::hash(b"adapteros-mlx-backend");
        let backend_seed = derive_seed(&global_seed, &format!("mlx-backend:{}", model_hash.to_short_hex()));

        assert!(mlx_set_seed_from_bytes(&backend_seed).is_ok());

        // Step 3: Plan loading seed
        let plan_bytes = b"test-inference-plan";
        let plan_hash = B3Hash::hash(plan_bytes);
        let plan_seed = derive_seed(&backend_seed, &format!("mlx-plan:{}", plan_hash.to_short_hex()));

        assert!(mlx_set_seed_from_bytes(&plan_seed).is_ok());

        // Step 4: Adapter-specific seed
        let adapter_id: u16 = 42;
        let adapter_seed = derive_seed(&backend_seed, &format!("mlx-adapter:{}", adapter_id));

        assert!(mlx_set_seed_from_bytes(&adapter_seed).is_ok());
    }

    #[test]
    fn test_inference_step_seeding() {
        let base_hash = B3Hash::hash(b"model");

        // Simulate multiple inference steps
        for position in 0..20 {
            let step_seed = derive_seed(&base_hash, &format!("mlx-step:{}", position));
            assert!(mlx_set_seed_from_bytes(&step_seed).is_ok());
        }
    }

    #[test]
    fn test_multi_adapter_seeding() {
        let base_hash = B3Hash::hash(b"model");

        // Simulate loading multiple adapters with unique seeds
        let adapter_ids = vec![1, 5, 10, 42, 100];

        for adapter_id in adapter_ids {
            let seed = derive_seed(&base_hash, &format!("mlx-adapter:{}", adapter_id));
            assert!(mlx_set_seed_from_bytes(&seed).is_ok());
        }
    }
}

#[cfg(test)]
mod seeding_edge_cases_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_seed_with_empty_label() {
        let base_hash = B3Hash::hash(b"model");
        let seed = derive_seed(&base_hash, "");

        assert!(mlx_set_seed_from_bytes(&seed).is_ok());
    }

    #[test]
    fn test_seed_with_long_label() {
        let base_hash = B3Hash::hash(b"model");
        let long_label = "a".repeat(1000);
        let seed = derive_seed(&base_hash, &long_label);

        assert!(mlx_set_seed_from_bytes(&seed).is_ok());
    }

    #[test]
    fn test_seed_with_special_characters() {
        let base_hash = B3Hash::hash(b"model");
        let labels = vec![
            "mlx-backend:特殊文字",
            "mlx-adapter:🦀",
            "mlx-step:with spaces",
            "mlx-plan:with/slashes",
        ];

        for label in labels {
            let seed = derive_seed(&base_hash, label);
            assert!(mlx_set_seed_from_bytes(&seed).is_ok());
        }
    }

    #[test]
    fn test_seed_with_numeric_labels() {
        let base_hash = B3Hash::hash(b"model");

        for i in 0..100 {
            let seed = derive_seed(&base_hash, &format!("mlx-numeric:{}", i));
            assert!(mlx_set_seed_from_bytes(&seed).is_ok());
        }
    }
}

#[cfg(test)]
mod seeding_reproducibility_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_same_seed_reproducible() {
        let base_hash = B3Hash::hash(b"reproducible-model");
        let label = "mlx-test:0";

        let seed1 = derive_seed(&base_hash, label);
        let seed2 = derive_seed(&base_hash, label);

        assert_eq!(seed1, seed2);

        // Setting same seed multiple times should work
        for _ in 0..5 {
            assert!(mlx_set_seed_from_bytes(&seed1).is_ok());
        }
    }

    #[test]
    fn test_different_base_different_seed() {
        let base1 = B3Hash::hash(b"model1");
        let base2 = B3Hash::hash(b"model2");

        let label = "mlx-backend:0";

        let seed1 = derive_seed(&base1, label);
        let seed2 = derive_seed(&base2, label);

        // Different base hashes produce different seeds
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_seed_byte_distribution() {
        let base_hash = B3Hash::hash(b"distribution-test");
        let seed = derive_seed(&base_hash, "mlx-test");

        // Seed should be 32 bytes
        assert_eq!(seed.len(), 32);

        // Check that seed has reasonable byte distribution (not all zeros/ones)
        let all_zeros = seed.iter().all(|&b| b == 0);
        let all_ones = seed.iter().all(|&b| b == 255);

        assert!(!all_zeros);
        assert!(!all_ones);
    }
}

#[cfg(test)]
mod seeding_error_handling_tests {
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_empty_seed_error() {
        let empty_seed: Vec<u8> = vec![];
        let result = mlx_set_seed_from_bytes(&empty_seed);

        assert!(result.is_err());
    }

    #[test]
    fn test_valid_short_seed() {
        let short_seed = vec![1, 2, 3, 4];
        let result = mlx_set_seed_from_bytes(&short_seed);

        // MLX accepts variable-length seeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_long_seed() {
        let long_seed = vec![42u8; 64]; // 64 bytes
        let result = mlx_set_seed_from_bytes(&long_seed);

        // Should handle longer seeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_seed_all_zeros() {
        let zero_seed = vec![0u8; 32];
        let result = mlx_set_seed_from_bytes(&zero_seed);

        // All-zero seed should still be valid
        assert!(result.is_ok());
    }

    #[test]
    fn test_seed_all_ones() {
        let ones_seed = vec![255u8; 32];
        let result = mlx_set_seed_from_bytes(&ones_seed);

        // All-ones seed should still be valid
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod seeding_integration_tests {
    use adapteros_core::{derive_seed, B3Hash};
    use adapteros_lora_mlx_ffi::mlx_set_seed_from_bytes;

    #[test]
    fn test_hierarchical_seeding() {
        // Level 1: Global seed
        let global_seed = B3Hash::hash(b"adapteros-global");

        // Level 2: Backend-specific seed
        let backend_seed = derive_seed(&global_seed, "mlx-backend:main");

        // Level 3: Model-specific seed
        let model_hash = B3Hash::hash(b"/models/qwen-7b");
        let model_seed = derive_seed(&backend_seed, &format!("model:{}", model_hash.to_short_hex()));

        // Level 4: Inference-specific seed
        let inference_seed = derive_seed(&model_seed, "inference:batch-0");

        // All levels should set successfully
        assert!(mlx_set_seed_from_bytes(&global_seed.as_bytes()).is_ok());
        assert!(mlx_set_seed_from_bytes(&backend_seed).is_ok());
        assert!(mlx_set_seed_from_bytes(&model_seed).is_ok());
        assert!(mlx_set_seed_from_bytes(&inference_seed).is_ok());
    }

    #[test]
    fn test_seeding_with_context() {
        let base_hash = B3Hash::hash(b"context-model");

        // Different contexts produce different seeds
        let contexts = vec![
            ("training", "epoch-0"),
            ("inference", "batch-0"),
            ("validation", "step-0"),
            ("testing", "sample-0"),
        ];

        let mut seeds = Vec::new();
        for (phase, label) in &contexts {
            let context_label = format!("mlx-{}:{}", phase, label);
            let seed = derive_seed(&base_hash, &context_label);
            seeds.push(seed.clone());

            assert!(mlx_set_seed_from_bytes(&seed).is_ok());
        }

        // All seeds should be unique
        for i in 0..seeds.len() {
            for j in (i + 1)..seeds.len() {
                assert_ne!(seeds[i], seeds[j]);
            }
        }
    }
}
