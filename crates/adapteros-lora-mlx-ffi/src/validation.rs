//! Input validation and pre-flight checks for MLX operations

use crate::error::MlxError;
use crate::memory;

/// Validate tensor shapes match expected dimensions
pub fn validate_shape(actual: &[usize], expected: &[usize], context: &str) -> Result<(), MlxError> {
    if actual.len() != expected.len() {
        return Err(MlxError::ShapeMismatch {
            expected: expected.to_vec(),
            actual: actual.to_vec(),
            context: format!("{}: dimension count mismatch", context),
        });
    }

    for (i, (&a, &e)) in actual.iter().zip(expected.iter()).enumerate() {
        if a != e && e != 0 {
            // 0 in expected means "any size"
            return Err(MlxError::ShapeMismatch {
                expected: expected.to_vec(),
                actual: actual.to_vec(),
                context: format!("{}: dimension {} mismatch", context, i),
            });
        }
    }

    Ok(())
}

/// Validate shapes are compatible for matrix multiplication
pub fn validate_matmul_shapes(
    a_shape: &[usize],
    b_shape: &[usize],
    context: &str,
) -> Result<(), MlxError> {
    if a_shape.len() < 2 || b_shape.len() < 2 {
        return Err(MlxError::ValidationError {
            check: "matmul_dimensions".to_string(),
            reason: format!(
                "Matrices must have at least 2 dimensions, got {} and {}",
                a_shape.len(),
                b_shape.len()
            ),
        });
    }

    let a_cols = a_shape[a_shape.len() - 1];
    let b_rows = b_shape[b_shape.len() - 2];

    if a_cols != b_rows {
        return Err(MlxError::ShapeMismatch {
            expected: vec![a_cols],
            actual: vec![b_rows],
            context: format!("{}: inner dimensions must match for matmul", context),
        });
    }

    Ok(())
}

/// Validate tensor shapes are compatible for broadcasting
pub fn validate_broadcastable(
    shape1: &[usize],
    shape2: &[usize],
    context: &str,
) -> Result<(), MlxError> {
    let max_dims = shape1.len().max(shape2.len());

    for i in 0..max_dims {
        let dim1 = if i < shape1.len() {
            shape1[shape1.len() - 1 - i]
        } else {
            1
        };
        let dim2 = if i < shape2.len() {
            shape2[shape2.len() - 1 - i]
        } else {
            1
        };

        if dim1 != dim2 && dim1 != 1 && dim2 != 1 {
            return Err(MlxError::ShapeMismatch {
                expected: shape1.to_vec(),
                actual: shape2.to_vec(),
                context: format!("{}: shapes not broadcastable", context),
            });
        }
    }

    Ok(())
}

/// Validate memory availability before allocation
pub fn validate_memory_available(
    required_mb: f32,
    threshold_mb: f32,
    context: &str,
) -> Result<(), MlxError> {
    let current_usage = memory::memory_usage();
    let current_mb = memory::bytes_to_mb(current_usage);

    if current_mb + required_mb > threshold_mb {
        return Err(MlxError::AllocationFailed {
            size_mb: required_mb,
            total_allocated_mb: current_mb,
            hint: format!(
                "Current usage: {:.2}MB, requested: {:.2}MB, threshold: {:.2}MB. Consider: \
                 1) Reduce batch size, 2) Unload unused adapters, 3) Call memory::gc_collect()",
                current_mb, required_mb, threshold_mb
            ),
        });
    }

    tracing::debug!(
        context = %context,
        required_mb = %required_mb,
        current_mb = %current_mb,
        threshold_mb = %threshold_mb,
        "Memory validation passed"
    );

    Ok(())
}

/// Validate LoRA configuration parameters
pub fn validate_lora_config(rank: usize, alpha: f32, dropout: f32) -> Result<(), MlxError> {
    if rank == 0 {
        return Err(MlxError::ConfigError {
            field: "rank".to_string(),
            reason: "rank must be greater than 0".to_string(),
        });
    }

    if rank > 256 {
        return Err(MlxError::ConfigError {
            field: "rank".to_string(),
            reason: format!("rank {} is unusually high, recommended maximum is 256", rank),
        });
    }

    if alpha <= 0.0 {
        return Err(MlxError::ConfigError {
            field: "alpha".to_string(),
            reason: "alpha must be positive".to_string(),
        });
    }

    if !(0.0..=1.0).contains(&dropout) {
        return Err(MlxError::ConfigError {
            field: "dropout".to_string(),
            reason: "dropout must be between 0.0 and 1.0".to_string(),
        });
    }

    Ok(())
}

