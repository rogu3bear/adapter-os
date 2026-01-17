//! Training command implementation

#![allow(clippy::field_reassign_with_default)]

use crate::commands::training_common::CommonTrainingArgs;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_worker::training::{
    BuiltDatasetManifest, DeterminismConfig, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
use adapteros_types::training::{ExampleMetadataV1, TRAINING_DATA_CONTRACT_VERSION};
use clap::Args;
use serde_json;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Train a LoRA adapter
#[derive(Args, Debug, Clone)]
pub struct TrainArgs {
    /// Training configuration file (JSON)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Training data path (dataset build output directory or examples.jsonl)
    #[arg(short, long)]
    data: PathBuf,

    /// Output directory for trained adapter
    #[arg(short, long)]
    output: PathBuf,

    /// Base model path for training
    #[arg(long)]
    base_model: PathBuf,

    /// Plan file for Metal backend initialization
    #[arg(long)]
    plan: Option<PathBuf>,

    /// Enable deterministic training
    #[arg(long)]
    deterministic: bool,

    /// Training seed (for deterministic training)
    #[arg(long)]
    seed: Option<u64>,

    /// Resume from latest checkpoint if available
    #[arg(long)]
    resume: bool,

    /// Force resume even if config changed (may produce incorrect results)
    #[arg(
        long,
        help = "Force resume even if config changed (may produce incorrect results)"
    )]
    force_resume: bool,

    /// Common training hyperparameters
    #[command(flatten)]
    common: CommonTrainingArgs,
}

struct LoadedTrainingData {
    examples: Vec<TrainingExample>,
    dataset_hash_b3: String,
    tokenizer_hash_b3: String,
    framing_policy: String,
}

struct TrainingRunMetadata {
    dataset_hash_b3: String,
    framing_policy: String,
    tokenizer_hash_b3: String,
    training_config_hash: String,
    determinism_tier: String,
}

impl TrainArgs {
    /// Execute the training command
    pub async fn execute(&self) -> Result<()> {
        info!("Starting LoRA training with Rust-native implementation");

        // Load training configuration
        let config = self.load_config()?;

        // Load training data
        let loaded = self.load_training_data()?;
        let examples = loaded.examples;
        info!("Loaded {} training examples", examples.len());

        let base_tokenizer_hash = compute_tokenizer_hash(&self.base_model)?;
        if base_tokenizer_hash != loaded.tokenizer_hash_b3 {
            return Err(AosError::Validation(format!(
                "Tokenizer hash mismatch: dataset {} vs base model {}",
                loaded.tokenizer_hash_b3, base_tokenizer_hash
            )));
        }
        let training_config_hash = compute_training_config_hash(&config)?;
        let use_gpu_backward = config.use_gpu_backward;

        // Create trainer
        let mut trainer = MicroLoRATrainer::new(config)?;
        trainer.set_force_resume(self.force_resume);

        // Enable checkpointing for resume support
        trainer.enable_checkpointing(&self.output, "training", 5);

        // Check for checkpoint availability
        let checkpoint_exists = trainer.has_checkpoint().await;

        // Initialize Metal kernels if plan is provided
        if let Some(plan_path) = &self.plan {
            let plan_bytes = std::fs::read(plan_path)
                .map_err(|e| AosError::Io(format!("Failed to read plan file: {}", e)))?;

            trainer.init_kernels(&plan_bytes)?;
            info!(
                "Initialized Metal kernels from plan: {}",
                plan_path.display()
            );
        } else if use_gpu_backward {
            warn!(
                "No plan file provided; GPU backward requires a Metal plan. \
                 Training will fail unless use_gpu_backward=false."
            );
        } else {
            info!("No plan file provided; using CPU proxy training (use_gpu_backward=false)");
        }

        // Train the adapter (with resume if requested)
        let (result, resumed_from_epoch) = if self.resume {
            if let Some(checkpoint) = trainer.try_resume_from_checkpoint().await? {
                info!(
                    "Resuming from checkpoint at epoch {} with config: {}",
                    checkpoint.epoch,
                    checkpoint.config.summary()
                );
                info!("Checkpoint loss at resume: {:.4}", checkpoint.loss);
                let epoch = checkpoint.epoch;
                let result =
                    trainer.train_with_resume_state(&examples, |_| {}, Some(checkpoint)).await?;
                (result, Some(epoch))
            } else {
                info!("No checkpoint found, starting fresh training");
                let result = trainer.train(&examples).await?;
                (result, None)
            }
        } else {
            let result = trainer.train(&examples).await?;
            (result, None)
        };

        // Save the trained adapter
        let determinism_tier = determinism_tier_for_backend(result.backend.as_deref());
        let run_metadata = TrainingRunMetadata {
            dataset_hash_b3: loaded.dataset_hash_b3,
            framing_policy: loaded.framing_policy,
            tokenizer_hash_b3: base_tokenizer_hash,
            training_config_hash,
            determinism_tier: determinism_tier.to_string(),
        };
        self.save_adapter(&result, &run_metadata)?;

        info!(
            "Training completed successfully: adapter_id={}, final_loss={:.4}, time={}ms ({}us)",
            result.adapter_id,
            result.final_loss,
            result.training_time_ms(),
            result.training_time_us
        );
        info!(
            "Checkpoint info: available={}, resumed_from_epoch={}",
            checkpoint_exists,
            resumed_from_epoch.unwrap_or(0)
        );

        Ok(())
    }

