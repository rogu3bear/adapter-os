//! Training and adapter lifecycle control-plane commands.
//!
//! Implements `aosctl train start|status|list` backed by the control-plane
//! training APIs, with JSON output support and lightweight client-side
//! validation for dataset_version_ids.

use crate::commands::train::TrainArgs as LocalTrainArgs;
use crate::http_client::send_with_refresh_from_store;
use crate::output::OutputWriter;
use adapteros_api_types::{
    training::{
        DatasetVersionSelection, StartTrainingRequest, TrainingConfigRequest,
        TrainingJobListResponse, TrainingJobResponse,
    },
    ErrorResponse,
};
use adapteros_types::training::{BranchClassification, TrainingBackendKind, TrainingBackendPolicy};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use reqwest::Client;

#[derive(Debug, Subcommand, Clone)]
pub enum TrainCommand {
    /// Start a control-plane managed training job
    #[command(visible_alias = "run")]
    Start(TrainStartArgs),

    /// Inspect a training job status and backend details
    Status(TrainStatusArgs),

    /// List training jobs with filters
    List(TrainListArgs),

    /// Legacy on-device training (kept for compatibility)
    #[command(name = "local", hide = true)]
    Local(LocalTrainArgs),
}

#[derive(Debug, Args, Clone)]
pub struct TrainStartArgs {
    /// Adapter repository ID
    pub repo_id: String,

    /// Target branch for the produced adapter version
    #[arg(long, default_value = "main")]
    pub branch: String,

    /// Optional adapter display name (defaults to {repo_id}:{branch})
    #[arg(long)]
    pub adapter_name: Option<String>,

    /// Base model ID (defaults to repo default on server if omitted)
    #[arg(long)]
    pub base_model_id: Option<String>,

    /// Dataset version IDs (comma separated or repeated). Required unless --synthetic-mode.
    #[arg(long, value_delimiter = ',')]
    pub dataset_version_ids: Vec<String>,

    /// Optional data spec hash (must match combined manifest hash when provided)
    #[arg(long)]
    pub data_spec_hash: Option<String>,

    /// Use synthetic/diagnostic mode (no dataset versions required)
    #[arg(long, default_value_t = false)]
    pub synthetic_mode: bool,

    /// Backend policy: auto|coreml_only|coreml_else_fallback
    #[arg(long, default_value = "auto")]
    pub backend_policy: String,

    /// Preferred backend: auto|coreml|mlx|metal|cpu
    #[arg(long, default_value = "auto")]
    pub backend: String,

    /// CoreML fallback backend when policy allows
    #[arg(long)]
    pub coreml_fallback: Option<String>,

    /// Assurance tier (protected|high|sandbox)
    #[arg(long)]
    pub assurance_tier: Option<String>,

    /// Control plane base URL
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    pub base_url: String,
}

#[derive(Debug, Args, Clone)]
pub struct TrainStatusArgs {
    /// Training job ID
    pub job_id: String,

    /// Control plane base URL
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    pub base_url: String,
}

#[derive(Debug, Args, Clone)]
pub struct TrainListArgs {
    /// Filter by repo ID
    #[arg(long)]
    pub repo_id: Option<String>,

    /// Filter by status (pending|running|completed|failed|cancelled)
    #[arg(long)]
    pub status: Option<String>,

    /// Filter to jobs that used CoreML backend
    #[arg(long)]
    pub coreml_used: bool,

    /// Filter by created-at >= RFC3339 timestamp
    #[arg(long)]
    pub created_after: Option<String>,

    /// Filter by created-at <= RFC3339 timestamp
    #[arg(long)]
    pub created_before: Option<String>,

    /// Control plane base URL
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    pub base_url: String,
}

pub async fn run(cmd: TrainCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        TrainCommand::Start(args) => start(args, output).await,
        TrainCommand::Status(args) => status(args, output).await,
        TrainCommand::List(args) => list(args, output).await,
        TrainCommand::Local(args) => args
            .execute()
            .await
            .map_err(|e| anyhow!("legacy training failed: {}", e)),
    }
}

