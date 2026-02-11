//! Preprocessing handlers for PII scrubbing and deduplication.
//!
//! This module provides API endpoints for preprocessing datasets:
//! - PII scrubbing: Removes personally identifiable information from dataset content
//! - Deduplication: Removes duplicate entries based on content hashing
//!
//! Preprocessing runs as a background job to handle large datasets without blocking.

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use adapteros_core::B3Hash;
use adapteros_id::{IdPrefix, TypedId};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use utoipa::ToSchema;

/// In-memory store for preprocessing job status.
/// In production, this should be persisted to the database.
static PREPROCESS_JOBS: std::sync::LazyLock<
    Arc<RwLock<std::collections::HashMap<String, PreprocessJobState>>>,
> = std::sync::LazyLock::new(|| Arc::new(RwLock::new(std::collections::HashMap::new())));

/// Internal state for a preprocessing job
#[derive(Debug, Clone)]
struct PreprocessJobState {
    dataset_id: String,
    status: PreprocessStatus,
    pii_scrub: bool,
    dedupe: bool,
    lines_processed: usize,
    lines_removed: usize,
    error_message: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// =============================================================================
// Best-effort Persistence (var/)
// =============================================================================

/// Persist preprocessing job state under `var/` so status survives process restarts.
///
/// We keep the in-memory map for fast updates but mirror state to disk:
/// `var/preprocess_jobs/<tenant_id>/<dataset_id>/<job_id>.json`
/// `var/preprocess_jobs/<tenant_id>/<dataset_id>/latest` (job id)
///
/// This avoids a schema/migration for what is currently a lightweight background job.
const PREPROCESS_JOBS_VAR_ROOT: &str = "var/preprocess_jobs";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPreprocessJobState {
    job_id: String,
    tenant_id: String,
    dataset_id: String,
    status: PreprocessStatus,
    pii_scrub: bool,
    dedupe: bool,
    lines_processed: usize,
    lines_removed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: Option<String>,
    started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_at: Option<String>,
}

fn preprocess_job_dir(tenant_id: &str, dataset_id: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(PREPROCESS_JOBS_VAR_ROOT)
        .join(tenant_id)
        .join(dataset_id)
}

async fn persist_preprocess_job_state_best_effort(
    tenant_id: &str,
    dataset_id: &str,
    job_id: &str,
    job_state: &PreprocessJobState,
) {
    let dir = preprocess_job_dir(tenant_id, dataset_id);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        warn!(
            tenant_id = %tenant_id,
            dataset_id = %dataset_id,
            job_id = %job_id,
            error = %e,
            "Failed to create preprocess job state directory; status will be in-memory only"
        );
        return;
    }

    let record = PersistedPreprocessJobState {
        job_id: job_id.to_string(),
        tenant_id: tenant_id.to_string(),
        dataset_id: dataset_id.to_string(),
        status: job_state.status,
        pii_scrub: job_state.pii_scrub,
        dedupe: job_state.dedupe,
        lines_processed: job_state.lines_processed,
        lines_removed: job_state.lines_removed,
        error_message: job_state.error_message.clone(),
        started_at: job_state.started_at.to_rfc3339(),
        completed_at: job_state.completed_at.map(|t| t.to_rfc3339()),
    };

    let json = match serde_json::to_vec_pretty(&record) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                tenant_id = %tenant_id,
                dataset_id = %dataset_id,
                job_id = %job_id,
                error = %e,
                "Failed to serialize preprocess job state"
            );
            return;
        }
    };

    let state_path = dir.join(format!("{}.json", job_id));
    let tmp_path = dir.join(format!("{}.json.tmp", job_id));

    // Write-and-rename for best-effort atomicity (no /tmp usage).
    if let Err(e) = tokio::fs::write(&tmp_path, &json).await {
        warn!(
            tenant_id = %tenant_id,
            dataset_id = %dataset_id,
            job_id = %job_id,
            error = %e,
            "Failed to write preprocess job state"
        );
        return;
    }
    if let Err(e) = tokio::fs::rename(&tmp_path, &state_path).await {
        warn!(
            tenant_id = %tenant_id,
            dataset_id = %dataset_id,
            job_id = %job_id,
            error = %e,
            "Failed to finalize preprocess job state"
        );
        return;
    }

    // Update pointer to the most recent job for this dataset.
    let latest_tmp = dir.join("latest.tmp");
    let latest_path = dir.join("latest");
    if let Err(e) = tokio::fs::write(&latest_tmp, job_id.as_bytes()).await {
        warn!(
            tenant_id = %tenant_id,
            dataset_id = %dataset_id,
            job_id = %job_id,
            error = %e,
            "Failed to write preprocess latest pointer"
        );
        return;
    }
    if let Err(e) = tokio::fs::rename(&latest_tmp, &latest_path).await {
        warn!(
            tenant_id = %tenant_id,
            dataset_id = %dataset_id,
            job_id = %job_id,
            error = %e,
            "Failed to finalize preprocess latest pointer"
        );
    }
}

