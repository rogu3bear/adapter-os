use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{
    load_json_dataset_with_tokenizer, LoRAQuantizer, MicroLoRATrainer, TrainingConfig,
    TrainingExample,
};
use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use tokenizers::Tokenizer;
use tracing::{info, warn};
use walkdir::WalkDir;

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

impl TrainingExampleData {
    fn into_example(self) -> TrainingExample {
        TrainingExample {
            input: self.input,
            target: self.target,
            metadata: self.metadata.unwrap_or_default().into_iter().map(|(k, v)| {
                (k, v.as_str().unwrap_or("").to_string())
            }).collect(),
            weight: 1.0,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .without_time()
        .try_init()
        .ok();

    info!("Starting LoRA training on /Users/star/Dev/jkca directory");

    // Training configuration
    let config = TrainingConfig {
        rank: 16,
        alpha: 32.0,
        learning_rate: 0.0001,
        batch_size: 8,
        epochs: 3,
        hidden_dim: 3584,
        weight_group_config: adapteros_single_file_adapter::format::WeightGroupConfig::default(),
    };

    // Load training data from directory
    let data_path = PathBuf::from("/Users/star/Dev/jkca");
    let examples = load_training_data_from_directory(&data_path).await?;
    info!("Loaded {} training examples", examples.len());

    // Create trainer
    let mut trainer = MicroLoRATrainer::new(config.clone())?;

    // Train the adapter
    let result = trainer.train(&examples).await?;

    // Save the trained adapter
    save_adapter(&result, &PathBuf::from("./jkca_adapter"))?;

    info!(
        "Training completed successfully: adapter_id={}, final_loss={:.4}, time={}ms",
        result.adapter_id, result.final_loss, result.training_time_ms
    );

    // Quantize and package
    package_adapter(&result, &config)?;

    Ok(())
}

async fn load_training_data_from_directory(data_path: &PathBuf) -> Result<Vec<TrainingExample>> {
    info!("Scanning directory for training data: {}", data_path.display());

    let tokenizer_path = PathBuf::from("models/qwen2.5-7b-mlx/tokenizer.json");
    let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to load tokenizer from {}: {}",
            tokenizer_path.display(),
            e
        ))
    })?;

    info!("Loaded tokenizer from: {}", tokenizer_path.display());

    // Scan directory and collect files
    let files = scan_directory(data_path)?;
    info!("Found {} files to process", files.len());

    // Process files into training examples
    let mut examples = Vec::new();
    for file_path in files {
        match process_file_for_training(&file_path, &tokenizer).await {
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
            data_path.display()
        )));
    }

    info!("Generated {} training examples from directory", examples.len());
    Ok(examples)
}

fn scan_directory(data_path: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let max_file_size = 1_048_576; // 1MB

    // Include patterns
    let include_patterns: Vec<&str> = vec!["*.md", "*.txt", "*.rs", "*.py", "*.js", "*.ts", "*.json"];

    // Exclude patterns
    let exclude_patterns: Vec<&str> = vec!["*.log", "*.tmp", "*.lock", ".git/**", "node_modules/**", "target/**"];

    for entry in WalkDir::new(data_path).into_iter().filter_map(|e| e.ok()) {
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

        if metadata.len() > max_file_size as u64 {
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
                pattern == &ext_pattern || pattern == "*"
            } else {
                false
            }
        });

        if should_include {
            files.push(path.to_path_buf());
        }
    }

    Ok(files)
}

async fn process_file_for_training(file_path: &PathBuf, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
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
                examples.extend(process_text_file(&content, file_path, tokenizer)?);
            }
            Some("rs") | Some("py") | Some("js") | Some("ts") => {
                // For code files, create examples from functions/classes
                examples.extend(process_code_file(&content, file_path, tokenizer)?);
            }
            Some("json") => {
                // For JSON files, try to extract structured data
                examples.extend(process_json_file(&content, file_path, tokenizer)?);
            }
            _ => {
                // Default to text processing
                examples.extend(process_text_file(&content, file_path, tokenizer)?);
            }
        }
    }

    Ok(examples)
}