async fn start(args: TrainStartArgs, output: &OutputWriter) -> Result<()> {
    validate_dataset_inputs(&args)?;

    let client = Client::new();
    let base = args.base_url.trim_end_matches('/');

    // Lightweight trust validation: ensure dataset versions are reachable.
    for ds in &args.dataset_version_ids {
        let url = format!("{}/v1/training/dataset_versions/{}/manifest", base, ds);
        let resp =
            send_with_refresh_from_store(&client, |c, auth| c.get(&url).bearer_auth(&auth.token))
                .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "dataset_version_id {} failed validation: {} {}",
                ds,
                status,
                body
            );
        }
    }

    let policy = parse_backend_policy(&args.backend_policy)?;
    let preferred_backend = parse_backend_kind(&args.backend)?;
    let coreml_fallback = match args.coreml_fallback {
        Some(ref raw) => Some(parse_backend_kind(raw)?),
        None => None,
    };

    let assurance = parse_assurance_tier(args.assurance_tier.as_deref())?;

    let config = TrainingConfigRequest {
        rank: 16,
        alpha: 32,
        targets: vec![],
        epochs: 3,
        learning_rate: 0.0001,
        batch_size: 8,
        warmup_steps: None,
        max_seq_length: None,
        gradient_accumulation_steps: None,
        preferred_backend: Some(preferred_backend),
        backend_policy: Some(policy),
        coreml_training_fallback: coreml_fallback,
        coreml_placement: None,
        enable_coreml_export: None,
        require_gpu: None,
        max_gpu_memory_mb: None,
    };

    let adapter_name = args
        .adapter_name
        .unwrap_or_else(|| format!("{}:{}", args.repo_id, args.branch));

    let dataset_version_ids = if args.dataset_version_ids.is_empty() {
        None
    } else {
        Some(
            args.dataset_version_ids
                .iter()
                .map(|id| DatasetVersionSelection {
                    dataset_version_id: id.clone(),
                    weight: 1.0,
                })
                .collect(),
        )
    };

    let request = StartTrainingRequest {
        adapter_name,
        config,
        template_id: None,
        repo_id: Some(args.repo_id.clone()),
        target_branch: Some(args.branch.clone()),
        branch_classification: assurance,
        base_version_id: None,
        code_commit_sha: None,
        data_spec: None,
        data_spec_hash: args.data_spec_hash.clone(),
        hyperparameters: None,
        dataset_id: None,
        dataset_version_ids,
        synthetic_mode: args.synthetic_mode,
        data_lineage_mode: None,
        base_model_id: args.base_model_id.clone(),
        collection_id: None,
        lora_tier: None,
        scope: None,
        category: None,
        description: None,
        language: None,
        symbol_targets: None,
        framework_id: None,
        framework_version: None,
        api_patterns: None,
        repo_scope: None,
        file_patterns: None,
        exclude_patterns: None,
        post_actions: None,
    };

    let url = format!("{}/v1/training/start", base);
    let resp = send_with_refresh_from_store(&client, |c, auth| {
        c.post(&url).bearer_auth(&auth.token).json(&request)
    })
    .await?;

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body_text) {
            if let Some(mapped) = map_trust_error(&err, args.dataset_version_ids.first()) {
                bail!(mapped);
            }
            bail!("failed to start training: {} ({})", err.error, err.code);
        }
        bail!("failed to start training: {} {}", status, body_text);
    }

    let job: TrainingJobResponse =
        serde_json::from_str(&body_text).context("failed to parse training response")?;

    if output.is_json() {
        output.result(&serde_json::to_string_pretty(&job)?);
    } else {
        output.success("Training job started");
        output.kv("job_id", &job.id);
        if let Some(repo_id) = job.repo_id.as_deref() {
            output.kv("repo", repo_id);
        }
        if let Some(branch) = job.target_branch.as_deref() {
            output.kv("branch", branch);
        }
        if let Some(backend) = job.requested_backend.as_deref() {
            output.kv("requested_backend", backend);
        }
        if let Some(backend_policy) = job.backend_policy.as_deref() {
            output.kv("backend_policy", backend_policy);
        }
        if let Some(ds) = job.dataset_version_ids.as_ref().map(|v| {
            v.iter()
                .map(|d| d.dataset_version_id.clone())
                .collect::<Vec<_>>()
        }) {
            output.kv("dataset_versions", &ds.join(","));
        }
    }

    Ok(())
}