    /// Load training configuration
    fn load_config(&self) -> Result<TrainingConfig> {
        if let Some(config_path) = &self.config {
            let config_str = std::fs::read_to_string(config_path)
                .map_err(|e| AosError::Io(format!("Failed to read config file: {}", e)))?;

            let mut config: TrainingConfig = serde_json::from_str(&config_str)
                .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

            config.base_model_path = Some(self.base_model.clone());
            info!(
                "Loaded training configuration from: {}",
                config_path.display()
            );
            Ok(config)
        } else {
            // Use command-line arguments from common struct
            let config = TrainingConfig {
                rank: self.common.rank,
                alpha: self.common.alpha,
                learning_rate: self.common.learning_rate,
                batch_size: self.common.batch_size,
                epochs: self.common.epochs,
                hidden_dim: self.common.hidden_dim,
                vocab_size: 50272,
                training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
                pad_token_id: 0,
                ignore_index: 0,
                max_gpu_memory_mb: 0,
                preferred_backend: None,
                require_gpu: false,
                checkpoint_interval: None,
                warmup_steps: None,
                max_seq_length: None,
                gradient_accumulation_steps: None,
                early_stopping: None,
                patience: None,
                min_delta: None,
                determinism: if self.deterministic || self.seed.is_some() {
                    Some(DeterminismConfig {
                        seed: self.seed,
                        ..Default::default()
                    })
                } else {
                    None
                },
                coreml_placement: None,
                backend_policy: None,
                coreml_fallback_backend: None,
                max_tokens_per_batch: None,
                device_policy: None,
                moe_config: None,
                use_gpu_backward: true,
                optimizer_config: Default::default(),
                base_model_path: Some(self.base_model.clone()),
                hidden_state_layer: None,
                validation_split: 0.0,
                preprocessing: None,
                targets: vec!["q_proj".to_string(), "v_proj".to_string()],
                multi_module_training: false,
                lora_layer_indices: Vec::new(),
            };

            info!("Using command-line training configuration");
            Ok(config)
        }
    }