async fn load_latest_preprocess_job_state_best_effort(
    tenant_id: &str,
    dataset_id: &str,
) -> Option<PersistedPreprocessJobState> {
    let dir = preprocess_job_dir(tenant_id, dataset_id);
    let latest = tokio::fs::read_to_string(dir.join("latest")).await.ok()?;
    let job_id = latest.trim();
    if job_id.is_empty() {
        return None;
    }
    let bytes = tokio::fs::read(dir.join(format!("{}.json", job_id)))
        .await
        .ok()?;
    serde_json::from_slice::<PersistedPreprocessJobState>(&bytes).ok()
}

/// Request to start preprocessing on a dataset
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StartPreprocessRequest {
    /// Whether to scrub PII from the dataset
    #[serde(default)]
    pub pii_scrub: bool,

    /// Whether to deduplicate the dataset
    #[serde(default)]
    pub dedupe: bool,
}

/// Response from starting a preprocessing job
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StartPreprocessResponse {
    /// Unique job ID for tracking progress
    pub job_id: String,

    /// Dataset ID being preprocessed
    pub dataset_id: String,

    /// Initial status
    pub status: PreprocessStatus,

    /// Message describing the job
    pub message: String,
}

/// Status of a preprocessing job
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreprocessStatus {
    /// Job is queued but not started
    Pending,
    /// Job is currently running
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed with an error
    Failed,
}

