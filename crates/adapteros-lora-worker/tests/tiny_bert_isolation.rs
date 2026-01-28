use adapteros_core::Result;
use adapteros_lora_worker::ane_embedder::TinyBertEmbedder;
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn test_tiny_bert_loading_mock() -> Result<()> {
    // 1. Setup mock model directory
    let temp_dir = tempfile::tempdir().unwrap();
    let model_path = temp_dir.path().join("mock-model.mlpackage");
    fs::create_dir_all(&model_path).unwrap();

    // Create config.json
    let config_content = r#"{ "hidden_size": 128 }"#;
    fs::write(model_path.join("config.json"), config_content).unwrap();

    // Create tokenizer.json (more complete valid JSON for WordPiece)
    let tokenizer_content = r###"{
        "version": "1.0",
        "truncation": null,
        "padding": null,
        "added_tokens": [],
        "normalizer": null,
        "pre_tokenizer": null,
        "post_processor": null,
        "decoder": null,
        "model": {
            "type": "WordPiece",
            "vocab": {"[PAD]": 0, "[UNK]": 1, "[CLS]": 2, "[SEP]": 3, "[MASK]": 4},
            "unk_token": "[UNK]",
            "continuing_subword_prefix": "##",
            "max_input_chars_per_word": 100
        }
    }"###;
    fs::write(model_path.join("tokenizer.json"), tokenizer_content).unwrap();

    // 2. Attempt load
    // This expects to fail at FFI level because it's not a real CoreML model,
    // but we verify it passes the initial validation steps.
    let result = TinyBertEmbedder::load(&model_path, None);

    match result {
        Ok(_) => {
            // Unexpected success with mock model?
            // If FFI is savvy enough to reject empty folder, it should be Err.
            // If unsafe block crashes, we'll know.
            println!("Load succeeded (unexpected for mock)");
        }
        Err(e) => {
            println!("Load failed as expected: {}", e);
            assert!(
                e.to_string().contains("Failed to load Tiny-BERT model")
                    || e.to_string().contains("CoreML backend feature not enabled")
            );
        }
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Run manually when model is present
async fn test_tiny_bert_loading_real() -> Result<()> {
    // Path to the downloaded model from script
    let repo_root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()); // This might be wrong in workspace
    let model_path = PathBuf::from("../.var/models/tiny-bert-4bit-ane.mlpackage");

    if !model_path.exists() {
        println!("Skipping real test: Model not found at {:?}", model_path);
        return Ok(());
    }

    let embedder = TinyBertEmbedder::load(&model_path, None)?;
    println!("Loaded embedder with dim: {}", embedder.dimension());

    let text = "Hello world";
    let embedding = embedder.embed(text);
    println!("Embedding len: {}", embedding.len());
    assert_eq!(embedding.len(), embedder.dimension());
    assert!(
        embedding.iter().any(|&x| x != 0.0),
        "Embedding should not be all zeros"
    );

    Ok(())
}