    /// Load training data from dataset build output (examples.jsonl + DatasetManifest.json).
    fn load_training_data(&self) -> Result<LoadedTrainingData> {
        let (examples_path, manifest_path) = resolve_dataset_paths(&self.data)?;

        let manifest_str = fs::read_to_string(&manifest_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read DatasetManifest.json {}: {}",
                manifest_path.display(),
                e
            ))
        })?;
        let manifest: BuiltDatasetManifest = serde_json::from_str(&manifest_str).map_err(|e| {
            AosError::Parse(format!(
                "Failed to parse DatasetManifest.json {}: {}",
                manifest_path.display(),
                e
            ))
        })?;
        if manifest.training_contract_version != TRAINING_DATA_CONTRACT_VERSION {
            return Err(AosError::Validation(format!(
                "Dataset manifest contract mismatch: expected {}, got {}",
                TRAINING_DATA_CONTRACT_VERSION, manifest.training_contract_version
            )));
        }
        B3Hash::from_hex(&manifest.dataset_hash_b3).map_err(|e| {
            AosError::Validation(format!(
                "Invalid dataset_hash_b3 in DatasetManifest.json: {}",
                e
            ))
        })?;
        B3Hash::from_hex(&manifest.tokenizer_hash_b3).map_err(|e| {
            AosError::Validation(format!(
                "Invalid tokenizer_hash_b3 in DatasetManifest.json: {}",
                e
            ))
        })?;

        let file = File::open(&examples_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to open examples file {}: {}",
                examples_path.display(),
                e
            ))
        })?;
        let reader = BufReader::new(file);
        let mut examples = Vec::new();

        for (idx, line) in reader.lines().enumerate() {
            let line_num = idx + 1;
            let line = line.map_err(|e| {
                AosError::Io(format!(
                    "Failed to read examples line {} in {}: {}",
                    line_num,
                    examples_path.display(),
                    e
                ))
            })?;
            if line.trim().is_empty() {
                return Err(AosError::Validation(format!(
                    "Empty JSONL line {} in {}",
                    line_num,
                    examples_path.display()
                )));
            }
            let example: TrainingExample = serde_json::from_str(&line).map_err(|e| {
                AosError::Parse(format!(
                    "Failed to parse examples line {} in {}: {}",
                    line_num,
                    examples_path.display(),
                    e
                ))
            })?;
            examples.push(example);
        }

        if examples.is_empty() {
            return Err(AosError::Validation(format!(
                "Examples file {} contains no entries",
                examples_path.display()
            )));
        }

        let framing_policy = resolve_framing_policy(&examples)?;

        Ok(LoadedTrainingData {
            examples,
            dataset_hash_b3: manifest.dataset_hash_b3,
            tokenizer_hash_b3: manifest.tokenizer_hash_b3,
            framing_policy,
        })
    }

    /// Save trained adapter
    fn save_adapter(
        &self,
        result: &adapteros_lora_worker::training::TrainingResult,
        metadata: &TrainingRunMetadata,
    ) -> Result<()> {
        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&self.output)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;

        // Save adapter metadata
        let metadata_path = self.output.join("adapter_metadata.json");
        let metadata = serde_json::json!({
            "adapter_id": result.adapter_id,
            "final_loss": result.final_loss,
            "training_time_ms": result.training_time_ms(),
            "training_time_us": result.training_time_us,
            "dataset_hash_b3": metadata.dataset_hash_b3,
            "framing_policy": metadata.framing_policy,
            "tokenizer_hash_b3": metadata.tokenizer_hash_b3,
            "training_config_hash": metadata.training_config_hash,
            "determinism_tier": metadata.determinism_tier,
            "config": {
                "rank": result.weights.lora_a.len(),
                "hidden_dim": result.weights.lora_a[0].len(),
            }
        });

        std::fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)
            .map_err(|e| AosError::Io(format!("Failed to write metadata: {}", e)))?;

        // Save LoRA weights
        let weights_path = self.output.join("lora_weights.json");
        let weights_json =
            serde_json::to_string_pretty(&result.weights).map_err(AosError::Serialization)?;

        std::fs::write(&weights_path, weights_json)
            .map_err(|e| AosError::Io(format!("Failed to write weights: {}", e)))?;

        info!("Saved trained adapter to: {}", self.output.display());
        info!("  Metadata: {}", metadata_path.display());
        info!("  Weights: {}", weights_path.display());

        Ok(())
    }
}

