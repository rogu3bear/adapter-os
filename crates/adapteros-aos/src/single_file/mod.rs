//! Single-file adapter format (.aos) implementation
//!
//! Provides the AOS binary format for adapters that includes:
//! - LoRA weights (safetensors format)
//! - Training data (JSONL format)
//! - Configuration (TOML format)
//! - Lineage tracking (JSON format)
//! - Cryptographic signatures (Ed25519)
//!
//! ## Format Specification
//!
//! AOS uses a 64-byte header with segment index for zero-copy weight loading.
//! See `adapteros-aos` crate for the canonical format definition.
//!
//! ## Example Usage
//!
//! Create adapters with `SingleFileAdapter::create()`, sign them with `sign()`,
//! and save them with `SingleFileAdapterPackager::save_with_options()`.

pub mod format;
pub mod loader;
pub mod migration;
pub mod packager;
pub mod training;
pub mod validator;
pub mod weights;

pub use format::{
    get_compatibility_report, verify_format_version, AdapterManifest, AdapterWeights, AosSignature,
    CompatibilityReport, LineageInfo, Mutation, SingleFileAdapter, WeightGroup, WeightGroupType,
    WeightMetadata, AOS_FORMAT_VERSION,
};
pub use loader::{LoadOptions, SingleFileAdapterLoader};
pub use migration::{migrate_adapter, migrate_file, MigrationResult};
pub use packager::{PackageOptions, SingleFileAdapterPackager};
pub use training::{TrainingConfig, TrainingExample};
pub use validator::{SingleFileAdapterValidator, ValidationResult};
