//! Train base adapter command implementation for CLI
//!
//! Loads the curated dataset manifest, runs the deterministic Micro-LoRA trainer,
//! and packages quantized weights into `adapters/<adapter_id>/`.

use adapteros_core::{AosError, Result};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{
    load_examples_from_manifest, load_examples_with_encoder, AdapterPackager, DatasetManifest,
    LoRAQuantizer, MicroLoRATrainer, TrainingConfig as LoRAWorkerTrainingConfig,
};
use adapteros_single_file_adapter::{
    format::{
        AdapterWeights, LineageInfo, WeightGroup, WeightGroupConfig, WeightGroupType,
        WeightMetadata,
    },
    SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig as SingleFileTrainingConfig,
};
use clap::Args;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;

/// Train base adapter arguments
#[derive(Args, Debug, Clone)]
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

impl TrainBaseAdapterArgs {
    /// Execute the train-base-adapter command
    pub async fn execute(&self) -> Result<()> {
        let tokenizer = match QwenTokenizer::from_file(&self.tokenizer) {
            Ok(tok) => {
                info!("Loaded tokenizer {}", self.tokenizer.display());
                Some(tok)
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to load tokenizer at {} ({}); falling back to naive char encoder",
                    self.tokenizer.display(),
                    err
                );
                None
            }
        };

        info!("Loading dataset manifest {}", self.manifest.display());
        let examples = if let Some(ref tok) = tokenizer {
            load_examples_from_manifest(&self.manifest, tok).map_err(|e| {
                AosError::Training(format!(
                    "Failed to build training set from manifest {}: {}",
                    self.manifest.display(),
                    e
                ))
            })?
        } else {
            load_examples_with_encoder(&self.manifest, |text| {
                Ok(text.chars().map(|c| c as u32).collect())
            })
            .map_err(|e| {
                AosError::Training(format!(
                    "Failed to build training set (fallback encoder) from manifest {}: {}",
                    self.manifest.display(),
                    e
                ))
            })?
        };

        let manifest_str = std::fs::read_to_string(&self.manifest).map_err(|e| {
            AosError::Io(format!(
                "Failed to read dataset manifest {}: {}",
                self.manifest.display(),
                e
            ))
        })?;

        let manifest: DatasetManifest = serde_json::from_str(&manifest_str).map_err(|e| {
            AosError::Parse(format!(
                "Failed to parse dataset manifest {}: {}",
                self.manifest.display(),
                e
            ))
        })?;

        let total_examples = examples.len();
        let positive_examples = examples
            .iter()
            .filter(|example| example.weight > 0.0)
            .count();
        let negative_examples = examples
            .iter()
            .filter(|example| example.weight < 0.0)
            .count();
        let zero_weight_examples =
            total_examples.saturating_sub(positive_examples + negative_examples);
        let total_weight: f32 = examples.iter().map(|example| example.weight).sum();
        let min_input = examples
            .iter()
            .map(|example| example.input.len())
            .min()
            .unwrap_or(0);
        let max_input = examples
            .iter()
            .map(|example| example.input.len())
            .max()
            .unwrap_or(0);
        let min_target = examples
            .iter()
            .map(|example| example.target.len())
            .min()
            .unwrap_or(0);
        let max_target = examples
            .iter()
            .map(|example| example.target.len())
            .max()
            .unwrap_or(0);
        let total_input_tokens: usize = examples.iter().map(|example| example.input.len()).sum();
        let total_target_tokens: usize = examples.iter().map(|example| example.target.len()).sum();

        info!(
            "Dataset {} v{} (entries={}): total={}, +{} / -{} / 0={}, total_weight={:.6}",
            manifest.name,
            manifest.version.as_deref().unwrap_or("unversioned"),
            manifest.entries.len(),
            total_examples,
            positive_examples,
            negative_examples,
            zero_weight_examples,
            total_weight
        );

        info!(
            "Token stats: input_sum={}, target_sum={}, min_input={}, max_input={}, min_target={}, max_target={}",
            total_input_tokens, total_target_tokens, min_input, max_input, min_target, max_target
        );

        let config = LoRAWorkerTrainingConfig {
            rank: self.rank,
            alpha: self.alpha,
            learning_rate: self.learning_rate,
            batch_size: self.batch_size,
            epochs: self.epochs,
            hidden_dim: self.hidden_dim,
            weight_group_config: WeightGroupConfig::default(),
        };

        let mut trainer = MicroLoRATrainer::new(config.clone()).map_err(|e| {
            AosError::Training(format!("Failed to initialize MicroLoRA trainer: {}", e))
        })?;

        let result = trainer
            .train_with_callback(&examples, |epoch, loss| {
                info!("Epoch {} complete (loss {:.6})", epoch, loss);
            })
            .await
            .map_err(|e| AosError::Training(format!("Training loop failed: {}", e)))?;

        info!(
            "Training finished: adapter_id={}, final_loss={:.6}, time_ms={}",
            result.adapter_id, result.final_loss, result.training_time_ms
        );

        if !result.final_loss.is_finite() {
            return Err(AosError::Training(
                "Training produced non-finite loss; aborting before quantization".to_string(),
            ));
        }

