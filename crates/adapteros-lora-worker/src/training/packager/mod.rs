//! Adapter packaging with safetensors and manifest generation
//!
//! Packages trained LoRA adapters into a format compatible with mplora-artifacts.

pub mod aos;
pub mod coreml;
pub mod manifest;
pub mod metadata;
pub mod types;

// Re-export all public types for backward compatibility
pub use manifest::AdapterManifest;
pub use types::{
    AdapterPackager, AdapterPlacement, BranchMetadata, CoremlPlacementSpec, CoremlTrainingMetadata,
    LayerHash, PackagedAdapter, PlacementRecord, ScanRootMetadata,
};
