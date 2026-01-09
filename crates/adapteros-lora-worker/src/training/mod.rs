//! Micro-LoRA training infrastructure
//!
//! Provides complete training pipeline for small LoRA adapters from code patches:
//! - Dataset generation from patches
//! - Dataset ingestion and normalization from raw sources
//! - Training loop with forward/backward pass
//! - Learning rate schedules (constant, linear, cosine) with warmup
//! - Early stopping with validation loss monitoring
//! - Checkpoint saving and resumption
//! - Q15 quantization
//! - Adapter packaging with safetensors
//! - Comprehensive metrics tracking and visualization

pub mod builder;
pub mod checkpoint;
pub mod coreml_pipeline;
pub mod dataset;
pub mod determinism_harness;
pub mod early_stopping;
pub mod formats;
pub mod json_loader;
mod limits;
pub mod learning_rate_schedule;
pub mod loader;
pub mod loss;
pub mod metrics;
pub mod normalize;
pub mod packager;
pub mod perplexity;
pub mod preprocessing;
pub mod quantizer;
pub mod trainer;
pub mod trainer_metrics_ext;
pub mod visualization;

// Re-export separated trainer for backward compatibility
pub mod separated_trainer;

pub use builder::{
    BuildConfig, BuildResult, BuiltDatasetManifest, DatasetBuilder, DatasetSource, GitAuth,
    SourceFileInfo,
};
pub use checkpoint::{CheckpointManager, TrainingCheckpoint};
pub use dataset::{split_examples_for_validation, DatasetGenerator, ValidationSplitSummary};
pub use adapteros_types::training::TrainingExampleV1;
pub type TrainingExample = TrainingExampleV1;
pub use formats::{ColumnMapping, DatasetFormat, ParserConfig, RawSample, TextStrategy};
pub use normalize::{normalize_text, NormalizationConfig, NORMALIZATION_SCHEME};
pub use determinism_harness::{
    build_harness_training_config, compute_drift, deterministic_slice, run_backend_with_examples,
    BackendRun, DriftMetrics, HarnessHyperparams,
};
pub use early_stopping::{EarlyStopping, EarlyStoppingConfig};
pub use json_loader::{load_json_dataset_with_tokenizer, JsonLoaderConfig};
pub use limits::DatasetSizeLimits;
pub use learning_rate_schedule::{LRScheduleType, LRScheduler, LRSchedulerConfig};
pub use loader::{load_examples_from_manifest, load_examples_with_encoder, DatasetManifest};
pub use metrics::{MetricsConfig, MetricsSnapshot, TrainingMetrics, TrainingReport};
pub use perplexity::{compute_perplexity, loss_to_perplexity_curve, PerplexityImprovement};
pub use packager::{
    AdapterManifest, AdapterPackager, BranchMetadata, PackagedAdapter, ScanRootMetadata,
};
pub use quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
pub use trainer::{
    DatasetSubsample, DeterminismConfig, LoRAWeights, MicroLoRATrainer, PreprocessCompression,
    PreprocessOutputFeature, PreprocessingConfig, TrainingBackend, TrainingConfig, TrainingResult,
};
pub use trainer_metrics_ext::{TrainerMetricsExt, TrainingMetricsSession};
pub use visualization::{TrainingCharts, TrainingProgress};

// Quantization and strength defaults must be versioned if changed.
pub const LORA_Q15_QUANTIZATION: &str = "q15";
pub const LORA_Q15_VERSION: &str = "q15-v1";
pub const LORA_Q15_DENOM: f32 = quantizer::LORA_Q15_DENOM;
pub const LORA_STRENGTH_DEFAULTS_VERSION: &str = "strengths-v1";
pub const LORA_STRENGTH_DEFAULT_MICRO: f32 = 0.25;
pub const LORA_STRENGTH_DEFAULT_STANDARD: f32 = 0.5;
pub const LORA_STRENGTH_DEFAULT_MAX: f32 = 1.0;
