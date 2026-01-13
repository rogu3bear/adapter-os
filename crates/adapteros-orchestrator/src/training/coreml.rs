//! CoreML export flow for trained adapters.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use adapteros_config::CoreMLComputePreference;
use adapteros_core::{adapter_fs_path_with_root, B3Hash};
use adapteros_core::{AosError, Result};
use adapteros_db::CreateCoremlFusionPairParams;
use adapteros_lora_worker::{ComputeUnits, CoreMLExportJob, CoreMLExportRecord};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::training::job::TrainingJob;

/// Run the CoreML export flow for a trained adapter.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_coreml_export_flow(
    jobs_ref: Arc<RwLock<HashMap<String, TrainingJob>>>,
    job_id: &str,
    adapter_id: &str,
    aos_path: &Path,
    base_model_id: &str,
    weights_hash_b3: &str,
    adapters_root: &Path,
    tenant_id: Option<&str>,
    db_for_packaging: Option<&adapteros_db::Db>,
) -> Result<()> {
    {
        let mut jobs = jobs_ref.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.coreml_export_requested = Some(true);
            job.coreml_export_status = Some("running".to_string());
            job.coreml_export_reason = None;
        }
    }

    let export_outcome = (|| -> Result<CoreMLExportRecord> {
        let base_package = adapteros_config::model::get_model_path_with_fallback()
            .map_err(|e| AosError::Config(e.to_string()))?;
        let tenant = tenant_id.unwrap_or("default");
        let fused_root = adapter_fs_path_with_root(adapters_root, tenant, adapter_id)
            .map_err(|e| AosError::Internal(e.to_string()))?
            .join("coreml");
        let output_package = if base_package.is_dir() {
            fused_root
        } else {
            let filename = base_package
                .file_name()
                .unwrap_or_else(|| OsStr::new("fused.mlpackage"));
            fused_root.join(filename)
        };

        let export_job = CoreMLExportJob {
            base_package,
            adapter_aos: aos_path.to_path_buf(),
            output_package,
            compute_units: resolve_coreml_compute_units(),
            base_model_id: Some(base_model_id.to_string()),
            adapter_id: Some(adapter_id.to_string()),
        };

        perform_coreml_export(export_job)
    })();

    match export_outcome {
        Ok(record) => {
            let fused_hash = record.fused_manifest_hash.to_string();
            let base_hash = record.base_manifest_hash.to_string();
            let adapter_hash = record.adapter_hash.to_string();
            let metadata_path_str = record.metadata_path.to_string_lossy().to_string();
            let fused_path_str = record.fused_package.to_string_lossy().to_string();

            let metadata_only = record.base_manifest_hash == record.fused_manifest_hash;
            let export_status = if metadata_only {
                warn!(
                    job_id = %job_id,
                    adapter_id = %adapter_id,
                    base_hash = %base_hash,
                    fused_hash = %fused_hash,
                    "CoreML export produced metadata-only package (fused manifest matches base)"
                );
                "metadata_only"
            } else {
                info!(
                    job_id = %job_id,
                    adapter_id = %adapter_id,
                    "CoreML export completed with fused package"
                );
                "succeeded"
            };
            if !record.fusion_verified && !metadata_only {
                warn!(
                    job_id = %job_id,
                    adapter_id = %adapter_id,
                    "CoreML export completed without fusion verification"
                );
            }
            let export_reason = if metadata_only {
                Some("Metadata-only export: fused package matches base model".to_string())
            } else if !record.fusion_verified {
                Some("Fusion path not verified during export".to_string())
            } else {
                None
            };
            let is_stub_export = !record.fusion_verified;

            {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.coreml_export_status = Some(export_status.to_string());
                    job.coreml_export_reason = export_reason.clone();
                    job.coreml_fused_package_hash = Some(fused_hash.clone());
                    job.coreml_package_path = Some(fused_path_str.clone());
                    job.coreml_metadata_path = Some(metadata_path_str.clone());
                    job.coreml_base_manifest_hash = Some(base_hash.clone());
                    job.coreml_adapter_hash_b3 = Some(adapter_hash.clone());
                    job.coreml_fusion_verified = Some(record.fusion_verified);
                }
            }

            if let Some(database) = db_for_packaging {
                let tenant_for_fusion = tenant_id.unwrap_or("default").to_string();
                let params = CreateCoremlFusionPairParams {
                    tenant_id: tenant_for_fusion,
                    base_model_id: base_model_id.to_string(),
                    adapter_id: adapter_id.to_string(),
                    fused_manifest_hash: fused_hash.clone(),
                    coreml_package_hash: fused_hash.clone(),
                    adapter_hash_b3: Some(adapter_hash.clone()),
                    base_model_hash_b3: Some(base_hash.clone()),
                    metadata_path: Some(metadata_path_str.clone()),
                };

                if let Err(e) = database.upsert_coreml_fusion_pair(params).await {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to upsert coreml_fusion_pairs record"
                    );
                }

                let export_meta = serde_json::json!({
                    "coreml_export": {
                        "requested": true,
                        "status": export_status,
                        "fusion_verified": record.fusion_verified,
                        "stub": is_stub_export,
                        "metadata_only": metadata_only,
                        "reason": export_reason,
                        "fused_manifest_hash": fused_hash,
                        "base_manifest_hash": base_hash,
                        "adapter_hash_b3": adapter_hash,
                        "package_path": fused_path_str,
                        "metadata_path": metadata_path_str
                    }
                });

                if let Err(e) = database
                    .update_training_job_artifact(
                        job_id,
                        aos_path.to_string_lossy().as_ref(),
                        adapter_id,
                        weights_hash_b3,
                        Some(export_meta),
                    )
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to persist CoreML export metadata (non-fatal)"
                    );
                }
            }
        }
        Err(e) => {
            let reason = e.to_string();
            {
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.coreml_export_status = Some("failed".to_string());
                    job.coreml_export_reason = Some(reason.clone());
                }
            }

            if let Some(database) = db_for_packaging {
                let export_meta = serde_json::json!({
                    "coreml_export": {
                        "requested": true,
                        "status": "failed",
                        "reason": reason
                    }
                });
                if let Err(err) = database
                    .update_training_job_artifact(
                        job_id,
                        aos_path.to_string_lossy().as_ref(),
                        adapter_id,
                        weights_hash_b3,
                        Some(export_meta),
                    )
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        error = %err,
                        "Failed to persist failed CoreML export metadata (non-fatal)"
                    );
                }
            }

            return Err(e);
        }
    }

    Ok(())
}

