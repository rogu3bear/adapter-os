//! End-to-end training workflow for the AdapterOS base code adapter.
//!
//! Loads the curated dataset manifest, runs the deterministic Micro-LoRA trainer,
//! and packages quantized weights into `adapters/<adapter_id>/`.

use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{MicroLoRATrainer, TrainingConfig, TrainingExample};
use adapteros_single_file_adapter::format::{
    AdapterWeights, LineageInfo, SingleFileAdapter, WeightGroup, WeightGroupType, WeightMetadata,
};
use adapteros_single_file_adapter::SingleFileAdapterPackager;
use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Parser, Clone)]
pub struct TrainBaseAdapterArgs {
    /// Dataset manifest describing positive/negative samples
    #[arg(
        long,
        default_value = "training/datasets/base/code/adapteros/manifest.json"
    )]
    pub manifest: PathBuf,

    /// Qwen tokenizer JSON file
    #[arg(long, default_value = "models/qwen2.5-7b-mlx/tokenizer.json")]
    pub tokenizer: PathBuf,

    /// Output adapters directory
    #[arg(long, default_value = "adapters")]
    pub output_dir: PathBuf,

    /// Output format: directory or aos
    #[arg(long, default_value = "directory")]
    pub output_format: String,

    /// Adapter ID (used for packaged directory name)
    #[arg(long, default_value = "code_lang_v1")]
    pub adapter_id: String,

    /// LoRA rank (MasterPlan Layer 2 default = 16)
    #[arg(long, default_value_t = 16)]
    pub rank: usize,

    /// LoRA alpha scaling factor (MasterPlan Layer 2 default = 32.0)
    #[arg(long, default_value_t = 32.0)]
    pub alpha: f32,

    /// Learning rate for deterministic trainer
    #[arg(long, default_value_t = 5e-4)]
    pub learning_rate: f32,

    /// Batch size for training
    #[arg(long, default_value_t = 8)]
    pub batch_size: usize,

    /// Number of epochs
    #[arg(long, default_value_t = 4)]
    pub epochs: usize,

    /// Hidden dimension (Qwen2.5-7B = 3584)
    #[arg(long, default_value_t = 3584)]
    pub hidden_dim: usize,
}

/// Dataset manifest structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatasetManifest {
    name: String,
    version: String,
    category: String,
    scope: String,
    tier: String,
    rank: u32,
    alpha: f32,
    target_modules: Vec<String>,
    entries: Vec<DatasetEntry>,
}

/// Dataset entry pointing to JSONL files
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatasetEntry {
    path: String,
    format: String,
    weight: f32,
    role: String, // "positive" or "negative"
}

