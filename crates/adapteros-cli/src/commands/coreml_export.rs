use crate::output::OutputWriter;
use adapteros_api_types::training::TrainingJobResponse;
use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_coreml::ComputeUnits;
use adapteros_lora_worker::{run_coreml_export, CoreMLExportJob};
use reqwest::StatusCode;
use std::path::PathBuf;

pub async fn run(
    base_package: PathBuf,
    adapter_aos: PathBuf,
    output_package: PathBuf,
    compute_units: Option<String>,
    base_model_id: Option<String>,
    adapter_id: Option<String>,
    output: &OutputWriter,
) -> Result<()> {
    let compute_units = parse_compute_units(compute_units)?;

    output.info("Exporting adapter to fused CoreML package");
    output.kv("Base package", base_package.display().to_string());
    output.kv("Adapter bundle", adapter_aos.display().to_string());
    output.kv("Output", output_package.display().to_string());
    output.kv("Compute units", format!("{compute_units:?}"));

    let record = run_coreml_export(CoreMLExportJob {
        base_package,
        adapter_aos,
        output_package,
        compute_units,
        base_model_id,
        adapter_id,
    })?;

    output.success("CoreML export completed");
    output.kv("Fused package", record.fused_package.display().to_string());
    output.kv("Metadata", record.metadata_path.display().to_string());
    output.kv("Base manifest hash", record.base_manifest_hash.to_string());
    output.kv(
        "Fused manifest hash",
        record.fused_manifest_hash.to_string(),
    );
    output.kv("Adapter hash", record.adapter_hash.to_string());

    Ok(())
}

/// Trigger CoreML export for a completed training job via control plane
pub async fn trigger_export_for_job(
    job_id: &str,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/training/jobs/{job_id}/export/coreml",
        base_url.trim_end_matches('/')
    );

    output.info(&format!(
        "Triggering CoreML export for job {} at {}",
        job_id, url
    ));
    let resp = client
        .post(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "CoreML export failed: {} {}",
            status, body
        )));
    }

    let job: TrainingJobResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(format!("Failed to parse response: {}", e)))?;

    output.success("CoreML export requested");
    output.kv(
        "coreml_export_status",
        job.coreml_export_status
            .as_deref()
            .unwrap_or("running/pending"),
    );
    if let Some(hash) = job.coreml_fused_package_hash.as_deref() {
        output.kv("fused_hash", hash);
    }
    if let Some(path) = job.coreml_package_path.as_deref() {
        output.kv("fused_package", path);
    }
    Ok(())
}

/// Inspect CoreML export status for a training job
pub async fn show_export_status(job_id: &str, base_url: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/training/jobs/{job_id}",
        base_url.trim_end_matches('/')
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    if resp.status() == StatusCode::NOT_FOUND {
        return Err(AosError::NotFound(format!(
            "Training job not found: {}",
            job_id
        )));
    }

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AosError::Http(format!(
            "Failed to fetch job: {} {}",
            status, body
        )));
    }

    let job: TrainingJobResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(format!("Failed to parse response: {}", e)))?;

    output.info(&format!("Training job {}", job.id));
    output.kv(
        "coreml_export_requested",
        job.coreml_export_requested
            .map(|b| b.to_string())
            .unwrap_or_else(|| "false".to_string()),
    );
    output.kv(
        "coreml_export_status",
        job.coreml_export_status
            .as_deref()
            .unwrap_or("not_requested"),
    );
    if let Some(reason) = job.coreml_export_reason.as_deref() {
        output.kv("coreml_export_reason", reason);
    }
    if let Some(hash) = job.coreml_fused_package_hash.as_deref() {
        output.kv("fused_manifest_hash", hash);
    }
    if let Some(hash) = job.coreml_adapter_hash_b3.as_deref() {
        output.kv("adapter_hash_b3", hash);
    }
    if let Some(hash) = job.coreml_base_manifest_hash.as_deref() {
        output.kv("base_manifest_hash", hash);
    }
    if let Some(path) = job.coreml_package_path.as_deref() {
        output.kv("fused_package_path", path);
    }
    if let Some(path) = job.coreml_metadata_path.as_deref() {
        output.kv("metadata_path", path);
    }

    Ok(())
}

fn parse_compute_units(value: Option<String>) -> Result<ComputeUnits> {
    if let Some(raw) = value {
        let normalized = raw.to_ascii_lowercase();
        match normalized.as_str() {
            "cpu_only" | "cpu-only" | "cpu" => Ok(ComputeUnits::CpuOnly),
            "cpu_and_gpu" | "cpu+gpu" | "gpu" => Ok(ComputeUnits::CpuAndGpu),
            "cpu_and_neural_engine" | "cpu+ne" | "ane" | "ne" => {
                Ok(ComputeUnits::CpuAndNeuralEngine)
            }
            "all" => Ok(ComputeUnits::All),
            other => Err(AosError::Validation(format!(
                "Unknown compute units '{}'; use cpu_only, cpu_and_gpu, cpu_and_neural_engine, or all",
                other
            ))),
        }
    } else {
        Ok(ComputeUnits::CpuAndNeuralEngine)
    }
}
