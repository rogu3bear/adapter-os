use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;

use adapteros_config::SelfHostingMode;
use adapteros_core::AosError;
use adapteros_db::{
    adapter_repositories::AdapterVersion,
    repositories::Repository,
    training_datasets::{TrainingDataset, TrainingDatasetVersion},
    CreateVersionParams, RepositoryTrainingPolicy,
};
use adapteros_lora_worker::backend_factory::{detect_capabilities, BackendCapabilities};
use adapteros_orchestrator::{
    training::TrainingVersioningContext, TrainingConfig, TrainingJobStatus,
};
use adapteros_types::training::{
    DataLineageMode, DatasetVersionSelection, TrainingBackendKind, TrainingBackendPolicy,
    TrainingJob,
};
use blake3::hash;
use chrono::Utc;
use prometheus::{IntCounterVec, Opts};
use serde_json::json;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::state::{AppState, SelfHostingConfigApi};

const COREML_DRIFT_EPSILON: f32 = 1e-3;

#[derive(Clone)]
struct PendingPromotion {
    version_id: String,
    repo_id: String,
    branch: String,
    requested_backend: TrainingBackendKind,
    dataset_version_ids: Vec<DatasetVersionSelection>,
    data_spec_hash: Option<String>,
}

struct DatasetSelectionResult {
    selections: Vec<DatasetVersionSelection>,
    dataset_id: Option<String>,
    data_spec_hash: String,
}

struct BackendDecision {
    requested: TrainingBackendKind,
    fallback: Option<TrainingBackendKind>,
    policy: Option<TrainingBackendPolicy>,
}

struct SelfHostingMetrics {
    jobs_started: IntCounterVec,
    jobs_completed: IntCounterVec,
    promotions: IntCounterVec,
}

impl SelfHostingMetrics {
    fn new(registry: &prometheus::Registry) -> Option<SelfHostingMetrics> {
        let jobs_started = IntCounterVec::new(
            Opts::new(
                "self_hosting_training_started_total",
                "Self-hosted training jobs started by backend preference",
            )
            .namespace("adapteros"),
            &["backend"],
        )
        .ok()?;
        let jobs_completed = IntCounterVec::new(
            Opts::new(
                "self_hosting_training_completed_total",
                "Self-hosted training jobs completed by backend and status",
            )
            .namespace("adapteros"),
            &["backend", "status"],
        )
        .ok()?;
        let promotions = IntCounterVec::new(
            Opts::new(
                "self_hosting_promotions_total",
                "Self-hosted adapter promotions (active/rollback)",
            )
            .namespace("adapteros"),
            &["status"],
        )
        .ok()?;

        registry.register(Box::new(jobs_started.clone())).ok()?;
        registry.register(Box::new(jobs_completed.clone())).ok()?;
        registry.register(Box::new(promotions.clone())).ok()?;

        Some(SelfHostingMetrics {
            jobs_started,
            jobs_completed,
            promotions,
        })
    }
}

pub fn spawn_self_hosting_agent(state: AppState) -> Option<tokio::task::JoinHandle<()>> {
    let cfg = state.config.read().ok()?.self_hosting.clone();
    let mode = SelfHostingMode::from_str(&cfg.mode).unwrap_or(SelfHostingMode::Off);
    if matches!(mode, SelfHostingMode::Off) {
        return None;
    }

    let agent = SelfHostingAgent::new(state, mode, cfg);
    agent
        .state
        .background_task_tracker()
        .record_spawned("Self-hosting agent", false);
    Some(tokio::spawn(async move {
        info!(mode = ?agent.mode, "Self-hosting agent started");
        agent.run().await;
    }))
}

struct SelfHostingAgent {
    state: AppState,
    mode: SelfHostingMode,
    allowlist: HashSet<String>,
    promotion_threshold: f64,
    require_human_approval: bool,
    last_seen_commit: HashMap<String, String>,
    pending: HashMap<String, PendingPromotion>,
    metrics: Option<SelfHostingMetrics>,
}

impl SelfHostingAgent {
    fn new(state: AppState, mode: SelfHostingMode, cfg: SelfHostingConfigApi) -> Self {
        let allowlist = cfg.repo_allowlist.into_iter().collect();
        let metrics_registry = state.metrics_registry.inner();
        let metrics = SelfHostingMetrics::new(metrics_registry.as_ref());
        Self {
            state,
            mode,
            allowlist,
            promotion_threshold: cfg.promotion_threshold,
            require_human_approval: cfg.require_human_approval,
            last_seen_commit: HashMap::new(),
            pending: HashMap::new(),
            metrics,
        }
    }

    async fn run(mut self) {
        loop {
            if let Err(e) = self.tick().await {
                warn!(error = %e, "Self-hosting agent tick failed");
            }
            sleep(Duration::from_secs(60)).await;
        }
    }

    fn is_allowed(&self, repo_id: &str) -> bool {
        if self.allowlist.is_empty() {
            return true;
        }
        self.allowlist.contains(repo_id)
    }

