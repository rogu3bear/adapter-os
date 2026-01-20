//! CoreML export orchestration for adapters.
//!
//! This module provides a lightweight job wrapper around the CoreML kernel export helper so
//! orchestrator/CLI layers can trigger an opt-in `.aos` → CoreML fused package export.

use adapteros_core::{AosError, B3Hash, Result};
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
    /// Indicates whether the fused package differs from the base manifest.
    #[serde(default)]
    pub fusion_verified: bool,
}

/// Run a CoreML export job and return a record that can be persisted by callers.
#[cfg(feature = "coreml-backend")]
pub fn run_coreml_export(job: CoreMLExportJob) -> Result<CoreMLExportRecord> {
    let outcome: CoreMLExportOutcome = export_coreml_adapter(&CoreMLExportRequest {
        base_package: job.base_package.clone(),
        adapter_aos: job.adapter_aos.clone(),
        output_package: job.output_package.clone(),
        compute_units: job.compute_units,
        allow_overwrite: false,
        timeout: std::time::Duration::from_secs(300),
        skip_ops_check: false,
    })?;
    Ok(CoreMLExportRecord {
        fused_package: outcome.fused_package,
        metadata_path: outcome.metadata_path,
        base_manifest_hash: outcome.base_manifest_hash,
        fused_manifest_hash: outcome.fused_manifest_hash,
        adapter_hash: outcome.adapter_hash,
        base_model_id: job.base_model_id,
        adapter_id: job.adapter_id,
        fusion_verified: outcome.fusion_verified,
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
    let metadata = validate_coreml_fusion(metadata_path)?;
    if metadata.base_manifest_hash == metadata.fused_manifest_hash {
        return Err(AosError::Validation(
            "CoreML fusion verification failed: fused manifest hash matches base (export is a copy)"
                .to_string(),
        ));
    }
    Ok(metadata)
}

/// Stubbed verification for builds without CoreML enabled.
#[cfg(not(feature = "coreml-backend"))]
pub fn verify_coreml_export(_metadata_path: &Path) -> Result<CoreMLFusionMetadata> {
    Err(AosError::Config(
        "CoreML export verification requires the coreml-backend feature".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // =========================================================================
    // ComputeUnits Tests (stub implementation when coreml-backend disabled)
    // =========================================================================

    #[cfg(not(feature = "coreml-backend"))]
    mod compute_units_tests {
        use super::*;

        #[test]
        fn compute_units_default() {
            let units = ComputeUnits::default();
            matches!(units, ComputeUnits::All);
        }

        #[test]
        fn compute_units_variants() {
            let _cpu_only = ComputeUnits::CpuOnly;
            let _cpu_gpu = ComputeUnits::CpuAndGpu;
            let _cpu_ane = ComputeUnits::CpuAndNeuralEngine;
            let _all = ComputeUnits::All;
        }

        #[test]
        fn compute_units_clone() {
            let original = ComputeUnits::CpuOnly;
            let cloned = original;
            matches!(cloned, ComputeUnits::CpuOnly);
        }
    }

    // =========================================================================
    // CoreMLExportJob Tests
    // =========================================================================

    #[test]
    fn coreml_export_job_basic_construction() {
        let job = CoreMLExportJob {
            base_package: PathBuf::from("/models/base.mlpackage"),
            adapter_aos: PathBuf::from("/adapters/my_adapter.aos"),
            output_package: PathBuf::from("/output/fused.mlpackage"),
            compute_units: ComputeUnits::default(),
            base_model_id: None,
            adapter_id: None,
        };

        assert_eq!(job.base_package, PathBuf::from("/models/base.mlpackage"));
        assert_eq!(job.adapter_aos, PathBuf::from("/adapters/my_adapter.aos"));
        assert_eq!(job.output_package, PathBuf::from("/output/fused.mlpackage"));
        assert!(job.base_model_id.is_none());
        assert!(job.adapter_id.is_none());
    }

    #[test]
    fn coreml_export_job_with_ids() {
        let job = CoreMLExportJob {
            base_package: PathBuf::from("/models/base.mlpackage"),
            adapter_aos: PathBuf::from("/adapters/adapter.aos"),
            output_package: PathBuf::from("/output/fused.mlpackage"),
            compute_units: ComputeUnits::default(),
            base_model_id: Some("llama-7b".to_string()),
            adapter_id: Some("my-lora-adapter".to_string()),
        };

        assert_eq!(job.base_model_id.as_deref(), Some("llama-7b"));
        assert_eq!(job.adapter_id.as_deref(), Some("my-lora-adapter"));
    }

    #[test]
    fn coreml_export_job_serialize_deserialize() {
        let job = CoreMLExportJob {
            base_package: PathBuf::from("/base.mlpackage"),
            adapter_aos: PathBuf::from("/adapter.aos"),
            output_package: PathBuf::from("/output.mlpackage"),
            compute_units: ComputeUnits::default(),
            base_model_id: Some("model-id".to_string()),
            adapter_id: Some("adapter-id".to_string()),
        };

        let json = serde_json::to_string(&job).unwrap();
        let deserialized: CoreMLExportJob = serde_json::from_str(&json).unwrap();

        assert_eq!(job.base_package, deserialized.base_package);
        assert_eq!(job.adapter_aos, deserialized.adapter_aos);
        assert_eq!(job.output_package, deserialized.output_package);
        assert_eq!(job.base_model_id, deserialized.base_model_id);
        assert_eq!(job.adapter_id, deserialized.adapter_id);
    }

    #[test]
    fn coreml_export_job_deserialize_defaults() {
        // Minimal JSON should use defaults for optional fields
        let json = r#"{
            "base_package": "/base.mlpackage",
            "adapter_aos": "/adapter.aos",
            "output_package": "/output.mlpackage"
        }"#;

        let job: CoreMLExportJob = serde_json::from_str(json).unwrap();

        assert_eq!(job.base_package, PathBuf::from("/base.mlpackage"));
        assert!(job.base_model_id.is_none());
        assert!(job.adapter_id.is_none());
    }

    // =========================================================================
    // CoreMLExportRecord Tests
    // =========================================================================

    #[test]
    fn coreml_export_record_construction() {
        let record = CoreMLExportRecord {
            fused_package: PathBuf::from("/output/fused.mlpackage"),
            metadata_path: PathBuf::from("/output/metadata.json"),
            base_manifest_hash: B3Hash::hash(b"base"),
            fused_manifest_hash: B3Hash::hash(b"fused"),
            adapter_hash: B3Hash::hash(b"adapter"),
            base_model_id: Some("model-123".to_string()),
            adapter_id: Some("adapter-456".to_string()),
            fusion_verified: true,
        };

        assert_eq!(
            record.fused_package,
            PathBuf::from("/output/fused.mlpackage")
        );
        assert_eq!(record.metadata_path, PathBuf::from("/output/metadata.json"));
        assert!(record.fusion_verified);
        assert_eq!(record.base_model_id.as_deref(), Some("model-123"));
        assert_eq!(record.adapter_id.as_deref(), Some("adapter-456"));
    }

    #[test]
    fn coreml_export_record_hashes_differ() {
        let record = CoreMLExportRecord {
            fused_package: PathBuf::from("/fused.mlpackage"),
            metadata_path: PathBuf::from("/metadata.json"),
            base_manifest_hash: B3Hash::hash(b"base manifest content"),
            fused_manifest_hash: B3Hash::hash(b"fused manifest content"),
            adapter_hash: B3Hash::hash(b"adapter weights"),
            base_model_id: None,
            adapter_id: None,
            fusion_verified: true,
        };

        // All three hashes should be different
        assert_ne!(record.base_manifest_hash, record.fused_manifest_hash);
        assert_ne!(record.base_manifest_hash, record.adapter_hash);
        assert_ne!(record.fused_manifest_hash, record.adapter_hash);
    }

    #[test]
    fn coreml_export_record_serialize_deserialize() {
        let record = CoreMLExportRecord {
            fused_package: PathBuf::from("/output.mlpackage"),
            metadata_path: PathBuf::from("/metadata.json"),
            base_manifest_hash: B3Hash::hash(b"base"),
            fused_manifest_hash: B3Hash::hash(b"fused"),
            adapter_hash: B3Hash::hash(b"adapter"),
            base_model_id: None,
            adapter_id: None,
            fusion_verified: false,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: CoreMLExportRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(record.fused_package, deserialized.fused_package);
        assert_eq!(record.metadata_path, deserialized.metadata_path);
        assert_eq!(record.base_manifest_hash, deserialized.base_manifest_hash);
        assert_eq!(record.fused_manifest_hash, deserialized.fused_manifest_hash);
        assert_eq!(record.adapter_hash, deserialized.adapter_hash);
        assert_eq!(record.fusion_verified, deserialized.fusion_verified);
    }

    #[test]
    fn coreml_export_record_fusion_verified_default() {
        // Test that fusion_verified defaults to false when not in JSON
        let json = r#"{
            "fused_package": "/fused.mlpackage",
            "metadata_path": "/meta.json",
            "base_manifest_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "fused_manifest_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "adapter_hash": "0000000000000000000000000000000000000000000000000000000000000000"
        }"#;

        let record: CoreMLExportRecord = serde_json::from_str(json).unwrap();
        assert!(!record.fusion_verified); // Default is false
    }

    // =========================================================================
    // Stubbed Function Tests (when coreml-backend is disabled)
    // =========================================================================

    #[cfg(not(feature = "coreml-backend"))]
    mod stubbed_tests {
        use super::*;

        #[test]
        fn run_coreml_export_returns_error_without_feature() {
            let job = CoreMLExportJob {
                base_package: PathBuf::from("/base.mlpackage"),
                adapter_aos: PathBuf::from("/adapter.aos"),
                output_package: PathBuf::from("/output.mlpackage"),
                compute_units: ComputeUnits::default(),
                base_model_id: None,
                adapter_id: None,
            };

            let result = run_coreml_export(job);
            assert!(result.is_err());

            let err = result.unwrap_err();
            assert!(err.to_string().contains("coreml-backend feature"));
        }

        #[test]
        fn verify_coreml_export_returns_error_without_feature() {
            let result = verify_coreml_export(Path::new("/nonexistent/metadata.json"));
            assert!(result.is_err());

            let err = result.unwrap_err();
            assert!(err.to_string().contains("coreml-backend feature"));
        }

        #[test]
        fn coreml_fusion_metadata_stub() {
            let metadata = CoreMLFusionMetadata { stub: true };
            assert!(metadata.stub);
        }
    }

    // =========================================================================
    // Path Construction Tests
    // =========================================================================

    #[test]
    fn export_job_path_construction() {
        let base_dir = PathBuf::from("/models");
        let adapter_dir = PathBuf::from("/adapters");
        let output_dir = PathBuf::from("/output");

        let job = CoreMLExportJob {
            base_package: base_dir.join("base_model.mlpackage"),
            adapter_aos: adapter_dir.join("fine_tuned.aos"),
            output_package: output_dir.join("fused_model.mlpackage"),
            compute_units: ComputeUnits::default(),
            base_model_id: None,
            adapter_id: None,
        };

        // Verify path construction is correct
        assert!(job
            .base_package
            .to_str()
            .unwrap()
            .contains("base_model.mlpackage"));
        assert!(job.adapter_aos.to_str().unwrap().contains("fine_tuned.aos"));
        assert!(job
            .output_package
            .to_str()
            .unwrap()
            .contains("fused_model.mlpackage"));
    }

    #[test]
    fn export_job_relative_paths() {
        // Test that relative paths are handled correctly
        let job = CoreMLExportJob {
            base_package: PathBuf::from("./local/base.mlpackage"),
            adapter_aos: PathBuf::from("./local/adapter.aos"),
            output_package: PathBuf::from("./output/fused.mlpackage"),
            compute_units: ComputeUnits::default(),
            base_model_id: None,
            adapter_id: None,
        };

        assert!(job.base_package.is_relative());
        assert!(job.adapter_aos.is_relative());
        assert!(job.output_package.is_relative());
    }
}
