//! Training command implementation

use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::packager::AdapterPackager;
use adapteros_lora_worker::training::{
    load_json_dataset_with_tokenizer, LoRAQuantizer, MicroLoRATrainer, TrainingConfig,
    TrainingExample,
};
use clap::Args;
use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;
use tracing::{info, warn};

/// Train a LoRA adapter
#[derive(Args, Debug, Default)]
pub struct TrainArgs {
    /// Training configuration file (JSON)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Training data file (JSON) or directory to scan for training data
    #[arg(short, long)]
    data: PathBuf,

    /// Output directory for trained adapter
    #[arg(short, long)]
    output: PathBuf,

    /// Plan file for Metal backend initialization
    #[arg(long)]
    plan: Option<PathBuf>,

    /// LoRA rank
    #[arg(long, default_value = "4")]
    rank: usize,

    /// LoRA alpha scaling factor
    #[arg(long, default_value = "16.0")]
    alpha: f32,

    /// Learning rate
    #[arg(long, default_value = "0.0001")]
    learning_rate: f32,

    /// Batch size
    #[arg(long, default_value = "8")]
    batch_size: usize,

    /// Number of epochs
    #[arg(long, default_value = "3")]
    epochs: usize,

    /// Hidden dimension size
    #[arg(long, default_value = "768")]
    hidden_dim: usize,

    /// Enable deterministic training
    #[arg(long)]
    deterministic: bool,

    /// Training seed (for deterministic training)
    #[arg(long)]
    seed: Option<u64>,

    /// Package trained adapter into adapters root with manifest/signature
    #[arg(long)]
    pack: bool,

    /// Adapters root directory (used when --pack is provided)
    #[arg(long, default_value = "./adapters")]
    adapters_root: PathBuf,

    /// Register adapter in the registry database after packaging
    #[arg(long)]
    register: bool,

    /// Base model identifier for the packaged adapter manifest
    #[arg(long, default_value = "qwen2.5-7b")]
    base_model: String,

    /// Adapter ID to use for packaging/registration (defaults to generated)
    #[arg(long)]
    adapter_id: Option<String>,

    /// Registration tier (e.g., ephemeral, persistent); used with --register
    #[arg(long, default_value = "ephemeral")]
    tier: String,

    /// Registration rank; defaults to training rank
    #[arg(long)]
    reg_rank: Option<u32>,

    /// Tokenizer path for text-based training data (defaults to models/qwen2.5-7b-mlx/tokenizer.json)
    #[arg(long)]
    tokenizer: Option<PathBuf>,

    /// Include patterns for directory scanning (e.g., "*.md,*.txt")
    #[arg(long)]
    include_patterns: Option<String>,

    /// Exclude patterns for directory scanning (e.g., "*.log,*.tmp")
    #[arg(long)]
    exclude_patterns: Option<String>,

    /// Maximum file size to process (in bytes, default 1MB)
    #[arg(long, default_value = "1048576")]
    max_file_size: usize,

    /// Maximum number of files to process from directory
    #[arg(long)]
    max_files: Option<usize>,
}