    async fn load_policy(
        &self,
        repo_id: &str,
        tenant_id: &str,
    ) -> Result<RepositoryTrainingPolicy, AosError> {
        match self
            .state
            .db
            .get_repository_training_policy(tenant_id, repo_id)
            .await
        {
            Ok(Some(policy)) => {
                if policy.preferred_backends.is_empty() {
                    return Err(AosError::Validation(
                        "self-hosting policy missing preferred_backends".to_string(),
                    ));
                }
                if policy.trust_states.is_empty() {
                    return Err(AosError::Validation(
                        "self-hosting policy missing trust_states".to_string(),
                    ));
                }
                if policy.allowed_dataset_types.is_empty()
                    && policy.pinned_dataset_version_ids.is_empty()
                {
                    return Err(AosError::Validation(
                        "self-hosting policy missing dataset selection rule (allowed_dataset_types or pinned_dataset_version_ids)"
                            .to_string(),
                    ));
                }

                Ok(policy)
            }
            _ => Err(AosError::Validation(
                "self-hosting requires repository_training_policies entry for repo and tenant"
                    .to_string(),
            )),
        }
    }

    fn find_fallback_backend(
        &self,
        policy: &RepositoryTrainingPolicy,
        capabilities: &BackendCapabilities,
        skip: TrainingBackendKind,
    ) -> Option<TrainingBackendKind> {
        policy.preferred_backends.iter().copied().find(|backend| {
            if *backend == skip {
                return false;
            }
            match backend {
                TrainingBackendKind::CoreML => {
                    policy.coreml_allowed && capabilities.has_coreml && capabilities.has_ane
                }
                TrainingBackendKind::Mlx => capabilities.has_mlx,
                TrainingBackendKind::Metal => capabilities.has_metal,
                TrainingBackendKind::Cpu => true,
                TrainingBackendKind::Auto => false,
            }
        })
    }

    fn select_backend(
        &self,
        policy: &RepositoryTrainingPolicy,
        capabilities: &BackendCapabilities,
    ) -> Result<BackendDecision, AosError> {
        if policy.coreml_required && !policy.coreml_allowed {
            return Err(AosError::Validation(
                "coreml_required set but coreml_allowed is false".to_string(),
            ));
        }

        for backend in &policy.preferred_backends {
            let decision = match backend {
                TrainingBackendKind::CoreML => {
                    if !policy.coreml_allowed {
                        continue;
                    }
                    if capabilities.has_coreml && capabilities.has_ane {
                        let fallback = if policy.coreml_required {
                            None
                        } else {
                            self.find_fallback_backend(
                                policy,
                                capabilities,
                                TrainingBackendKind::CoreML,
                            )
                        };
                        Some(BackendDecision {
                            requested: TrainingBackendKind::CoreML,
                            fallback,
                            policy: Some(if policy.coreml_required {
                                TrainingBackendPolicy::CoremlOnly
                            } else {
                                TrainingBackendPolicy::CoremlElseFallback
                            }),
                        })
                    } else if policy.coreml_required {
                        return Err(AosError::Validation(
                            "coreml_required but CoreML device unavailable".to_string(),
                        ));
                    } else {
                        None
                    }
                }
                TrainingBackendKind::Mlx => {
                    if capabilities.has_mlx {
                        Some(BackendDecision {
                            requested: TrainingBackendKind::Mlx,
                            fallback: None,
                            policy: Some(TrainingBackendPolicy::Auto),
                        })
                    } else {
                        None
                    }
                }
                TrainingBackendKind::Metal => {
                    if capabilities.has_metal {
                        Some(BackendDecision {
                            requested: TrainingBackendKind::Metal,
                            fallback: None,
                            policy: Some(TrainingBackendPolicy::Auto),
                        })
                    } else {
                        None
                    }
                }
                TrainingBackendKind::Cpu => Some(BackendDecision {
                    requested: TrainingBackendKind::Cpu,
                    fallback: None,
                    policy: Some(TrainingBackendPolicy::Auto),
                }),
                TrainingBackendKind::Auto => None,
            };

            if let Some(decision) = decision {
                return Ok(decision);
            }
        }

        Err(AosError::Validation(
            "No backend available for self-hosting policy".to_string(),
        ))
    }

    async fn select_datasets(
        &self,
        policy: &RepositoryTrainingPolicy,
        tenant_id: &str,
    ) -> Result<DatasetSelectionResult, AosError> {
        let mut candidates: HashMap<String, Vec<(TrainingDatasetVersion, String)>> = HashMap::new();

        let versions = self.state.db.list_all_dataset_versions().await?;

        for version in versions {
            if version.soft_deleted_at.is_some() {
                continue;
            }

            let dataset: TrainingDataset = match self
                .state
                .db
                .get_training_dataset(&version.dataset_id)
                .await?
            {
                Some(ds) => ds,
                None => continue,
            };

            if let Some(dataset_tenant) = dataset.tenant_id.as_deref() {
                if dataset_tenant != tenant_id {
                    continue;
                }
            }

            if let Some(version_tenant) = version.tenant_id.as_deref() {
                if version_tenant != tenant_id {
                    continue;
                }
            }

            let dataset_type = dataset
                .dataset_type
                .clone()
                .unwrap_or_else(|| "train".to_string())
                .to_ascii_lowercase();
            if !policy
                .allowed_dataset_types
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&dataset_type))
            {
                continue;
            }

