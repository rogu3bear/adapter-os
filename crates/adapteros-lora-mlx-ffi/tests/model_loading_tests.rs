//! Model loading and inference tests for MLX FFI backend
//!
//! Tests for model loading, configuration parsing, and forward passes.
//! Most tests require actual MLX models and are marked as ignored.

#[cfg(test)]
mod model_config_tests {
    use adapteros_lora_mlx_ffi::ModelConfig;

    #[test]
    fn test_lora_loading_without_file() {
        // Test that LoRA loading fails gracefully without a file
        let result = adapteros_lora_mlx_ffi::lora::LoRAAdapter::load(
            "nonexistent.safetensors",
            "test_adapter".to_string(),
            Default::default(),
        );
        assert!(result.is_err(), "Should fail when file doesn't exist");
    }

    #[test]
    fn test_model_config_parsing() {
        let config_json = r#"
        {
            "hidden_size": 4096,
            "num_hidden_layers": 32,
            "num_attention_heads": 32,
            "num_key_value_heads": 8,
            "intermediate_size": 11008,
            "vocab_size": 32000,
            "max_position_embeddings": 32768,
            "rope_theta": 10000.0
        }
        "#;

        let config: ModelConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.hidden_size, 4096);
        assert_eq!(config.num_hidden_layers, 32);
        assert_eq!(config.num_attention_heads, 32);
        assert_eq!(config.num_key_value_heads, 8);
        assert_eq!(config.intermediate_size, 11008);
        assert_eq!(config.vocab_size, 32000);
        assert_eq!(config.max_position_embeddings, 32768);
        assert_eq!(config.rope_theta, 10000.0);
    }

    #[test]
    fn test_model_config_with_defaults() {
        let config_json = r#"
        {
            "hidden_size": 2048,
            "num_hidden_layers": 16,
            "num_attention_heads": 16,
            "num_key_value_heads": 4,
            "intermediate_size": 5504,
            "vocab_size": 50000,
            "max_position_embeddings": 8192
        }
        "#;

        let config: ModelConfig = serde_json::from_str(config_json).unwrap();

        // rope_theta should default to 10000.0
        assert_eq!(config.rope_theta, 10000.0);
    }

    #[test]
    fn test_model_config_different_sizes() {
        let configs = vec![
            (1024, 12, 12, "Small model"),
            (2048, 24, 24, "Medium model"),
            (4096, 32, 32, "Large model"),
            (8192, 64, 64, "XL model"),
        ];

        for (hidden_size, num_layers, num_heads, description) in configs {
            let config_json = format!(
                r#"{{
                "hidden_size": {},
                "num_hidden_layers": {},
                "num_attention_heads": {},
                "num_key_value_heads": {},
                "intermediate_size": {},
                "vocab_size": 32000,
                "max_position_embeddings": 2048
            }}"#,
                hidden_size,
                num_layers,
                num_heads,
                num_heads / 4,
                hidden_size * 4
            );

            let config: ModelConfig = serde_json::from_str(&config_json).unwrap();

            assert_eq!(config.hidden_size, hidden_size, "{}", description);
            assert_eq!(config.num_hidden_layers, num_layers, "{}", description);
            assert_eq!(config.num_attention_heads, num_heads, "{}", description);
        }
    }

    #[test]
    fn test_model_config_serialization_roundtrip() {
        let original = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(original.hidden_size, deserialized.hidden_size);
        assert_eq!(original.num_hidden_layers, deserialized.num_hidden_layers);
        assert_eq!(original.rope_theta, deserialized.rope_theta);
    }
}

#[cfg(test)]
mod model_loading_tests {
    use adapteros_lora_mlx_ffi::MLXFFIModel;
    use tempfile::TempDir;

    #[test]
    #[ignore] // Requires actual MLX model files (enable when MLX available)
    fn test_model_load_from_path() {
        // This test requires actual model files:
        // - config.json
        // - model weights
        // Skipped for automated testing
        let _ = TempDir::new();
    }

    #[test]
    #[ignore] // Requires actual MLX model
    fn test_model_load_invalid_path() {
        let result = MLXFFIModel::load("/nonexistent/path/to/model");
        assert!(result.is_err());
    }