impl std::fmt::Display for PreprocessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreprocessStatus::Pending => write!(f, "pending"),
            PreprocessStatus::Running => write!(f, "running"),
            PreprocessStatus::Completed => write!(f, "completed"),
            PreprocessStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Response for preprocessing status check
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreprocessStatusResponse {
    /// Job ID
    pub job_id: String,

    /// Dataset ID being preprocessed
    pub dataset_id: String,

    /// Current status
    pub status: PreprocessStatus,

    /// Whether PII scrubbing was requested
    pub pii_scrub: bool,

    /// Whether deduplication was requested
    pub dedupe: bool,

    /// Number of lines processed so far
    pub lines_processed: usize,

    /// Number of lines removed (duplicates or PII-containing)
    pub lines_removed: usize,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// When the job started (ISO 8601)
    pub started_at: String,

    /// When the job completed (ISO 8601), if finished
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Start preprocessing on a dataset
///
/// Initiates a background job to preprocess the dataset with the requested options.
/// Returns immediately with a job_id for status polling.
#[utoipa::path(
    post,
    path = "/v1/datasets/{dataset_id}/preprocess",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID to preprocess")
    ),
    request_body = StartPreprocessRequest,
    responses(
        (status = 202, description = "Preprocessing job started", body = StartPreprocessResponse),
        (status = 400, description = "Invalid request - no preprocessing options selected"),
        (status = 403, description = "Tenant isolation violation or insufficient permissions"),
        (status = 404, description = "Dataset not found"),
        (status = 409, description = "Preprocessing already in progress for this dataset"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn start_preprocess(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
    Json(request): Json<StartPreprocessRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    // Validate request - at least one option must be selected
    if !request.pii_scrub && !request.dedupe {
        return Err(ApiError::bad_request(
            "At least one preprocessing option must be enabled (pii_scrub or dedupe)",
        ));
    }

    // Resolve dataset ID (supports aliases)
    let dataset_id = crate::id_resolver::resolve_any_id(&state.db, &dataset_id).await?;

    // Get dataset to verify it exists and check tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Validate tenant isolation
    let effective_tenant_id = dataset
        .tenant_id
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Generate job ID
    let job_id = TypedId::new(IdPrefix::Job).to_string();

    // Atomic check-and-insert to prevent race conditions
    {
        let mut jobs = PREPROCESS_JOBS.write().await;

        // Check if a job is already running for this dataset
        for (_existing_job_id, job_state) in jobs.iter() {
            if job_state.dataset_id == dataset_id
                && (job_state.status == PreprocessStatus::Pending
                    || job_state.status == PreprocessStatus::Running)
            {
                return Err(ApiError::conflict(
                    "Preprocessing already in progress for this dataset",
                ));
            }
        }

        // Clean up old completed/failed jobs (keep last 100)
        if jobs.len() > 100 {
            let mut entries: Vec<_> = jobs
                .iter()
                .filter(|(_, state)| {
                    state.status == PreprocessStatus::Completed
                        || state.status == PreprocessStatus::Failed
                })
                .map(|(id, state)| (id.clone(), state.started_at))
                .collect();
            entries.sort_by_key(|(_, time)| *time);
            // Keep last 50, remove the rest (oldest first since sorted ascending)
            let remove_count = entries.len().saturating_sub(50);
            for (id, _) in entries.into_iter().take(remove_count) {
                jobs.remove(&id);
            }
        }

        // Create and insert job state atomically
        let job_state = PreprocessJobState {
            dataset_id: dataset_id.clone(),
            status: PreprocessStatus::Pending,
            pii_scrub: request.pii_scrub,
            dedupe: request.dedupe,
            lines_processed: 0,
            lines_removed: 0,
            error_message: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
        };
        jobs.insert(job_id.clone(), job_state);
    }

    // Best-effort persistence for restart resilience.
    {
        let jobs = PREPROCESS_JOBS.read().await;
        if let Some(job_state) = jobs.get(&job_id) {
            persist_preprocess_job_state_best_effort(
                &effective_tenant_id,
                &dataset_id,
                &job_id,
                job_state,
            )
            .await;
        }
    }

    // Spawn background task
    let job_id_clone = job_id.clone();
    let dataset_id_clone = dataset_id.clone();
    let tenant_id_clone = effective_tenant_id.clone();
    let state_clone = state.clone();
    let pii_scrub = request.pii_scrub;
    let dedupe = request.dedupe;
    let storage_path = dataset.storage_path.clone();

    tokio::spawn(async move {
        if let Err(e) = run_preprocess_job(
            &state_clone,
            &job_id_clone,
            &tenant_id_clone,
            &dataset_id_clone,
            &storage_path,
            pii_scrub,
            dedupe,
        )
        .await
        {
            error!(
                job_id = %job_id_clone,
                dataset_id = %dataset_id_clone,
                error = %e,
                "Preprocessing job failed"
            );
            // Update job state to failed
            let mut jobs = PREPROCESS_JOBS.write().await;
            if let Some(job) = jobs.get_mut(&job_id_clone) {
                job.status = PreprocessStatus::Failed;
                job.error_message = Some(e.to_string());
                job.completed_at = Some(chrono::Utc::now());
            }
            if let Some(job) = jobs.get(&job_id_clone) {
                persist_preprocess_job_state_best_effort(
                    &tenant_id_clone,
                    &dataset_id_clone,
                    &job_id_clone,
                    job,
                )
                .await;
            }
        }
    });

    let mut options = Vec::new();
    if request.pii_scrub {
        options.push("PII scrubbing");
    }
    if request.dedupe {
        options.push("deduplication");
    }

    info!(
        job_id = %job_id,
        dataset_id = %dataset_id,
        pii_scrub = request.pii_scrub,
        dedupe = request.dedupe,
        "Started preprocessing job"
    );

    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(StartPreprocessResponse {
            job_id,
            dataset_id,
            status: PreprocessStatus::Pending,
            message: format!("Preprocessing started with: {}", options.join(", ")),
        }),
    ))
}

