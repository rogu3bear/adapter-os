//! KV storage for training jobs and metrics.
//!
//! Provides a simple KV mirror of `repository_training_jobs` and
//! `repository_training_metrics` for dual-write / KV-primary modes.

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingJobKv {
    pub id: String,
    pub repo_id: String,
    pub target_branch: Option<String>,
    pub base_version_id: Option<String>,
    pub draft_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub training_config_json: String,
    pub status: String,
    pub progress_json: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub created_by: String,
    pub adapter_name: Option<String>,
    pub template_id: Option<String>,
    pub created_at: Option<String>,
    pub metadata_json: Option<String>,
    pub config_hash_b3: Option<String>,
    pub dataset_id: Option<String>,
    pub correlation_id: Option<String>,
    /// Dataset version ID for provenance tracking and trust gating
    /// Evidence: migrations/0177_dataset_trust_gates.sql:67
    pub dataset_version_id: Option<String>,
    pub base_model_id: Option<String>,
    pub collection_id: Option<String>,
    pub tenant_id: Option<String>,
    pub build_id: Option<String>,
    pub source_documents_json: Option<String>,
    pub synthetic_mode: Option<bool>,
    pub data_lineage_mode: Option<String>,
    pub retryable: Option<i64>,
    pub retry_of_job_id: Option<String>,
    pub stack_id: Option<String>,
    pub adapter_id: Option<String>,
    pub weights_hash_b3: Option<String>,
    pub artifact_path: Option<String>,
    pub produced_version_id: Option<String>,
    pub hyperparameters_json: Option<String>,
    pub data_spec_json: Option<String>,
    pub metrics_snapshot_id: Option<String>,
    // Fields from migration 0247 - deterministic run tracking
    pub is_deterministic_run: Option<bool>,
    pub global_seed_hex: Option<String>,
    pub determinism_config_json: Option<String>,
    pub seed_mode: Option<String>,
    // Fields from migration 0253 - API contract alignment
    pub category: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub symbol_targets_json: Option<String>,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub lora_tier: Option<String>,
    pub lora_strength: Option<f64>,
    pub scope: Option<String>,
    pub api_patterns_json: Option<String>,
    pub repo_scope: Option<String>,
    pub file_patterns_json: Option<String>,
    pub exclude_patterns_json: Option<String>,
    pub backend: Option<String>,
    pub backend_reason: Option<String>,
    pub backend_device: Option<String>,
    pub dataset_hash_b3: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainingMetricKv {
    pub id: String,
    pub training_job_id: String,
    pub step: i64,
    pub epoch: Option<i64>,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_timestamp: Option<String>,
}

pub struct TrainingJobKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl TrainingJobKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn tenant_from_job(job: &TrainingJobKv) -> Result<&str> {
        job.tenant_id.as_deref().ok_or_else(|| {
            AosError::Validation("training job missing tenant_id for KV keying".into())
        })
    }

    fn job_key(tenant_id: &str, id: &str) -> String {
        format!("tenant/{tenant_id}/training_job/{id}")
    }

    fn job_repo_index(tenant_id: &str, repo_id: &str, created_at: &str, id: &str) -> String {
        format!("tenant/{tenant_id}/training_job_repo/{repo_id}/{created_at}:{id}")
    }

    fn job_status_index(tenant_id: &str, status: &str, created_at: &str, id: &str) -> String {
        format!("tenant/{tenant_id}/training_job_status/{status}/{created_at}:{id}")
    }

    fn job_tenant_index(tenant_id: &str, created_at: &str, id: &str) -> String {
        format!("tenant/{tenant_id}/training_jobs/{created_at}:{id}")
    }

    fn job_lookup_key(id: &str) -> String {
        format!("training_job_lookup/{id}")
    }

    fn metric_key(tenant_id: &str, id: &str) -> String {
        format!("tenant/{tenant_id}/training_metric/{id}")
    }

    fn metric_job_index(tenant_id: &str, job_id: &str, step: i64, id: &str) -> String {
        format!("tenant/{tenant_id}/training_metric_job/{job_id}:{step:020}:{id}")
    }

    #[allow(dead_code)]
    fn now_ts() -> String {
        Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    pub async fn put_job(&self, job: &TrainingJobKv) -> Result<()> {
        let tenant_id = Self::tenant_from_job(job)?;
        let bytes = serde_json::to_vec(job).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::job_key(tenant_id, &job.id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV store training job failed: {e}")))?;

        self.backend
            .set(
                &Self::job_repo_index(
                    tenant_id,
                    &job.repo_id,
                    job.created_at.as_deref().unwrap_or(&job.started_at),
                    &job.id,
                ),
                job.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV index job repo failed: {e}")))?;

        self.backend
            .set(
                &Self::job_status_index(
                    tenant_id,
                    &job.status,
                    job.created_at.as_deref().unwrap_or(&job.started_at),
                    &job.id,
                ),
                job.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV index job status failed: {e}")))?;

        self.backend
            .set(
                &Self::job_tenant_index(
                    tenant_id,
                    job.created_at.as_deref().unwrap_or(&job.started_at),
                    &job.id,
                ),
                job.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV index job tenant failed: {e}")))?;

        self.backend
            .set(
                &Self::job_lookup_key(&job.id),
                tenant_id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV job lookup write failed: {e}")))?;

        Ok(())
    }

    pub async fn get_job(&self, id: &str) -> Result<Option<TrainingJobKv>> {
        let Some(tenant_bytes) = self
            .backend
            .get(&Self::job_lookup_key(id))
            .await
            .map_err(|e| AosError::Database(format!("KV get training job lookup failed: {e}")))?
        else {
            return Ok(None);
        };
        let tenant_id = String::from_utf8(tenant_bytes).unwrap_or_default();

        let Some(bytes) = self
            .backend
            .get(&Self::job_key(&tenant_id, id))
            .await
            .map_err(|e| AosError::Database(format!("KV get training job failed: {e}")))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    async fn scan_jobs(&self, prefix: &str) -> Result<Vec<TrainingJobKv>> {
        let mut jobs = Vec::new();
        for key in self
            .backend
            .scan_prefix(prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan training jobs failed: {e}")))?
        {
            if let Some(bytes) = self
                .backend
                .get(&key)
                .await
                .map_err(|e| AosError::Database(format!("KV load training job failed: {e}")))?
            {
                if let Ok(job) = serde_json::from_slice::<TrainingJobKv>(&bytes) {
                    jobs.push(job);
                }
            }
        }
        Ok(jobs)
    }

    pub async fn list_jobs_for_repo(
        &self,
        repo_id: &str,
        limit: usize,
    ) -> Result<Vec<TrainingJobKv>> {
        let mut jobs = Vec::new();
        for key in self
            .backend
            .scan_prefix("tenant/")
            .await
            .map_err(|e| AosError::Database(format!("KV scan training jobs failed: {e}")))?
        {
            if key.contains(&format!("/training_job_repo/{repo_id}/")) {
                if let Some((_seq, id)) = key.rsplit_once(':') {
                    if let Some(job) = self.get_job(id).await? {
                        jobs.push(job);
                    }
                }
            }
        }
        jobs.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        jobs.truncate(limit);
        Ok(jobs)
    }

    pub async fn list_jobs_by_status(
        &self,
        status: &str,
        limit: usize,
    ) -> Result<Vec<TrainingJobKv>> {
        let mut jobs = Vec::new();
        for key in self
            .backend
            .scan_prefix("tenant/")
            .await
            .map_err(|e| AosError::Database(format!("KV scan training jobs failed: {e}")))?
        {
            if key.contains(&format!("/training_job_status/{status}/")) {
                if let Some((_seq, id)) = key.rsplit_once(':') {
                    if let Some(job) = self.get_job(id).await? {
                        jobs.push(job);
                    }
                }
            }
        }
        jobs.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        jobs.truncate(limit);
        Ok(jobs)
    }

    pub async fn list_jobs_for_tenant(&self, tenant_id: &str) -> Result<Vec<TrainingJobKv>> {
        let mut jobs = self
            .scan_jobs(&format!("tenant/{tenant_id}/training_jobs/"))
            .await?;
        jobs.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(jobs)
    }

    pub async fn list_all_jobs(&self) -> Result<Vec<TrainingJobKv>> {
        let mut jobs = Vec::new();
        for key in self
            .backend
            .scan_prefix("tenant/")
            .await
            .map_err(|e| AosError::Database(format!("KV scan training jobs failed: {e}")))?
        {
            if key.contains("/training_job/") && !key.contains("/metric") {
                if let Some((_, id)) = key.rsplit_once('/') {
                    if let Some(job) = self.get_job(id).await? {
                        jobs.push(job);
                    }
                }
            }
        }
        Ok(jobs)
    }

    pub async fn update_job(
        &self,
        id: &str,
        update_fn: impl FnOnce(&mut TrainingJobKv),
    ) -> Result<()> {
        if let Some(mut job) = self.get_job(id).await? {
            update_fn(&mut job);
            self.put_job(&job).await?;
        }
        Ok(())
    }

    pub async fn delete_job(&self, id: &str) -> Result<()> {
        let Some(job) = self.get_job(id).await? else {
            return Ok(());
        };

        self.backend
            .delete(&Self::job_key(Self::tenant_from_job(&job)?, id))
            .await
            .map_err(|e| AosError::Database(format!("KV delete training job failed: {e}")))?;

        // Clean up indexes best-effort
        let _ = self
            .backend
            .delete(&Self::job_repo_index(
                Self::tenant_from_job(&job)?,
                &job.repo_id,
                job.created_at.as_deref().unwrap_or(&job.started_at),
                &job.id,
            ))
            .await;
        let _ = self
            .backend
            .delete(&Self::job_status_index(
                Self::tenant_from_job(&job)?,
                &job.status,
                job.created_at.as_deref().unwrap_or(&job.started_at),
                &job.id,
            ))
            .await;
        let _ = self
            .backend
            .delete(&Self::job_tenant_index(
                Self::tenant_from_job(&job)?,
                job.created_at.as_deref().unwrap_or(&job.started_at),
                &job.id,
            ))
            .await;
        let _ = self.backend.delete(&Self::job_lookup_key(id)).await;
        Ok(())
    }

    pub async fn put_metric(&self, metric: &TrainingMetricKv) -> Result<()> {
        let Some(job) = self.get_job(&metric.training_job_id).await? else {
            return Err(AosError::NotFound(
                "Training job not found for metric".to_string(),
            ));
        };
        let tenant_id = Self::tenant_from_job(&job)?;

        let bytes = serde_json::to_vec(metric).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::metric_key(tenant_id, &metric.id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV store training metric failed: {e}")))?;

        self.backend
            .set(
                &Self::metric_job_index(
                    tenant_id,
                    &metric.training_job_id,
                    metric.step,
                    &metric.id,
                ),
                metric.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("KV index training metric failed: {e}")))?;
        Ok(())
    }

    pub async fn list_metrics(
        &self,
        job_id: &str,
        metric_name: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<TrainingMetricKv>> {
        let Some(job) = self.get_job(job_id).await? else {
            return Ok(Vec::new());
        };
        let tenant_id = Self::tenant_from_job(&job)?;
        let mut metrics = Vec::new();
        let prefix = format!("tenant/{tenant_id}/training_metric_job/{job_id}:");
        for key in self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("KV scan training metrics failed: {e}")))?
        {
            if let Some(bytes) =
                self.backend.get(&key).await.map_err(|e| {
                    AosError::Database(format!("KV load training metric failed: {e}"))
                })?
            {
                if let Ok(metric) = serde_json::from_slice::<TrainingMetricKv>(&bytes) {
                    if let Some(name) = metric_name {
                        if metric.metric_name != name {
                            continue;
                        }
                    }
                    metrics.push(metric);
                }
            }
        }

        metrics.sort_by(|a, b| a.step.cmp(&b.step).then_with(|| a.id.cmp(&b.id)));
        if let Some(lim) = limit {
            metrics.truncate(lim);
        }
        Ok(metrics)
    }
}