/// Validate gate weights (Q15 format)
pub fn validate_gates_q15(gates: &[u16], num_adapters: usize) -> Result<(), MlxError> {
    if gates.len() != num_adapters {
        return Err(MlxError::ValidationError {
            check: "gate_count".to_string(),
            reason: format!(
                "Gate count ({}) must match adapter count ({})",
                gates.len(),
                num_adapters
            ),
        });
    }

    // Q15 range: 0..=32767 (not 32768 for symmetric range)
    for (i, &gate) in gates.iter().enumerate() {
        if gate > 32767 {
            return Err(MlxError::ValidationError {
                check: "gate_value".to_string(),
                reason: format!(
                    "Gate {} value {} exceeds Q15 maximum (32767)",
                    i, gate
                ),
            });
        }
    }

    Ok(())
}

/// Validate adapter ID is within valid range
pub fn validate_adapter_id(adapter_id: u16) -> Result<(), MlxError> {
    // Reserve ID 0 for base model (no adapter)
    if adapter_id == 0 {
        return Err(MlxError::ValidationError {
            check: "adapter_id".to_string(),
            reason: "Adapter ID 0 is reserved for base model".to_string(),
        });
    }

    // Maximum K=8 adapters, plus reasonable buffer
    if adapter_id > 1024 {
        return Err(MlxError::ValidationError {
            check: "adapter_id".to_string(),
            reason: format!(
                "Adapter ID {} exceeds maximum (1024)",
                adapter_id
            ),
        });
    }

    Ok(())
}

/// Validate model configuration
pub fn validate_model_config(
    hidden_size: usize,
    num_layers: usize,
    num_heads: usize,
    vocab_size: usize,
) -> Result<(), MlxError> {
    if hidden_size == 0 {
        return Err(MlxError::ConfigError {
            field: "hidden_size".to_string(),
            reason: "hidden_size must be greater than 0".to_string(),
        });
    }

    if hidden_size % num_heads != 0 {
        return Err(MlxError::ConfigError {
            field: "hidden_size".to_string(),
            reason: format!(
                "hidden_size ({}) must be divisible by num_attention_heads ({})",
                hidden_size, num_heads
            ),
        });
    }

    if num_layers == 0 {
        return Err(MlxError::ConfigError {
            field: "num_layers".to_string(),
            reason: "num_layers must be greater than 0".to_string(),
        });
    }

    if vocab_size == 0 {
        return Err(MlxError::ConfigError {
            field: "vocab_size".to_string(),
            reason: "vocab_size must be greater than 0".to_string(),
        });
    }

    Ok(())
}

/// Validate token IDs are within vocabulary range
pub fn validate_token_ids(token_ids: &[u32], vocab_size: usize) -> Result<(), MlxError> {
    for (i, &token_id) in token_ids.iter().enumerate() {
        if token_id as usize >= vocab_size {
            return Err(MlxError::ValidationError {
                check: "token_id_range".to_string(),
                reason: format!(
                    "Token ID {} at position {} exceeds vocabulary size ({})",
                    token_id, i, vocab_size
                ),
            });
        }
    }

    Ok(())
}

/// Validate input is not empty
pub fn validate_non_empty<T>(data: &[T], field: &str) -> Result<(), MlxError> {
    if data.is_empty() {
        return Err(MlxError::ValidationError {
            check: format!("{}_empty", field),
            reason: format!("{} cannot be empty", field),
        });
    }
    Ok(())
}

/// Validate floating point values are finite (not NaN or infinite)
pub fn validate_finite(value: f32, field: &str) -> Result<(), MlxError> {
    if !value.is_finite() {
        return Err(MlxError::ValidationError {
            check: format!("{}_finite", field),
            reason: format!("{} must be finite (got: {})", field, value),
        });
    }
    Ok(())
}

/// Validate all values in slice are finite
pub fn validate_all_finite(values: &[f32], field: &str) -> Result<(), MlxError> {
    for (i, &value) in values.iter().enumerate() {
        if !value.is_finite() {
            return Err(MlxError::ValidationError {
                check: format!("{}_finite", field),
                reason: format!("{} at index {} must be finite (got: {})", field, i, value),
            });
        }
    }
    Ok(())
}

/// Pre-flight checks for model loading
pub struct ModelLoadChecks {
    pub path_exists: bool,
    pub has_config: bool,
    pub has_weights: bool,
    pub config_valid: bool,
    pub estimated_memory_mb: Option<f32>,
}

