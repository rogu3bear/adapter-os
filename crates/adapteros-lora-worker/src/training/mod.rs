//! Micro-LoRA training infrastructure
//!
//! Provides complete training pipeline for small LoRA adapters from code patches:
//! - Dataset generation from patches
//! - Training loop with forward/backward pass
//! - Q15 quantization
//! - Adapter packaging with safetensors

pub mod dataset;
pub mod packager;
pub mod quantizer;
pub mod trainer;

pub use dataset::{DatasetGenerator, TrainingExample};
pub use packager::{AdapterManifest, AdapterPackager, PackagedAdapter};
pub use quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
pub use trainer::{LoRAWeights, MicroLoRATrainer, TrainingConfig, TrainingResult};
