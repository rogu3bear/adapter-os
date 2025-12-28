//! Adapter packaging and registration utilities.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use base64::Engine as _;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, TrainingConfig as WorkerTrainingConfig, TrainingResult,
};

use crate::training::config::PostActions;
use crate::training::coreml::run_coreml_export_flow;
use crate::training::job::{
    DataLineageMode, DatasetVersionSelection, LoraTier, TrainingConfig, TrainingJob,
    TrainingJobStatus,
};
use crate::training::versioning::{compute_combined_data_spec_hash, VersioningSnapshot};

/// Load plan/model bytes for GPU initialization.
///
/// - Uses `AOS_MODEL_PATH` (or legacy fallbacks) to find model assets.
/// - Returns path bytes for CoreML `.mlpackage` bundles.
/// - Returns safetensors bytes for Metal/CPU/MLX.
/// - When GPU is optional and assets are missing, returns an empty Vec so CPU can proceed.
pub(crate) fn load_plan_bytes_for_training(require_gpu: bool, job_id: &str) -> Result<Vec<u8>> {
    let model_path = match adapteros_config::model::get_model_path_with_fallback() {
        Ok(path) => path,
        Err(e) => {
            if require_gpu {
                return Err(anyhow::anyhow!(
                    "GPU initialization requested but model path is not configured: {}",
                    e
                ));
            }

            warn!(
                job_id = %job_id,
                error = %e,
                "No model path configured; GPU init will be skipped and CPU will be used"
            );
            return Ok(Vec::new());
        }
    };

    fn read_plan_bytes(model_path: &Path) -> Result<Vec<u8>> {
        let is_mlpackage = model_path
            .extension()
            .map(|ext| ext == "mlpackage" || ext == "mlmodel")
            .unwrap_or(false);

        if is_mlpackage {
            return Ok(model_path.to_string_lossy().into_owned().into_bytes());
        }

        if model_path.is_file() {
            return std::fs::read(model_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read model plan from '{}': {}",
                    model_path.display(),
                    e
                )
            });
        }

        if model_path.is_dir() {
            let safetensors_path = model_path.join("model.safetensors");
            if safetensors_path.exists() {
                return std::fs::read(&safetensors_path).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to read model.safetensors at '{}': {}",
                        safetensors_path.display(),
                        e
                    )
                });
            }

            if let Ok(entries) = std::fs::read_dir(model_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();

                    // Sharded safetensors first shard
                    if name.starts_with("model-00001-of-") && name.ends_with(".safetensors") {
                        return std::fs::read(&path).map_err(|e| {
                            anyhow::anyhow!(
                                "Failed to read sharded model file '{}': {}",
                                path.display(),
                                e
                            )
                        });
                    }

                    // CoreML bundle inside the directory
                    if path
                        .extension()
                        .map(|ext| ext == "mlpackage")
                        .unwrap_or(false)
                    {
                        return Ok(path.to_string_lossy().into_owned().into_bytes());
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Model assets not found under '{}'. Provide model.safetensors or a .mlpackage path.",
            model_path.display()
        ))
    }

    match read_plan_bytes(&model_path) {
        Ok(bytes) => Ok(bytes),
        Err(e) => {
            if require_gpu {
                Err(e)
            } else {
                warn!(
                    job_id = %job_id,
                    path = %model_path.display(),
                    error = %e,
                    "Plan bytes unavailable; GPU init will be skipped and CPU will be used"
                );
                Ok(Vec::new())
            }
        }
    }
}