async fn status(args: TrainStatusArgs, output: &OutputWriter) -> Result<()> {
    let client = Client::new();
    let base = args.base_url.trim_end_matches('/');
    let url = format!("{}/v1/training/jobs/{}", base, args.job_id);

    let resp =
        send_with_refresh_from_store(&client, |c, auth| c.get(&url).bearer_auth(&auth.token))
            .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("failed to fetch training job: {} {}", status, text);
    }

    let job: TrainingJobResponse =
        serde_json::from_str(&text).context("failed to parse training job response")?;

    if output.is_json() {
        output.result(&serde_json::to_string_pretty(&job)?);
        return Ok(());
    }

    output.info(format!("Training job {}", job.id));
    output.kv("status", &job.status);
    if let Some(backend) = job.backend.as_deref() {
        output.kv("backend", backend);
    }
    if let Some(reason) = job.backend_reason.as_deref() {
        output.kv("backend_reason", reason);
    }
    if let Some(coreml) = job.coreml_export_status.as_deref() {
        output.kv("coreml_export_status", coreml);
    }
    if let Some(ds) = job.dataset_version_ids.as_ref().map(|v| {
        v.iter()
            .map(|d| d.dataset_version_id.clone())
            .collect::<Vec<_>>()
    }) {
        output.kv("dataset_versions", &ds.join(","));
    }
    let progress_pct = job
        .progress_pct
        .map(|pct| format!("{:.1}", pct))
        .unwrap_or_else(|| "n/a".to_string());
    output.kv("progress_pct", &progress_pct);
    let current_epoch = job
        .current_epoch
        .map(|epoch| epoch.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    output.kv("current_epoch", &current_epoch);
    output.kv("total_epochs", &job.total_epochs.to_string());
    let current_loss = job
        .current_loss
        .map(|loss| format!("{:.4}", loss))
        .unwrap_or_else(|| "n/a".to_string());
    output.kv("current_loss", &current_loss);
    let tokens_per_second = job
        .tokens_per_second
        .map(|tps| format!("{:.2}", tps))
        .unwrap_or_else(|| "n/a".to_string());
    output.kv("tokens_per_second", &tokens_per_second);
    if let Some(coreml_used) = job.backend.as_ref().map(|b| b == "coreml") {
        output.kv("coreml_used", if coreml_used { "yes" } else { "no" });
    }
    Ok(())
}