            if !version.validation_status.eq_ignore_ascii_case("valid") {
                continue;
            }

            let effective_trust = self
                .state
                .db
                .get_effective_trust_state(&version.id)
                .await?
                .unwrap_or_else(|| version.trust_state.clone());
            if !policy
                .trust_states
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&effective_trust))
            {
                debug!(
                    dataset_version_id = %version.id,
                    trust_state = %effective_trust,
                    "Dataset skipped due to trust gating"
                );
                continue;
            }

            candidates
                .entry(version.dataset_id.clone())
                .or_default()
                .push((version, dataset_type));
        }

        let mut selections = Vec::new();
        let mut dataset_ids = HashSet::new();
        if policy.pinned_dataset_version_ids.is_empty() {
            for (dataset_id, mut versions) in candidates {
                versions.sort_by_key(|(v, _)| v.version_number);
                if let Some((selected, _)) = versions.pop() {
                    dataset_ids.insert(dataset_id);
                    selections.push(DatasetVersionSelection {
                        dataset_version_id: selected.id.clone(),
                        weight: 1.0,
                    });
                }
            }
        } else {
            let mut pinned_found = 0usize;
            for pinned in &policy.pinned_dataset_version_ids {
                if let Some((version, _)) = candidates
                    .values()
                    .flat_map(|v| v.iter())
                    .find(|(v, _)| &v.id == pinned)
                {
                    pinned_found += 1;
                    dataset_ids.insert(version.dataset_id.clone());
                    selections.push(DatasetVersionSelection {
                        dataset_version_id: version.id.clone(),
                        weight: 1.0,
                    });
                }
            }

            if pinned_found != policy.pinned_dataset_version_ids.len() {
                return Err(AosError::Validation(
                    "Pinned datasets not available or blocked by trust policy".to_string(),
                ));
            }
        }

        if selections.is_empty() {
            return Err(AosError::Validation(
                "No trainable datasets matched policy and trust gates".to_string(),
            ));
        }

        selections.sort_by(|a, b| a.dataset_version_id.cmp(&b.dataset_version_id));

        let dataset_id = if dataset_ids.len() == 1 {
            dataset_ids.into_iter().next()
        } else {
            None
        };

        let data_spec_hash = compute_data_spec_hash(&selections);

        Ok(DatasetSelectionResult {
            selections,
            dataset_id,
            data_spec_hash,
        })
    }

    async fn create_blocked_version(
        &self,
        adapter_repo_id: &str,
        branch: &str,
        parent_version_id: Option<&str>,
        commit_sha: &str,
        reason: &str,
        details: &str,
    ) -> Result<(), AosError> {
        let version_label = format!("v{}", Utc::now().format("%Y%m%d%H%M%S"));
        let summary = json!({
            "self_hosting": {
                "status": "failed",
                "reason": reason,
                "details": details,
            }
        })
        .to_string();

        let _ = self
            .state
            .db
            .create_adapter_version(CreateVersionParams {
                repo_id: adapter_repo_id,
                tenant_id: "system",
                version: &version_label,
                branch,
                branch_classification: "protected",
                aos_path: None,
                aos_hash: None,
                manifest_schema_version: None,
                parent_version_id,
                code_commit_sha: Some(commit_sha),
                data_spec_hash: None,
                training_backend: None,
                coreml_used: None,
                coreml_device_type: None,
                dataset_version_ids: None,
                release_state: "failed",
                metrics_snapshot_id: None,
                evaluation_summary: Some(&summary),
                allow_archived: false,
                actor: Some("self-hosting-agent"),
                reason: Some(reason),
                train_job_id: None,
            })
            .await?;

        Ok(())
    }

    async fn tick(&mut self) -> Result<(), AosError> {
        let repos = self
            .state
            .db
            .list_repositories("system", 100, 0)
            .await
            .unwrap_or_default();

        for repo in repos {
            if !self.is_allowed(&repo.repo_id) {
                continue;
            }
            self.handle_repo(repo).await?;
        }

        self.check_promotions().await;
        Ok(())
    }

    async fn handle_repo(&mut self, repo: Repository) -> Result<(), AosError> {
        let commit = match repo.latest_scan_commit {
            Some(ref sha) if !sha.is_empty() => sha.clone(),
            _ => return Ok(()),
        };

        if self
            .last_seen_commit
            .get(&repo.repo_id)
            .map(|seen| seen == &commit)
            .unwrap_or(false)
        {
            return Ok(());
        }

        self.last_seen_commit
            .insert(repo.repo_id.clone(), commit.clone());

        // Launch training for the new commit
        match self.start_training_for_repo(&repo, &commit).await {
            Ok(job_id) => {
                info!(
                    repo_id = %repo.repo_id,
                    commit = %commit,
                    job_id = %job_id,
                    "Self-hosting agent started training for new commit"
                );
            }
            Err(e) => {
                warn!(
                    repo_id = %repo.repo_id,
                    commit = %commit,
                    error = %e,
                    "Self-hosting agent failed to start training"
                );
            }
        }

        Ok(())
    }

    async fn start_training_for_repo(
        &mut self,
        repo: &Repository,
        commit_sha: &str,
    ) -> Result<String, AosError> {
        // Adapter repo must exist under system tenant
        let adapter_repo = match self
            .state
            .db
            .get_adapter_repository("system", &repo.repo_id)
            .await?
        {
            Some(r) => r,
            None => {
                warn!(
                    repo_id = %repo.repo_id,
                    "Skipping self-hosting training; adapter repository not found"
                );
                return Err(AosError::NotFound("adapter repository missing".into()));
            }
        };

        let branch = repo.default_branch.clone();
        let parent_version_id = self
            .state
            .db
            .find_active_version_for_branch(&adapter_repo.id, "system", &branch)
            .await?
            .map(|v: AdapterVersion| v.id);

        let policy = match self.load_policy(&repo.repo_id, "system").await {
            Ok(p) => p,
            Err(e) => {
                let _ = self
                    .create_blocked_version(
                        &adapter_repo.id,
                        &branch,
                        parent_version_id.as_deref(),
                        commit_sha,
                        "policy_missing",
                        &e.to_string(),
                    )
                    .await;
                return Err(e);
            }
        };
        let capabilities = detect_capabilities();

        let dataset_selection = match self.select_datasets(&policy, "system").await {
            Ok(sel) => sel,
            Err(e) => {
                let _ = self
                    .create_blocked_version(
                        &adapter_repo.id,
                        &branch,
                        parent_version_id.as_deref(),
                        commit_sha,
                        "dataset_selection_failed",
                        &e.to_string(),
                    )
                    .await;
                return Err(e);
            }
        };

        let backend_decision = match self.select_backend(&policy, &capabilities) {
            Ok(decision) => decision,
            Err(e) => {
                let _ = self
                    .create_blocked_version(
                        &adapter_repo.id,
                        &branch,
                        parent_version_id.as_deref(),
                        commit_sha,
                        "backend_selection_failed",
                        &e.to_string(),
                    )
                    .await;
                return Err(e);
            }
        };

        let version_label = format!("v{}", Utc::now().format("%Y%m%d%H%M%S"));
        let dataset_ids: Vec<String> = dataset_selection
            .selections
            .iter()
            .map(|s| s.dataset_version_id.clone())
            .collect();

        let initial_summary = json!({
            "self_hosting": {
                "status": "scheduled",
                "requested_backend": backend_decision.requested.as_str(),
                "fallback_backend": backend_decision.fallback.map(|b| b.as_str()),
                "dataset_version_ids": dataset_selection
                    .selections
                    .iter()
                    .map(|s| s.dataset_version_id.clone())
                    .collect::<Vec<_>>(),
            }
        })
        .to_string();
        let version_id = self
            .state
            .db
            .create_adapter_version(CreateVersionParams {
                repo_id: &adapter_repo.id,
                tenant_id: "system",
                version: &version_label,
                branch: &branch,
                branch_classification: "protected",
                aos_path: None,
                aos_hash: None,
                manifest_schema_version: None,
                parent_version_id: parent_version_id.as_deref(),
                code_commit_sha: Some(commit_sha),
                data_spec_hash: Some(&dataset_selection.data_spec_hash),
                training_backend: Some(backend_decision.requested.as_str()),
                coreml_used: None,
                coreml_device_type: None,
                dataset_version_ids: Some(dataset_ids.as_slice()),
                release_state: "draft",
                metrics_snapshot_id: None,
                evaluation_summary: Some(&initial_summary),
                allow_archived: false,
                actor: Some("self-hosting-agent"),
                reason: Some("self_hosting_bootstrap"),
                train_job_id: None,
            })
            .await?;

        let versioning_context = TrainingVersioningContext {
            adapter_version_id: version_id.clone(),
            version_label: version_label.clone(),
            branch: branch.clone(),
            repo_id: adapter_repo.id.clone(),
            repo_name: adapter_repo.name.clone(),
            parent_version_id: parent_version_id.clone(),
            draft_version_id: Some(version_id.clone()),
            code_commit_sha: Some(commit_sha.to_string()),
            data_spec_json: None,
            data_spec_hash: Some(dataset_selection.data_spec_hash.clone()),
        };

        let mut training_config = TrainingConfig::default_for_adapter();
        training_config.preferred_backend = Some(backend_decision.requested);
        training_config.backend_policy = backend_decision.policy;
        training_config.coreml_training_fallback = backend_decision.fallback;
        training_config.require_gpu = matches!(
            backend_decision.requested,
            TrainingBackendKind::CoreML | TrainingBackendKind::Metal | TrainingBackendKind::Mlx
        );

        let job = self
            .state
            .training_service
            .start_training(
                format!("auto-{}", repo.repo_id),
                training_config,
                None,
                Some(adapter_repo.id.clone()),
                Some(branch.clone()),
                parent_version_id.clone(),
                dataset_selection.dataset_id.clone(),
                Some(dataset_selection.selections.clone()),
                false,                                  // synthetic_mode
                DataLineageMode::Versioned,             // data_lineage_mode
                Some("system".to_string()),             // tenant_id
                Some("self-hosting-agent".to_string()), // initiated_by
                Some("system".to_string()),             // initiated_by_role
                adapter_repo.base_model_id.clone(),
                None,
                Some("tenant".to_string()),
                None,
                Some("codebase".to_string()),
                Some(format!("Self-hosted training for {}", repo.repo_id)),
                None,
                None,
                None,
                None,
                None,
                Some(versioning_context),
                Some(commit_sha.to_string()),
                None, // data_spec_json
                Some(dataset_selection.data_spec_hash.clone()),
            )
            .await?;

        let training_summary = json!({
            "self_hosting": {
                "status": "training",
                "job_id": job.id,
                "requested_backend": backend_decision.requested.as_str(),
                "fallback_backend": backend_decision.fallback.map(|b| b.as_str()),
                "dataset_version_ids": dataset_selection
                    .selections
                    .iter()
                    .map(|s| s.dataset_version_id.clone())
                    .collect::<Vec<_>>(),
            }
        })
        .to_string();

        self.state
            .db
            .update_adapter_version_artifact(
                &version_id,
                "training",
                None,
                None,
                Some(&dataset_selection.data_spec_hash),
                Some(backend_decision.requested.as_str()),
                None,
                None,
                None,
                Some(&training_summary),
                Some("self-hosting-agent"),
                Some("self_hosting_training_start"),
                Some(&job.id),
            )
            .await?;

        if let Some(metrics) = &self.metrics {
            metrics
                .jobs_started
                .with_label_values(&[backend_decision.requested.as_str()])
                .inc();
        }

        self.pending.insert(
            job.id.clone(),
            PendingPromotion {
                version_id,
                repo_id: adapter_repo.id.clone(),
                branch,
                requested_backend: backend_decision.requested,
                dataset_version_ids: dataset_selection.selections,
                data_spec_hash: Some(dataset_selection.data_spec_hash),
            },
        );

        Ok(job.id)
    }

    async fn check_promotions(&mut self) {
        let jobs = match self.state.training_service.list_jobs().await {
            Ok(j) => j,
            Err(e) => {
                warn!(error = %e, "Self-hosting agent failed to list jobs");
                return;
            }
        };

        let mut completed_groups: HashMap<
            (String, String, String),
            Vec<(TrainingJob, PendingPromotion)>,
        > = HashMap::new();
        let mut to_remove = Vec::new();

        for (job_id, pending) in self.pending.iter() {
            if let Some(job) = jobs.iter().find(|j| &j.id == job_id) {
                match job.status {
                    TrainingJobStatus::Completed => {
                        if let Err(e) = self.mark_training_complete(job, pending).await {
                            warn!(
                                error = %e,
                                job_id = %job.id,
                                version_id = %pending.version_id,
                                "Self-hosting agent failed to mark training complete"
                            );
                            to_remove.push(job_id.clone());
                            continue;
                        }

                        let key = (
                            pending.repo_id.clone(),
                            pending.branch.clone(),
                            self.resolve_data_spec_hash(job, pending),
                        );
                        completed_groups
                            .entry(key)
                            .or_default()
                            .push((job.clone(), pending.clone()));
                    }
                    TrainingJobStatus::Failed => {
                        if let Err(e) = self.mark_training_failed(job, pending).await {
                            warn!(
                                error = %e,
                                job_id = %job.id,
                                version_id = %pending.version_id,
                                "Self-hosting agent failed to mark training failure"
                            );
                        }
                        if let Some(metrics) = &self.metrics {
                            let backend = job
                                .backend
                                .as_deref()
                                .unwrap_or_else(|| pending.requested_backend.as_str());
                            metrics
                                .jobs_completed
                                .with_label_values(&[backend, "failed"])
                                .inc();
                        }
                        to_remove.push(job_id.clone());
                    }
                    _ => {}
                }
            }
        }

        for (_key, entries) in completed_groups {
            if entries.is_empty() {
                continue;
            }

            let winner = self.pick_winner(&entries);
            let winner_id = winner.map(|j| j.id.clone());

            for (job, pending) in entries {
                if Some(job.id.clone()) == winner_id {
                    if self.require_human_approval {
                        info!(
                            job_id = %job.id,
                            version_id = %pending.version_id,
                            "Self-hosting agent awaiting human approval (safe mode)"
                        );
                    } else if self.should_promote(&job) {
                        if let Err(e) = self
                            .promote_version(
                                &pending.version_id,
                                &pending.repo_id,
                                &pending.branch,
                                Some(&job),
                            )
                            .await
                        {
                            warn!(
                                error = %e,
                                job_id = %job.id,
                                version_id = %pending.version_id,
                                "Self-hosting agent failed to promote version"
                            );
                        } else if let Some(metrics) = &self.metrics {
                            metrics.promotions.with_label_values(&["promoted"]).inc();
                        }
                    }
                    to_remove.push(job.id.clone());
                } else {
                    to_remove.push(job.id.clone());
                }
            }
        }

        for job_id in to_remove {
            self.pending.remove(&job_id);
        }
    }

    fn should_promote(&self, job: &TrainingJob) -> bool {
        if let Some(hash) = job.data_spec_hash.as_ref() {
            // Presence of evaluated data spec hash indicates downstream validation ran
            if !hash.is_empty() && self.promotion_threshold <= 0.0 {
                return true;
            }
        }

        if self.promotion_threshold <= 0.0 {
            return true;
        }

        job.current_loss <= self.promotion_threshold as f32
    }

    async fn mark_training_complete(
        &self,
        job: &TrainingJob,
        pending: &PendingPromotion,
    ) -> Result<(), AosError> {
        let backend = job
            .backend
            .clone()
            .unwrap_or_else(|| pending.requested_backend.as_str().to_string());
        let data_spec_hash = job
            .data_spec_hash
            .clone()
            .or_else(|| pending.data_spec_hash.clone());
        let dataset_version_ids = job
            .dataset_version_ids
            .clone()
            .unwrap_or_else(|| pending.dataset_version_ids.clone());

        let summary = json!({
            "self_hosting": {
                "status": "completed",
                "job_id": job.id,
                "requested_backend": job
                    .requested_backend
                    .clone()
                    .or_else(|| Some(pending.requested_backend.as_str().to_string())),
                "backend_used": backend,
                "backend_reason": job.backend_reason,
                "current_loss": job.current_loss,
                "dataset_version_ids": dataset_version_ids
                    .iter()
                    .map(|s| s.dataset_version_id.clone())
                    .collect::<Vec<_>>(),
                "coreml_export_status": job.coreml_export_status,
                "data_spec_hash": data_spec_hash,
            }
        })
        .to_string();

        self.state
            .db
            .update_adapter_version_artifact(
                &pending.version_id,
                "ready",
                None,
                None,
                data_spec_hash.as_deref(),
                Some(&backend),
                Some(backend.eq_ignore_ascii_case("coreml")),
                job.backend_device.as_deref(),
                None,
                Some(&summary),
                Some("self-hosting-agent"),
                Some("self_hosting_training_complete"),
                Some(&job.id),
            )
            .await?;

        if let Some(metrics) = &self.metrics {
            metrics
                .jobs_completed
                .with_label_values(&[backend.as_str(), "completed"])
                .inc();
        }

        Ok(())
    }

    async fn mark_training_failed(
        &self,
        job: &TrainingJob,
        pending: &PendingPromotion,
    ) -> Result<(), AosError> {
        let backend = job
            .backend
            .clone()
            .unwrap_or_else(|| pending.requested_backend.as_str().to_string());
        let summary = json!({
            "self_hosting": {
                "status": "failed",
                "job_id": job.id,
                "requested_backend": job
                    .requested_backend
                    .clone()
                    .or_else(|| Some(pending.requested_backend.as_str().to_string())),
                "backend_used": backend,
                "error": job.error_message,
                "dataset_version_ids": job
                    .dataset_version_ids
                    .clone()
                    .unwrap_or_else(|| pending.dataset_version_ids.clone())
                    .iter()
                    .map(|s| s.dataset_version_id.clone())
                    .collect::<Vec<_>>(),
            }
        })
        .to_string();

        self.state
            .db
            .update_adapter_version_artifact(
                &pending.version_id,
                "failed",
                None,
                None,
                pending.data_spec_hash.as_deref(),
                Some(&backend),
                Some(backend.eq_ignore_ascii_case("coreml")),
                job.backend_device.as_deref(),
                None,
                Some(&summary),
                Some("self-hosting-agent"),
                Some("self_hosting_training_failed"),
                Some(&job.id),
            )
            .await
    }

    fn resolve_backend(&self, job: &TrainingJob, pending: &PendingPromotion) -> String {
        job.backend
            .clone()
            .or_else(|| job.requested_backend.clone())
            .unwrap_or_else(|| pending.requested_backend.as_str().to_string())
    }

    fn parse_backend_kind(kind: &str) -> Option<TrainingBackendKind> {
        match kind.to_ascii_lowercase().as_str() {
            "coreml" => Some(TrainingBackendKind::CoreML),
            "cpu" => Some(TrainingBackendKind::Cpu),
            "mlx" => Some(TrainingBackendKind::Mlx),
            "metal" => Some(TrainingBackendKind::Metal),
            "auto" => Some(TrainingBackendKind::Auto),
            _ => None,
        }
    }

    fn resolve_data_spec_hash(&self, job: &TrainingJob, pending: &PendingPromotion) -> String {
        job.data_spec_hash
            .clone()
            .or_else(|| pending.data_spec_hash.clone())
            .unwrap_or_default()
    }

    fn pick_winner<'a>(
        &self,
        entries: &'a [(TrainingJob, PendingPromotion)],
    ) -> Option<&'a TrainingJob> {
        let best_by_backend =
            |target: TrainingBackendKind| -> Option<&'a (TrainingJob, PendingPromotion)> {
                entries
                    .iter()
                    .filter(|(job, pending)| {
                        SelfHostingAgent::parse_backend_kind(&self.resolve_backend(job, pending))
                            == Some(target)
                    })
                    .min_by(|(a, _), (b, _)| {
                        let loss_cmp = a
                            .current_loss
                            .partial_cmp(&b.current_loss)
                            .unwrap_or(Ordering::Equal);
                        if loss_cmp != Ordering::Equal {
                            loss_cmp
                        } else {
                            b.id.cmp(&a.id)
                        }
                    })
            };

        let best_cpu = best_by_backend(TrainingBackendKind::Cpu);
        let best_coreml = best_by_backend(TrainingBackendKind::CoreML);

        if let (Some(cpu), Some(coreml)) = (best_cpu, best_coreml) {
            let diff = (coreml.0.current_loss - cpu.0.current_loss).abs();
            if diff <= COREML_DRIFT_EPSILON {
                return Some(&coreml.0);
            } else {
                return Some(&cpu.0);
            }
        }

        entries
            .iter()
            .min_by(|(a, _), (b, _)| {
                let loss_cmp = a
                    .current_loss
                    .partial_cmp(&b.current_loss)
                    .unwrap_or(Ordering::Equal);
                if loss_cmp != Ordering::Equal {
                    loss_cmp
                } else {
                    b.id.cmp(&a.id)
                }
            })
            .map(|(job, _)| job)
    }

    async fn promote_version(
        &self,
        version_id: &str,
        repo_id: &str,
        branch: &str,
        job: Option<&TrainingJob>,
    ) -> Result<(), AosError> {
        // Deprecate any active version on the branch first
        let previous_active = if let Some(active) = self
            .state
            .db
            .find_active_version_for_branch(repo_id, "system", branch)
            .await?
        {
            if active.id != version_id {
                if let Err(e) = self
                    .state
                    .db
                    .set_adapter_version_state_with_metadata(
                        &active.id,
                        "deprecated",
                        Some("superseded by self-hosting agent"),
                        Some("self-hosting-agent"),
                        Some("self_hosting_promotion"),
                        job.map(|j| j.id.as_str()),
                    )
                    .await
                {
                    warn!(
                        error = %e,
                        active_version = %active.id,
                        "Failed to deprecate previous active version"
                    );
                    return Err(e);
                }
            }
            Some(active.id)
        } else {
            None
        };

        if let Err(e) = self
            .state
            .db
            .set_adapter_version_state_with_metadata(
                version_id,
                "active",
                Some("self-hosting agent promotion"),
                Some("self-hosting-agent"),
                Some("self_hosting_promotion"),
                job.map(|j| j.id.as_str()),
            )
            .await
        {
            // Best-effort rollback to previous active on failure
            if let Some(active_id) = previous_active {
                let _ = self
                    .state
                    .db
                    .set_adapter_version_state_with_metadata(
                        &active_id,
                        "active",
                        Some("rollback after failed self-hosting promotion"),
                        Some("self-hosting-agent"),
                        Some("self_hosting_promotion_rollback"),
                        job.map(|j| j.id.as_str()),
                    )
                    .await;
                if let Some(metrics) = &self.metrics {
                    metrics.promotions.with_label_values(&["rollback"]).inc();
                }
            }
            return Err(e);
        }

        Ok(())
    }
}

