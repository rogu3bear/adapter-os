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
pub mod embedding_loss;
pub mod embedding_trainer;
pub mod formats;
pub mod json_loader;
pub mod learning_rate_schedule;
mod limits;
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

pub use adapteros_types::training::{ExampleMetadataV1, TrainingExampleV1};
pub use builder::{
    BuildConfig, BuildResult, BuiltDatasetManifest, DatasetBuilder, DatasetSource, GitAuth,
    SourceFileInfo,
};
pub use checkpoint::{CheckpointManager, CheckpointSignature, TrainingCheckpoint};
pub use dataset::{
    compute_examples_hash, split_examples_for_validation, DatasetGenerator, ValidationSplitSummary,
};
pub type TrainingExample = TrainingExampleV1;
pub use determinism_harness::{
    build_harness_training_config, compute_drift, deterministic_slice, run_backend_with_examples,
    BackendRun, DriftMetrics, HarnessHyperparams,
};
pub use early_stopping::{EarlyStopping, EarlyStoppingConfig};
pub use formats::{ColumnMapping, DatasetFormat, ParserConfig, RawSample, TextStrategy};
pub use json_loader::{load_json_dataset_with_tokenizer, JsonLoaderConfig};
pub use learning_rate_schedule::{LRScheduleType, LRScheduler, LRSchedulerConfig};
pub use limits::DatasetSizeLimits;
pub use loader::{
    load_examples_from_manifest, load_examples_from_manifest_with_framing,
    load_examples_with_encoder, load_examples_with_encoder_and_framing, DatasetManifest,
    FramingConfig,
};
pub use metrics::{MetricsConfig, MetricsSnapshot, TrainingMetrics, TrainingReport};
pub use normalize::{normalize_text, NormalizationConfig, NORMALIZATION_SCHEME};
pub use packager::{
    AdapterManifest, AdapterPackager, BranchMetadata, PackagedAdapter, ScanRootMetadata,
};
pub use perplexity::{compute_perplexity, loss_to_perplexity_curve, PerplexityImprovement};
pub use quantizer::{LoRAQuantizer, QuantizedLoRAWeights};

// Embedding training
pub use embedding_loss::{
    batch_triplet_loss, contrastive_loss, cosine_distance, cosine_similarity, info_nce_loss,
    l2_distance, l2_normalize, l2_normalized, symmetric_info_nce_loss, triplet_loss,
};
pub use embedding_trainer::{EmbeddingTrainer, EmbeddingTrainingResult, ProjectionLayer};
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