const SCHEMA_SUPERVISED: &str = "supervised";
const SCHEMA_RAW_CONTINUATION: &str = "raw_continuation_v1";

fn resolve_dataset_paths(data_path: &Path) -> Result<(PathBuf, PathBuf)> {
    let (examples_path, manifest_path) = if data_path.is_dir() {
        (
            data_path.join("examples.jsonl"),
            data_path.join("DatasetManifest.json"),
        )
    } else {
        let ext = data_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "jsonl" && ext != "ndjson" {
            return Err(AosError::Validation(
                "Training data must point to a dataset build output directory or examples.jsonl"
                    .to_string(),
            ));
        }
        let parent = data_path.parent().ok_or_else(|| {
            AosError::Validation(format!(
                "Training data path {} has no parent directory",
                data_path.display()
            ))
        })?;
        (data_path.to_path_buf(), parent.join("DatasetManifest.json"))
    };

    if !examples_path.exists() {
        return Err(AosError::Io(format!(
            "Examples file not found: {}",
            examples_path.display()
        )));
    }
    if !manifest_path.exists() {
        return Err(AosError::Io(format!(
            "DatasetManifest.json not found next to training data: {}",
            manifest_path.display()
        )));
    }

    Ok((examples_path, manifest_path))
}

fn resolve_framing_policy(examples: &[TrainingExample]) -> Result<String> {
    let mut schema_mode: Option<String> = None;
    for (idx, example) in examples.iter().enumerate() {
        let provenance: serde_json::Value =
            serde_json::from_str(&example.metadata.provenance).map_err(|e| {
                AosError::Validation(format!(
                    "Invalid provenance JSON at example {}: {}",
                    idx, e
                ))
            })?;
        let schema = provenance
            .get("schema")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AosError::Validation(format!(
                    "Missing schema in provenance for example {}",
                    idx
                ))
            })?;
        if schema != SCHEMA_SUPERVISED && schema != SCHEMA_RAW_CONTINUATION {
            return Err(AosError::Validation(format!(
                "Unsupported schema '{}' in example {}",
                schema, idx
            )));
        }
        if let Some(active) = schema_mode.as_ref() {
            if active != schema {
                return Err(AosError::Validation(format!(
                    "Mixed JSONL schemas detected: expected {}, found {} at example {}",
                    active, schema, idx
                )));
            }
        } else {
            schema_mode = Some(schema.to_string());
        }
    }
    schema_mode.ok_or_else(|| AosError::Validation("No schema detected in examples".to_string()))
}

fn compute_tokenizer_hash(base_model_path: &Path) -> Result<String> {
    let tokenizer_path = base_model_path.join("tokenizer.json");
    if !tokenizer_path.exists() {
        return Err(AosError::Validation(format!(
            "Tokenizer not found at {}",
            tokenizer_path.display()
        )));
    }
    let hash = B3Hash::hash_file(&tokenizer_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to hash tokenizer at {}: {}",
            tokenizer_path.display(),
            e
        ))
    })?;
    Ok(hash.to_hex().to_string())
}

fn compute_training_config_hash(config: &TrainingConfig) -> Result<String> {
    let mut snapshot = config.clone();
    snapshot.base_model_path = None;
    let bytes = serde_json::to_vec(&snapshot).map_err(AosError::Serialization)?;
    Ok(B3Hash::hash(&bytes).to_hex().to_string())
}