impl ModelLoadChecks {
    pub fn run(model_path: &std::path::Path) -> Self {
        let path_exists = model_path.exists();
        let config_path = model_path.join("config.json");
        let has_config = config_path.exists();

        // Check for weight files
        let safetensors_path = model_path.join("model.safetensors");
        let alt_path = model_path.join("pytorch_model.bin.safetensors");
        let has_weights = safetensors_path.exists() || alt_path.exists();

        // Try to validate config
        let config_valid = if has_config {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .is_some()
        } else {
            false
        };

        // Estimate memory requirements (very rough)
        let estimated_memory_mb = if has_weights {
            safetensors_path
                .metadata()
                .ok()
                .or_else(|| alt_path.metadata().ok())
                .map(|m| memory::bytes_to_mb(m.len() as usize))
        } else {
            None
        };

        Self {
            path_exists,
            has_config,
            has_weights,
            config_valid,
            estimated_memory_mb,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.path_exists && self.has_config && self.has_weights && self.config_valid
    }

    pub fn errors(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if !self.path_exists {
            errors.push("Model path does not exist".to_string());
        }
        if !self.has_config {
            errors.push("config.json not found".to_string());
        }
        if !self.has_weights {
            errors.push("No weight files found (model.safetensors or pytorch_model.bin.safetensors)".to_string());
        }
        if !self.config_valid {
            errors.push("config.json is malformed or invalid".to_string());
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_shape() {
        assert!(validate_shape(&[2, 2], &[2, 2], "test").is_ok());
        assert!(validate_shape(&[2, 3], &[2, 2], "test").is_err());
        assert!(validate_shape(&[2, 2, 2], &[2, 2], "test").is_err());

        // 0 means "any size"
        assert!(validate_shape(&[2, 3], &[2, 0], "test").is_ok());
    }

    #[test]
    fn test_validate_matmul_shapes() {
        assert!(validate_matmul_shapes(&[2, 3], &[3, 4], "test").is_ok());
        assert!(validate_matmul_shapes(&[2, 3], &[2, 4], "test").is_err());
        assert!(validate_matmul_shapes(&[2], &[2, 3], "test").is_err());
    }

    #[test]
    fn test_validate_broadcastable() {
        assert!(validate_broadcastable(&[2, 3], &[2, 3], "test").is_ok());
        assert!(validate_broadcastable(&[1, 3], &[2, 3], "test").is_ok());
        assert!(validate_broadcastable(&[2, 3], &[2, 1], "test").is_ok());
        assert!(validate_broadcastable(&[2, 3], &[3, 4], "test").is_err());
    }

    #[test]
    fn test_validate_lora_config() {
        assert!(validate_lora_config(8, 16.0, 0.1).is_ok());
        assert!(validate_lora_config(0, 16.0, 0.1).is_err());
        assert!(validate_lora_config(8, -1.0, 0.1).is_err());
        assert!(validate_lora_config(8, 16.0, 1.5).is_err());
        assert!(validate_lora_config(300, 16.0, 0.1).is_err());
    }

    #[test]
    fn test_validate_gates_q15() {
        assert!(validate_gates_q15(&[16384, 8192], 2).is_ok());
        assert!(validate_gates_q15(&[16384, 8192], 3).is_err());
        assert!(validate_gates_q15(&[32768], 1).is_err()); // Exceeds max
        assert!(validate_gates_q15(&[32767], 1).is_ok()); // Max valid value
    }

    #[test]
    fn test_validate_adapter_id() {
        assert!(validate_adapter_id(1).is_ok());
        assert!(validate_adapter_id(0).is_err()); // Reserved
        assert!(validate_adapter_id(1025).is_err()); // Too high
    }

    #[test]
    fn test_validate_model_config() {
        assert!(validate_model_config(768, 12, 12, 32000).is_ok());
        assert!(validate_model_config(0, 12, 12, 32000).is_err());
        assert!(validate_model_config(768, 0, 12, 32000).is_err());
        assert!(validate_model_config(768, 12, 13, 32000).is_err()); // Not divisible
    }

    #[test]
    fn test_validate_token_ids() {
        assert!(validate_token_ids(&[1, 2, 3], 100).is_ok());
        assert!(validate_token_ids(&[1, 100, 3], 100).is_err());
    }

    #[test]
    fn test_validate_finite() {
        assert!(validate_finite(1.0, "test").is_ok());
        assert!(validate_finite(f32::NAN, "test").is_err());
        assert!(validate_finite(f32::INFINITY, "test").is_err());
    }

    #[test]
    fn test_validate_all_finite() {
        assert!(validate_all_finite(&[1.0, 2.0, 3.0], "test").is_ok());
        assert!(validate_all_finite(&[1.0, f32::NAN, 3.0], "test").is_err());
    }

    #[test]
    fn test_validate_non_empty() {
        assert!(validate_non_empty(&[1, 2, 3], "test").is_ok());
        assert!(validate_non_empty::<i32>(&[], "test").is_err());
    }
}