/// Resolve CoreML compute units from environment.
pub(crate) fn resolve_coreml_compute_units() -> ComputeUnits {
    let pref = std::env::var("AOS_COREML_COMPUTE_UNITS")
        .ok()
        .and_then(|v| CoreMLComputePreference::from_str(&v).ok())
        .unwrap_or_default();
    match pref {
        CoreMLComputePreference::CpuOnly => ComputeUnits::CpuOnly,
        CoreMLComputePreference::CpuAndGpu => ComputeUnits::CpuAndGpu,
        CoreMLComputePreference::CpuAndNe => ComputeUnits::CpuAndNeuralEngine,
        CoreMLComputePreference::All => ComputeUnits::All,
    }
}

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
fn perform_coreml_export(job: CoreMLExportJob) -> Result<CoreMLExportRecord> {
    adapteros_lora_worker::run_coreml_export(job).map_err(|e| AosError::CoreML(e.to_string()))
}

#[cfg(not(all(target_os = "macos", feature = "coreml-backend")))]
pub(crate) fn perform_coreml_export(job: CoreMLExportJob) -> Result<CoreMLExportRecord> {
    if std::env::var("AOS_ALLOW_COREML_EXPORT_STUB").is_err() && !cfg!(test) {
        return Err(AosError::CoreML(
            "CoreML export requires macOS with coreml-backend feature. \
             Set AOS_ALLOW_COREML_EXPORT_STUB=1 to create metadata-only package (not recommended for production)".to_string()
        ));
    }

    // Emit prominent warning when stub mode is used
    warn!(
        adapter_id = ?job.adapter_id,
        base_model_id = ?job.base_model_id,
        output_package = ?job.output_package,
        "CoreML export running in STUB MODE - producing metadata-only package. \
         This package will NOT contain fused weights and is NOT suitable for production deployment. \
         To produce a functional CoreML package, run on macOS with --features coreml-backend"
    );

    if let Some(parent) = job.output_package.parent() {
        fs::create_dir_all(parent).map_err(|e| AosError::Io(e.to_string()))?;
    }
    if job.output_package.is_dir() || job.base_package.is_dir() {
        fs::create_dir_all(&job.output_package).map_err(|e| AosError::Io(e.to_string()))?;
    }

    let base_manifest_path = if job.base_package.is_dir() {
        job.base_package.join("Manifest.json")
    } else {
        job.base_package.clone()
    };
    let manifest_bytes = fs::read(&base_manifest_path).unwrap_or_default();
    let base_manifest_hash = B3Hash::hash(&manifest_bytes);
    let fused_manifest_hash = base_manifest_hash;

    let adapter_bytes = fs::read(&job.adapter_aos)
        .map_err(|e| AosError::Io(format!("Failed to read adapter bundle: {}", e)))?;
    let adapter_hash = B3Hash::hash(&adapter_bytes);

    let metadata_path = if job.output_package.is_dir() {
        job.output_package.join("adapteros_coreml_fusion.json")
    } else {
        job.output_package.with_extension("fusion.json")
    };
    let metadata = serde_json::json!({
        "base_manifest_hash": base_manifest_hash.to_string(),
        "fused_manifest_hash": fused_manifest_hash.to_string(),
        "adapter_hash": adapter_hash.to_string(),
        "base_package": job.base_package,
        "fused_package": job.output_package,
        "adapter_path": job.adapter_aos,
        "stub": true,
        "fusion_verified": false
    });
    fs::write(&metadata_path, serde_json::to_vec_pretty(&metadata)?)
        .map_err(|e| AosError::Io(e.to_string()))?;

    Ok(CoreMLExportRecord {
        fused_package: job.output_package.clone(),
        metadata_path,
        base_manifest_hash,
        fused_manifest_hash,
        adapter_hash,
        base_model_id: job.base_model_id,
        adapter_id: job.adapter_id,
        fusion_verified: false, // Stub exports never verify fusion
    })
}