fn determinism_tier_for_backend(backend: Option<&str>) -> &'static str {
    let label = backend.unwrap_or("cpu").to_ascii_lowercase();
    match label.as_str() {
        "mlx" | "metal" => "bit_exact",
        "coreml" => "bounded_tolerance",
        "cpu" | "mlxbridge" | "auto" => "none",
        _ => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    #[test]
    fn test_training_config_loading() {
        let temp_dir = new_test_tempdir();
        let config_path = temp_dir.path().join("config.json");

        let mut config = TrainingConfig::default();
        config.rank = 8;
        config.alpha = 32.0;
        config.learning_rate = 0.001;
        config.batch_size = 16;
        config.epochs = 5;
        config.hidden_dim = 1024;
        config.vocab_size = 50272;

        std::fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();

        let args = TrainArgs {
            config: Some(config_path),
            data: PathBuf::from("dummy"),
            output: PathBuf::from("dummy"),
            base_model: PathBuf::from("dummy-model"),
            plan: None,
            deterministic: false,
            seed: None,
            resume: false,
            force_resume: false,
            common: CommonTrainingArgs {
                rank: 4,
                alpha: 16.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
        };

        let loaded_config = args.load_config().unwrap();
        assert_eq!(loaded_config.rank, 8);
        assert_eq!(loaded_config.alpha, 32.0);
        assert_eq!(loaded_config.learning_rate, 0.001);
    }

    #[test]
    fn test_training_data_loading() {
        let temp_dir = new_test_tempdir();
        let data_dir = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let examples_path = data_dir.join("examples.jsonl");
        let manifest_path = data_dir.join("DatasetManifest.json");

        let provenance = serde_json::json!({
            "schema": "supervised",
            "str": "hello",
            "num": "123",
            "bool": "true"
        })
        .to_string();
        let training_examples = vec![
            TrainingExample::new(
                vec![1, 2, 3],
                vec![4, 5, 6],
                TrainingExample::attention_mask_from_tokens(&[1, 2, 3], 0),
                ExampleMetadataV1::new("test", 1, "row-hash-1", "{}", 0),
            ),
            TrainingExample::new(
                vec![7, 8, 9],
                vec![10, 11, 12],
                TrainingExample::attention_mask_from_tokens(&[7, 8, 9], 0),
                ExampleMetadataV1::new("test", 2, "row-hash-2", provenance, 0),
            ),
        ];

        let examples_jsonl = training_examples
            .iter()
            .map(|ex| serde_json::to_string(ex).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&examples_path, format!("{}\n", examples_jsonl)).unwrap();

        let manifest = serde_json::json!({
            "name": "test",
            "version": "1.0",
            "training_contract_version": TRAINING_DATA_CONTRACT_VERSION,
            "tokenizer_hash_b3": B3Hash::hash(b"tokenizer").to_hex(),
            "dataset_hash_b3": B3Hash::hash(b"dataset").to_hex(),
            "build_config": {
                "format": "jsonl",
                "normalization": "none",
                "ordering": "input_hash_asc"
            },
            "sample_count": training_examples.len(),
            "source_files": [],
            "created_at": "now"
        });
        std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let args = TrainArgs {
            config: None,
            data: data_dir,
            output: PathBuf::from("dummy"),
            base_model: PathBuf::from("dummy-model"),
            plan: None,
            deterministic: false,
            seed: None,
            resume: false,
            force_resume: false,
            common: CommonTrainingArgs {
                rank: 4,
                alpha: 16.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
        };

        let loaded = args.load_training_data().unwrap();
        let examples = loaded.examples;
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].input_tokens, vec![1, 2, 3]);
        assert_eq!(examples[0].target_tokens, vec![4, 5, 6]);

        // Non-string metadata must not be silently coerced to empty string.
        // The provenance is stored as a JSON string in ExampleMetadataV1.
        let prov: serde_json::Value =
            serde_json::from_str(&examples[1].metadata.provenance).unwrap();
        assert_eq!(prov.get("str").unwrap().as_str().unwrap(), "hello");
        assert_eq!(prov.get("num").unwrap().as_str().unwrap(), "123");
        assert_eq!(prov.get("bool").unwrap().as_str().unwrap(), "true");
    }
}