/// Process text/markdown files into training examples
fn process_text_file(content: &str, file_path: &PathBuf, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
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
        let input_text = format!("Document chunk: {}", chunk);
        let target_text = chunk.to_string();

        let input_tokens = tokenizer.encode(&input_text, false)
            .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;
        let target_tokens = tokenizer.encode(&target_text, false)
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
fn process_code_file(content: &str, file_path: &PathBuf, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
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
                if let Ok(example) = create_code_example(&current_block, file_path, tokenizer) {
                    examples.push(example);
                }
            }
            current_block = vec![line];
            in_block = true;
        } else if in_block {
            current_block.push(line);

            // End block on empty line after content
            if trimmed.is_empty() && current_block.len() > 1 {
                if let Ok(example) = create_code_example(&current_block, file_path, tokenizer) {
                    examples.push(example);
                }
                current_block.clear();
                in_block = false;
            }
        }
    }

    // Handle remaining block
    if !current_block.is_empty() {
        if let Ok(example) = create_code_example(&current_block, file_path, tokenizer) {
            examples.push(example);
        }
    }

    // If no blocks found, treat as regular text
    if examples.is_empty() {
        examples.extend(process_text_file(content, file_path, tokenizer)?);
    }

    Ok(examples)
}

/// Create a training example from code block
fn create_code_example(lines: &[&str], file_path: &PathBuf, tokenizer: &Tokenizer) -> Result<TrainingExample> {
    let code_text = lines.join("\n");

    if code_text.trim().len() < 20 {
        return Err(AosError::Training("Code block too short".to_string()));
    }

    let input_text = format!("Code snippet: {}", code_text);
    let target_text = code_text;

    let input_tokens = tokenizer.encode(&input_text, false)
        .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;
    let target_tokens = tokenizer.encode(&target_text, false)
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
fn process_json_file(content: &str, file_path: &PathBuf, tokenizer: &Tokenizer) -> Result<Vec<TrainingExample>> {
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

            let input_tokens = tokenizer.encode(&input_text, false)
                .map_err(|e| AosError::Training(format!("Tokenization failed: {}", e)))?;
            let target_tokens = tokenizer.encode(&target_text, false)
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
            process_text_file(content, file_path, tokenizer)
        }
    }
}

/// Save trained adapter
fn save_adapter(result: &adapteros_lora_worker::training::TrainingResult, output_dir: &PathBuf) -> Result<()> {
    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_dir)
        .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;

    // Save adapter metadata
    let metadata_path = output_dir.join("adapter_metadata.json");
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
    let weights_path = output_dir.join("lora_weights.json");
    let weights_json =
        serde_json::to_string_pretty(&result.weights).map_err(AosError::Serialization)?;

    std::fs::write(&weights_path, weights_json)
        .map_err(|e| AosError::Io(format!("Failed to write weights: {}", e)))?;

    info!("Saved trained adapter to: {}", output_dir.display());
    info!("  Metadata: {}", metadata_path.display());
    info!("  Weights: {}", weights_path.display());

    Ok(())
}

/// Package adapter into .aos format
fn package_adapter(result: &adapteros_lora_worker::training::TrainingResult, config: &TrainingConfig) -> Result<()> {
    use adapteros_lora_worker::training::AdapterPackager;

    // Create adapters directory
    let adapters_root = PathBuf::from("./adapters");
    std::fs::create_dir_all(&adapters_root)?;

    // Quantize weights to Q15
    let quantized = LoRAQuantizer::quantize_to_q15(&result.weights);
    let mse = LoRAQuantizer::calculate_error(&result.weights, &quantized);
    info!("Quantization MSE: {:.6}", mse);

    // Package
    let packager = AdapterPackager::new(&adapters_root);
    let packaged = packager
        .package(&result.adapter_id, &quantized, config, "qwen2.5-7b")
        .map_err(|e| AosError::Io(format!("Packaging failed: {}", e)))?;

    info!(
        "Packaged adapter at {} (hash_b3={})",
        adapters_root.join(&result.adapter_id).display(),
        packaged.hash_b3
    );

    Ok(())
}
