//! Common training arguments shared across training commands
//!
//! This module provides reusable clap arguments for LoRA training parameters
//! to avoid duplication across multiple training commands.

use clap::Args;
use std::path::PathBuf;

/// Common LoRA training hyperparameters
///
/// These arguments are shared across all training commands (train, train-docs,
/// train-from-code, train-base-adapter) to ensure consistency and avoid duplication.
///
/// # Usage
///
/// ```rust
/// use clap::Parser;
///
/// #[derive(Parser)]
/// struct TrainCommand {
///     #[command(flatten)]
///     common: CommonTrainingArgs,
///
///     // Command-specific arguments
///     #[arg(long)]
///     dataset: PathBuf,
/// }
/// ```
#[derive(Args, Debug, Clone)]
pub struct CommonTrainingArgs {
    /// LoRA rank (number of low-rank dimensions)
    ///
    /// Controls the capacity of the LoRA adapter. Higher ranks capture more
    /// information but increase memory usage and training time.
    ///
    /// Common values:
    /// - 4-8: Lightweight adapters for simple tasks
    /// - 16: General-purpose adapters (MasterPlan Layer 2 default)
    /// - 24: Codebase-specific adapters
    /// - 32+: High-capacity adapters for complex domains
    #[arg(long, default_value = "16")]
    pub rank: usize,

    /// LoRA alpha scaling factor
    ///
    /// Controls how much the LoRA weights influence the base model.
    /// Typically set to 2x the rank value.
    ///
    /// Common values:
    /// - 16.0: For rank=8
    /// - 32.0: For rank=16 (MasterPlan Layer 2 default)
    /// - 48.0: For rank=24
    #[arg(long, default_value = "32.0")]
    pub alpha: f32,

    /// Learning rate for gradient descent
    ///
    /// Controls how quickly the model adapts to training data.
    /// Lower values are more stable but slower to converge.
    ///
    /// Common values:
    /// - 1e-5 to 5e-5: Conservative, stable training
    /// - 1e-4: Standard default (0.0001)
    /// - 5e-4: Faster convergence for large datasets
    #[arg(long, default_value = "0.0001")]
    pub learning_rate: f32,

    /// Batch size (number of examples processed together)
    ///
    /// Larger batches provide more stable gradients but require more memory.
    /// Must be balanced with available GPU/CPU memory.
    ///
    /// Common values:
    /// - 4: Low-memory systems
    /// - 8: Standard default
    /// - 16-32: High-memory systems or small examples
    #[arg(long, default_value = "8")]
    pub batch_size: usize,

    /// Number of training epochs (full passes over the dataset)
    ///
    /// More epochs allow the model to learn better but risk overfitting.
    /// Monitor validation loss to determine optimal epoch count.
    ///
    /// Common values:
    /// - 3: Quick training for small datasets
    /// - 4-5: Standard training
    /// - 10+: Large datasets with early stopping
    #[arg(long, default_value = "3")]
    pub epochs: usize,

    /// Hidden dimension size of the base model
    ///
    /// Must match the base model architecture:
    /// - Qwen2.5-7B: 3584
    /// - Smaller models: 768, 1024, 2048
    ///
    /// This is used to initialize LoRA weight matrices.
    #[arg(long, default_value = "768")]
    pub hidden_dim: usize,
}

impl CommonTrainingArgs {
    /// Validate training arguments
    ///
    /// Ensures that all hyperparameters are within valid ranges.
    pub fn validate(&self) -> adapteros_core::Result<()> {
        use adapteros_core::AosError;

        if self.rank == 0 {
            return Err(AosError::Validation(
                "LoRA rank must be greater than zero".to_string(),
            ));
        }

        if self.rank > 256 {
            return Err(AosError::Validation(
                "LoRA rank cannot exceed 256 (typical max is 32)".to_string(),
            ));
        }

        if self.alpha <= 0.0 {
            return Err(AosError::Validation(
                "LoRA alpha must be greater than zero".to_string(),
            ));
        }

        if self.learning_rate <= 0.0 {
            return Err(AosError::Validation(
                "Learning rate must be greater than zero".to_string(),
            ));
        }

        if self.learning_rate > 0.1 {
            return Err(AosError::Validation(
                "Learning rate too high (>0.1), risk of unstable training".to_string(),
            ));
        }

        if self.batch_size == 0 {
            return Err(AosError::Validation(
                "Batch size must be greater than zero".to_string(),
            ));
        }

        if self.batch_size > 1024 {
            return Err(AosError::Validation(
                "Batch size too large (>1024), likely to cause OOM".to_string(),
            ));
        }

        if self.epochs == 0 {
            return Err(AosError::Validation(
                "Number of epochs must be greater than zero".to_string(),
            ));
        }

        if self.hidden_dim == 0 {
            return Err(AosError::Validation(
                "Hidden dimension must be greater than zero".to_string(),
            ));
        }

        // Warn about common misconfigurations (don't error, just validate)
        if self.alpha < self.rank as f32 {
            tracing::warn!(
                "Alpha ({}) is less than rank ({}). Typical convention is alpha = 2 * rank",
                self.alpha,
                self.rank
            );
        }

        Ok(())
    }
}

/// Common tokenizer argument
///
/// Shared tokenizer path argument with environment variable support.
#[derive(Args, Debug, Clone)]
pub struct TokenizerArg {
    /// Tokenizer path (auto-discovered from AOS_TOKENIZER_PATH or model directory)
    ///
    /// If not provided, will attempt to resolve from:
    /// 1. AOS_TOKENIZER_PATH environment variable
    /// 2. AOS_MODEL_PATH/tokenizer.json
    ///
    /// If neither is set or found, an error is returned with remediation steps.
    #[arg(long, env = "AOS_TOKENIZER_PATH")]
    pub tokenizer: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_args() {
        let args = CommonTrainingArgs {
            rank: 16,
            alpha: 32.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
        };

        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_rank() {
        let args = CommonTrainingArgs {
            rank: 0,
            alpha: 32.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
        };

        let result = args.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("rank must be greater than zero"));
    }

    #[test]
    fn test_validate_zero_epochs() {
        let args = CommonTrainingArgs {
            rank: 16,
            alpha: 32.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 0,
            hidden_dim: 768,
        };

        let result = args.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("epochs must be greater than zero"));
    }

    #[test]
    fn test_validate_invalid_learning_rate() {
        let args = CommonTrainingArgs {
            rank: 16,
            alpha: 32.0,
            learning_rate: 0.0,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
        };

        let result = args.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Learning rate must be greater than zero"));
    }

    #[test]
    fn test_validate_excessive_rank() {
        let args = CommonTrainingArgs {
            rank: 512,
            alpha: 32.0,
            learning_rate: 0.0001,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
        };

        let result = args.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot exceed 256"));
    }

    #[test]
    fn test_validate_high_learning_rate() {
        let args = CommonTrainingArgs {
            rank: 16,
            alpha: 32.0,
            learning_rate: 0.5,
            batch_size: 8,
            epochs: 3,
            hidden_dim: 768,
        };

        let result = args.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too high"));
    }

    #[test]
    fn test_validate_large_batch_size() {
        let args = CommonTrainingArgs {
            rank: 16,
            alpha: 32.0,
            learning_rate: 0.0001,
            batch_size: 2048,
            epochs: 3,
            hidden_dim: 768,
        };

        let result = args.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }
}
