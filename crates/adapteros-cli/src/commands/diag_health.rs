//! Diagnostics/health subcommands: drift checks, adapter/dataset health, storage reconciler.
//!
//! Focused on status surfaces (JSON friendly) without pulling full logs.
use crate::http_client::send_with_refresh_from_store;
use crate::output::OutputWriter;
use adapteros_api_types::adapters::{
    AdapterHealthDomain, AdapterHealthFlag, AdapterHealthResponse, AdapterHealthSubcode,
};
use adapteros_core::{AosError, Result};
use adapteros_db::storage_reconciliation::StorageReconciliationIssue;
use adapteros_db::{sqlx, Db};
use adapteros_lora_worker::training::{
    compute_drift, deterministic_slice, run_backend_with_examples, DatasetSubsample,
    HarnessHyperparams, TrainingBackend, TrainingExample,
};
use adapteros_manifest::{AssuranceTier, ManifestV3};
use clap::{Args, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Top-level dispatcher for diagnostic health commands.
#[derive(Debug, Args, Clone)]
pub struct HealthCommand {
    #[command(subcommand)]
    pub subcommand: HealthSubcommand,
}

/// Subcommands under `aosctl health` (drift/health/storage surfaces).
#[derive(Debug, Subcommand, Clone)]
pub enum HealthSubcommand {
    /// Run drift harness against a dataset version and backend under test.
    DriftRun {
        /// Repository identifier (used to resolve adapter/version metadata)
        #[arg(long)]
        repo_id: String,
        /// Optional adapter/version id override
        #[arg(long)]
        adapter_id: Option<String>,
        /// Dataset version ids to sample (first is used)
        #[arg(long, required = true)]
        dataset_version_ids: Vec<String>,
        /// Backend under test (cpu|coreml|metal|mlx)
        #[arg(long)]
        backend: String,
        /// Baseline backend (defaults to cpu if available)
        #[arg(long)]
        baseline_backend: Option<String>,
        /// Assurance tier (low|standard|high)
        #[arg(long)]
        assurance_tier: Option<String>,
        /// Optional manifest (.aos manifest) to persist drift metadata
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Path to base model for hidden state extraction (required for training)
        #[arg(long, env = "AOS_BASE_MODEL_PATH")]
        base_model: PathBuf,
        /// Deterministic seed
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Steps/epochs to train in harness
        #[arg(long, default_value_t = 1)]
        steps: usize,
        /// Optional slice size
        #[arg(long)]
        slice_size: Option<usize>,
        /// Optional slice offset
        #[arg(long)]
        slice_offset: Option<usize>,
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// Show stored drift metrics for an adapter (DB or manifest).
    DriftShow {
        /// Repository identifier (used to resolve latest version if adapter_id missing)
        #[arg(long)]
        repo_id: Option<String>,
        /// Adapter/version id (if known)
        #[arg(long)]
        adapter_id: Option<String>,
        /// Optional manifest to read metrics from
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Tenant identifier
        #[arg(long, default_value = "default")]
        tenant: String,
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// Show adapter health rollup and contributing signals.
    Adapter {
        /// Repository identifier
        #[arg(long)]
        repo_id: String,
        /// Optional adapter/version id
        #[arg(long)]
        version_id: Option<String>,
        /// Server URL
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// Show dataset trust/validation signals.
    Dataset {
        /// Dataset version id
        #[arg(long)]
        dataset_version_id: String,
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// Summarize storage reconciler issues by tenant.
    StorageStatus {
        /// JSON output
        #[arg(long)]
        json: bool,
    },

    /// List individual storage reconciliation issues.
    StorageListIssues {
        /// Max rows to return (default 50)
        #[arg(long, default_value_t = 50)]
        limit: i64,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
}

/// Entry point from main command dispatch.
pub async fn run(cmd: HealthCommand, output: &OutputWriter) -> Result<()> {
    match cmd.subcommand {
        HealthSubcommand::DriftRun {
            repo_id,
            adapter_id,
            dataset_version_ids,
            backend,
            baseline_backend,
            assurance_tier,
            manifest,
            base_model,
            seed,
            steps,
            slice_size,
            slice_offset,
            json,
        } => {
            run_drift_harness(
                &repo_id,
                adapter_id,
                dataset_version_ids,
                &backend,
                baseline_backend,
                assurance_tier,
                manifest,
                base_model,
                seed,
                steps,
                slice_size,
                slice_offset,
                json,
                output,
            )
            .await?;
        }
        HealthSubcommand::DriftShow {
            repo_id,
            adapter_id,
            manifest,
            tenant,
            json,
        } => {
            show_drift(repo_id, adapter_id, manifest, &tenant, json, output).await?;
        }
        HealthSubcommand::Adapter {
            repo_id,
            version_id,
            server_url,
            json,
        } => show_adapter_health(&repo_id, version_id, &server_url, json, output).await?,
        HealthSubcommand::Dataset {
            dataset_version_id,
            json,
        } => show_dataset_health(&dataset_version_id, json, output).await?,
        HealthSubcommand::StorageStatus { json } => show_storage_status(json, output).await?,
        HealthSubcommand::StorageListIssues { limit, json } => {
            list_storage_issues(limit, json, output).await?
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct DriftMetricRow {
    backend: String,
    weight_l_inf: f32,
    loss_l_inf: f32,
    decision: String,
}

#[derive(Debug, Serialize)]
struct DriftRunReport {
    repo_id: String,
    adapter_id: Option<String>,
    reference_backend: String,
    assurance_tier: String,
    overall: String,
    metrics: Vec<DriftMetricRow>,
}

#[allow(clippy::too_many_arguments)]
async fn run_drift_harness(
    repo_id: &str,
    adapter_id: Option<String>,
    dataset_version_ids: Vec<String>,
    backend_under_test: &str,
    baseline_backend: Option<String>,
    assurance_tier: Option<String>,
    manifest: Option<PathBuf>,
    base_model: PathBuf,
    seed: u64,
    steps: usize,
    slice_size: Option<usize>,
    slice_offset: Option<usize>,
    json: bool,
    output: &OutputWriter,
) -> Result<()> {
    let db = Db::connect_env().await?;
    let dataset_version_id = dataset_version_ids
        .first()
        .ok_or_else(|| AosError::Validation("dataset_version_id is required".into()))?
        .to_string();
    let ds = db
        .get_training_dataset_version(&dataset_version_id)
        .await?
        .ok_or_else(|| AosError::Validation("dataset version not found".into()))?;

    let dataset_path = PathBuf::from(&ds.storage_path);
    if !dataset_path.exists() {
        return Err(AosError::Io(format!(
            "dataset file not found at {}",
            dataset_path.display()
        )));
    }

    let examples = load_dataset(&dataset_path)?;
    let subsample = slice_offset.map(|offset| DatasetSubsample {
        offset,
        length: slice_size.unwrap_or(examples.len()),
    });
    let sliced = deterministic_slice(examples, seed, slice_size, subsample.clone());

    let mut backends = Vec::new();
    if let Some(base) = baseline_backend.as_deref() {
        backends.push(parse_backend(base)?);
    }
    let test_backend = parse_backend(backend_under_test)?;
    if backends.is_empty() || !backends.iter().any(|b| b.tag() == test_backend.tag()) {
        backends.push(test_backend);
    }
    let reference_backend = choose_reference_backend(&backends, baseline_backend.clone())
        .unwrap_or_else(|| backends[0].tag().to_string());

    let tier = parse_assurance_tier(assurance_tier.as_deref());
    let mut runs = Vec::with_capacity(backends.len());
    for backend in backends {
        let run = run_backend_with_examples(
            HarnessHyperparams::default(),
            backend,
            steps,
            seed,
            Some(dataset_version_id.clone()),
            None,
            subsample.clone(),
            base_model.clone(),
            &sliced,
        )
        .await?;
        runs.push(run);
    }

    // Find reference result
    let reference = runs
        .iter()
        .find(|r| r.backend.tag() == reference_backend)
        .unwrap_or(&runs[0])
        .result
        .clone();

    let mut rows = Vec::new();
    let mut overall = DriftDecision::RecordOnly;
    for run in &runs {
        if run.result.backend == reference.backend {
            continue;
        }
        let metrics = compute_drift(&reference, &run.result);
        let decision = evaluate_drift(&metrics, tier);
        overall = merge_decision(overall, decision);
        rows.push(DriftMetricRow {
            backend: run.backend.tag().to_string(),
            weight_l_inf: metrics.weight_l_inf,
            loss_l_inf: metrics.loss_l_inf,
            decision: format!("{:?}", decision),
        });
    }

    let report = DriftRunReport {
        repo_id: repo_id.to_string(),
        adapter_id,
        reference_backend,
        assurance_tier: format!("{:?}", tier),
        overall: format!("{:?}", overall),
        metrics: rows.clone(),
    };

    if json {
        output.json(&report)?;
    } else {
        output.section("Drift harness");
        output.kv("Repo", repo_id);
        output.kv("Dataset version", &dataset_version_id);
        output.kv("Reference backend", &report.reference_backend);
        output.kv("Assurance tier", &report.assurance_tier);
        output.kv("Overall", &report.overall);

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(vec!["Backend", "weight_l_inf", "loss_l_inf", "Decision"]);
        for row in &rows {
            table.add_row(vec![
                Cell::new(&row.backend),
                Cell::new(format!("{:.4}", row.weight_l_inf)),
                Cell::new(format!("{:.4}", row.loss_l_inf)),
                Cell::new(&row.decision),
            ]);
        }
        output.table(&table as &dyn std::fmt::Display, Some(&report))?;
    }

    // Optionally persist drift metadata back into manifest (best-effort).
    if let Some(manifest_path) = manifest {
        if let Err(e) = persist_drift_metadata(&manifest_path, &report) {
            output.warning(format!(
                "Failed to persist drift metadata into manifest: {e}"
            ));
        }
    }

    Ok(())
}

async fn show_drift(
    repo_id: Option<String>,
    adapter_id: Option<String>,
    manifest: Option<PathBuf>,
    tenant_id: &str,
    json: bool,
    output: &OutputWriter,
) -> Result<()> {
    if let Some(path) = manifest {
        let manifest_str = fs::read_to_string(&path)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {e}")))?;
        let manifest = ManifestV3::from_json(&manifest_str)?;
        let adapter = match adapter_id {
            Some(id) => manifest
                .adapters
                .iter()
                .find(|a| a.id == id)
                .cloned()
                .ok_or_else(|| {
                    AosError::Validation(format!("Adapter '{id}' not found in manifest"))
                })?,
            None => {
                if manifest.adapters.is_empty() {
                    return Err(AosError::Validation(
                        "No adapter entries in manifest".into(),
                    ));
                }
                if manifest.adapters.len() == 1 {
                    manifest.adapters.first().cloned().ok_or_else(|| {
                        AosError::Validation("No adapter entries in manifest".into())
                    })?
                } else {
                    return Err(AosError::Validation(
                        "Manifest contains multiple adapters; pass --adapter-id to select one"
                            .into(),
                    ));
                }
            }
        };
        let summary = DriftShowSummary::from_manifest(adapter, path.to_string_lossy());
        render_drift_show(summary, json, output)?;
        return Ok(());
    }

    let db = Db::connect_env().await?;
    let resolved_adapter_id = resolve_adapter_id(&db, repo_id, adapter_id).await?;
    let adapter = db
        .get_adapter_for_tenant(tenant_id, &resolved_adapter_id)
        .await?
        .ok_or_else(|| AosError::Validation("adapter not found".into()))?;
    let summary = DriftShowSummary::from_adapter_record(adapter);
    render_drift_show(summary, json, output)?;
    Ok(())
}

#[derive(Debug, Serialize, Clone)]
struct DriftShowSummary {
    adapter_id: String,
    repo_id: Option<String>,
    source: Option<String>,
    reference_backend: Option<String>,
    baseline_backend: Option<String>,
    test_backend: Option<String>,
    weight_l_inf: Option<f64>,
    loss_l_inf: Option<f64>,
    tier: Option<String>,
    decision: Option<String>,
}

impl DriftShowSummary {
    fn from_adapter_record(adapter: adapteros_db::adapters::Adapter) -> Self {
        // NOTE: the SQL/KV `adapters` record does not currently store drift metrics.
        // Drift metrics are surfaced via .aos manifests and/or worker harness outputs.
        Self {
            adapter_id: adapter.adapter_id.unwrap_or(adapter.id),
            repo_id: adapter.repo_id,
            source: None,
            reference_backend: None,
            baseline_backend: None,
            test_backend: None,
            weight_l_inf: None,
            loss_l_inf: None,
            tier: None,
            decision: None,
        }
    }

    fn from_manifest(
        adapter: adapteros_manifest::Adapter,
        manifest_path: std::borrow::Cow<str>,
    ) -> Self {
        let tier = adapter.drift_tier.unwrap_or(AssuranceTier::Standard);
        let decision = adapter
            .drift_metric
            .zip(adapter.drift_loss_metric)
            .map(|(w, l)| {
                let metrics = adapteros_lora_worker::training::DriftMetrics {
                    backend: adapter
                        .drift_test_backend
                        .clone()
                        .unwrap_or_else(|| "unknown".into()),
                    weight_l_inf: w,
                    loss_l_inf: l,
                    cosine_similarity: None,
                };
                format!("{:?}", evaluate_drift(&metrics, tier))
            });
        Self {
            adapter_id: adapter.id.clone(),
            repo_id: adapter.repo_id.clone(),
            source: Some(manifest_path.into_owned()),
            reference_backend: adapter.drift_reference_backend.clone(),
            baseline_backend: adapter.drift_baseline_backend.clone(),
            test_backend: adapter.drift_test_backend.clone(),
            weight_l_inf: adapter.drift_metric.map(|v| v as f64),
            loss_l_inf: adapter.drift_loss_metric.map(|v| v as f64),
            tier: Some(format!("{:?}", tier).to_lowercase()),
            decision,
        }
    }
}

fn render_drift_show(summary: DriftShowSummary, json: bool, output: &OutputWriter) -> Result<()> {
    if json {
        output.json(&summary)?;
        return Ok(());
    }

    output.section("Drift metrics");
    output.kv("Adapter", &summary.adapter_id);
    if let Some(repo) = &summary.repo_id {
        output.kv("Repo", repo);
    }
    if let Some(source) = &summary.source {
        output.kv("Source", source);
    }
    if let Some(ref b) = summary.reference_backend {
        output.kv("Reference backend", b);
    }
    if let Some(ref b) = summary.test_backend {
        output.kv("Test backend", b);
    }
    if let Some(tier) = &summary.tier {
        output.kv("Assurance tier", tier);
    }
    if let Some(w) = summary.weight_l_inf {
        output.kv("weight_l_inf", &format!("{w:.4}"));
    }
    if let Some(l) = summary.loss_l_inf {
        output.kv("loss_l_inf", &format!("{l:.4}"));
    }
    if let Some(decision) = summary.decision {
        output.kv("Decision", &decision);
    }
    Ok(())
}

fn display_health_flag(flag: AdapterHealthFlag) -> &'static str {
    match flag {
        AdapterHealthFlag::Healthy => "Healthy",
        AdapterHealthFlag::Degraded => "Degraded",
        AdapterHealthFlag::Unsafe => "Unsafe",
        AdapterHealthFlag::Corrupt => "Corrupt",
    }
}

fn display_trust_state(state: &str) -> &'static str {
    match state {
        "allowed" => "Allowed",
        "allowed_with_warning" => "Allowed w/ warning",
        "warn" => "Allowed w/ warning",
        "blocked" => "Blocked",
        "blocked_regressed" => "Blocked",
        "needs_approval" => "Needs approval",
        _ => "Unknown",
    }
}

fn display_domain(domain: AdapterHealthDomain) -> &'static str {
    match domain {
        AdapterHealthDomain::Drift => "drift",
        AdapterHealthDomain::Trust => "trust",
        AdapterHealthDomain::Storage => "storage",
        AdapterHealthDomain::Other => "other",
    }
}

fn subcode_label(sub: &AdapterHealthSubcode) -> String {
    match (sub.domain, sub.code.as_str()) {
        (AdapterHealthDomain::Trust, "trust_blocked") => "Trust blocked".to_string(),
        (AdapterHealthDomain::Drift, "drift_high") => "Drift above threshold".to_string(),
        (AdapterHealthDomain::Storage, "hash_mismatch") => "Artifact hash mismatch".to_string(),
        (AdapterHealthDomain::Storage, "missing_bytes" | "missing_file") => {
            "Artifact missing".to_string()
        }
        (AdapterHealthDomain::Storage, "orphan_bytes" | "orphan_file") => {
            "Orphaned artifact".to_string()
        }
        _ => sub
            .message
            .as_ref()
            .cloned()
            .unwrap_or_else(|| sub.code.replace('_', " ")),
    }
}

fn subcode_explanation(sub: &AdapterHealthSubcode) -> String {
    if let Some(msg) = &sub.message {
        return msg.clone();
    }
    match (sub.domain, sub.code.as_str()) {
        (AdapterHealthDomain::Trust, "trust_blocked") => {
            "Dataset trust is blocked or regressed".to_string()
        }
        (AdapterHealthDomain::Drift, "drift_high") => {
            "Drift exceeded hard threshold for tier".to_string()
        }
        (AdapterHealthDomain::Storage, "hash_mismatch") => {
            "Stored artifact hash does not match expected value".to_string()
        }
        (AdapterHealthDomain::Storage, "missing_bytes" | "missing_file") => {
            "Adapter artifact missing from storage".to_string()
        }
        (AdapterHealthDomain::Storage, "orphan_bytes" | "orphan_file") => {
            "Artifact exists on disk without matching metadata".to_string()
        }
        _ => sub.code.replace('_', " "),
    }
}

fn render_adapter_health_text(body: &AdapterHealthResponse, output: &OutputWriter) {
    output.section("Adapter health");
    output.kv("Adapter", &body.adapter_id);
    output.kv("Health", display_health_flag(body.health));

    if !body.datasets.is_empty() {
        output.section("Datasets & trust");
        for ds in &body.datasets {
            output.kv(
                &ds.dataset_version_id,
                &format!(
                    "{} ({})",
                    display_trust_state(&ds.trust_state),
                    ds.trust_state
                ),
            );
        }
    }

    if let Some(primary) = &body.primary_subcode {
        let label = subcode_label(primary);
        output.kv(
            "Primary cause",
            &format!("{} ({})", label, display_domain(primary.domain)),
        );
    }
    if !body.subcodes.is_empty() {
        output.section("Signals");
        for sub in &body.subcodes {
            let label = subcode_label(sub);
            let explanation = subcode_explanation(sub);
            let detail = if label == explanation {
                label.clone()
            } else {
                format!("{label} — {explanation}")
            };
            output.kv(
                &format!("{}: {}", display_domain(sub.domain), sub.code),
                &detail,
            );
        }
    }
    if let Some(storage) = body.storage.as_ref() {
        output.section("Storage");
        output.kv("Reconciler status", &storage.reconciler_status);
        if let Some(issues) = storage.issues.as_ref() {
            for issue in issues {
                let label = subcode_label(issue);
                let explanation = subcode_explanation(issue);
                output.kv(
                    &format!("{}: {}", display_domain(issue.domain), issue.code),
                    &format!("{label} — {explanation}"),
                );
            }
        }
    }
}

#[derive(Serialize)]
struct DatasetHealth<'a> {
    dataset_version_id: &'a str,
    validation_status: &'a str,
    trust_state: &'a str,
    overall_trust_status: &'a str,
    validation_errors: Option<&'a str>,
}

fn render_dataset_health_text(health: &DatasetHealth<'_>, output: &OutputWriter) {
    output.section("Dataset");
    output.kv("Version", health.dataset_version_id);
    output.kv("Validation", health.validation_status);
    output.kv(
        "Trust",
        &format!(
            "{} ({})",
            display_trust_state(health.trust_state),
            health.trust_state
        ),
    );
    output.kv(
        "Overall trust",
        &format!(
            "{} ({})",
            display_trust_state(health.overall_trust_status),
            health.overall_trust_status
        ),
    );
    if let Some(errs) = health.validation_errors {
        output.kv("Warnings", errs);
    }
}

async fn show_adapter_health(
    repo_id: &str,
    version_id: Option<String>,
    server_url: &str,
    json: bool,
    output: &OutputWriter,
) -> Result<()> {
    let db = Db::connect_env().await?;
    let adapter_id = resolve_adapter_id(&db, Some(repo_id.to_string()), version_id).await?;
    let url = format!(
        "{}/v1/adapters/{}/health",
        server_url.trim_end_matches('/'),
        adapter_id
    );
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AosError::Io(format!("Failed to build client: {e}")))?;

    let resp =
        send_with_refresh_from_store(&client, |c, auth| c.get(&url).bearer_auth(&auth.token))
            .await
            .map_err(|e| AosError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(AosError::Http(format!(
            "health request failed: {}",
            resp.status()
        )));
    }
    let body: AdapterHealthResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Http(format!("Failed to parse response: {e}")))?;

    if json {
        output.json(&body)?;
        return Ok(());
    }

    render_adapter_health_text(&body, output);
    Ok(())
}

async fn show_dataset_health(
    dataset_version_id: &str,
    json: bool,
    output: &OutputWriter,
) -> Result<()> {
    let db = Db::connect_env().await?;
    let version = db
        .get_training_dataset_version(dataset_version_id)
        .await?
        .ok_or_else(|| AosError::Validation("dataset version not found".into()))?;

    let health = DatasetHealth {
        dataset_version_id,
        validation_status: &version.validation_status,
        trust_state: &version.trust_state,
        overall_trust_status: &version.overall_trust_status,
        validation_errors: version.validation_errors_json.as_deref(),
    };

    if json {
        output.json(&health)?;
    } else {
        render_dataset_health_text(&health, output);
    }
    Ok(())
}

async fn show_storage_status(json: bool, output: &OutputWriter) -> Result<()> {
    let db = Db::connect_env().await?;
    let issues = db.list_storage_reconciliation_issues(500).await?;

    #[derive(Debug, Default, Serialize)]
    struct Counters {
        missing: usize,
        orphaned: usize,
        hash_mismatch: usize,
    }

    let mut by_tenant: HashMap<String, Counters> = HashMap::new();
    for issue in &issues {
        let tenant = issue.tenant_id.clone().unwrap_or_else(|| "unknown".into());
        let counter = by_tenant.entry(tenant).or_default();
        match issue.issue_type.as_str() {
            "missing_bytes" | "missing_file" => counter.missing += 1,
            "orphan_bytes" | "orphan_file" => counter.orphaned += 1,
            "hash_mismatch" => counter.hash_mismatch += 1,
            _ => {}
        }
    }

    if json {
        output.json(&by_tenant)?;
        return Ok(());
    }

    output.section("Storage reconciler (last issues)");
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Tenant", "Missing", "Orphaned", "Hash mismatches"]);
    for (tenant, counts) in &by_tenant {
        table.add_row(vec![
            Cell::new(tenant),
            Cell::new(counts.missing),
            Cell::new(counts.orphaned),
            Cell::new(counts.hash_mismatch),
        ]);
    }
    output.table(&table as &dyn std::fmt::Display, Some(&by_tenant))?;
    Ok(())
}

async fn list_storage_issues(limit: i64, json: bool, output: &OutputWriter) -> Result<()> {
    let db = Db::connect_env().await?;
    let rows: Vec<StorageReconciliationIssue> = sqlx::query_as::<_, StorageReconciliationIssue>(
        r#"
        SELECT id, tenant_id, owner_type, owner_id, version_id, issue_type, severity,
               path, expected_hash, actual_hash, message, detected_at, resolved_at
        FROM storage_reconciliation_issues
        ORDER BY detected_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(db.pool())
    .await?;

    if json {
        output.json(&rows)?;
        return Ok(());
    }

    output.section("Storage issues");
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "Type", "Owner", "Version", "Severity", "Detected", "Path",
    ]);
    for row in &rows {
        table.add_row(vec![
            Cell::new(&row.issue_type),
            Cell::new(row.owner_id.as_deref().unwrap_or("-")),
            Cell::new(row.version_id.as_deref().unwrap_or("-")),
            Cell::new(&row.severity),
            Cell::new(&row.detected_at),
            Cell::new(&row.path),
        ]);
    }
    output.table(&table as &dyn std::fmt::Display, Some(&rows))?;
    Ok(())
}

fn load_dataset(path: &PathBuf) -> Result<Vec<TrainingExample>> {
    #[derive(Deserialize)]
    struct TrainingData {
        examples: Vec<TrainingExample>,
    }

    let content = fs::read_to_string(path)
        .map_err(|e| AosError::Io(format!("Failed to read dataset: {e}")))?;
    let data: TrainingData = serde_json::from_str(&content)
        .map_err(|e| AosError::Parse(format!("Dataset parse error: {e}")))?;
    Ok(data.examples)
}

fn parse_backend(raw: &str) -> Result<TrainingBackend> {
    match raw.to_lowercase().as_str() {
        "cpu" => Ok(TrainingBackend::Cpu),
        "coreml" => Ok(TrainingBackend::CoreML),
        "mlx" => Ok(TrainingBackend::Mlx),
        "metal" => Ok(TrainingBackend::Metal),
        other => Err(AosError::Validation(format!("Unknown backend '{other}'"))),
    }
}

fn choose_reference_backend(
    backends: &[TrainingBackend],
    override_ref: Option<String>,
) -> Option<String> {
    if backends.iter().any(|b| matches!(b, TrainingBackend::Cpu)) {
        return Some("cpu".to_string());
    }
    if let Some(pref) = override_ref {
        let pref_lower = pref.to_lowercase();
        if backends
            .iter()
            .any(|b| b.tag().eq_ignore_ascii_case(&pref_lower))
        {
            return Some(pref_lower);
        }
    }
    if backends.iter().any(|b| matches!(b, TrainingBackend::Metal)) {
        return Some("metal".to_string());
    }
    if backends.iter().any(|b| matches!(b, TrainingBackend::Mlx)) {
        return Some("mlx".to_string());
    }
    backends.first().map(|b| b.tag().to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriftDecision {
    RecordOnly,
    ReviewRequired,
    Block,
}

fn evaluate_drift(
    metrics: &adapteros_lora_worker::training::DriftMetrics,
    tier: AssuranceTier,
) -> DriftDecision {
    const HIGH_WEIGHT_EPS: f32 = 1e-6;
    const HIGH_LOSS_EPS: f32 = 1e-4;
    const STANDARD_WEIGHT_EPS: f32 = 5e-5;
    const STANDARD_LOSS_EPS: f32 = 5e-4;

    match tier {
        AssuranceTier::Low => DriftDecision::RecordOnly,
        AssuranceTier::Standard => {
            if metrics.weight_l_inf > STANDARD_WEIGHT_EPS || metrics.loss_l_inf > STANDARD_LOSS_EPS
            {
                DriftDecision::ReviewRequired
            } else {
                DriftDecision::RecordOnly
            }
        }
        AssuranceTier::High => {
            if metrics.weight_l_inf > HIGH_WEIGHT_EPS || metrics.loss_l_inf > HIGH_LOSS_EPS {
                DriftDecision::Block
            } else {
                DriftDecision::RecordOnly
            }
        }
    }
}

fn merge_decision(left: DriftDecision, right: DriftDecision) -> DriftDecision {
    use DriftDecision::*;
    match (left, right) {
        (Block, _) | (_, Block) => Block,
        (ReviewRequired, _) | (_, ReviewRequired) => ReviewRequired,
        _ => RecordOnly,
    }
}

fn parse_assurance_tier(value: Option<&str>) -> AssuranceTier {
    match value.unwrap_or("standard").to_lowercase().as_str() {
        "low" => AssuranceTier::Low,
        "high" => AssuranceTier::High,
        _ => AssuranceTier::Standard,
    }
}

fn persist_drift_metadata(manifest_path: &PathBuf, report: &DriftRunReport) -> Result<()> {
    let content = fs::read_to_string(manifest_path)
        .map_err(|e| AosError::Io(format!("Failed to read manifest: {e}")))?;
    let mut manifest = ManifestV3::from_json(&content)?;

    let adapter = match report.adapter_id.as_deref() {
        Some(id) => manifest
            .adapters
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| {
                AosError::Validation(format!(
                    "Adapter '{id}' not found in manifest; pass a manifest containing that adapter"
                ))
            })?,
        None => {
            if manifest.adapters.is_empty() {
                return Err(AosError::Validation(
                    "No adapter entries in manifest".into(),
                ));
            }
            if manifest.adapters.len() == 1 {
                manifest
                    .adapters
                    .first_mut()
                    .ok_or_else(|| AosError::Validation("No adapter entries in manifest".into()))?
            } else {
                return Err(AosError::Validation(
                    "Manifest contains multiple adapters; pass --adapter-id so drift metadata can be persisted"
                        .into(),
                ));
            }
        }
    };

    if let Some(first_metric) = report.metrics.first() {
        adapter.drift_reference_backend = Some(report.reference_backend.clone());
        adapter.drift_baseline_backend = Some(report.reference_backend.clone());
        adapter.drift_test_backend = Some(first_metric.backend.clone());
        adapter.drift_metric = Some(first_metric.weight_l_inf);
        adapter.drift_loss_metric = Some(first_metric.loss_l_inf);
        adapter.drift_tier = Some(parse_assurance_tier(Some(report.assurance_tier.as_str())));
    }

    let serialized = manifest.to_json()?;
    fs::write(manifest_path, serialized)
        .map_err(|e| AosError::Io(format!("Failed to write manifest: {e}")))?;
    Ok(())
}

async fn resolve_adapter_id(
    db: &Db,
    repo_id: Option<String>,
    adapter_id: Option<String>,
) -> Result<String> {
    if let Some(id) = adapter_id {
        return Ok(id);
    }
    let repo = repo_id.ok_or_else(|| AosError::Validation("repo_id is required".into()))?;
    let row = sqlx::query_scalar::<_, String>(
        "SELECT adapter_id FROM adapters WHERE repo_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(repo)
    .fetch_optional(db.pool())
    .await?;
    row.ok_or_else(|| AosError::Validation("no adapter found for repo".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputMode, OutputWriter};
    use adapteros_api_types::{adapters::AdapterDatasetHealth, API_SCHEMA_VERSION};
    use std::sync::{Arc, Mutex};

    #[test]
    fn display_trust_state_maps_warn_and_blocked_regressed() {
        assert_eq!(display_trust_state("warn"), "Allowed w/ warning");
        assert_eq!(display_trust_state("blocked_regressed"), "Blocked");
    }

    #[test]
    fn render_adapter_health_uses_ui_labels_and_order() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, true, sink.clone());
        let body = AdapterHealthResponse {
            schema_version: API_SCHEMA_VERSION.to_string(),
            adapter_id: "adapter-1".to_string(),
            health: AdapterHealthFlag::Unsafe,
            primary_subcode: None,
            subcodes: vec![],
            drift_summary: None,
            datasets: vec![AdapterDatasetHealth {
                dataset_version_id: "dsv-1".to_string(),
                trust_state: "needs_approval".to_string(),
                overall_trust_status: Some("needs_approval".to_string()),
            }],
            storage: None,
            backend: None,
            recent_activations: vec![],
            total_activations: 0,
            selected_count: 0,
            avg_gate_value: 0.0,
            memory_usage_mb: 0.0,
            policy_violations: vec![],
        };

        render_adapter_health_text(&body, &writer);

        let lines = sink.lock().unwrap();
        assert_eq!(lines[0], "section:Adapter health");
        assert!(lines.contains(&"Health:Unsafe".to_string()));
        assert!(lines.contains(&"dsv-1:Needs approval (needs_approval)".to_string()));
    }

    #[test]
    fn render_dataset_health_uses_ui_labels_and_order() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = OutputWriter::with_sink(OutputMode::Text, true, sink.clone());
        let health = DatasetHealth {
            dataset_version_id: "dsv-2",
            validation_status: "valid",
            trust_state: "allowed",
            overall_trust_status: "allowed",
            validation_errors: None,
        };

        render_dataset_health_text(&health, &writer);

        let lines = sink.lock().unwrap().clone();
        let expected = vec![
            "section:Dataset".to_string(),
            "Version:dsv-2".to_string(),
            "Validation:valid".to_string(),
            "Trust:Allowed (allowed)".to_string(),
            "Overall trust:Allowed (allowed)".to_string(),
        ];
        assert!(lines.starts_with(&expected));
    }

    #[test]
    fn drift_decision_thresholds() {
        let metrics = adapteros_lora_worker::training::DriftMetrics {
            backend: "cpu".into(),
            weight_l_inf: 1e-4,
            loss_l_inf: 1e-4,
            cosine_similarity: None,
        };
        assert_eq!(
            evaluate_drift(&metrics, AssuranceTier::High),
            DriftDecision::Block
        );

        let metrics_ok = adapteros_lora_worker::training::DriftMetrics {
            backend: "cpu".into(),
            weight_l_inf: 1e-6,
            loss_l_inf: 1e-6,
            cosine_similarity: None,
        };
        assert_eq!(
            evaluate_drift(&metrics_ok, AssuranceTier::Standard),
            DriftDecision::RecordOnly
        );
    }
}