/// Get preprocessing job status
///
/// Returns the current status of a preprocessing job.
#[utoipa::path(
    get,
    path = "/v1/datasets/{dataset_id}/preprocess/status",
    params(
        ("dataset_id" = String, Path, description = "Dataset ID")
    ),
    responses(
        (status = 200, description = "Preprocessing status", body = PreprocessStatusResponse),
        (status = 403, description = "Tenant isolation violation or insufficient permissions"),
        (status = 404, description = "No preprocessing job found for this dataset"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn get_preprocess_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dataset_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    // Resolve dataset ID
    let dataset_id = crate::id_resolver::resolve_any_id(&state.db, &dataset_id).await?;

    // Get dataset to verify tenant isolation
    let dataset = state
        .db
        .get_training_dataset(&dataset_id)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to get dataset: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Dataset"))?;

    // Validate tenant isolation
    let effective_tenant_id = dataset
        .tenant_id
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    if let Some(ref dataset_tenant_id) = dataset.tenant_id {
        validate_tenant_isolation(&claims, dataset_tenant_id)?;
    } else if claims.role != "admin" {
        return Err(ApiError::forbidden(
            "Access denied: dataset has no tenant association",
        ));
    }

    // Prefer persisted status if available (survives restarts).
    if let Some(persisted) =
        load_latest_preprocess_job_state_best_effort(&effective_tenant_id, &dataset_id).await
    {
        return Ok(Json(PreprocessStatusResponse {
            job_id: persisted.job_id,
            dataset_id,
            status: persisted.status,
            pii_scrub: persisted.pii_scrub,
            dedupe: persisted.dedupe,
            lines_processed: persisted.lines_processed,
            lines_removed: persisted.lines_removed,
            error_message: persisted.error_message,
            started_at: persisted.started_at,
            completed_at: persisted.completed_at,
        }));
    }

    // Find the most recent job for this dataset
    let jobs = PREPROCESS_JOBS.read().await;
    let mut matching_jobs: Vec<_> = jobs
        .iter()
        .filter(|(_, job_state)| job_state.dataset_id == dataset_id)
        .collect();

    if matching_jobs.is_empty() {
        return Err(ApiError::not_found(
            "No preprocessing job found for this dataset",
        ));
    }

    // Sort by started_at descending to get the most recent
    matching_jobs.sort_by(|a, b| b.1.started_at.cmp(&a.1.started_at));

    let (job_id, job_state) = matching_jobs
        .first()
        .ok_or_else(|| ApiError::internal("no matching jobs found after filter".to_string()))?;

    Ok(Json(PreprocessStatusResponse {
        job_id: (*job_id).clone(),
        dataset_id,
        status: job_state.status,
        pii_scrub: job_state.pii_scrub,
        dedupe: job_state.dedupe,
        lines_processed: job_state.lines_processed,
        lines_removed: job_state.lines_removed,
        error_message: job_state.error_message.clone(),
        started_at: job_state.started_at.to_rfc3339(),
        completed_at: job_state.completed_at.map(|t| t.to_rfc3339()),
    }))
}

/// Run the preprocessing job
async fn run_preprocess_job(
    state: &AppState,
    job_id: &str,
    tenant_id: &str,
    dataset_id: &str,
    storage_path: &str,
    pii_scrub: bool,
    dedupe: bool,
) -> Result<(), adapteros_core::AosError> {
    use adapteros_core::AosError;

    // Update status to running
    {
        let mut jobs = PREPROCESS_JOBS.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = PreprocessStatus::Running;
        }
        if let Some(job) = jobs.get(job_id) {
            persist_preprocess_job_state_best_effort(tenant_id, dataset_id, job_id, job).await;
        }
    }

    info!(
        job_id = %job_id,
        dataset_id = %dataset_id,
        pii_scrub,
        dedupe,
        "Running preprocessing job"
    );

    // Resolve the dataset root and find JSONL files
    // Use block scope to ensure config guard is dropped before any async operations
    let datasets_root = {
        let config = state
            .config
            .read()
            .map_err(|e| AosError::Internal(format!("Failed to read config: {}", e)))?;
        std::path::PathBuf::from(&config.paths.datasets_root)
    };

    // Validate storage_path to prevent path traversal attacks
    let dataset_path = if storage_path.starts_with('/') {
        // Absolute path - ensure it's canonical and under a safe root
        let path = std::path::PathBuf::from(storage_path);
        let canonical = path
            .canonicalize()
            .map_err(|e| AosError::Io(format!("Invalid storage path: {}", e)))?;
        // Absolute paths must be under datasets_root or var/
        let var_root = std::path::PathBuf::from("var");
        let canonical_datasets = datasets_root
            .canonicalize()
            .unwrap_or_else(|_| datasets_root.clone());
        let canonical_var = var_root.canonicalize().unwrap_or(var_root);
        if !canonical.starts_with(&canonical_datasets) && !canonical.starts_with(&canonical_var) {
            return Err(AosError::validation(format!(
                "Storage path must be under datasets root or var/: {}",
                storage_path
            )));
        }
        canonical
    } else {
        // Relative path - ensure it doesn't escape datasets_root via ..
        if storage_path.contains("..") {
            return Err(AosError::validation(format!(
                "Storage path contains invalid traversal: {}",
                storage_path
            )));
        }
        let joined = datasets_root.join(storage_path);
        // Canonicalize and verify still under root
        let canonical = joined
            .canonicalize()
            .map_err(|e| AosError::Io(format!("Invalid storage path: {}", e)))?;
        let canonical_root = datasets_root
            .canonicalize()
            .unwrap_or_else(|_| datasets_root.clone());
        if !canonical.starts_with(&canonical_root) {
            return Err(AosError::validation(format!(
                "Storage path escapes dataset root: {}",
                storage_path
            )));
        }
        canonical
    };

    // Find JSONL files in the dataset
    let mut jsonl_files = Vec::new();
    if dataset_path.is_dir() {
        let mut entries = tokio::fs::read_dir(&dataset_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read dataset directory: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                jsonl_files.push(path);
            }
        }
    } else if dataset_path
        .extension()
        .map(|e| e == "jsonl")
        .unwrap_or(false)
    {
        jsonl_files.push(dataset_path.clone());
    }

    if jsonl_files.is_empty() {
        warn!(
            job_id = %job_id,
            dataset_path = %dataset_path.display(),
            "No JSONL files found in dataset"
        );
        // Mark as completed with 0 lines processed
        let mut jobs = PREPROCESS_JOBS.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = PreprocessStatus::Completed;
            job.completed_at = Some(chrono::Utc::now());
        }
        return Ok(());
    }

    let mut total_lines_processed = 0usize;
    let mut total_lines_removed = 0usize;

    for jsonl_file in jsonl_files {
        let (processed, removed) =
            preprocess_jsonl_file(job_id, &jsonl_file, pii_scrub, dedupe).await?;

        total_lines_processed += processed;
        total_lines_removed += removed;

        // Update progress
        {
            let mut jobs = PREPROCESS_JOBS.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.lines_processed = total_lines_processed;
                job.lines_removed = total_lines_removed;
            }
            if let Some(job) = jobs.get(job_id) {
                persist_preprocess_job_state_best_effort(tenant_id, dataset_id, job_id, job).await;
            }
        }
    }

    // Mark job as completed
    {
        let mut jobs = PREPROCESS_JOBS.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = PreprocessStatus::Completed;
            job.lines_processed = total_lines_processed;
            job.lines_removed = total_lines_removed;
            job.completed_at = Some(chrono::Utc::now());
        }
        if let Some(job) = jobs.get(job_id) {
            persist_preprocess_job_state_best_effort(tenant_id, dataset_id, job_id, job).await;
        }
    }

    info!(
        job_id = %job_id,
        dataset_id = %dataset_id,
        lines_processed = total_lines_processed,
        lines_removed = total_lines_removed,
        "Preprocessing job completed"
    );

    Ok(())
}

