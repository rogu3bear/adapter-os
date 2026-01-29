//! Implementation helpers for AOS commands

use adapteros_aos::single_file::{
    AdapterWeights, TrainingConfig, TrainingExample, WeightGroup, WeightGroupType, WeightMetadata,
};
use adapteros_core::{AosError, Result};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// Load weights from a source path (directory or file)
pub fn load_weights_from_source(source: &PathBuf) -> Result<AdapterWeights> {
    // For this implementation, we load from JSON-formatted weight files
    // The weights files should contain lora_a and lora_b matrices

    if source.is_file() {
        // Load from a single safetensors file
        let data = fs::read(source)
            .map_err(|e| AosError::Io(format!("Failed to read weights file: {}", e)))?;

        // Parse as JSON weight group payload
        let payload: serde_json::Value = serde_json::from_slice(&data)
            .map_err(|e| AosError::Parse(format!("Failed to parse weights: {}", e)))?;

        // Extract LoRA matrices
        let lora_a = payload
            .get("lora_a")
            .and_then(|v| serde_json::from_value::<Vec<Vec<f32>>>(v.clone()).ok())
            .ok_or_else(|| AosError::Parse("Missing lora_a in weights file".to_string()))?;

        let lora_b = payload
            .get("lora_b")
            .and_then(|v| serde_json::from_value::<Vec<Vec<f32>>>(v.clone()).ok())
            .ok_or_else(|| AosError::Parse("Missing lora_b in weights file".to_string()))?;

        // Create weight groups with default metadata
        let positive = WeightGroup {
            lora_a: lora_a.clone(),
            lora_b: lora_b.clone(),
            metadata: WeightMetadata {
                example_count: 0,
                avg_loss: 0.0,
                training_time_ms: 0,
                group_type: WeightGroupType::Positive,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        };

        let negative = WeightGroup {
            lora_a,
            lora_b,
            metadata: WeightMetadata {
                example_count: 0,
                avg_loss: 0.0,
                training_time_ms: 0,
                group_type: WeightGroupType::Negative,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        };

        Ok(AdapterWeights {
            positive,
            negative,
            combined: None,
        })
    } else if source.is_dir() {
        // Load from directory with separate positive/negative files
        let positive_path = source.join("weights_positive.safetensors");
        let negative_path = source.join("weights_negative.safetensors");

        if !positive_path.exists() || !negative_path.exists() {
            return Err(AosError::Config(
                "Directory must contain weights_positive.safetensors and weights_negative.safetensors"
                    .to_string(),
            ));
        }

        let positive = load_weight_group(&positive_path, WeightGroupType::Positive)?;
        let negative = load_weight_group(&negative_path, WeightGroupType::Negative)?;

        // Check for optional combined weights
        let combined_path = source.join("weights_combined.safetensors");
        let combined = if combined_path.exists() {
            Some(load_weight_group(
                &combined_path,
                WeightGroupType::Combined,
            )?)
        } else {
            None
        };

        Ok(AdapterWeights {
            positive,
            negative,
            combined,
        })
    } else {
        Err(AosError::Config(format!(
            "Source path does not exist or is not a file/directory: {}",
            source.display()
        )))
    }
}

/// Load a single weight group from a safetensors file
pub fn load_weight_group(path: &PathBuf, group_type: WeightGroupType) -> Result<WeightGroup> {
    let data = fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read weight group file: {}", e)))?;

    let payload: serde_json::Value = serde_json::from_slice(&data)
        .map_err(|e| AosError::Parse(format!("Failed to parse weight group: {}", e)))?;

    let lora_a = payload
        .get("lora_a")
        .and_then(|v| serde_json::from_value::<Vec<Vec<f32>>>(v.clone()).ok())
        .ok_or_else(|| AosError::Parse("Missing lora_a in weight group".to_string()))?;

    let lora_b = payload
        .get("lora_b")
        .and_then(|v| serde_json::from_value::<Vec<Vec<f32>>>(v.clone()).ok())
        .ok_or_else(|| AosError::Parse("Missing lora_b in weight group".to_string()))?;

    Ok(WeightGroup {
        lora_a,
        lora_b,
        metadata: WeightMetadata {
            example_count: 0,
            avg_loss: 0.0,
            training_time_ms: 0,
            group_type,
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    })
}

/// Load training data from JSONL file
pub fn load_training_data(path: &PathBuf) -> Result<Vec<TrainingExample>> {
    let file = fs::File::open(path)
        .map_err(|e| AosError::Io(format!("Failed to open training data file: {}", e)))?;

    let reader = BufReader::new(file);
    let mut examples = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line =
            line.map_err(|e| AosError::Io(format!("Failed to read line {}: {}", line_num + 1, e)))?;

        if line.trim().is_empty() {
            return Err(AosError::Validation(format!(
                "Empty JSONL line {} in training data",
                line_num + 1
            )));
        }

        let example: TrainingExample = serde_json::from_str(&line).map_err(|e| {
            AosError::Parse(format!("Failed to parse line {}: {}", line_num + 1, e))
        })?;

        examples.push(example);
    }

    Ok(examples)
}

/// Load training config from TOML file
pub fn load_config(path: &PathBuf) -> Result<TrainingConfig> {
    let content = fs::read_to_string(path)
        .map_err(|e| AosError::Io(format!("Failed to read config file: {}", e)))?;

    toml::from_str(&content)
        .map_err(|e| AosError::Parse(format!("Failed to parse config TOML: {}", e)))
}
