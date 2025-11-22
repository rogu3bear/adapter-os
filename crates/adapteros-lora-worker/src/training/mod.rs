//! Micro-LoRA training infrastructure
//!
//! Provides complete training pipeline for small LoRA adapters from code patches:
//! - Dataset generation from patches
//! - Training loop with forward/backward pass
//! - Q15 quantization
//! - Adapter packaging with safetensors
//! - Comprehensive metrics tracking and visualization

pub mod dataset;
pub mod json_loader;
pub mod loader;
pub mod metrics;
pub mod packager;
pub mod quantizer;
pub mod trainer;
pub mod trainer_metrics_ext;
pub mod visualization;

pub use dataset::{DatasetGenerator, TrainingExample};
pub use json_loader::{load_json_dataset_with_tokenizer, JsonLoaderConfig};
pub use loader::{load_examples_from_manifest, load_examples_with_encoder, DatasetManifest};
pub use metrics::{MetricsConfig, MetricsSnapshot, TrainingMetrics, TrainingReport};
pub use packager::{AdapterManifest, AdapterPackager, PackagedAdapter};
pub use quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
pub use trainer::{LoRAWeights, MicroLoRATrainer, TrainingConfig, TrainingResult};
pub use trainer_metrics_ext::{TrainerMetricsExt, TrainingMetricsSession};
pub use visualization::{TrainingCharts, TrainingProgress};
