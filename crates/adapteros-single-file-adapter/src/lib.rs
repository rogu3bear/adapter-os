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
//! ```rust,no_run
//! use adapteros_single_file_adapter::{
//!     SingleFileAdapter, SingleFileAdapterPackager,
//!     CompressionLevel, PackageOptions
//! };
//! use adapteros_crypto::Keypair;
//!
//! # async fn example() -> adapteros_core::Result<()> {
//! // Create adapter
//! let mut adapter = SingleFileAdapter::create(/* ... */)?;
//!
//! // Sign it
//! let keypair = Keypair::generate();
//! adapter.sign(&keypair)?;
//!
//! // Save with compression
//! let options = PackageOptions {
//!     compression: CompressionLevel::Best,
//! };
//! SingleFileAdapterPackager::save_with_options(&adapter, "adapter.aos", options).await?;
//! # Ok(())
//! # }
//! ```

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

pub mod format;
pub mod loader;
pub mod mmap_loader;
pub mod migration;
pub mod packager;
pub mod training;
pub mod validator;
pub mod weights;

pub use format::{
    get_compatibility_report, verify_format_version, AdapterManifest, AosSignature,
    CompatibilityReport, CompressionLevel, LineageInfo, Mutation, SingleFileAdapter,
    AOS_FORMAT_VERSION,
};
pub use loader::{LoadOptions, SingleFileAdapterLoader};
pub use mmap_loader::{MmapAdapter, MmapAdapterLoader, WeightsKind};
pub use migration::{migrate_adapter, migrate_file, MigrationResult};
pub use packager::{PackageOptions, SingleFileAdapterPackager};
pub use training::{TrainingConfig, TrainingExample};
pub use validator::{SingleFileAdapterValidator, ValidationResult};