fn compute_data_spec_hash(selections: &[DatasetVersionSelection]) -> String {
    let mut parts: Vec<String> = selections
        .iter()
        .map(|s| format!("{}:{:.4}", s.dataset_version_id, s.weight))
        .collect();
    parts.sort();
    hash(parts.join("|").as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_db::adapter_repositories::CreateRepositoryParams;
    use adapteros_lora_worker::memory::UmaPressureMonitor;
    use adapteros_metrics_exporter::MetricsExporter;
    use adapteros_telemetry::MetricsCollector;
    use std::sync::{Arc, RwLock};

    async fn build_agent() -> SelfHostingAgent {
        let db = adapteros_db::Db::new_in_memory().await.unwrap();
        adapteros_db::sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name) VALUES ('system', 'System')",
        )
        .execute(db.pool())
        .await
        .unwrap();
        let config = Arc::new(RwLock::new(crate::state::ApiConfig::default()));
        let metrics_exporter =
            Arc::new(MetricsExporter::new(vec![0.1, 0.5, 1.0]).expect("metrics exporter"));
        let metrics_collector = Arc::new(MetricsCollector::new(Default::default()));
        let metrics_registry = Arc::new(crate::telemetry::MetricsRegistry::new());
        let uma_monitor = Arc::new(UmaPressureMonitor::new(15, None));

        let state = AppState::new(
            db,
            b"test-secret-self-hosting".to_vec(),
            config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            uma_monitor,
        );

        let cfg = SelfHostingConfigApi {
            mode: "on".to_string(),
            repo_allowlist: vec![],
            promotion_threshold: 0.0,
            require_human_approval: false,
        };

        SelfHostingAgent::new(state, SelfHostingMode::On, cfg)
    }

    #[tokio::test]
    async fn coreml_policy_respects_allow_and_forbid() {
        let agent = build_agent().await;
        let mut policy = RepositoryTrainingPolicy {
            repo_id: "repo1".to_string(),
            tenant_id: "system".to_string(),
            ..Default::default()
        };
        let mut caps = BackendCapabilities {
            has_metal: false,
            metal_device_name: None,
            has_ane: true,
            has_coreml: true,
            has_mlx: false,
            has_mlx_bridge: false,
            gpu_memory_bytes: None,
        };

        let decision = agent.select_backend(&policy, &caps).unwrap();
        assert_eq!(decision.requested, TrainingBackendKind::CoreML);

        policy.coreml_allowed = false;
        policy.preferred_backends = vec![TrainingBackendKind::CoreML, TrainingBackendKind::Metal];
        caps.has_metal = true;
        let decision = agent.select_backend(&policy, &caps).unwrap();
        assert_eq!(decision.requested, TrainingBackendKind::Metal);
    }

    #[tokio::test]
    async fn datasets_blocked_by_trust_policy_are_rejected() {
        let agent = build_agent().await;
        let ds_id = agent
            .state
            .db
            .create_training_dataset(
                "ds",
                None,
                "jsonl",
                "hash",
                "var/test-data/path",
                None, // created_by must be valid user ID or None (FK constraint)
                None,
                Some("ready"),
                Some("hash"),
                None,
            )
            .await
            .unwrap();
        adapteros_db::sqlx::query(
            "UPDATE training_datasets SET tenant_id = 'system', dataset_type = 'training', validation_status = 'valid' WHERE id = ?",
        )
        .bind(&ds_id)
        .execute(agent.state.db.pool())
        .await
        .unwrap();

        let version_id = agent
            .state
            .db
            .create_training_dataset_version(
                &ds_id,
                Some("system"),
                Some("v1"),
                "var/test-data/path/v1",
                "hash-v1",
                None,
                None,
                None, // created_by must be valid user ID or None (FK constraint)
            )
            .await
            .unwrap();

        adapteros_db::sqlx::query("UPDATE training_dataset_versions SET validation_status = 'valid', trust_state = 'blocked', overall_trust_status = 'blocked' WHERE id = ?")
            .bind(&version_id)
            .execute(agent.state.db.pool())
            .await
            .unwrap();

        let policy = RepositoryTrainingPolicy {
            repo_id: "repo1".to_string(),
            tenant_id: "system".to_string(),
            ..Default::default()
        };

        let result = agent.select_datasets(&policy, "system").await;
        assert!(result.is_err(), "blocked dataset should be rejected");
    }

    #[tokio::test]
    async fn promotion_rolls_back_on_invalid_transition() {
        let agent = build_agent().await;
        let repo_id = agent
            .state
            .db
            .create_adapter_repository(CreateRepositoryParams {
                tenant_id: "system",
                name: "repo",
                base_model_id: None,
                default_branch: None,
                created_by: Some("tester"),
                description: None,
            })
            .await
            .unwrap();

        let active_version = agent
            .state
            .db
            .create_adapter_version(CreateVersionParams {
                repo_id: &repo_id,
                tenant_id: "system",
                version: "v1",
                branch: "main",
                branch_classification: "protected",
                aos_path: None,
                aos_hash: None,
                manifest_schema_version: None,
                parent_version_id: None,
                code_commit_sha: Some("abc123"),
                data_spec_hash: None,
                training_backend: None,
                coreml_used: None,
                coreml_device_type: None,
                dataset_version_ids: None,
                release_state: "active",
                metrics_snapshot_id: None,
                evaluation_summary: None,
                allow_archived: false,
                actor: Some("tester"),
                reason: None,
                train_job_id: None,
            })
            .await
            .unwrap();

        let draft_version = agent
            .state
            .db
            .create_adapter_version(CreateVersionParams {
                repo_id: &repo_id,
                tenant_id: "system",
                version: "v2",
                branch: "main",
                branch_classification: "protected",
                aos_path: None,
                aos_hash: None,
                manifest_schema_version: None,
                parent_version_id: None,
                code_commit_sha: Some("def456"),
                data_spec_hash: None,
                training_backend: None,
                coreml_used: None,
                coreml_device_type: None,
                dataset_version_ids: None,
                release_state: "draft",
                metrics_snapshot_id: None,
                evaluation_summary: None,
                allow_archived: false,
                actor: Some("tester"),
                reason: None,
                train_job_id: None,
            })
            .await
            .unwrap();

        let promote_result = agent
            .promote_version(&draft_version, &repo_id, "main", None)
            .await;
        assert!(promote_result.is_err(), "draft -> active should fail");

        let active = agent
            .state
            .db
            .find_active_version_for_branch(&repo_id, "system", "main")
            .await
            .unwrap()
            .expect("active version should exist");
        assert_eq!(active.id, active_version);
    }
}