async fn list(args: TrainListArgs, output: &OutputWriter) -> Result<()> {
    let client = Client::new();
    let base = args.base_url.trim_end_matches('/');
    let url = format!("{}/v1/training/jobs", base);

    let resp =
        send_with_refresh_from_store(&client, |c, auth| c.get(&url).bearer_auth(&auth.token))
            .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("failed to list training jobs: {} {}", status, text);
    }

    let mut list: TrainingJobListResponse =
        serde_json::from_str(&text).context("failed to parse training jobs list")?;

    // client-side filters not covered by API
    if let Some(ref repo) = args.repo_id {
        list.jobs
            .retain(|j| j.repo_id.as_deref() == Some(repo.as_str()));
    }
    if let Some(ref status_filter) = args.status {
        list.jobs
            .retain(|j| j.status.eq_ignore_ascii_case(status_filter));
    }
    if args.coreml_used {
        list.jobs
            .retain(|j| j.backend.as_deref().map(|b| b == "coreml").unwrap_or(false));
    }
    if let Some(ref after) = args.created_after {
        let cutoff = parse_time(after)?;
        list.jobs.retain(|j| {
            parse_time(&j.created_at)
                .map(|t| t >= cutoff)
                .unwrap_or(true)
        });
    }
    if let Some(ref before) = args.created_before {
        let cutoff = parse_time(before)?;
        list.jobs.retain(|j| {
            parse_time(&j.created_at)
                .map(|t| t <= cutoff)
                .unwrap_or(true)
        });
    }

    list.total = list.jobs.len();

    if output.is_json() {
        output.result(&serde_json::to_string_pretty(&list)?);
        return Ok(());
    }

    output.info(format!("{} training jobs", list.total));
    for job in &list.jobs {
        let progress_pct = job
            .progress_pct
            .map(|pct| format!("{:.1}", pct))
            .unwrap_or_else(|| "n/a".to_string());
        output.result(format!(
            "- {} | {} | repo={} | backend={:?} | pct={}",
            job.id,
            job.status,
            job.repo_id.as_deref().unwrap_or("-"),
            job.backend.as_deref().unwrap_or("unknown"),
            progress_pct
        ));
    }
    Ok(())
}

fn parse_backend_policy(raw: &str) -> Result<TrainingBackendPolicy> {
    match raw.to_ascii_lowercase().as_str() {
        "auto" => Ok(TrainingBackendPolicy::Auto),
        "coreml_only" | "coreml-only" | "coreml" => Ok(TrainingBackendPolicy::CoremlOnly),
        "coreml_else_fallback" | "coreml-fallback" | "coreml_else" | "coreml_elsefallback" => {
            Ok(TrainingBackendPolicy::CoremlElseFallback)
        }
        other => bail!(
            "invalid backend_policy '{}'; use auto|coreml_only|coreml_else_fallback",
            other
        ),
    }
}

fn parse_backend_kind(raw: &str) -> Result<TrainingBackendKind> {
    match raw.to_ascii_lowercase().as_str() {
        "auto" => Ok(TrainingBackendKind::Auto),
        "coreml" | "core-ml" => Ok(TrainingBackendKind::CoreML),
        "mlx" => Ok(TrainingBackendKind::Mlx),
        "metal" => Ok(TrainingBackendKind::Metal),
        "cpu" | "cpu_only" | "cpu-only" => Ok(TrainingBackendKind::Cpu),
        other => bail!("invalid backend '{}'; use auto|coreml|mlx|metal|cpu", other),
    }
}

fn parse_assurance_tier(raw: Option<&str>) -> Result<Option<BranchClassification>> {
    match raw {
        None => Ok(None),
        Some(val) => match val.to_ascii_lowercase().as_str() {
            "protected" => Ok(Some(BranchClassification::Protected)),
            "high" => Ok(Some(BranchClassification::High)),
            "sandbox" => Ok(Some(BranchClassification::Sandbox)),
            other => bail!(
                "invalid assurance tier '{}'; use protected|high|sandbox",
                other
            ),
        },
    }
}

fn parse_time(raw: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| anyhow!("invalid timestamp {}: {}", raw, e))
}

fn validate_dataset_inputs(args: &TrainStartArgs) -> Result<()> {
    if args.synthetic_mode {
        if !args.dataset_version_ids.is_empty() {
            bail!(
                "synthetic-mode requires dataset_version_ids to be empty; \
                 omit --dataset-version-ids or disable --synthetic-mode."
            );
        }
        return Ok(());
    }

    if args.dataset_version_ids.is_empty() {
        bail!(
            "--dataset-version-ids is required for non-synthetic training. \
             Pick dataset versions or enable --synthetic-mode for diagnostic runs."
        );
    }

    Ok(())
}