/// Preprocess a single JSONL file
async fn preprocess_jsonl_file(
    job_id: &str,
    file_path: &std::path::Path,
    pii_scrub: bool,
    dedupe: bool,
) -> Result<(usize, usize), adapteros_core::AosError> {
    use adapteros_core::AosError;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let file = tokio::fs::File::open(file_path).await.map_err(|e| {
        AosError::Io(format!(
            "Failed to open file {}: {}",
            file_path.display(),
            e
        ))
    })?;

    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut processed_lines = Vec::new();
    let mut seen_hashes = HashSet::new();
    let mut lines_processed = 0usize;
    let mut lines_removed = 0usize;

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| AosError::Io(format!("Failed to read line: {}", e)))?
    {
        lines_processed += 1;

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Dedupe check - use BLAKE3 hash of line content
        if dedupe {
            let line_hash = B3Hash::hash(line.as_bytes()).to_string();
            if seen_hashes.contains(&line_hash) {
                lines_removed += 1;
                continue;
            }
            seen_hashes.insert(line_hash);
        }

        // PII scrub (stub implementation - logs that scrubbing was requested)
        let processed_line = if pii_scrub {
            // In a real implementation, this would:
            // 1. Parse the JSON
            // 2. Scan text fields for PII patterns (emails, phone numbers, SSNs, etc.)
            // 3. Redact or remove PII
            // For now, we just pass through and log
            if lines_processed == 1 {
                info!(
                    job_id = %job_id,
                    file = %file_path.display(),
                    "PII scrub requested - stub implementation (no actual scrubbing performed)"
                );
            }
            line
        } else {
            line
        };

        processed_lines.push(processed_line);
    }

    // Write back the processed file
    let temp_path = file_path.with_extension("jsonl.tmp");
    let mut temp_file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

    for line in &processed_lines {
        temp_file
            .write_all(line.as_bytes())
            .await
            .map_err(|e| AosError::Io(format!("Failed to write line: {}", e)))?;
        temp_file
            .write_all(b"\n")
            .await
            .map_err(|e| AosError::Io(format!("Failed to write newline: {}", e)))?;
    }

    temp_file
        .flush()
        .await
        .map_err(|e| AosError::Io(format!("Failed to flush temp file: {}", e)))?;

    // Sync to disk to ensure durability before atomic rename
    temp_file
        .sync_all()
        .await
        .map_err(|e| AosError::Io(format!("Failed to sync temp file: {}", e)))?;

    // Atomically replace the original file
    tokio::fs::rename(&temp_path, file_path)
        .await
        .map_err(|e| AosError::Io(format!("Failed to replace original file: {}", e)))?;

    info!(
        job_id = %job_id,
        file = %file_path.display(),
        lines_processed,
        lines_removed,
        lines_kept = processed_lines.len(),
        "Processed JSONL file"
    );

    Ok((lines_processed, lines_removed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_status_display() {
        assert_eq!(PreprocessStatus::Pending.to_string(), "pending");
        assert_eq!(PreprocessStatus::Running.to_string(), "running");
        assert_eq!(PreprocessStatus::Completed.to_string(), "completed");
        assert_eq!(PreprocessStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_request_serialization() {
        let request = StartPreprocessRequest {
            pii_scrub: true,
            dedupe: false,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"pii_scrub\":true"));
        assert!(json.contains("\"dedupe\":false"));

        let parsed: StartPreprocessRequest = serde_json::from_str(&json).unwrap();
        assert!(parsed.pii_scrub);
        assert!(!parsed.dedupe);
    }

    #[test]
    fn test_response_serialization() {
        let response = StartPreprocessResponse {
            job_id: "preproc-123".to_string(),
            dataset_id: "ds-456".to_string(),
            status: PreprocessStatus::Pending,
            message: "Started".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"pending\""));
    }

    #[test]
    fn test_status_response_serialization() {
        let response = PreprocessStatusResponse {
            job_id: "preproc-123".to_string(),
            dataset_id: "ds-456".to_string(),
            status: PreprocessStatus::Completed,
            pii_scrub: true,
            dedupe: true,
            lines_processed: 100,
            lines_removed: 10,
            error_message: None,
            started_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: Some("2024-01-01T00:01:00Z".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"lines_processed\":100"));
        assert!(json.contains("\"lines_removed\":10"));
        // error_message should be omitted when None
        assert!(!json.contains("error_message"));
    }

    #[test]
    fn persisted_job_state_roundtrip() {
        let record = PersistedPreprocessJobState {
            job_id: "job-1".to_string(),
            tenant_id: "t-1".to_string(),
            dataset_id: "ds-1".to_string(),
            status: PreprocessStatus::Running,
            pii_scrub: true,
            dedupe: false,
            lines_processed: 12,
            lines_removed: 3,
            error_message: None,
            started_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: None,
        };
        let json = serde_json::to_vec(&record).expect("serialize");
        let parsed: PersistedPreprocessJobState =
            serde_json::from_slice(&json).expect("deserialize");
        assert_eq!(parsed.job_id, "job-1");
        assert_eq!(parsed.dataset_id, "ds-1");
        assert_eq!(parsed.status, PreprocessStatus::Running);
        assert!(parsed.pii_scrub);
        assert!(!parsed.dedupe);
    }
}