    #[test]
    fn test_model_config_missing_required_fields() {
        let incomplete_json = r#"
        {
            "hidden_size": 4096,
            "num_hidden_layers": 32
        }
        "#;

        let result = serde_json::from_str::<adapteros_lora_mlx_ffi::ModelConfig>(incomplete_json);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod forward_pass_tests {
    use adapteros_lora_mlx_ffi::mock::MockMLXFFIModel;
    use adapteros_lora_mlx_ffi::ModelConfig;

    #[test]
    fn test_forward_pass_single_token() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };

        let model = MockMLXFFIModel::new(config);
        let token_ids = vec![42];

        let result = model.forward(&token_ids, 0);
        assert!(result.is_ok());

        let logits = result.unwrap();
        assert_eq!(logits.len(), 32000);
    }

    #[test]
    fn test_forward_pass_multiple_tokens() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };

        let model = MockMLXFFIModel::new(config);
        let token_ids = vec![1, 2, 3, 4, 5];

        let result = model.forward(&token_ids, 0);
        assert!(result.is_ok());

        let logits = result.unwrap();
        assert_eq!(logits.len(), 32000);
    }

    #[test]
    fn test_forward_pass_empty_tokens() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };

        let model = MockMLXFFIModel::new(config);
        let token_ids: Vec<u32> = vec![];

        let result = model.forward(&token_ids, 0);
        // Empty tokens should either work or fail gracefully
        let _ = result;
    }

    #[test]
    fn test_forward_with_hidden_states() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };

        let model = MockMLXFFIModel::new(config);
        let token_ids = vec![1, 2, 3];

        let result = model.forward_with_hidden_states(&token_ids);
        assert!(result.is_ok());

        let (logits, hidden_states) = result.unwrap();
        assert_eq!(logits.len(), 32000);

        // Should have hidden states for attention modules
        assert!(hidden_states.contains_key("q_proj"));
        assert!(hidden_states.contains_key("k_proj"));
        assert!(hidden_states.contains_key("v_proj"));
        assert!(hidden_states.contains_key("o_proj"));
    }
}

#[cfg(test)]
mod generation_tests {
    use adapteros_lora_mlx_ffi::mock::MockMLXFFIModel;
    use adapteros_lora_mlx_ffi::ModelConfig;

    #[test]
    #[ignore] // Generate not fully implemented
    fn test_text_generation() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };

        let model = MockMLXFFIModel::new(config);
        let prompt = "Hello, world!";
        let max_tokens = 10;

        // Generate is currently a placeholder
        let _ = model.forward(&[1, 2, 3], 0);
    }
}

#[cfg(test)]
mod model_thread_safety_tests {
    use adapteros_lora_mlx_ffi::mock::MockMLXFFIModel;
    use adapteros_lora_mlx_ffi::ModelConfig;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_model_is_send_sync() {
        let config = ModelConfig {
            hidden_size: 4096,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: 8,
            intermediate_size: 11008,
            vocab_size: 32000,
            max_position_embeddings: 32768,
            rope_theta: 10000.0,
        };

        let model = Arc::new(MockMLXFFIModel::new(config));

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let model_clone = Arc::clone(&model);
                thread::spawn(move || {
                    let token_ids = vec![i as u32];
                    model_clone.forward(&token_ids, 0).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

#[cfg(test)]
mod embedding_config_tests {
    use adapteros_lora_mlx_ffi::embedding::EmbeddingConfig;

    #[test]
    fn test_embedding_config_parsing() {
        let config_json = r#"
        {
            "hidden_size": 384,
            "num_hidden_layers": 12,
            "num_attention_heads": 12,
            "max_position_embeddings": 512,
            "vocab_size": 30522
        }
        "#;

        let config: EmbeddingConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.hidden_size, 384);
        assert_eq!(config.num_hidden_layers, 12);
        assert_eq!(config.pooling_mode, "mean"); // Default
        assert!(config.normalize_embeddings); // Default true
    }

    #[test]
    fn test_embedding_config_with_pooling() {
        let config_json = r#"
        {
            "hidden_size": 768,
            "num_hidden_layers": 12,
            "num_attention_heads": 12,
            "max_position_embeddings": 512,
            "vocab_size": 30522,
            "pooling_mode": "cls",
            "normalize_embeddings": false
        }
        "#;

        let config: EmbeddingConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.pooling_mode, "cls");
        assert!(!config.normalize_embeddings);
    }
}

#[cfg(test)]
mod embedding_model_tests {
    #[test]
    #[ignore] // Requires actual embedding model files
    fn test_embedding_model_load() {
        // This test requires:
        // - model.safetensors
        // - config.json
        // - tokenizer.json
        // Skipped for automated testing
    }

    #[test]
    #[ignore] // Requires actual embedding model
    fn test_embedding_encode_text() {
        // This test would verify text encoding
        // Skipped as it requires real model files
    }
}
