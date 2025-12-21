//! Quantize and package trained LoRA weights

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
pub struct PackLoraArgs {
    /// Input directory produced by training (contains lora_weights.json)
    #[arg(long, default_value = "out/code2db")]
    pub input_dir: PathBuf,

    /// Output adapters directory
    #[arg(long, default_value = "adapters")]
    pub output_dir: PathBuf,

    /// Adapter ID to assign
    #[arg(long, default_value = "code2db")]
    pub adapter_id: String,

    /// Base model identifier to record in manifest
    #[arg(long, default_value = "qwen2.5-7b")]
    pub base_model: String,
}

pub async fn run(args: PackLoraArgs) -> Result<()> {
    // Load weights JSON produced by aosctl train
    let weights_path = args.input_dir.join("lora_weights.json");
    let weights_json = std::fs::read_to_string(&weights_path)
        .with_context(|| format!("reading {}", weights_path.display()))?;
    let weights: adapteros_lora_worker::training::LoRAWeights =
        serde_json::from_str(&weights_json).context("parsing lora_weights.json")?;

    // Quantize to Q15
    let quant = adapteros_lora_worker::training::LoRAQuantizer::quantize_to_q15(&weights);

    // Use default training config for manifest, but preserve rank
    let cfg = adapteros_lora_worker::training::TrainingConfig {
        rank: weights.lora_a.len(),
        ..Default::default()
    };

    // Package
    let packager = adapteros_lora_worker::training::AdapterPackager::new(&args.output_dir);
    let packaged = packager
        .package("default", &args.adapter_id, &quant, &cfg, &args.base_model)
        .await
        .context("packaging adapter")?;

    println!(
        "✓ Packaged adapter {} → {} (b3:{})",
        packaged.adapter_id,
        packaged.weights_path.display(),
        packaged.hash_b3
    );
    Ok(())
}
