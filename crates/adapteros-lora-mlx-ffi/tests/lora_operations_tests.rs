//! LoRA operations tests for MLX FFI backend
//!
//! Tests for single and multi-adapter LoRA operations,
//! routing logic, and adapter management.

#[cfg(test)]
mod lora_adapter_tests {
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};

    #[test]
    fn test_lora_adapter_creation() {
        let config = LoRAConfig {
            rank: 4,
            alpha: 16.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.1,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        let adapter = LoRAAdapter::new("test_adapter".to_string(), config);

        assert_eq!(adapter.id(), "test_adapter");
        assert_eq!(adapter.config().rank, 4);
        assert_eq!(adapter.config().alpha, 16.0);
    }

    #[test]
    fn test_lora_adapter_add_weights() {
        let config = LoRAConfig::default();
        let mut adapter = LoRAAdapter::new("test".to_string(), config);

        let lora_a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let lora_b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];

        adapter.add_module_weights("q_proj", lora_a, lora_b);

        assert!(adapter.has_module("q_proj"));
        assert!(!adapter.has_module("k_proj"));
    }

    #[test]
    fn test_lora_adapter_get_weights() {
        let config = LoRAConfig::default();
        let mut adapter = LoRAAdapter::new("test".to_string(), config);

        let lora_a = vec![vec![1.0, 2.0]];
        let lora_b = vec![vec![3.0, 4.0]];

        adapter.add_module_weights("q_proj", lora_a.clone(), lora_b.clone());

        let (retrieved_a, retrieved_b) = adapter.get_module_weights("q_proj").unwrap();

        assert_eq!(retrieved_a.len(), lora_a.len());
        assert_eq!(retrieved_b.len(), lora_b.len());
    }

    #[test]
    fn test_lora_adapter_parameter_count() {
        let config = LoRAConfig::default();
        let mut adapter = LoRAAdapter::new("test".to_string(), config);

        // Add 2x2 matrix for lora_a
        let lora_a = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let lora_b = vec![vec![5.0, 6.0], vec![7.0, 8.0]];

        adapter.add_module_weights("q_proj", lora_a, lora_b);

        // Shape is 2x2 + 2x2 = 8 parameters (both lora_a and lora_b)
        assert_eq!(adapter.parameter_count(), 8);
    }

    #[test]
    fn test_lora_adapter_memory_usage() {
        let config = LoRAConfig::default();
        let mut adapter = LoRAAdapter::new("test".to_string(), config);

        let lora_a = vec![vec![1.0; 10]; 10];
        let lora_b = vec![vec![2.0; 10]; 10];

        adapter.add_module_weights("q_proj", lora_a, lora_b);

        // 10x10 + 10x10 = 200 parameters * 4 bytes per f32 = 800 bytes
        assert_eq!(adapter.memory_usage(), 800);
    }

    #[test]
    fn test_lora_adapter_multiple_modules() {
        let config = LoRAConfig {
            rank: 4,
            alpha: 16.0,
            target_modules: vec![
                "q_proj".to_string(),
                "k_proj".to_string(),
                "v_proj".to_string(),
            ],
            dropout: 0.1,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        let mut adapter = LoRAAdapter::new("test".to_string(), config);

        for module in &["q_proj", "k_proj", "v_proj"] {
            let lora_a = vec![vec![1.0; 8]; 4];
            let lora_b = vec![vec![2.0; 4]; 8];
            adapter.add_module_weights(module, lora_a, lora_b);
        }

        assert!(adapter.has_module("q_proj"));
        assert!(adapter.has_module("k_proj"));
        assert!(adapter.has_module("v_proj"));
        assert!(!adapter.has_module("o_proj"));

        // 3 modules * (4x8 + 8x4) = 3 * 64 = 192 parameters (both lora_a and lora_b)
        assert_eq!(adapter.parameter_count(), 192);
    }
}

#[cfg(test)]
mod lora_config_tests {
    use adapteros_lora_mlx_ffi::lora::LoRAConfig;

    #[test]
    fn test_lora_config_default() {
        let config = LoRAConfig::default();

        assert_eq!(config.rank, 4);
        assert_eq!(config.alpha, 16.0);
        assert_eq!(config.dropout, 0.1);
        assert_eq!(config.target_modules.len(), 4);
    }

