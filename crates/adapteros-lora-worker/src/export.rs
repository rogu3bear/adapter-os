//! CoreML export orchestration for adapters.
//!
//! This module provides a lightweight job wrapper around the CoreML kernel export helper so
//! orchestrator/CLI layers can trigger an opt-in `.aos` → CoreML fused package export.

#[cfg(not(feature = "coreml-backend"))]
use adapteros_core::AosError;
use adapteros_core::{B3Hash, Result};
#[cfg(feature = "coreml-backend")]
use adapteros_lora_kernel_coreml::export::{
    export_coreml_adapter, validate_coreml_fusion, CoreMLExportOutcome, CoreMLExportRequest,
    CoreMLFusionMetadata,
};
#[cfg(feature = "coreml-backend")]
pub use adapteros_lora_kernel_coreml::ComputeUnits;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(not(feature = "coreml-backend"))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum ComputeUnits {
    CpuOnly,
    CpuAndGpu,
    CpuAndNeuralEngine,
    #[default]
    All,
}

#[cfg(not(feature = "coreml-backend"))]
#[cfg(not(feature = "coreml-backend"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLFusionMetadata {
    pub stub: bool,
}

/// Input parameters for a CoreML export job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLExportJob {
    pub base_package: PathBuf,
    pub adapter_aos: PathBuf,
    pub output_package: PathBuf,
    #[serde(default)]
    pub compute_units: ComputeUnits,
    /// Optional logical IDs used for registries or auditing.
    #[serde(default)]
    pub base_model_id: Option<String>,
    #[serde(default)]
    pub adapter_id: Option<String>,
}

/// Output record for a CoreML export job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMLExportRecord {
    pub fused_package: PathBuf,
    pub metadata_path: PathBuf,
    pub base_manifest_hash: B3Hash,
    pub fused_manifest_hash: B3Hash,
    pub adapter_hash: B3Hash,
    pub base_model_id: Option<String>,
    pub adapter_id: Option<String>,
}

/// Run a CoreML export job and return a record that can be persisted by callers.
#[cfg(feature = "coreml-backend")]
pub fn run_coreml_export(job: CoreMLExportJob) -> Result<CoreMLExportRecord> {
    let outcome: CoreMLExportOutcome = export_coreml_adapter(&CoreMLExportRequest {
        base_package: job.base_package.clone(),
        adapter_aos: job.adapter_aos.clone(),
        output_package: job.output_package.clone(),
        compute_units: job.compute_units,
    })?;

    Ok(CoreMLExportRecord {
        fused_package: outcome.fused_package,
        metadata_path: outcome.metadata_path,
        base_manifest_hash: outcome.base_manifest_hash,
        fused_manifest_hash: outcome.fused_manifest_hash,
        adapter_hash: outcome.adapter_hash,
        base_model_id: job.base_model_id,
        adapter_id: job.adapter_id,
    })
}

/// Stubbed CoreML export path for builds without the CoreML backend enabled.
#[cfg(not(feature = "coreml-backend"))]
pub fn run_coreml_export(_job: CoreMLExportJob) -> Result<CoreMLExportRecord> {
    Err(AosError::Config(
        "CoreML export requires the coreml-backend feature".to_string(),
    ))
}

/// Validate a fused package using its emitted metadata JSON.
#[cfg(feature = "coreml-backend")]
pub fn verify_coreml_export(metadata_path: &Path) -> Result<CoreMLFusionMetadata> {
    validate_coreml_fusion(metadata_path)
}

/// Stubbed verification for builds without CoreML enabled.
#[cfg(not(feature = "coreml-backend"))]
pub fn verify_coreml_export(_metadata_path: &Path) -> Result<CoreMLFusionMetadata> {
    Err(AosError::Config(
        "CoreML export verification requires the coreml-backend feature".to_string(),
    ))
}
