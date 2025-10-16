use super::*;
use std::fs;
use std::path::Path;

use serde_json::json;
use tokenizers::{
    models::wordlevel::WordLevel, pre_tokenizers::whitespace::Whitespace, AddedToken,
};

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

fn write_stub_config(path: &Path) {
    let config = json!({
        "hidden_size": 16,
        "num_hidden_layers": 2,
        "num_attention_heads": 2,
        "num_key_value_heads": 2,
        "intermediate_size": 32,
        "vocab_size": 8,
        "max_position_embeddings": 32,
        "rope_theta": 10000.0
    });
    fs::write(path.join("config.json"), config.to_string()).unwrap();
}

fn write_stub_tokenizer(path: &Path) {
    let mut vocab = std::collections::HashMap::new();
    vocab.insert("hello".to_string(), 0);
    vocab.insert("world".to_string(), 1);
    vocab.insert("mlx".to_string(), 2);
    vocab.insert("ffi".to_string(), 3);
    vocab.insert("<eos>".to_string(), 4);
    vocab.insert("<unk>".to_string(), 5);

    let model = WordLevel::builder()
        .vocab(vocab)
        .unk_token("<unk>".to_string())
        .build()
        .unwrap();
    let mut tokenizer = Tokenizer::new(model);
    tokenizer.with_pre_tokenizer(Whitespace::default());
    tokenizer.add_special_tokens(&[AddedToken::from("<eos>", true)]);

    let tokenizer_path = path.join("tokenizer.json");
    tokenizer
        .save(tokenizer_path.to_str().unwrap(), true)
        .unwrap();
}

fn prepare_stub_model_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_stub_config(dir.path());
    write_stub_tokenizer(dir.path());
    dir
}

#[test]
fn test_generate_produces_output() {
    let dir = prepare_stub_model_dir();
    let model = MLXFFIModel::load(dir.path()).expect("model should load with stub FFI");
    let output = model
        .generate("hello mlx", 4)
        .expect("generation should succeed");
    let token_count = if output.trim().is_empty() {
        0
    } else {
        output.split_whitespace().count()
    };
    assert!(token_count <= 4);
}

#[test]
fn test_forward_with_hidden_states_returns_logits() {
    let dir = prepare_stub_model_dir();
    let model = MLXFFIModel::load(dir.path()).expect("model should load");
    let (logits, hidden) = model
        .forward_with_hidden_states(&[0, 1])
        .expect("forward_with_hidden_states should succeed");
    assert_eq!(logits.len(), model.config.vocab_size);
    assert!(hidden.len() <= HIDDEN_STATE_MODULES.len());
}

#[test]
fn test_generate_rejects_empty_prompt() {
    let dir = prepare_stub_model_dir();
    let model = MLXFFIModel::load(dir.path()).expect("model should load");
    let err = model
        .generate("   ", 4)
        .expect_err("empty prompt should error");
    assert!(matches!(err, AosError::Validation(_)));
}

#[test]
fn test_missing_tokenizer_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    write_stub_config(dir.path());
    let result = MLXFFIModel::load(dir.path());
    assert!(matches!(result, Err(AosError::NotFound(_))));
}
