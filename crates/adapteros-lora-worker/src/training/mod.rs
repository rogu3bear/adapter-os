//! Micro-LoRA training infrastructure
//!
//! Provides complete training pipeline for small LoRA adapters from code patches:
//! - Dataset generation from patches
//! - Training loop with forward/backward pass
//! - Learning rate schedules (constant, linear, cosine) with warmup
//! - Early stopping with validation loss monitoring
//! - Checkpoint saving and resumption
//! - Q15 quantization
//! - Adapter packaging with safetensors
//! - Comprehensive metrics tracking and visualization

pub mod checkpoint;
pub mod coreml_pipeline;
pub mod dataset;
pub mod determinism_harness;
pub mod early_stopping;
pub mod json_loader;
pub mod learning_rate_schedule;
pub mod loader;
pub mod metrics;
pub mod packager;
pub mod quantizer;
pub mod trainer;
pub mod trainer_metrics_ext;
pub mod visualization;

// Re-export separated trainer for backward compatibility
pub mod separated_trainer;

pub use checkpoint::{CheckpointManager, TrainingCheckpoint};
pub use dataset::{DatasetGenerator, TrainingExample};
pub use determinism_harness::{
    build_harness_training_config, compute_drift, deterministic_slice, run_backend_with_examples,
    BackendRun, DriftMetrics, HarnessHyperparams,
};
pub use early_stopping::{EarlyStopping, EarlyStoppingConfig};
pub use json_loader::{load_json_dataset_with_tokenizer, JsonLoaderConfig};
pub use learning_rate_schedule::{LRScheduleType, LRScheduler, LRSchedulerConfig};
pub use loader::{load_examples_from_manifest, load_examples_with_encoder, DatasetManifest};
pub use metrics::{MetricsConfig, MetricsSnapshot, TrainingMetrics, TrainingReport};
pub use packager::{AdapterManifest, AdapterPackager, PackagedAdapter};
pub use quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
pub use trainer::{
    DatasetSubsample, DeterminismConfig, LoRAWeights, MicroLoRATrainer, TrainingBackend,
    TrainingConfig, TrainingResult,
};
pub use trainer_metrics_ext::{TrainerMetricsExt, TrainingMetricsSession};
pub use visualization::{TrainingCharts, TrainingProgress};