    #[test]
    fn test_lora_config_custom() {
        let config = LoRAConfig {
            rank: 8,
            alpha: 32.0,
            target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
            dropout: 0.05,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        assert_eq!(config.rank, 8);
        assert_eq!(config.alpha, 32.0);
        assert_eq!(config.dropout, 0.05);
        assert_eq!(config.target_modules.len(), 2);
    }

    #[test]
    fn test_lora_config_serialization() {
        let config = LoRAConfig::default();

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: LoRAConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.rank, deserialized.rank);
        assert_eq!(config.alpha, deserialized.alpha);
        assert_eq!(config.dropout, deserialized.dropout);
    }
}

#[cfg(test)]
mod lora_routing_tests {
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_lora_mlx_ffi::routing::{
        apply_multi_lora, compute_adapter_score, select_top_k_adapters,
    };

    fn create_test_adapter(id: &str, rank: usize) -> LoRAAdapter {
        let config = LoRAConfig {
            rank,
            alpha: 16.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.1,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        let mut adapter = LoRAAdapter::new(id.to_string(), config);

        let lora_a = vec![vec![1.0; 8]; rank];
        let lora_b = vec![vec![2.0; rank]; 8];

        adapter.add_module_weights("q_proj", lora_a, lora_b);
        adapter
    }

    #[test]
    fn test_apply_multi_lora_single_adapter() {
        let adapter = create_test_adapter("adapter1", 4);
        let adapters = vec![&adapter];
        let gates = vec![32767]; // Full weight (Q15 max)

        let input = vec![1.0; 8];
        let base_output = vec![0.0; 8];

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        assert_eq!(result.len(), 8);
        // Result should be non-zero due to LoRA application
        assert!(result.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_apply_multi_lora_multiple_adapters() {
        let adapter1 = create_test_adapter("adapter1", 4);
        let adapter2 = create_test_adapter("adapter2", 4);
        let adapter3 = create_test_adapter("adapter3", 4);

        let adapters = vec![&adapter1, &adapter2, &adapter3];
        let gates = vec![16384, 8192, 8191]; // Different weights

        let input = vec![1.0; 8];
        let base_output = vec![0.0; 8];

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        assert_eq!(result.len(), 8);
        assert!(result.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_apply_multi_lora_no_adapters() {
        let adapters: Vec<&LoRAAdapter> = vec![];
        let gates: Vec<u16> = vec![];

        let input = vec![1.0; 8];
        let base_output = vec![2.0; 8];

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        // Should return base output unchanged
        assert_eq!(result, base_output);
    }

    #[test]
    fn test_apply_multi_lora_zero_gates() {
        let adapter = create_test_adapter("adapter1", 4);
        let adapters = vec![&adapter];
        let gates = vec![0]; // Zero weight

        let input = vec![1.0; 8];
        let base_output = vec![2.0; 8];

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_compute_adapter_score() {
        let adapter = create_test_adapter("test", 8);
        let input_features = vec![1.0; 8];

        let score = compute_adapter_score(&adapter, &input_features, "q_proj");

        assert!(score > 0.0);
        assert!(score <= 1.0); // Should be normalized
    }

    #[test]
    fn test_compute_adapter_score_missing_module() {
        let adapter = create_test_adapter("test", 8);
        let input_features = vec![1.0; 8];

        let score = compute_adapter_score(&adapter, &input_features, "nonexistent");

        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_select_top_k_adapters() {
        let adapter1 = create_test_adapter("adapter1", 2);
        let adapter2 = create_test_adapter("adapter2", 4);
        let adapter3 = create_test_adapter("adapter3", 8);
        let adapter4 = create_test_adapter("adapter4", 6);

        let adapters = vec![&adapter1, &adapter2, &adapter3, &adapter4];
        let scores = vec![0.3, 0.7, 0.9, 0.5];

        let top_k = select_top_k_adapters(&adapters, &scores, 2);

        assert_eq!(top_k.len(), 2);
        // Should be sorted by score descending
        assert_eq!(top_k[0].0, 2); // adapter3 (0.9)
        assert_eq!(top_k[1].0, 1); // adapter2 (0.7)
    }

    #[test]
    fn test_select_top_k_adapters_k_larger_than_list() {
        let adapter1 = create_test_adapter("adapter1", 2);
        let adapter2 = create_test_adapter("adapter2", 4);

        let adapters = vec![&adapter1, &adapter2];
        let scores = vec![0.3, 0.7];

        let top_k = select_top_k_adapters(&adapters, &scores, 10);

        // Should return all adapters
        assert_eq!(top_k.len(), 2);
    }

    #[test]
    fn test_select_top_k_adapters_k_zero() {
        let adapter1 = create_test_adapter("adapter1", 2);

        let adapters = vec![&adapter1];
        let scores = vec![0.5];

        let top_k = select_top_k_adapters(&adapters, &scores, 0);

        // Should return empty
        assert_eq!(top_k.len(), 0);
    }
}

#[cfg(test)]
mod lora_transform_tests {
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};
    use adapteros_lora_mlx_ffi::routing::apply_multi_lora;

    #[test]
    fn test_lora_transform_dimensions() {
        let config = LoRAConfig {
            rank: 4,
            alpha: 16.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.0,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        let mut adapter = LoRAAdapter::new("test".to_string(), config);

        // lora_a: rank x input_dim (4 x 8)
        // lora_b: output_dim x rank (8 x 4)
        let lora_a = vec![vec![0.1; 8]; 4];
        let lora_b = vec![vec![0.2; 4]; 8];

        adapter.add_module_weights("q_proj", lora_a, lora_b);

        let adapters = vec![&adapter];
        let gates = vec![32767];

        let input = vec![1.0; 8];
        let base_output = vec![0.0; 8];

        let result = apply_multi_lora(&adapters, &gates, "q_proj", &input, &base_output).unwrap();

        // Output should have same dimension as input
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_lora_alpha_scaling() {
        let config_low_alpha = LoRAConfig {
            rank: 4,
            alpha: 1.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.0,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        let config_high_alpha = LoRAConfig {
            rank: 4,
            alpha: 32.0,
            target_modules: vec!["q_proj".to_string()],
            dropout: 0.0,
            language_affinities: Vec::new(),
            framework: None,
            tier: None,
        };

        let mut adapter_low = LoRAAdapter::new("low".to_string(), config_low_alpha);
        let mut adapter_high = LoRAAdapter::new("high".to_string(), config_high_alpha);

        let lora_a = vec![vec![1.0; 8]; 4];
        let lora_b = vec![vec![1.0; 4]; 8];

        adapter_low.add_module_weights("q_proj", lora_a.clone(), lora_b.clone());
        adapter_high.add_module_weights("q_proj", lora_a, lora_b);

        let input = vec![1.0; 8];
        let base_output = vec![0.0; 8];
        let gates = vec![32767];

        let result_low =
            apply_multi_lora(&[&adapter_low], &gates, "q_proj", &input, &base_output).unwrap();
        let result_high =
            apply_multi_lora(&[&adapter_high], &gates, "q_proj", &input, &base_output).unwrap();

        // Higher alpha should produce larger magnitude changes
        let magnitude_low: f32 = result_low.iter().map(|x| x.abs()).sum();
        let magnitude_high: f32 = result_high.iter().map(|x| x.abs()).sum();

        assert!(magnitude_high > magnitude_low);
    }
}

#[cfg(test)]
mod lora_loading_tests {
    use adapteros_lora_mlx_ffi::lora::{LoRAAdapter, LoRAConfig};
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        tempfile::Builder::new()
            .prefix("aos-test-")
            .tempdir()
            .expect("create temp dir")
    }

    #[test]
    fn test_lora_adapter_load_missing_file() {
        let temp_dir = new_test_tempdir();
        let adapter_path = temp_dir.path().join("nonexistent_adapter.safetensors");

        let config = LoRAConfig::default();

        // Load should fail when file doesn't exist
        let result = LoRAAdapter::load(&adapter_path, "test_adapter".to_string(), config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Failed to read LoRA file"),
            "Expected IO error, got: {}",
            err
        );
    }

    #[test]
    fn test_lora_adapter_hash_consistency() {
        let config = LoRAConfig::default();

        let adapter1 = LoRAAdapter::new("test".to_string(), config.clone());
        let adapter2 = LoRAAdapter::new("test".to_string(), config);

        // Same ID should produce same hash
        assert_eq!(adapter1.hash(), adapter2.hash());
    }

    #[test]
    fn test_lora_adapter_hash_uniqueness() {
        let config = LoRAConfig::default();

        let adapter1 = LoRAAdapter::new("test1".to_string(), config.clone());
        let adapter2 = LoRAAdapter::new("test2".to_string(), config);

        // Different IDs should produce different hashes
        assert_ne!(adapter1.hash(), adapter2.hash());
    }
}