fn map_trust_error(err: &ErrorResponse, dataset_hint: Option<&String>) -> Option<String> {
    let ds = dataset_hint
        .map(|s| s.as_str())
        .unwrap_or("dataset version");
    match err.code.as_str() {
        "DATASET_TRUST_BLOCKED" => Some(format!(
            "Dataset trust_state is blocked; override or adjust the dataset to proceed. (dataset: {ds})"
        )),
        "DATASET_TRUST_NEEDS_APPROVAL" => Some(format!(
            "Dataset trust_state requires approval or validation before training. (dataset: {ds})"
        )),
        "LINEAGE_REQUIRED" => Some(
            "Non-synthetic training requires dataset_version_ids; use --dataset-version-ids or enable --synthetic-mode."
                .to_string(),
        ),
        "DATA_SPEC_HASH_MISMATCH" => Some(
            "data_spec_hash does not match the combined dataset manifest hash. Recompute and retry."
                .to_string(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> TrainStartArgs {
        TrainStartArgs {
            repo_id: "repo".into(),
            branch: "main".into(),
            adapter_name: None,
            base_model_id: None,
            dataset_version_ids: Vec::new(),
            data_spec_hash: None,
            synthetic_mode: false,
            backend_policy: "auto".into(),
            backend: "auto".into(),
            coreml_fallback: None,
            assurance_tier: None,
            base_url: "http://127.0.0.1:8080".into(),
        }
    }

    #[test]
    fn requires_dataset_ids_when_not_synthetic() {
        let args = base_args();
        let err = validate_dataset_inputs(&args).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--dataset-version-ids is required for non-synthetic training"));
        assert!(msg.contains("--synthetic-mode"));
    }

    #[test]
    fn synthetic_mode_allows_missing_dataset_ids() {
        let mut args = base_args();
        args.synthetic_mode = true;
        assert!(validate_dataset_inputs(&args).is_ok());
    }

    #[test]
    fn synthetic_mode_rejects_dataset_ids() {
        let mut args = base_args();
        args.synthetic_mode = true;
        args.dataset_version_ids = vec!["dsv-1".into()];
        let err = validate_dataset_inputs(&args).unwrap_err();
        assert!(format!("{err}").contains("synthetic-mode requires"));
    }

    #[test]
    fn maps_trust_errors_to_ui_copy() {
        let err = ErrorResponse {
            schema_version: "v1".into(),
            error: "Dataset version dsv-1 is not trainable (trust_state: blocked)".into(),
            code: "DATASET_TRUST_BLOCKED".into(),
            details: None,
            failure_code: None,
        };
        let msg = map_trust_error(&err, Some(&"dsv-1".to_string())).unwrap();
        assert!(msg.contains("dsv-1"));
        assert!(msg.contains("blocked"));
    }

    #[test]
    fn maps_needs_approval_errors() {
        let err = ErrorResponse {
            schema_version: "v1".into(),
            error: "dataset version dsv-2 trust_state=needs_approval blocks training".into(),
            code: "DATASET_TRUST_NEEDS_APPROVAL".into(),
            details: None,
            failure_code: None,
        };
        let msg = map_trust_error(&err, Some(&"dsv-2".to_string())).unwrap();
        assert!(msg.contains("needs approval"));
        assert!(msg.contains("dsv-2"));
    }

    #[test]
    fn maps_lineage_required_and_hash_mismatch() {
        let lineage = ErrorResponse {
            schema_version: "v1".into(),
            error: "dataset_version_ids are required for non-synthetic training jobs".into(),
            code: "LINEAGE_REQUIRED".into(),
            details: None,
            failure_code: None,
        };
        let hash = ErrorResponse {
            schema_version: "v1".into(),
            error: "data_spec_hash mismatch".into(),
            code: "DATA_SPEC_HASH_MISMATCH".into(),
            details: None,
            failure_code: None,
        };
        let lineage_msg = map_trust_error(&lineage, None).unwrap();
        assert!(lineage_msg.contains("--dataset-version-ids"));
        let hash_msg = map_trust_error(&hash, None).unwrap();
        assert!(hash_msg.contains("data_spec_hash"));
    }
}
