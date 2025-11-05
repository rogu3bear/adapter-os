//! Single-file adapter format (.aos) implementation
//!
//! Provides a ZIP-based container format for adapters that includes:
//! - LoRA weights (safetensors format)
//! - Training data (JSONL format)
//! - Configuration (TOML format)
//! - Lineage tracking (JSON format)
//! - Cryptographic signatures (Ed25519)
//!
//! ## Format Versioning
//!
//! The .aos format uses semantic versioning tracked in the manifest.
//! Current version: 1
//!
//! ## Example Usage
//!
//! Create adapters with `SingleFileAdapter::create()`, sign them with `sign()`,
//! and save them with `SingleFileAdapterPackager::save_with_options()`.

// ============================================================================
// AOS COORDINATION HEADER
// ============================================================================
// File: crates/adapteros-single-file-adapter/src/lib.rs
// Phase: 1 - Core Infrastructure (Format Definition)
// Assigned: Intern F (Single-File Adapter Team)
// Status: Complete - Core format implementation finished
// Dependencies: Crypto, Compression, Serialization
// Last Updated: 2024-01-15
//
// COORDINATION NOTES:
// - This file affects: All .aos file operations, format compatibility
// - Changes require: Updates to CLI commands, UI components, database schemas
// - Testing needed: Format validation tests, compatibility tests, security tests
// - CLI Impact: All CLI commands depend on this format
// - UI Impact: UI components display .aos file information
// - Database Impact: Database stores .aos file metadata
// ============================================================================

pub mod aos2_format;
pub mod aos2_packager;
pub mod format;
pub mod format_detector;
pub mod loader;
pub mod migration;
pub mod mmap_loader;
pub mod packager;
pub mod training;
pub mod validator;
pub mod weights;

pub use aos2_format::{Aos2Adapter, Aos2Header};
pub use aos2_packager::{Aos2PackageOptions, Aos2Packager};
pub use format::{
    get_compatibility_report, verify_format_version, AdapterManifest, AdapterWeights, AosSignature,
    CompatibilityReport, CompressionLevel, LineageInfo, Mutation, SingleFileAdapter, WeightGroup,
    WeightGroupType, WeightMetadata, AOS_FORMAT_VERSION,
};
pub use format_detector::{detect_format, FormatVersion};
pub use loader::{LoadOptions, SingleFileAdapterLoader};
pub use migration::{migrate_adapter, migrate_file, MigrationResult};
pub use mmap_loader::{MmapAdapter, MmapAdapterLoader, WeightsKind};
pub use packager::{PackageOptions, SingleFileAdapterPackager};
pub use training::{TrainingConfig, TrainingExample};
pub use validator::{SingleFileAdapterValidator, ValidationResult};