/// Training example from JSONL
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonlExample {
    input: String,
    target: String,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
    #[serde(default = "default_weight")]
    weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

pub async fn run(args: TrainBaseAdapterArgs) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .without_time()
        .try_init()
        .ok();

    info!("Starting base adapter training workflow");
    info!("  Manifest: {:?}", args.manifest);
    info!("  Tokenizer: {:?}", args.tokenizer);
    info!("  Output: {:?}/{}", args.output_dir, args.adapter_id);
    info!("  Format: {}", args.output_format);
    info!("  Rank: {}, Alpha: {}", args.rank, args.alpha);

    // Step 1: Load dataset manifest
    info!("Loading dataset manifest from {:?}", args.manifest);
    let manifest = load_manifest(&args.manifest).context("Failed to load dataset manifest")?;

    info!(
        "Loaded manifest: {} v{} ({} entries)",
        manifest.name,
        manifest.version,
        manifest.entries.len()
    );

    // Step 2: Initialize tokenizer
    info!("Initializing QwenTokenizer from {:?}", args.tokenizer);
    let tokenizer =
        QwenTokenizer::from_file(&args.tokenizer).context("Failed to initialize tokenizer")?;
    info!("Tokenizer initialized successfully");

    // Step 3: Load and tokenize training examples
    let manifest_dir = args
        .manifest
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let (positive_examples, negative_examples) =
        load_and_tokenize_examples(&manifest, manifest_dir, &tokenizer)
            .context("Failed to load training examples")?;

    info!(
        "Loaded {} positive and {} negative examples",
        positive_examples.len(),
        negative_examples.len()
    );

    // Step 4: Train positive and negative adapters
    let config = TrainingConfig {
        rank: args.rank,
        alpha: args.alpha,
        learning_rate: args.learning_rate,
        batch_size: args.batch_size,
        epochs: args.epochs,
        hidden_dim: args.hidden_dim,
        vocab_size: 151936, // Qwen2.5 vocab size
        preferred_backend: None,
        require_gpu: false,
        max_gpu_memory_mb: 0,
    };

    // Train positive adapter
    info!("Training positive adapter...");
    let mut trainer_pos =
        MicroLoRATrainer::new(config.clone()).context("Failed to create positive trainer")?;

    let positive_result = trainer_pos
        .train(&positive_examples)
        .await
        .context("Failed to train positive adapter")?;

    info!(
        "Positive training complete: loss={:.4}, time={}ms",
        positive_result.final_loss,
        positive_result.training_time_ms()
    );

    // Train negative adapter (if available)
    let negative_result = if !negative_examples.is_empty() {
        info!("Training negative adapter...");
        let mut trainer_neg =
            MicroLoRATrainer::new(config.clone()).context("Failed to create negative trainer")?;

        let result = trainer_neg
            .train(&negative_examples)
            .await
            .context("Failed to train negative adapter")?;

        info!(
            "Negative training complete: loss={:.4}, time={}ms",
            result.final_loss,
            result.training_time_ms()
        );
        Some(result)
    } else {
        warn!("No negative examples found, skipping negative adapter training");
        None
    };

    // Step 5: Package adapter weights
    info!("Packaging adapter weights...");
    let adapter_weights = create_adapter_weights(
        positive_result,
        negative_result,
        positive_examples.len(),
        negative_examples.len(),
    );

    let lineage = LineageInfo {
        adapter_id: args.adapter_id.clone(),
        version: manifest.version.clone(),
        parent_version: None,
        parent_hash: None,
        mutations: vec![],
        quality_delta: 0.0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let training_config = adapteros_single_file_adapter::training::TrainingConfig {
        rank: args.rank,
        alpha: args.alpha,
        learning_rate: args.learning_rate,
        batch_size: args.batch_size,
        epochs: args.epochs,
        hidden_dim: args.hidden_dim,
        weight_group_config: adapteros_single_file_adapter::format::WeightGroupConfig::default(),
    };

    let adapter = SingleFileAdapter::create(
        args.adapter_id.clone(),
        adapter_weights,
        vec![], // Training data (empty for production adapters)
        training_config,
        lineage,
    )
    .context("Failed to create SingleFileAdapter")?;

    // Step 6: Save adapter in requested format
    let output_path = match args.output_format.as_str() {
        "aos" => {
            let aos_path = args.output_dir.join(format!("{}.aos", args.adapter_id));
            info!("Saving adapter to .aos file: {:?}", aos_path);

            fs::create_dir_all(&args.output_dir).context("Failed to create output directory")?;

            SingleFileAdapterPackager::save(&adapter, &aos_path)
                .await
                .context("Failed to save .aos file")?;

            aos_path
        }
        "directory" => {
            let dir_path = args.output_dir.join(&args.adapter_id);
            info!("Saving adapter to directory: {:?}", dir_path);

            fs::create_dir_all(&dir_path).context("Failed to create adapter directory")?;

            // Save manifest
            let manifest_path = dir_path.join("manifest.json");
            let manifest_json = serde_json::to_string_pretty(&adapter.manifest)
                .context("Failed to serialize manifest")?;
            fs::write(&manifest_path, manifest_json).context("Failed to write manifest")?;

            // Save weights
            let weights_path = dir_path.join("weights.json");
            let weights_json = serde_json::to_string_pretty(&adapter.weights)
                .context("Failed to serialize weights")?;
            fs::write(&weights_path, weights_json).context("Failed to write weights")?;

            // Save config
            let config_path = dir_path.join("config.json");
            let config_json = serde_json::to_string_pretty(&adapter.config)
                .context("Failed to serialize config")?;
            fs::write(&config_path, config_json).context("Failed to write config")?;

            dir_path
        }
        other => {
            bail!(
                "Unsupported output format: {}. Use 'directory' or 'aos'",
                other
            );
        }
    };

    info!("Adapter training and packaging complete!");
    info!("  Output: {:?}", output_path);
    info!("  Format: {}", args.output_format);
    info!("  Rank: {}, Alpha: {}", args.rank, args.alpha);

    Ok(())
}

fn load_manifest(path: &PathBuf) -> Result<DatasetManifest> {
    let content =
        fs::read_to_string(path).context(format!("Failed to read manifest file: {:?}", path))?;

    let manifest: DatasetManifest =
        serde_json::from_str(&content).context("Failed to parse manifest JSON")?;

    Ok(manifest)
}

fn load_and_tokenize_examples(
    manifest: &DatasetManifest,
    manifest_dir: &std::path::Path,
    tokenizer: &QwenTokenizer,
) -> Result<(Vec<TrainingExample>, Vec<TrainingExample>)> {
    let mut positive_examples = Vec::new();
    let mut negative_examples = Vec::new();

    for entry in &manifest.entries {
        info!(
            "Loading examples from: {} (role: {})",
            entry.path, entry.role
        );

        let entry_path = manifest_dir.join(&entry.path);
        let file = fs::File::open(&entry_path)
            .context(format!("Failed to open dataset file: {:?}", entry_path))?;

        let reader = BufReader::new(file);
        let mut count = 0;

        for (line_num, line) in reader.lines().enumerate() {
            let line = line.context(format!(
                "Failed to read line {} from {:?}",
                line_num + 1,
                entry_path
            ))?;

            if line.trim().is_empty() {
                continue;
            }

            let jsonl_example: JsonlExample = serde_json::from_str(&line).context(format!(
                "Failed to parse JSON on line {} of {:?}",
                line_num + 1,
                entry_path
            ))?;

            // Tokenize input and target
            let input_ids = tokenizer
                .encode(&jsonl_example.input)
                .context(format!("Failed to tokenize input on line {}", line_num + 1))?;

            let target_ids = tokenizer.encode(&jsonl_example.target).context(format!(
                "Failed to tokenize target on line {}",
                line_num + 1
            ))?;

            // Convert metadata to string map
            let metadata: HashMap<String, String> = jsonl_example
                .metadata
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect();

            let example = TrainingExample {
                input: input_ids,
                target: target_ids,
                metadata,
                weight: jsonl_example.weight * entry.weight,
            };

            match entry.role.as_str() {
                "positive" => positive_examples.push(example),
                "negative" => negative_examples.push(example),
                other => {
                    warn!("Unknown role '{}' in entry, treating as positive", other);
                    positive_examples.push(example);
                }
            }

            count += 1;
        }

        info!("  Loaded {} examples from {}", count, entry.path);
    }

    if positive_examples.is_empty() {
        bail!("No positive training examples found in manifest");
    }

    Ok((positive_examples, negative_examples))
}

fn create_adapter_weights(
    positive_result: adapteros_lora_worker::training::TrainingResult,
    negative_result: Option<adapteros_lora_worker::training::TrainingResult>,
    positive_count: usize,
    negative_count: usize,
) -> AdapterWeights {
    let pos_time_ms = positive_result.training_time_ms();
    let pos_loss = positive_result.final_loss;

    let positive_weights = WeightGroup {
        lora_a: positive_result.weights.lora_a,
        lora_b: positive_result.weights.lora_b,
        metadata: WeightMetadata {
            example_count: positive_count,
            avg_loss: pos_loss,
            training_time_ms: pos_time_ms,
            group_type: WeightGroupType::Positive,
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    };

    let negative_weights = if let Some(neg_result) = negative_result {
        let neg_time_ms = neg_result.training_time_ms();
        let neg_loss = neg_result.final_loss;

        WeightGroup {
            lora_a: neg_result.weights.lora_a,
            lora_b: neg_result.weights.lora_b,
            metadata: WeightMetadata {
                example_count: negative_count,
                avg_loss: neg_loss,
                training_time_ms: neg_time_ms,
                group_type: WeightGroupType::Negative,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        }
    } else {
        // Create empty negative weights
        let rank = positive_weights.lora_a.len();
        let hidden_dim = positive_weights.lora_b.len();

        WeightGroup {
            lora_a: vec![vec![0.0; hidden_dim]; rank],
            lora_b: vec![vec![0.0; rank]; hidden_dim],
            metadata: WeightMetadata {
                example_count: 0,
                avg_loss: 0.0,
                training_time_ms: 0,
                group_type: WeightGroupType::Negative,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        }
    };

    AdapterWeights {
        positive: positive_weights,
        negative: negative_weights,
        combined: None, // Will be computed at inference time
    }
}