/// Training data format
#[derive(serde::Deserialize, serde::Serialize)]
pub struct TrainingData {
    examples: Vec<TrainingExampleData>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct TrainingExampleData {
    input: Vec<u32>,
    target: Vec<u32>,
    metadata: Option<HashMap<String, serde_json::Value>>,
}

impl TrainArgs {
    /// Execute the training command
    pub async fn execute(&self) -> Result<()> {
        info!("Starting LoRA training with Rust-native implementation");

        if self.register && !self.pack {
            return Err(AosError::Validation(
                "--register requires --pack to produce adapter artifacts".to_string(),
            ));
        }

        // Load training configuration
        let config = self.load_config()?;

        // Load training data
        let examples = self.load_training_data()?;
        info!("Loaded {} training examples", examples.len());

        // Create trainer
        let mut trainer = MicroLoRATrainer::new(config.clone())?;

        if let Some(seed) = self.resolved_seed(&config)? {
            trainer.override_training_seed(seed)?;
            info!("Using deterministic training seed {}", seed);
        }

        // Initialize Metal kernels if plan is provided
        if let Some(plan_path) = &self.plan {
            let plan_bytes = std::fs::read(plan_path)
                .map_err(|e| AosError::Io(format!("Failed to read plan file: {}", e)))?;

            trainer.init_kernels(&plan_bytes)?;
            info!(
                "Initialized Metal kernels from plan: {}",
                plan_path.display()
            );
        } else {
            warn!("No plan file provided, training will use CPU-only mode");
        }

        // Train the adapter
        let result = trainer.train(&examples).await?;

        // Save the trained adapter (legacy outputs for compatibility)
        self.save_adapter(&result)?;

        info!(
            "Training completed successfully: adapter_id={}, final_loss={:.4}, time={}ms",
            result.adapter_id, result.final_loss, result.training_time_ms
        );

        // Optional: package and register
        if self.pack {
            std::fs::create_dir_all(&self.adapters_root).map_err(|e| {
                AosError::Io(format!(
                    "Failed to ensure adapters root {}: {}",
                    self.adapters_root.display(),
                    e
                ))
            })?;

            // Quantize weights to Q15
            let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
            let mse = LoRAQuantizer::calculate_error(&result.weights, &quantized);
            info!("Quantization MSE: {:.6}", mse);

            // Determine adapter_id
            let adapter_id = self
                .adapter_id
                .clone()
                .unwrap_or_else(|| result.adapter_id.clone());

            // Package
            let packager = AdapterPackager::new(&self.adapters_root);
            let packaged = packager
                .package(&adapter_id, &quantized, &config, &self.base_model)
                .await
                .map_err(|e| AosError::Io(format!("Packaging failed: {}", e)))?;

            info!(
                "Packaged adapter at {} (hash_b3={})",
                self.adapters_root.join(&adapter_id).display(),
                packaged.hash_b3
            );

            // Optional register into DB via existing CLI helper
            if self.register {
                let reg_rank = self.reg_rank.unwrap_or(self.rank as u32);
                // Reuse existing register command (DB-backed)
                crate::commands::register_adapter::run(
                    &adapter_id,
                    &packaged.hash_b3,
                    &self.tier,
                    reg_rank,
                    // Respect current output mode by inheriting JSON/verbosity from environment flags
                    &crate::output::OutputWriter::new(crate::output::OutputMode::from_env(), false),
                )
                .await
                .map_err(|e| AosError::Io(format!("Registration failed: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Load training configuration
    fn load_config(&self) -> Result<TrainingConfig> {
        if let Some(config_path) = &self.config {
            let config_str = std::fs::read_to_string(config_path)
                .map_err(|e| AosError::Io(format!("Failed to read config file: {}", e)))?;

            let config: TrainingConfig = serde_json::from_str(&config_str)
                .map_err(|e| AosError::Parse(format!("Failed to parse config: {}", e)))?;

            info!(
                "Loaded training configuration from: {}",
                config_path.display()
            );
            self.validate_training_config(&config)?;
            Ok(config)
        } else {
            // Use command-line arguments
            let config = TrainingConfig {
                rank: self.rank,
                alpha: self.alpha,
                learning_rate: self.learning_rate,
                batch_size: self.batch_size,
                epochs: self.epochs,
                hidden_dim: self.hidden_dim,
                weight_group_config:
                    adapteros_single_file_adapter::format::WeightGroupConfig::default(),
            };

            info!("Using command-line training configuration");
            self.validate_training_config(&config)?;
            Ok(config)
        }
    }

    /// Load training data with auto-detection of format (text-based vs pre-tokenized) or scan directory
    fn load_training_data(&self) -> Result<Vec<TrainingExample>> {
        // Check if data path is a directory
        if self.data.is_dir() {
            return self.load_training_data_from_directory();
        }

        // Handle single file as before
        let data_str = std::fs::read_to_string(&self.data)
            .map_err(|e| AosError::Io(format!("Failed to read training data: {}", e)))?;

        // Try to detect format by parsing JSON structure
        let json_value: serde_json::Value = serde_json::from_str(&data_str)
            .map_err(|e| AosError::Parse(format!("Failed to parse training data JSON: {}", e)))?;

        // Check if this is a text-based format (has "name" field and examples with object/string inputs)
        let is_text_format = json_value.get("name").and_then(|v| v.as_str()).is_some()
            && json_value
                .get("examples")
                .and_then(|v| v.as_array())
                .map(|exs| {
                    // Check if at least one example has input as object or string (text format)
                    exs.iter().any(|ex| {
                        ex.get("input")
                            .map_or(false, |input| input.is_object() || input.is_string())
                    })
                })
                .unwrap_or(false);

        if is_text_format {
            // Use JSON loader for text-based format
            info!("Detected text-based training data format, using JSON loader with tokenization");

            let tokenizer_path = self
                .tokenizer
                .clone()
                .unwrap_or_else(|| PathBuf::from("models/qwen2.5-7b-mlx/tokenizer.json"));

            if !tokenizer_path.exists() {
                return Err(AosError::Io(format!(
                    "Tokenizer file not found: {}. Please specify --tokenizer or ensure default path exists",
                    tokenizer_path.display()
                )));
            }

            let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to load tokenizer from {}: {}",
                    tokenizer_path.display(),
                    e
                ))
            })?;

            info!("Loaded tokenizer from: {}", tokenizer_path.display());

            let examples =
                load_json_dataset_with_tokenizer(&self.data, &tokenizer).map_err(|e| {
                    AosError::Training(format!("Failed to load text-based dataset: {}", e))
                })?;

            info!(
                "Successfully loaded {} examples from text-based dataset",
                examples.len()
            );
            Ok(examples)
        } else {
            // Use existing pre-tokenized loader
            info!("Detected pre-tokenized training data format, using legacy loader");

            let training_data: TrainingData = serde_json::from_value(json_value).map_err(|e| {
                AosError::Parse(format!(
                    "Failed to parse pre-tokenized training data: {}",
                    e
                ))
            })?;

            self.validate_training_dataset(&training_data)?;

            let examples: Vec<TrainingExample> = training_data
                .examples
                .into_iter()
                .map(|ex| {
                    let metadata = ex
                        .metadata
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(k, v)| {
                            let k = k.clone();
                            v.as_str()
                                .map(|value| (k.clone(), value.to_string()))
                                .ok_or_else(|| {
                                    AosError::Validation(format!(
                                        "Metadata value for key '{}' must be a string",
                                        k
                                    ))
                                })
                        })
                        .collect::<Result<HashMap<_, _>>>()?;

                    Ok(TrainingExample {
                        input: ex.input,
                        target: ex.target,
                        metadata,
                        weight: 1.0,
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            info!(
                "Successfully loaded {} examples from pre-tokenized dataset",
                examples.len()
            );
            Ok(examples)
        }
    }

    /// Load training data by scanning a directory and processing files
    fn load_training_data_from_directory(&self) -> Result<Vec<TrainingExample>> {
        info!("Scanning directory for training data: {}", self.data.display());

        let tokenizer_path = self
            .tokenizer
            .clone()
            .unwrap_or_else(|| PathBuf::from("models/qwen2.5-7b-mlx/tokenizer.json"));

        if !tokenizer_path.exists() {
            return Err(AosError::Io(format!(
                "Tokenizer file not found: {}. Please specify --tokenizer or ensure default path exists",
                tokenizer_path.display()
            )));
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to load tokenizer from {}: {}",
                tokenizer_path.display(),
                e
            ))
        })?;

        info!("Loaded tokenizer from: {}", tokenizer_path.display());

        // Scan directory and collect files
        let files = self.scan_directory()?;
        info!("Found {} files to process", files.len());

        // Process files into training examples
        let mut examples = Vec::new();
        for file_path in files {
            match self.process_file_for_training(&file_path, &tokenizer) {
                Ok(file_examples) => {
                    examples.extend(file_examples);
                }
                Err(e) => {
                    warn!("Failed to process file {}: {}", file_path.display(), e);
                    continue;
                }
            }
        }

        if examples.is_empty() {
            return Err(AosError::Training(format!(
                "No training examples generated from directory scan of {}",
                self.data.display()
            )));
        }

        info!("Generated {} training examples from directory", examples.len());
        Ok(examples)
    }

    fn validate_training_config(&self, config: &TrainingConfig) -> Result<()> {
        if config.rank == 0 {
            return Err(AosError::Validation(
                "Training rank must be greater than zero".to_string(),
            ));
        }
        if config.batch_size == 0 {
            return Err(AosError::Validation(
                "Batch size must be greater than zero".to_string(),
            ));
        }
        if config.epochs == 0 {
            return Err(AosError::Validation(
                "Epochs must be greater than zero".to_string(),
            ));
        }
        if config.hidden_dim == 0 {
            return Err(AosError::Validation(
                "Hidden dimension must be greater than zero".to_string(),
            ));
        }
        if config.learning_rate <= 0.0 {
            return Err(AosError::Validation(
                "Learning rate must be greater than zero".to_string(),
            ));
        }
        if config.alpha <= 0.0 {
            return Err(AosError::Validation(
                "Alpha must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_training_dataset(&self, data: &TrainingData) -> Result<()> {
        if data.examples.is_empty() {
            return Err(AosError::Validation(
                "Training dataset must include at least one example".to_string(),
            ));
        }

        for (idx, ex) in data.examples.iter().enumerate() {
            if ex.input.is_empty() {
                return Err(AosError::Validation(format!(
                    "Example {} has empty input sequence",
                    idx
                )));
            }
            if ex.target.is_empty() {
                return Err(AosError::Validation(format!(
                    "Example {} has empty target sequence",
                    idx
                )));
            }
        }

        Ok(())
    }

    fn resolved_seed(&self, config: &TrainingConfig) -> Result<Option<u64>> {
        if let Some(explicit) = self.seed {
            return Ok(Some(explicit));
        }

        if !self.deterministic {
            return Ok(None);
        }

        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(self.data.to_string_lossy().as_bytes());
        hasher.update(&config.rank.to_le_bytes());
        hasher.update(&config.alpha.to_le_bytes());
        hasher.update(&config.learning_rate.to_le_bytes());
        hasher.update(&config.batch_size.to_le_bytes());
        hasher.update(&config.epochs.to_le_bytes());
        hasher.update(&config.hidden_dim.to_le_bytes());
        let hash = hasher.finalize();
        let mut seed_bytes = [0u8; 8];
        seed_bytes.copy_from_slice(&hash.as_bytes()[..8]);

        Ok(Some(u64::from_le_bytes(seed_bytes)))
    }

    /// Save trained adapter
    fn save_adapter(&self, result: &adapteros_lora_worker::training::TrainingResult) -> Result<()> {
        // Create output directory if it doesn't exist
        std::fs::create_dir_all(&self.output)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;

        // Save adapter metadata
        let metadata_path = self.output.join("adapter_metadata.json");
        let metadata = serde_json::json!({
            "adapter_id": result.adapter_id,
            "final_loss": result.final_loss,
            "training_time_ms": result.training_time_ms,
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

    /// Scan directory for files to process for training
    fn scan_directory(&self) -> Result<Vec<PathBuf>> {
        use walkdir::WalkDir;

        let mut files = Vec::new();
        let mut processed_count = 0;

        // Parse include/exclude patterns
        let include_patterns: Vec<&str> = self.include_patterns
            .as_ref()
            .map(|s| s.split(',').map(|s| s.trim()).collect())
            .unwrap_or_else(|| vec!["*.md", "*.txt", "*.rs", "*.py", "*.js", "*.ts", "*.json"]);

        let exclude_patterns: Vec<&str> = self.exclude_patterns
            .as_ref()
            .map(|s| s.split(',').map(|s| s.trim()).collect())
            .unwrap_or_else(|| vec!["*.log", "*.tmp", "*.lock", ".git/**", "node_modules/**", "target/**"]);

        for entry in WalkDir::new(&self.data).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Check file size limit
            let metadata = match path.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            if metadata.len() > self.max_file_size as u64 {
                continue;
            }

            // Check exclude patterns
            let path_str = path.to_string_lossy();
            let should_exclude = exclude_patterns.iter().any(|pattern| {
                if pattern.ends_with("/**") {
                    let dir_pattern = &pattern[..pattern.len() - 3];
                    path_str.contains(dir_pattern)
                } else {
                    path_str.contains(pattern.trim_start_matches('*'))
                }
            });

            if should_exclude {
                continue;
            }

            // Check include patterns
            let should_include = include_patterns.iter().any(|pattern| {
                if let Some(ext) = path.extension() {
                    let ext_pattern = format!("*.{}", ext.to_string_lossy());
                    pattern == &ext_pattern || *pattern == "*"
                } else {
                    false
                }
            });

            if should_include {
                files.push(path.to_path_buf());

                processed_count += 1;
                if let Some(max_files) = self.max_files {
                    if processed_count >= max_files {
                        break;
                    }
                }
            }
        }

        Ok(files)
    }

    /// Process a single file into training examples
    fn process_file_for_training(&self, file_path: &Path, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
        use std::fs;

        let content = fs::read_to_string(file_path)
            .map_err(|e| AosError::Io(format!("Failed to read file {}: {}", file_path.display(), e)))?;

        if content.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut examples = Vec::new();

        // Determine file type and process accordingly
        if let Some(ext) = file_path.extension() {
            match ext.to_str() {
                Some("md") | Some("txt") => {
                    // For text/markdown files, create examples from content chunks
                    examples.extend(self.process_text_file(&content, file_path, tokenizer)?);
                }
                Some("rs") | Some("py") | Some("js") | Some("ts") => {
                    // For code files, create examples from functions/classes
                    examples.extend(self.process_code_file(&content, file_path, tokenizer)?);
                }
                Some("json") => {
                    // For JSON files, try to extract structured data
                    examples.extend(self.process_json_file(&content, file_path, tokenizer)?);
                }
                _ => {
                    // Default to text processing
                    examples.extend(self.process_text_file(&content, file_path, tokenizer)?);
                }
            }
        }

        Ok(examples)
    }

    /// Process text/markdown files into training examples
    fn process_text_file(&self, content: &str, file_path: &Path, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
        let mut examples = Vec::new();

        // Split content into chunks (simple approach: by paragraphs or fixed size)
        let chunks: Vec<&str> = content
            .split("\n\n")
            .filter(|chunk| !chunk.trim().is_empty())
            .collect();

        for (i, chunk) in chunks.iter().enumerate() {
            if chunk.trim().len() < 10 {
                continue; // Skip very short chunks
            }

            // Create input/output pair (simple approach: use chunk as both input and target for now)
            // In a real implementation, you'd want to create more sophisticated training pairs
            let input_text = format!("Document chunk: {}", chunk);
            let target_text = chunk.to_string();

            let input_tokens = tokenizer.encode(input_text.as_str(), false)
                .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;
            let target_tokens = tokenizer.encode(target_text.as_str(), false)
                .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;

            if input_tokens.is_empty() || target_tokens.is_empty() {
                continue;
            }

            let mut metadata = HashMap::new();
            metadata.insert("source_file".to_string(), file_path.to_string_lossy().to_string());
            metadata.insert("chunk_index".to_string(), i.to_string());
            metadata.insert("file_type".to_string(), "text".to_string());

            examples.push(TrainingExample {
                input: input_tokens.get_ids().to_vec(),
                target: target_tokens.get_ids().to_vec(),
                metadata,
                weight: 1.0,
            });
        }

        Ok(examples)
    }

    /// Process code files into training examples
    fn process_code_file(&self, content: &str, file_path: &Path, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
        let mut examples = Vec::new();

        // Simple approach: split by functions/classes (very basic)
        let lines: Vec<&str> = content.lines().collect();
        let mut current_block = Vec::new();
        let mut in_block = false;

        for line in lines {
            let trimmed = line.trim();

            // Simple heuristic for function/class starts
            if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") ||
               trimmed.starts_with("class ") || trimmed.starts_with("function ") ||
               trimmed.starts_with("def ") {
                // Save previous block if it exists
                if !current_block.is_empty() {
                    if let Ok(example) = self.create_code_example(&current_block, file_path, tokenizer) {
                        examples.push(example);
                    }
                }
                current_block = vec![line];
                in_block = true;
            } else if in_block {
                current_block.push(line);

                // End block on empty line after content
                if trimmed.is_empty() && current_block.len() > 1 {
                    if let Ok(example) = self.create_code_example(&current_block, file_path, tokenizer) {
                        examples.push(example);
                    }
                    current_block.clear();
                    in_block = false;
                }
            }
        }

        // Handle remaining block
        if !current_block.is_empty() {
            if let Ok(example) = self.create_code_example(&current_block, file_path, tokenizer) {
                examples.push(example);
            }
        }

        // If no blocks found, treat as regular text
        if examples.is_empty() {
            examples.extend(self.process_text_file(content, file_path, tokenizer)?);
        }

        Ok(examples)
    }

    /// Create a training example from code block
    fn create_code_example(&self, lines: &[&str], file_path: &Path, tokenizer: &Tokenizer) -> Result<TrainingExample> {
        let code_text = lines.join("\n");

        if code_text.trim().len() < 20 {
            return Err(AosError::Training("Code block too short".to_string()));
        }

        let input_text = format!("Code snippet: {}", code_text);
        let target_text = code_text;

        let input_tokens = tokenizer.encode(input_text.as_str(), false)
            .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;
        let target_tokens = tokenizer.encode(target_text.as_str(), false)
            .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;

        if input_tokens.is_empty() || target_tokens.is_empty() {
            return Err(AosError::Training("Empty tokens after encoding".to_string()));
        }

        let mut metadata = HashMap::new();
        metadata.insert("source_file".to_string(), file_path.to_string_lossy().to_string());
        metadata.insert("file_type".to_string(), "code".to_string());
        metadata.insert("language".to_string(), file_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_string());

        Ok(TrainingExample {
            input: input_tokens.get_ids().to_vec(),
            target: target_tokens.get_ids().to_vec(),
            metadata,
            weight: 1.0,
        })
    }

    /// Process JSON files into training examples
    fn process_json_file(&self, content: &str, file_path: &Path, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
        // Try to parse as JSON and create examples from key-value pairs or structure
        match serde_json::from_str::<serde_json::Value>(content) {
            Ok(json_value) => {
                let mut examples = Vec::new();

                // Simple approach: flatten JSON structure into text
                let json_text = serde_json::to_string_pretty(&json_value)
                    .unwrap_or_else(|_| content.to_string());

                // Create example from JSON structure
                let input_text = format!("JSON data: {}", json_text);
                let target_text = json_text;

                let input_tokens = tokenizer.encode(input_text.as_str(), false)
                    .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;
                let target_tokens = tokenizer.encode(target_text.as_str(), false)
                    .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;

                if !input_tokens.is_empty() && !target_tokens.is_empty() {
                    let mut metadata = HashMap::new();
                    metadata.insert("source_file".to_string(), file_path.to_string_lossy().to_string());
                    metadata.insert("file_type".to_string(), "json".to_string());

                    examples.push(TrainingExample {
                        input: input_tokens.get_ids().to_vec(),
                        target: target_tokens.get_ids().to_vec(),
                        metadata,
                        weight: 1.0,
                    });
                }

                Ok(examples)
            }
            Err(_) => {
                // If not valid JSON, treat as text
                self.process_text_file(content, file_path, tokenizer)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tokio::runtime::Runtime;

    #[test]
    fn test_training_config_loading() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let config = TrainingConfig {
            rank: 8,
            alpha: 32.0,
            learning_rate: 0.001,
            batch_size: 16,
            epochs: 5,
            hidden_dim: 1024,
            weight_group_config: adapteros_single_file_adapter::format::WeightGroupConfig::default(
            ),
        };

        std::fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();

        let args = TrainArgs {
            config: Some(config_path),
            data: PathBuf::from("dummy"),
            output: PathBuf::from("dummy"),
            plan: None,
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            deterministic: false,
            seed: None,
            base_model: "qwen2.5-7b".to_string(),
            ..Default::default()
        };

        let loaded_config = args.load_config().unwrap();
        assert_eq!(loaded_config.rank, 8);
        assert_eq!(loaded_config.alpha, 32.0);
        assert_eq!(loaded_config.learning_rate, 0.001);
    }

    #[test]
    fn test_training_data_loading() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path().join("data.json");

        let training_data = TrainingData {
            examples: vec![
                TrainingExampleData {
                    input: vec![1, 2, 3],
                    target: vec![4, 5, 6],
                    metadata: None,
                },
                TrainingExampleData {
                    input: vec![7, 8, 9],
                    target: vec![10, 11, 12],
                    metadata: Some(HashMap::new()),
                },
            ],
        };

        std::fs::write(&data_path, serde_json::to_string(&training_data).unwrap()).unwrap();

        let args = TrainArgs {
            config: None,
            data: data_path,
            output: PathBuf::from("dummy"),
            plan: None,
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            deterministic: false,
            seed: None,
            base_model: "qwen2.5-7b".to_string(),
            ..Default::default()
        };

        let examples = args.load_training_data().unwrap();
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].input, vec![1, 2, 3]);
        assert_eq!(examples[0].target, vec![4, 5, 6]);
    }

    #[test]
    fn test_training_data_metadata_validation() {
        let temp_dir = TempDir::new().unwrap();
        let data_path = temp_dir.path().join("data.json");

        let mut metadata = HashMap::new();
        metadata.insert("notes".to_string(), serde_json::json!({"nested": "value"}));

        let training_data = TrainingData {
            examples: vec![TrainingExampleData {
                input: vec![1, 2, 3],
                target: vec![1, 2, 3],
                metadata: Some(metadata),
            }],
        };

        std::fs::write(&data_path, serde_json::to_string(&training_data).unwrap()).unwrap();

        let args = TrainArgs {
            config: None,
            data: data_path,
            output: PathBuf::from("dummy"),
            plan: None,
            rank: 4,
            alpha: 16.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
            deterministic: false,
            seed: None,
            base_model: "qwen2.5-7b".to_string(),
            ..Default::default()
        };

        let err = args.load_training_data().unwrap_err();
        assert!(
            format!("{err}").contains("Metadata value for key 'notes' must be a string"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_register_requires_pack() {
        let args = TrainArgs {
            register: true,
            pack: false,
            data: PathBuf::from("dummy"),
            output: PathBuf::from("dummy"),
            base_model: "qwen2.5-7b".to_string(),
            ..Default::default()
        };

        let rt = Runtime::new().unwrap();
        let err = rt.block_on(args.execute()).unwrap_err();
        assert!(
            format!("{err}").contains("--register requires --pack"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_resolved_seed_deterministic_flag() {
        let args = TrainArgs {
            deterministic: true,
            data: PathBuf::from("dataset.json"),
            base_model: "qwen2.5-7b".to_string(),
            ..Default::default()
        };

        let config = TrainingConfig::default();
        let seed = args.resolved_seed(&config).unwrap().unwrap();

        let seed_again = args.resolved_seed(&config).unwrap().unwrap();
        assert_eq!(seed, seed_again);
    }
}