/// Package adapter and register it in the database.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn package_and_register_adapter(
    jobs_ref: Arc<RwLock<HashMap<String, TrainingJob>>>,
    job_id: &str,
    adapter_name: &str,
    training_result: &TrainingResult,
    worker_cfg: &WorkerTrainingConfig,
    orchestrator_cfg: &TrainingConfig,
    post_actions: &PostActions,
    adapters_root: &std::path::Path,
    tenant: &str,
    tenant_id: Option<&str>,
    dataset_id: Option<&str>,
    synthetic_mode: bool,
    data_lineage_mode: DataLineageMode,
    base_model_id: Option<&str>,
    category: Option<&str>,
    versioning_snapshot: Option<&VersioningSnapshot>,
    dataset_version_ids_for_training: Option<&Vec<DatasetVersionSelection>>,
    data_spec_hash_for_training: Option<&str>,
    trainer_seed: u64,
    db: Option<&adapteros_db::Db>,
) -> Result<()> {
    // Step 1: Quantize weights to Q15 format
    let quantized_weights = LoRAQuantizer::quantize_to_q15(&training_result.weights);

    // Build packaging metadata for auditability
    let (scope_value, lora_tier_meta, backend_policy_meta) = {
        let jobs = jobs_ref.read().await;
        let scope_val = jobs
            .get(job_id)
            .and_then(|j| j.scope.clone())
            .unwrap_or_else(|| "project".to_string());
        let tier_val = jobs.get(job_id).and_then(|j| j.lora_tier);
        let backend_policy = jobs.get(job_id).and_then(|j| j.backend_policy.clone());
        (scope_val, tier_val, backend_policy)
    };

    let mut package_metadata = HashMap::new();
    package_metadata.insert("training_job_id".to_string(), job_id.to_string());
    package_metadata.insert("adapter_name".to_string(), adapter_name.to_string());
    if let Some(ds) = dataset_id {
        package_metadata.insert("dataset_id".to_string(), ds.to_string());
    }
    if let Some(tid) = tenant_id {
        package_metadata.insert("tenant_id".to_string(), tid.to_string());
    }
    package_metadata.insert("scope".to_string(), scope_value.clone());
    package_metadata.insert("lora_scope".to_string(), scope_value.clone());
    package_metadata.insert(
        "data_lineage_mode".to_string(),
        data_lineage_mode.as_str().to_string(),
    );
    package_metadata.insert("synthetic_mode".to_string(), synthetic_mode.to_string());
    if let Some(base_model) = base_model_id {
        package_metadata.insert("base_model_id".to_string(), base_model.to_string());
    }
    if let Some(cat) = category {
        package_metadata.insert("category".to_string(), cat.to_string());
    }
    if let Some(tier) = lora_tier_meta {
        let tier_label = match tier {
            LoraTier::Micro => "micro",
            LoraTier::Standard => "standard",
            LoraTier::Max => "max",
        };
        package_metadata.insert("lora_tier".to_string(), tier_label.to_string());
    }
    let backend_label = training_result
        .backend
        .as_deref()
        .unwrap_or("CPU")
        .to_ascii_lowercase();
    package_metadata.insert("training_backend".to_string(), backend_label);
    if let Some(device) = training_result.backend_device.clone() {
        package_metadata.insert("training_backend_device".to_string(), device);
    }
    if let Some(ref policy) = backend_policy_meta {
        package_metadata.insert("backend_policy".to_string(), policy.clone());
    }
    package_metadata.insert(
        "determinism".to_string(),
        if cfg!(feature = "deterministic-only") {
            "deterministic-only".to_string()
        } else {
            "best-effort".to_string()
        },
    );
    package_metadata.insert("quantization".to_string(), "q15".to_string());
    package_metadata.insert(
        "gate_q15_denominator".to_string(),
        adapteros_lora_router::ROUTER_GATE_Q15_DENOM.to_string(),
    );
    if let Some(hash) = data_spec_hash_for_training {
        package_metadata.insert("data_spec_hash".to_string(), hash.to_string());
    }
    if let Some(versions) = dataset_version_ids_for_training {
        if let Ok(json) = serde_json::to_string(versions) {
            package_metadata.insert("dataset_version_ids".to_string(), json);
        }
    }

    // Step 2: Package the adapter
    let packager = AdapterPackager::new(adapters_root);

    // Create worker training config for packaging
    let packager_cfg = WorkerTrainingConfig {
        rank: worker_cfg.rank,
        alpha: worker_cfg.alpha,
        learning_rate: worker_cfg.learning_rate,
        batch_size: worker_cfg.batch_size,
        epochs: worker_cfg.epochs,
        hidden_dim: worker_cfg.hidden_dim,
        vocab_size: worker_cfg.vocab_size,
        coreml_placement: worker_cfg.coreml_placement.clone(),
        preferred_backend: worker_cfg.preferred_backend,
        backend_policy: worker_cfg.backend_policy,
        coreml_fallback_backend: worker_cfg.coreml_fallback_backend,
        require_gpu: worker_cfg.require_gpu,
        max_gpu_memory_mb: worker_cfg.max_gpu_memory_mb,
        max_tokens_per_batch: worker_cfg.max_tokens_per_batch,
        device_policy: worker_cfg.device_policy.clone(),
        checkpoint_interval: worker_cfg.checkpoint_interval,
        warmup_steps: worker_cfg.warmup_steps,
        max_seq_length: worker_cfg.max_seq_length,
        gradient_accumulation_steps: worker_cfg.gradient_accumulation_steps,
        determinism: None,
        moe_config: None,
    };

    // Generate unique adapter ID from job_id
    let adapter_id = format!("adapter-{}", job_id.trim_start_matches("train-"));

    let base_model_for_manifest = base_model_id.unwrap_or("unknown-base-model");

    let dataset_hash_for_metadata =
        if let (Some(database), Some(versions)) = (db, dataset_version_ids_for_training) {
            let mut combined: Vec<(String, String, f32)> = Vec::new();
            for sel in versions.iter() {
                if let Ok(Some(ver)) = database
                    .get_training_dataset_version(&sel.dataset_version_id)
                    .await
                {
                    combined.push((
                        sel.dataset_version_id.clone(),
                        ver.hash_b3.clone(),
                        sel.weight,
                    ));
                }
            }
            if combined.is_empty() {
                None
            } else {
                Some(compute_combined_data_spec_hash(&combined))
            }
        } else if let (Some(database), Some(ds_id)) = (db, dataset_id) {
            match database.get_training_dataset(ds_id).await {
                Ok(Some(ds)) => Some(ds.hash_b3),
                _ => data_spec_hash_for_training.map(|s| s.to_string()),
            }
        } else {
            data_spec_hash_for_training.map(|s| s.to_string())
        };

    let seed_inputs_json = serde_json::to_string(&serde_json::json!({
        "dataset_version_ids": dataset_version_ids_for_training,
        "dataset_hash_b3": dataset_hash_for_metadata.clone(),
        "base_model_id": base_model_id,
        "job_id": job_id,
        "scope": scope_value,
    }))
    .unwrap_or_else(|_| "{}".to_string());

    let mut artifact_metadata = serde_json::Map::new();
    artifact_metadata.insert(
        "backend".to_string(),
        serde_json::json!(training_result.backend),
    );
    artifact_metadata.insert(
        "backend_device".to_string(),
        serde_json::json!(training_result.backend_device),
    );
    artifact_metadata.insert(
        "requested_backend".to_string(),
        serde_json::json!(worker_cfg.preferred_backend.map(|b| b.tag().to_string())),
    );
    artifact_metadata.insert(
        "coreml_training_fallback".to_string(),
        serde_json::json!(worker_cfg
            .coreml_fallback_backend
            .map(|b| b.tag().to_string())),
    );
    artifact_metadata.insert(
        "data_spec_hash".to_string(),
        serde_json::json!(data_spec_hash_for_training),
    );
    artifact_metadata.insert(
        "dataset_version_ids".to_string(),
        serde_json::json!(dataset_version_ids_for_training),
    );
    artifact_metadata.insert(
        "dataset_hash_b3".to_string(),
        serde_json::json!(dataset_hash_for_metadata.clone()),
    );
    artifact_metadata.insert(
        "synthetic_mode".to_string(),
        serde_json::json!(synthetic_mode),
    );
    artifact_metadata.insert(
        "data_lineage_mode".to_string(),
        serde_json::json!(data_lineage_mode.as_str()),
    );
    artifact_metadata.insert(
        "seed_inputs".to_string(),
        serde_json::from_str(&seed_inputs_json).unwrap_or(serde_json::Value::Null),
    );

    let packaged = match packager
        .package_aos_with_metadata(
            tenant,
            &adapter_id,
            &quantized_weights,
            &packager_cfg,
            base_model_for_manifest,
            package_metadata,
        )
        .await
    {
        Ok(p) => p,
        Err(e) => {
            error!(job_id = %job_id, error = %e, "Failed to package adapter");
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = TrainingJobStatus::Failed;
                job.error_message = Some(format!("Packaging failed: {}", e));
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
            drop(jobs);

            if let Some(database) = db {
                if let Err(db_err) = database.update_training_status(job_id, "failed").await {
                    warn!(job_id = %job_id, error = %db_err, "Failed to persist training failure status to DB (non-fatal)");
                }
            }

            return Err(e.into());
        }
    };

    info!(
        job_id = %job_id,
        adapter_id = %packaged.adapter_id,
        weights_path = %packaged.weights_path.display(),
        hash_b3 = %packaged.hash_b3,
        "Adapter packaged successfully"
    );

    let (final_aos_path, final_aos_hash, final_aos_size_bytes) = {
        let target = if let (Some(repo_name), Some(version_label)) = (
            versioning_snapshot.and_then(|v| v.repo_name.clone()),
            versioning_snapshot.and_then(|v| v.version_label.clone()),
        ) {
            let repo_dir = adapters_root.join(tenant).join(repo_name);
            if let Err(e) = tokio::fs::create_dir_all(&repo_dir).await {
                warn!(
                    job_id = %job_id,
                    error = %e,
                    "Failed to create repo directory for versioned artifact"
                );
            }
            let dest = repo_dir.join(format!("{}.aos", version_label));
            if dest != packaged.weights_path {
                if let Err(e) = tokio::fs::copy(&packaged.weights_path, &dest).await {
                    warn!(
                        job_id = %job_id,
                        error = %e,
                        dest = %dest.display(),
                        "Failed to copy packaged artifact to versioned path"
                    );
                }
            }
            dest
        } else {
            packaged.weights_path.clone()
        };

        let (hash, size_bytes) = tokio::fs::read(&target)
            .await
            .map(|bytes| {
                (
                    blake3::hash(&bytes).to_hex().to_string(),
                    bytes.len() as i64,
                )
            })
            .unwrap_or_else(|_| (packaged.hash_b3.clone(), 0));

        (target, hash, size_bytes)
    };
    let final_aos_path_str = final_aos_path.to_string_lossy().to_string();
    artifact_metadata.insert(
        "manifest_hash_b3".to_string(),
        serde_json::json!(final_aos_hash.clone()),
    );
    artifact_metadata.insert(
        "adapter_hash_b3".to_string(),
        serde_json::json!(packaged.hash_b3.clone()),
    );
    artifact_metadata.insert(
        "artifact_path".to_string(),
        serde_json::json!(final_aos_path_str.clone()),
    );
    artifact_metadata.insert("training_seed".to_string(), serde_json::json!(trainer_seed));

    if let Some(database) = db {
        let signature_b64 = match tokio::fs::read(final_aos_path.with_extension("aos.sig")).await {
            Ok(sig) => base64::engine::general_purpose::STANDARD.encode(sig),
            Err(e) => {
                warn!(
                    job_id = %job_id,
                    error = %e,
                    "Failed to read adapter signature; recording placeholder"
                );
                "unsigned".to_string()
            }
        };

        if let Err(e) = database
            .create_artifact(
                &packaged.hash_b3,
                "adapter",
                &signature_b64,
                None,
                final_aos_size_bytes,
                final_aos_path_str.as_str(),
            )
            .await
        {
            warn!(
                job_id = %job_id,
                adapter_id = %packaged.adapter_id,
                error = %e,
                "Failed to create adapter artifact record (non-fatal)"
            );
        }
    }

    // Step 3: Register adapter in database (if db available and register is enabled)
    if let Some(database) = db {
        if !post_actions.register {
            info!(
                job_id = %job_id,
                adapter_id = %packaged.adapter_id,
                "Adapter packaged but registration skipped per post_actions"
            );
            // Update job status to completed with artifact info but no registration
            let mut jobs = jobs_ref.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = TrainingJobStatus::Completed;
                job.progress_pct = 100.0;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                job.artifact_path = Some(final_aos_path_str.clone());
                job.adapter_id = Some(packaged.adapter_id.clone());
                job.weights_hash_b3 = Some(packaged.hash_b3.clone());
                job.aos_path = Some(final_aos_path_str.clone());
                job.package_hash_b3 = Some(final_aos_hash.clone());
                job.manifest_hash_b3 = Some(final_aos_hash.clone());
                job.dataset_hash_b3 = dataset_hash_for_metadata.clone();
                job.seed_inputs_json = Some(seed_inputs_json.clone());
                job.manifest_rank = Some(packaged.manifest.rank as u32);
                job.manifest_base_model = Some(packaged.manifest.base_model.clone());
                job.manifest_per_layer_hashes = Some(packaged.manifest.per_layer_hashes.is_some());
                job.signature_status = Some("signed".to_string());
            }
            drop(jobs);

            if let Err(e) = database.update_training_status(job_id, "completed").await {
                warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
            }

            // Persist artifact metadata even when registration is disabled.
            if let Err(e) = database
                .update_training_job_artifact(
                    job_id,
                    final_aos_path_str.as_str(),
                    &packaged.adapter_id,
                    &final_aos_hash,
                    Some(serde_json::Value::Object(artifact_metadata.clone())),
                )
                .await
            {
                warn!(
                    job_id = %job_id,
                    error = %e,
                    "Failed to persist training job artifact metadata (non-fatal)"
                );
            }

            if let Some(version_id) = versioning_snapshot.and_then(|v| v.adapter_version_id.clone())
            {
                let backend_lower = training_result
                    .backend
                    .as_deref()
                    .map(|b| b.to_ascii_lowercase());
                let coreml_used = training_result
                    .backend
                    .as_deref()
                    .map(|b| b.eq_ignore_ascii_case("coreml"));
                let artifact_result = database
                    .update_adapter_version_artifact(
                        &version_id,
                        "ready",
                        Some(final_aos_path_str.as_str()),
                        Some(&final_aos_hash),
                        data_spec_hash_for_training,
                        backend_lower.as_deref(),
                        coreml_used,
                        training_result.backend_device.as_deref(),
                        None,
                        None,
                        Some("orchestrator"),
                        Some("training_complete"),
                        Some(job_id),
                    )
                    .await;
                if let Err(e) = artifact_result {
                    warn!(
                        version_id = %version_id,
                        error = %e,
                        "Failed to mark adapter version ready (non-fatal)"
                    );
                } else if let Err(e) = database
                    .set_training_produced_version(job_id, &version_id, None)
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        version_id = %version_id,
                        error = %e,
                        "Failed to record produced version for training job (non-fatal)"
                    );
                }

                if coreml_used.unwrap_or(false) {
                    if let Some(repo_id) = versioning_snapshot.and_then(|v| v.repo_id.clone()) {
                        let tenant_for_repo = tenant_id.unwrap_or("default");
                        if let Ok(Some(policy)) = database
                            .get_adapter_repository_policy(tenant_for_repo, &repo_id)
                            .await
                        {
                            if policy.autopromote_coreml {
                                let _ = database
                                    .promote_adapter_version(
                                        tenant_for_repo,
                                        &repo_id,
                                        &version_id,
                                        Some("orchestrator"),
                                        Some("auto_coreml_promotion"),
                                    )
                                    .await;
                            }
                        }
                    }
                }

                info!(
                    job_id = %job_id,
                    version_id = %version_id,
                    branch = ?versioning_snapshot.and_then(|v| v.target_branch.clone()),
                    "history event: training_succeeded"
                );
            }

            return Ok(());
        }

        use adapteros_db::AdapterRegistrationBuilder;

        // Use category from request or default to "trained"
        let adapter_category = category.unwrap_or("code");
        let meta = &packaged.manifest.metadata;
        let domain = meta
            .get("domain")
            .cloned()
            .unwrap_or_else(|| "unspecified".to_string());
        let group = meta
            .get("group")
            .cloned()
            .unwrap_or_else(|| "unspecified".to_string());
        let scope_value = packaged.manifest.scope.clone();

        let reg_params = AdapterRegistrationBuilder::new()
            .tenant_id(tenant_id.unwrap_or("default"))
            .adapter_id(&packaged.adapter_id)
            .name(adapter_name)
            .hash_b3(&packaged.hash_b3)
            .rank(orchestrator_cfg.rank as i32)
            .tier(&post_actions.tier)
            .alpha(orchestrator_cfg.alpha as f64)
            .category(adapter_category)
            .scope(&scope_value)
            .domain(Some(domain))
            .purpose(Some(group))
            .base_model_id(base_model_id)
            .manifest_schema_version(Some(packaged.manifest.version.clone()))
            .content_hash_b3(Some(packaged.hash_b3.clone()))
            .aos_file_path(Some(packaged.weights_path.to_string_lossy().to_string()))
            .aos_file_hash(Some(packaged.hash_b3.clone()))
            .provenance_json(serde_json::to_string(&packaged.manifest.metadata).ok())
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build registration params: {}", e))?;

        match database.register_adapter(reg_params).await {
            Ok(db_id) => {
                info!(
                    job_id = %job_id,
                    adapter_id = %packaged.adapter_id,
                    db_id = %db_id,
                    "Adapter registered in database"
                );

                // Update training job with artifact metadata
                if let Err(e) = database
                    .update_training_job_artifact(
                        job_id,
                        final_aos_path_str.as_str(),
                        &packaged.adapter_id,
                        &final_aos_hash,
                        Some(serde_json::Value::Object(artifact_metadata.clone())),
                    )
                    .await
                {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %e,
                        "Failed to update job artifact metadata (non-fatal)"
                    );
                }

                if let Err(e) = database
                    .transition_adapter_lifecycle(
                        &packaged.adapter_id,
                        "ready",
                        "training_completed",
                        "system",
                    )
                    .await
                {
                    warn!(
                        job_id = %job_id,
                        adapter_id = %packaged.adapter_id,
                        error = %e,
                        "Failed to mark adapter ready after training"
                    );
                }

                // Link adapter back to training job for provenance
                if let Err(e) = database
                    .update_adapter_training_job_id(&packaged.adapter_id, job_id)
                    .await
                {
                    tracing::warn!(
                        job_id = %job_id,
                        adapter_id = %packaged.adapter_id,
                        error = %e,
                        "Failed to link adapter to training job (non-fatal)"
                    );
                }

                // Step 4: Optionally create stack with adapter (NOT set as default)
                if post_actions.create_stack {
                    let tenant_id_val = tenant_id.unwrap_or("default");
                    let stack_name = format!("stack.{}.{}", tenant_id_val, adapter_name);

                    use adapteros_db::traits::CreateStackRequest;
                    let stack_request = CreateStackRequest {
                        tenant_id: tenant_id_val.to_string(),
                        name: stack_name.clone(),
                        description: Some(format!(
                            "Auto-created stack for adapter {}",
                            adapter_name
                        )),
                        adapter_ids: vec![packaged.adapter_id.clone()],
                        workflow_type: Some("Sequential".to_string()),
                        determinism_mode: None,
                        routing_determinism_mode: None,
                    };

                    match database.insert_stack(&stack_request).await {
                        Ok(stack_id) => {
                            info!(
                                job_id = %job_id,
                                adapter_id = %packaged.adapter_id,
                                stack_id = %stack_id,
                                "Stack created automatically (not set as default)"
                            );

                            // Update training job with stack_id
                            {
                                let mut jobs = jobs_ref.write().await;
                                if let Some(job) = jobs.get_mut(job_id) {
                                    job.stack_id = Some(stack_id.clone());
                                }
                            }

                            // Persist stack_id and adapter_id to database
                            if let Err(e) = database
                                .update_training_job_result_ids(
                                    job_id,
                                    Some(&stack_id),
                                    Some(&packaged.adapter_id),
                                )
                                .await
                            {
                                warn!(job_id = %job_id, error = %e, "Failed to persist training job result IDs to database");
                            }

                            // Step 5: Optionally activate the stack
                            if post_actions.activate_stack {
                                match database.set_default_stack(tenant_id_val, &stack_id).await {
                                    Ok(_) => {
                                        info!(
                                            job_id = %job_id,
                                            tenant_id = %tenant_id_val,
                                            stack_id = %stack_id,
                                            "Stack activated as tenant default"
                                        );

                                        if let Err(e) =
                                            database.activate_stack(tenant_id_val, &stack_id).await
                                        {
                                            warn!(
                                                job_id = %job_id,
                                                tenant_id = %tenant_id_val,
                                                stack_id = %stack_id,
                                                error = %e,
                                                "Failed to mark stack active in DB after training (non-fatal)"
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        warn!(
                                            job_id = %job_id,
                                            tenant_id = %tenant_id_val,
                                            stack_id = %stack_id,
                                            error = %e,
                                            "Failed to activate stack (non-fatal)"
                                        );
                                    }
                                }
                            } else {
                                info!(
                                    job_id = %job_id,
                                    stack_id = %stack_id,
                                    "Stack created but NOT activated (activate_stack=false)"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                job_id = %job_id,
                                adapter_id = %packaged.adapter_id,
                                error = %e,
                                "Failed to create stack (non-fatal)"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                error!(job_id = %job_id, error = %e, "Failed to register adapter in database");
                let mut jobs = jobs_ref.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.status = TrainingJobStatus::Failed;
                    job.error_message = Some(format!("Registration failed: {}", e));
                    job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                }
                drop(jobs);

                if let Err(db_err) = database.update_training_status(job_id, "failed").await {
                    warn!(job_id = %job_id, error = %db_err, "Failed to persist training failure status to DB (non-fatal)");
                }

                return Err(e.into());
            }
        }
    } else {
        tracing::warn!(
            job_id = %job_id,
            "No database connection available, skipping adapter registration"
        );
    }

    // Step 5: Update job status to completed with artifact info
    let (initiated_by, initiated_by_role, tenant_id_for_audit) = {
        let mut jobs = jobs_ref.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = TrainingJobStatus::Completed;
            job.progress_pct = 100.0;
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            job.artifact_path = Some(final_aos_path_str.clone());
            job.adapter_id = Some(packaged.adapter_id.clone());
            job.weights_hash_b3 = Some(packaged.hash_b3.clone());
            job.aos_path = Some(final_aos_path_str.clone());
            job.package_hash_b3 = Some(final_aos_hash.clone());
            job.manifest_hash_b3 = Some(final_aos_hash.clone());
            job.dataset_hash_b3 = dataset_hash_for_metadata.clone();
            job.seed_inputs_json = Some(seed_inputs_json.clone());
            job.manifest_rank = Some(packaged.manifest.rank as u32);
            job.manifest_base_model = Some(packaged.manifest.base_model.clone());
            job.manifest_per_layer_hashes = Some(packaged.manifest.per_layer_hashes.is_some());
            job.signature_status = Some("signed".to_string());

            (
                job.initiated_by.clone(),
                job.initiated_by_role.clone(),
                job.tenant_id.clone(),
            )
        } else {
            (None, None, None)
        }
    };

    // Persist completion status to database
    if let Some(database) = db {
        if let Err(e) = database.update_training_status(job_id, "completed").await {
            warn!(job_id = %job_id, error = %e, "Failed to persist training completion status to DB (non-fatal)");
        }

        if let Some(version_id) = versioning_snapshot.and_then(|v| v.adapter_version_id.clone()) {
            let backend_lower = training_result
                .backend
                .as_deref()
                .map(|b| b.to_ascii_lowercase());
            let coreml_used = training_result
                .backend
                .as_deref()
                .map(|b| b.eq_ignore_ascii_case("coreml"));
            let artifact_result = database
                .update_adapter_version_artifact(
                    &version_id,
                    "ready",
                    Some(final_aos_path_str.as_str()),
                    Some(&final_aos_hash),
                    data_spec_hash_for_training,
                    backend_lower.as_deref(),
                    coreml_used,
                    training_result.backend_device.as_deref(),
                    None,
                    None,
                    Some("orchestrator"),
                    Some("training_complete"),
                    Some(job_id),
                )
                .await;
            if let Err(e) = artifact_result {
                warn!(
                    version_id = %version_id,
                    error = %e,
                    "Failed to mark adapter version ready (non-fatal)"
                );
            } else if let Err(e) = database
                .set_training_produced_version(job_id, &version_id, None)
                .await
            {
                warn!(
                    job_id = %job_id,
                    version_id = %version_id,
                    error = %e,
                    "Failed to record produced version for training job (non-fatal)"
                );
            }

            if coreml_used.unwrap_or(false) {
                if let Some(repo_id) = versioning_snapshot.and_then(|v| v.repo_id.clone()) {
                    let tenant_for_repo = tenant_id.unwrap_or("default");
                    if let Ok(Some(policy)) = database
                        .get_adapter_repository_policy(tenant_for_repo, &repo_id)
                        .await
                    {
                        if policy.autopromote_coreml {
                            let _ = database
                                .promote_adapter_version(
                                    tenant_for_repo,
                                    &repo_id,
                                    &version_id,
                                    Some("orchestrator"),
                                    Some("auto_coreml_promotion"),
                                )
                                .await;
                        }
                    }
                }
            }

            info!(
                job_id = %job_id,
                version_id = %version_id,
                branch = ?versioning_snapshot.and_then(|v| v.target_branch.clone()),
                "history event: training_succeeded"
            );
        }
    }

    // Optional CoreML export (post-training) - best-effort, does not change training status
    if orchestrator_cfg.enable_coreml_export.unwrap_or(false) {
        if let Err(e) = run_coreml_export_flow(
            jobs_ref.clone(),
            job_id,
            &packaged.adapter_id,
            &final_aos_path,
            &packaged.manifest.base_model,
            &packaged.hash_b3,
            adapters_root,
            tenant_id,
            db,
        )
        .await
        {
            warn!(
                job_id = %job_id,
                error = %e,
                "CoreML export failed (non-fatal)"
            );
        }
    } else {
        let mut jobs = jobs_ref.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.coreml_export_requested = Some(false);
        }
    }

    // Audit log: training completion
    if let (Some(database), Some(user_id), Some(user_role)) = (db, initiated_by, initiated_by_role)
    {
        let tenant_id_str = tenant_id_for_audit.unwrap_or_else(|| "system".to_string());

        if let Err(e) = database
            .log_audit(
                &user_id,
                &user_role,
                &tenant_id_str,
                "training.complete",
                "training_job",
                Some(job_id),
                "success",
                None,
                None,
                None,
            )
            .await
        {
            tracing::warn!(
                job_id = %job_id,
                error = %e,
                "Failed to log training completion audit event"
            );
        }
    }

    info!(
        job_id = %job_id,
        adapter_id = %packaged.adapter_id,
        "Training job completed successfully"
    );

    Ok(())
}
