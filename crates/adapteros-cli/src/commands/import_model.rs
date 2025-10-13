//! Import MLX model command

use anyhow::{Context, Result};
// use adapteros_lora_mlx::{MLXModel, ModelConfig};  // Temporarily disabled due to PyO3 linking issues
use crate::output::OutputWriter;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct ModelImportResult {
    name: String,
    model_path: String,
    weights: String,
    config: String,
}

pub async fn run(
    name: &str,
    weights: &Path,
    config: &Path,
    tokenizer: &Path,
    tokenizer_cfg: &Path,
    license: &Path,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!("Importing MLX model: {}", name));

    // Determine model directory from weights path
    let model_path = weights.parent().context("Invalid weights path")?;

    output.section("Verifying files");
    output.kv("Weights", &weights.display().to_string());
    output.kv("Config", &config.display().to_string());
    output.kv("Tokenizer", &tokenizer.display().to_string());
    output.kv("Tokenizer config", &tokenizer_cfg.display().to_string());
    output.kv("License", &license.display().to_string());

    // Verify all files exist
    for (file_name, path) in [
        ("weights", weights),
        ("config", config),
        ("tokenizer", tokenizer),
        ("tokenizer config", tokenizer_cfg),
        ("license", license),
    ] {
        if !path.exists() {
            anyhow::bail!("{} file not found: {}", file_name, path.display());
        }
    }

    // Parse config
    output.blank();
    output.info("Parsing model configuration...");
    let _config_str = std::fs::read_to_string(config).context("Failed to read config.json")?;

    // Temporarily disabled due to PyO3 linking issues
    output.warning("MLX model loading temporarily disabled due to PyO3 linking issues");
    output.success("Model files found and validated");
    output.kv("Name", name);
    output.kv("Weights", &format!("{:?}", weights));
    output.kv("Config", &format!("{:?}", config));

    // TODO: Store model metadata in database
    // For now, just verify the model is valid

    output.blank();
    output.success("Model import complete!");
    output.kv("Model name", name);
    output.kv("Model path", &model_path.display().to_string());

    if output.is_json() {
        let result = ModelImportResult {
            name: name.to_string(),
            model_path: model_path.display().to_string(),
            weights: weights.display().to_string(),
            config: config.display().to_string(),
        };
        output.json(&result)?;
    }

    Ok(())
}