        let has_non_finite = result
            .weights
            .lora_a
            .iter()
            .flat_map(|row| row.iter())
            .chain(result.weights.lora_b.iter().flat_map(|row| row.iter()))
            .any(|value| !value.is_finite());

        if has_non_finite {
            return Err(AosError::Training(
                "Training produced non-finite weights; inspect dataset and trainer output"
                    .to_string(),
            ));
        }

        let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);

        std::fs::create_dir_all(&self.output_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create output directory {}: {}",
                self.output_dir.display(),
                e
            ))
        })?;

        let packager = AdapterPackager::new(&self.output_dir);

        let mut manifest_metadata: HashMap<String, String> = HashMap::new();
        manifest_metadata.insert("dataset_name".to_string(), manifest.name.clone());
        if let Some(ref version) = manifest.version {
            manifest_metadata.insert("dataset_version".to_string(), version.clone());
        }
        if let Some(ref description) = manifest.description {
            manifest_metadata.insert("dataset_description".to_string(), description.clone());
        }
        manifest_metadata.insert(
            "manifest_path".to_string(),
            self.manifest.display().to_string(),
        );
        manifest_metadata.insert(
            "tokenizer_path".to_string(),
            self.tokenizer.display().to_string(),
        );
        manifest_metadata.insert(
            "manifest_entries".to_string(),
            manifest.entries.len().to_string(),
        );
        manifest_metadata.insert("total_examples".to_string(), total_examples.to_string());
        manifest_metadata.insert(
            "positive_examples".to_string(),
            positive_examples.to_string(),
        );
        manifest_metadata.insert(
            "negative_examples".to_string(),
            negative_examples.to_string(),
        );
        manifest_metadata.insert(
            "zero_weight_examples".to_string(),
            zero_weight_examples.to_string(),
        );
        manifest_metadata.insert("total_weight".to_string(), format!("{:.6}", total_weight));
        manifest_metadata.insert(
            "input_token_sum".to_string(),
            total_input_tokens.to_string(),
        );
        manifest_metadata.insert(
            "target_token_sum".to_string(),
            total_target_tokens.to_string(),
        );

        // Package adapter
        let base_model = "qwen2.5-7b".to_string();
        let mut packaged = packager
            .package(&self.adapter_id, &quantized, &config, &base_model)
            .await
            .map_err(|e| {
                AosError::Training(format!("Packaging adapter artifacts failed: {}", e))
            })?;

        // Update manifest with metadata
        packaged.manifest.metadata = manifest_metadata;

        // Save updated manifest
        let manifest_path = self.output_dir.join(&self.adapter_id).join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&packaged.manifest)?;
        std::fs::write(&manifest_path, manifest_json)
            .map_err(|e| AosError::Io(format!("Failed to write manifest: {}", e)))?;

        if self.output_format == "aos" {
            let aos_config = SingleFileTrainingConfig {
                rank: self.rank,
                alpha: self.alpha,
                learning_rate: self.learning_rate,
                batch_size: self.batch_size,
                epochs: self.epochs,
                hidden_dim: self.hidden_dim,
                weight_group_config: WeightGroupConfig::default(),
            };

            let lineage = LineageInfo {
                adapter_id: self.adapter_id.clone(),
                version: "1.0.0".to_string(),
                parent_version: None,
                parent_hash: None,
                mutations: vec![],
                quality_delta: 0.0,
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            // Construct AdapterWeights from training result
            let adapter_weights = AdapterWeights {
                positive: WeightGroup {
                    lora_a: result.weights.lora_a.clone(),
                    lora_b: result.weights.lora_b.clone(),
                    metadata: WeightMetadata {
                        example_count: positive_examples,
                        avg_loss: result.final_loss,
                        training_time_ms: result.training_time_ms,
                        group_type: WeightGroupType::Positive,
                        created_at: chrono::Utc::now().to_rfc3339(),
                    },
                },
                negative: WeightGroup {
                    lora_a: vec![],
                    lora_b: vec![],
                    metadata: WeightMetadata {
                        example_count: negative_examples,
                        avg_loss: 0.0,
                        training_time_ms: 0,
                        group_type: WeightGroupType::Negative,
                        created_at: chrono::Utc::now().to_rfc3339(),
                    },
                },
                combined: None,
            };

            let adapter = SingleFileAdapter::create(
                self.adapter_id.clone(),
                adapter_weights,
                examples
                    .into_iter()
                    .map(|ex| adapteros_single_file_adapter::TrainingExample {
                        input: ex.input,
                        target: ex.target,
                        metadata: ex.metadata,
                        weight: ex.weight,
                    })
                    .collect(),
                aos_config.clone(),
                lineage,
            )
            .map_err(|e| {
                AosError::Training(format!("Failed to create SingleFileAdapter: {}", e))
            })?;

            let aos_path = self.output_dir.join(format!("{}.aos", self.adapter_id));
            SingleFileAdapterPackager::save(&adapter, &aos_path)
                .await
                .map_err(|e| AosError::Training(format!("Failed to save .aos file: {}", e)))?;

            info!("Created .aos file: {}", aos_path.display());
        }

        info!(
            "Packaged adapter {} → {} (hash b3:{})",
            packaged.adapter_id,
            packaged.weights_path.display(),
            packaged.hash_b3
        );

        Ok(())
    }
}
