//! Test data generators for AOS 2.0 format
//!
//! This module provides comprehensive generators for creating test AOS files with various
//! configurations, corruption patterns, and edge cases. All generators are deterministic
//! when provided with a seed.
//!
//! ## Features
//!
//! - **Valid AOS generation**: Create properly formatted AOS files with custom parameters
//! - **Corruption patterns**: Generate files with specific corruption types for error testing
//! - **Edge cases**: Empty weights, huge files, missing sections, version mismatches
//! - **Deterministic**: Seeded RNG ensures reproducible test data
//! - **Format variants**: Support for different manifest versions and tensor configurations
//! - **Semantic naming**: Generate realistic adapter IDs following naming conventions
//!
//! ## Usage
//!
//! ```rust,ignore
//! use adapteros_aos::test_utils::{AosGenerator, GeneratorConfig};
//! use std::path::Path;
//!
//! # fn example() -> adapteros_core::Result<()> {
//! // Generate a valid AOS file
//! let config = GeneratorConfig {
//!     rank: 4,
//!     hidden_dim: 256,
//!     num_tensors: 2,
//!     seed: Some(42),
//!     ..Default::default()
//! };
//!
//! let mut generator = AosGenerator::new(config);
//! let aos_data = generator.generate_valid()?;
//!
//! // Or generate directly to a file
//! generator.generate_to_file(Path::new("test.aos"))?;
//!
//! // Generate corrupted data for error testing
//! let corrupted = generator.generate_corrupted(CorruptionType::BadHeader)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Example Test
//!
//! ```rust,ignore
//! use adapteros_aos::test_utils::{AosGenerator, GeneratorConfig};
//! use adapteros_aos::AOS2Writer;
//! use tempfile::NamedTempFile;
//!
//! # fn test_example() -> adapteros_core::Result<()> {
//! let mut generator = AosGenerator::new(GeneratorConfig::default());
//! let temp_file = NamedTempFile::new().unwrap();
//!
//! generator.generate_to_file(temp_file.path())?;
//!
//! // Verify header is valid
//! let (offset, len) = AOS2Writer::read_header(temp_file.path())?;
//! assert!(offset > 8);
//! assert!(len > 0);
//! # Ok(())
//! # }
//! ```

// Note: This module requires the optional dependencies to be compiled in
// It's primarily intended for use in tests and development

mod generators;
mod safetensors;
mod semantic_ids;

pub use generators::{
    AosGenerator, CorruptionType, EdgeCaseType, GeneratorConfig, ManifestVersion, TestManifest,
    TrainingConfig,
};
pub use safetensors::{
    f32_to_f16_simple, f32_to_q15, SafetensorsBuilder, TensorConfig, TensorDtype,
};
pub use semantic_ids::{
    generate_tenant_id, generate_test_id, parse_adapter_id, validate_adapter_id,
    SemanticIdGenerator,
};

// Re-export common test utilities
pub use generators::{
    generate_corrupted_aos, generate_edge_case_aos, generate_valid_aos,
    generate_valid_aos_with_params,
};
