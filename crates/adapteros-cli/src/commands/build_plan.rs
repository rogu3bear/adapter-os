//! Build plan from manifest

use crate::output::OutputWriter;
use adapteros_db::Db;
use adapteros_manifest::ManifestV3;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Serialize)]
struct PlanResult {
    plan_id: String,
    manifest_hash: String,
}

pub async fn run(
    manifest: &Path,
    output_path: &Path,
    tenant_id: Option<&str>,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!(
        "Building plan from manifest: {}",
        manifest.display()
    ));

    // Load and parse manifest file
    let manifest_content = fs::read_to_string(manifest).context("Failed to read manifest file")?;

    // Try parsing as YAML first, then JSON
    let manifest_v3: ManifestV3 = if manifest_content.trim_start().starts_with('{') {
        serde_json::from_str(&manifest_content).context("Failed to parse manifest as JSON")?
    } else {
        serde_yaml::from_str(&manifest_content).context("Failed to parse manifest as YAML")?
    };

    // Validate manifest
    manifest_v3
        .validate()
        .context("Manifest validation failed")?;

    output.success("Manifest validated");

    // Compute manifest hash
    let manifest_hash = manifest_v3.compute_hash()?;
    let manifest_json = manifest_v3.to_json()?;

    // Show determinism verification
    output.success(format!("Manifest hash (deterministic): {}", manifest_hash));

    // Connect to database
    let db = Db::connect_env()
        .await
        .context("Failed to connect to database")?;

    // Insert manifest into database
    let tenant_id = tenant_id.unwrap_or("default");
    db.create_manifest(tenant_id, &manifest_hash.to_string(), &manifest_json)
        .await
        .context("Failed to create manifest record")?;

    output.success("Manifest stored in database");

    // Create build_plan job
    let payload = serde_json::json!({
        "manifest_hash": manifest_hash.to_string(),
    });

    let job_id = db
        .create_job("build_plan", Some(tenant_id), None, &payload.to_string())
        .await
        .context("Failed to create build_plan job")?;

    output.success(format!("Build job created: {}", job_id));
    output.info("Waiting for job to complete...");

    // Poll for job completion
    let mut attempts = 0;
    let max_attempts = 60; // 2 minutes with 2-second intervals

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let job = db
            .get_job(&job_id)
            .await
            .context("Failed to get job status")?
            .ok_or_else(|| anyhow::anyhow!("Job not found"))?;

        match job.status.as_str() {
            "finished" => {
                let result: serde_json::Value =
                    serde_json::from_str(job.result_json.as_deref().unwrap_or("{}"))?;

                let plan_id = result["data"]["plan_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Plan ID not in result"))?;

                output.success(format!("Plan built successfully: {}", plan_id));
                output.progress(format!("Output: {}", output_path.display()));

                // Write plan ID to output file
                fs::write(output_path, plan_id)
                    .context("Failed to write plan ID to output file")?;

                if output.is_json() {
                    let result = PlanResult {
                        plan_id: plan_id.to_string(),
                        manifest_hash: manifest_hash.to_string(),
                    };
                    output.json(&result)?;
                }

                return Ok(());
            }
            "failed" => {
                let result_json = job.result_json.as_deref().unwrap_or("{}");
                let result: serde_json::Value = serde_json::from_str(result_json)?;
                let error_msg = result["message"].as_str().unwrap_or("Unknown error");

                return Err(anyhow::anyhow!("Build job failed: {}", error_msg));
            }
            "running" => {
                output.progress(".");
            }
            _ => {}
        }

        attempts += 1;
        if attempts >= max_attempts {
            return Err(anyhow::anyhow!("Build job timed out"));
        }
    }
}
