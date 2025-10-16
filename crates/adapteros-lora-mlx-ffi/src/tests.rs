use super::*;

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
    assert_eq!(config.rope_theta, 10000.0);
}

#[test]
#[ignore] // Requires MLX model
fn test_model_loading() {
    // This test would require a real MLX model
    // Skipped for now
}

#[test]
fn test_forward_with_hidden_states_rejects_empty_input() {
    let config = ModelConfig {
        hidden_size: 1,
        num_hidden_layers: 1,
        num_attention_heads: 1,
        num_key_value_heads: 1,
        intermediate_size: 1,
        vocab_size: 4,
        max_position_embeddings: 8,
        rope_theta: 10000.0,
    };
    let model = MLXFFIModel {
        model: std::ptr::null_mut(),
        config,
    };

    let err = model.forward_with_hidden_states(&[]).unwrap_err();
    assert!(matches!(err, AosError::Validation(_)));
}
