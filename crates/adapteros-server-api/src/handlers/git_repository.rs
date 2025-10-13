//! Git repository management handlers
//!
//! Implements git repository registration, analysis, and training pipeline integration.
//! Follows evidence-first philosophy and security-first principles established in the codebase.

use crate::handlers::{require_role, AppState, Claims, ErrorResponse};
use axum::{
    extract::{Path, State, Extension},
    http::StatusCode,
    response::Json,
};
use adapteros_core::error::AosError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

/// Git repository registration request
#[derive(Debug, Deserialize)]
pub struct RegisterRepositoryRequest {
    pub repo_id: String,
    pub path: String,
    pub branch: Option<String>,
    pub description: Option<String>,
}

/// Git repository registration response
#[derive(Debug, Serialize)]
pub struct RegisterRepositoryResponse {
    pub repo_id: String,
    pub status: String,
    pub analysis: RepositoryAnalysis,
    pub evidence_count: usize,
}

/// Repository analysis result
#[derive(Debug, Serialize)]
pub struct RepositoryAnalysis {
    pub repo_id: String,
    pub languages: Vec<LanguageInfo>,
    pub frameworks: Vec<FrameworkInfo>,
    pub security_scan: SecurityScanResult,
    pub git_info: GitInfo,
    pub evidence_spans: Vec<EvidenceSpan>,
}

/// Language detection result
#[derive(Debug, Serialize)]
pub struct LanguageInfo {
    pub name: String,
    pub files: usize,
    pub lines: usize,
    pub percentage: f32,
}

/// Framework detection result
#[derive(Debug, Serialize)]
pub struct FrameworkInfo {
    pub name: String,
    pub version: Option<String>,
    pub confidence: f32,
    pub files: Vec<String>,
}

/// Security scan result
#[derive(Debug, Serialize)]
pub struct SecurityScanResult {
    pub violations: Vec<SecurityViolation>,
    pub scan_timestamp: String,
    pub status: String,
}

/// Security violation
#[derive(Debug, Serialize)]
pub struct SecurityViolation {
    pub file_path: String,
    pub pattern: String,
    pub line_number: Option<usize>,
    pub severity: String,
}

/// Git repository information
#[derive(Debug, Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub commit_count: usize,
    pub last_commit: String,
    pub authors: Vec<String>,
}

/// Evidence span for repository analysis
#[derive(Debug, Serialize)]
pub struct EvidenceSpan {
    pub span_id: String,
    pub evidence_type: String,
    pub file_path: String,
    pub line_range: (usize, usize),
    pub relevance_score: f32,
    pub content: String,
}

/// Repository training request
#[derive(Debug, Deserialize)]
pub struct TrainRepositoryRequest {
    pub repo_id: String,
    pub config: TrainingConfig,
}

/// Training configuration
#[derive(Debug, Deserialize)]
pub struct TrainingConfig {
    pub rank: usize,
    pub alpha: usize,
    pub epochs: usize,
    pub learning_rate: f32,
    pub batch_size: usize,
    pub targets: Vec<String>,
}

/// Repository training response
#[derive(Debug, Serialize)]
pub struct TrainRepositoryResponse {
    pub training_id: String,
    pub status: String,
    pub estimated_duration: String,
    pub evidence_count: usize,
}

/// Register a new git repository
///
/// Evidence: docs/code-intelligence/code-policies.md:45-78
/// Policy: Evidence requirements for code suggestions
pub async fn register_git_repository(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<RegisterRepositoryRequest>,
) -> std::result::Result<Json<RegisterRepositoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for registration
    require_role(&claims, &["Admin", "Operator"])?;

    info!(
        "Registering git repository: {} at path: {}",
        request.repo_id, request.path
    );

    // Evidence: docs/code-intelligence/code-policies.md:82-84
    // Policy: Path validation and security checks
    let path_validator = PathValidator::new(&state.config.path_policy);
    path_validator
        .validate_repo_path(&request.path, &claims.tenant_id)
        .map_err(|e| {
            warn!("Path validation failed for {}: {}", request.path, e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Path validation failed".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    // Evidence: docs/llm-interface-specification.md:42-47
    // Policy: Deterministic behavior
    let analysis = state
        .git_manager
        .analyze_repository(&request.path, &claims.tenant_id)
        .await
        .map_err(|e| {
            warn!("Repository analysis failed for {}: {}", request.path, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Repository analysis failed".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for analysis
    if analysis.evidence_spans.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Insufficient evidence".to_string(),
                details: "Repository analysis must include at least one evidence span".to_string(),
            }),
        ));
    }

    // Store repository in database
    let repo_id = Uuid::now_v7().to_string();
    state
        .db
        .create_git_repository(
            &repo_id,
            &request.repo_id,
            &request.path,
            &request.branch.unwrap_or_else(|| "main".to_string()),
            &serde_json::to_string(&analysis)
                .map_err(|e| {
                    warn!("Failed to serialize analysis: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "Failed to serialize analysis".to_string(),
                            details: Some(e.to_string()),
                        }),
                    )
                })?,
            &claims.user_id,
        )
        .await
        .map_err(|e| {
            warn!("Failed to store repository {}: {}", request.repo_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to store repository".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    info!("Successfully registered repository: {}", request.repo_id);

    Ok(Json(RegisterRepositoryResponse {
        repo_id: request.repo_id,
        status: "registered".to_string(),
        analysis,
        evidence_count: analysis.evidence_spans.len(),
    }))
}

/// Get repository analysis
///
/// Evidence: docs/code-intelligence/code-policies.md:45-78
/// Policy: Evidence requirements for analysis retrieval
pub async fn get_repository_analysis(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> std::result::Result<Json<RepositoryAnalysis>, (StatusCode, Json<ErrorResponse>)> {
    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for analysis retrieval
    require_role(&claims, &["Admin", "Operator", "SRE", "Viewer"])?;

    info!("Retrieving analysis for repository: {}", repo_id);

    let repository = state
        .db
        .get_git_repository(&repo_id)
        .await
        .map_err(|e| {
            warn!("Failed to retrieve repository {}: {}", repo_id, e);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Repository not found".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    let analysis: RepositoryAnalysis = serde_json::from_str(&repository.analysis_json)
        .map_err(|e| {
            warn!("Failed to parse analysis for {}: {}", repo_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to parse analysis".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    Ok(Json(analysis))
}

/// Train repository adapter
///
/// Evidence: docs/code-intelligence/code-implementation-roadmap.md:173-270
/// Pattern: Training pipeline with evidence-based adapter creation
pub async fn train_repository_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
    Json(request): Json<TrainRepositoryRequest>,
) -> std::result::Result<Json<TrainRepositoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for training
    require_role(&claims, &["Admin", "Operator"])?;

    info!("Starting adapter training for repository: {}", repo_id);

    // Get repository analysis
    let repository = state
        .db
        .get_git_repository(&repo_id)
        .await
        .map_err(|e| {
            warn!("Failed to retrieve repository {}: {}", repo_id, e);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Repository not found".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    let analysis: RepositoryAnalysis = serde_json::from_str(&repository.analysis_json)
        .map_err(|e| {
            warn!("Failed to parse analysis for {}: {}", repo_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to parse analysis".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for training
    if analysis.evidence_spans.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Insufficient evidence".to_string(),
                details: "Repository must have evidence spans for training".to_string(),
            }),
        ));
    }

    // Evidence: docs/code-intelligence/code-implementation-roadmap.md:173-270
    // Pattern: Training pipeline with evidence-based adapter creation
    let training_id = Uuid::now_v7().to_string();
    let estimated_duration = estimate_training_duration(&request.config, &analysis);

    // Start training job
    state
        .training_manager
        .start_training(
            &training_id,
            &repo_id,
            &request.config,
            &analysis.evidence_spans,
            &claims.user_id,
        )
        .await
        .map_err(|e| {
            warn!("Failed to start training for {}: {}", repo_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to start training".to_string(),
                    details: e.to_string(),
                }),
            )
        })?;

    info!("Started training job: {} for repository: {}", training_id, repo_id);

    Ok(Json(TrainRepositoryResponse {
        training_id,
        status: "started".to_string(),
        estimated_duration,
        evidence_count: analysis.evidence_spans.len(),
    }))
}

/// Path validator for repository paths
///
/// Evidence: docs/code-intelligence/code-policies.md:82-84
/// Policy: Path restrictions and security validation
struct PathValidator {
    allowlist: Vec<glob::Pattern>,
    denylist: Vec<glob::Pattern>,
}

impl PathValidator {
    fn new(config: &PathPolicy) -> Self {
        Self {
            allowlist: config.allowlist.clone(),
            denylist: config.denylist.clone(),
        }
    }

    fn validate_repo_path(&self, path: &str, tenant_id: &str) -> Result<()> {
        // Evidence: docs/code-intelligence/code-policies.md:82-84
        // Policy: Path allowlist and denylist enforcement
        let canonical_path = std::fs::canonicalize(path)
            .map_err(|e| AosError::Validation(format!("Invalid path: {}", e)))?;

        // Check allowlist
        let allowed = self
            .allowlist
            .iter()
            .any(|pattern| pattern.matches(path));
        if !allowed {
            return Err(AosError::Validation(format!(
                "Path not allowed: {}",
                path
            )));
        }

        // Check denylist
        let denied = self
            .denylist
            .iter()
            .any(|pattern| pattern.matches(path));
        if denied {
            return Err(AosError::Validation(format!("Path denied: {}", path)));
        }

        Ok(())
    }
}

/// Path policy configuration
#[derive(Debug, Clone)]
pub struct PathPolicy {
    pub allowlist: Vec<glob::Pattern>,
    pub denylist: Vec<glob::Pattern>,
}

/// Estimate training duration based on configuration and analysis
///
/// Evidence: docs/code-intelligence/code-implementation-roadmap.md:173-270
/// Pattern: Training duration estimation
fn estimate_training_duration(config: &TrainingConfig, analysis: &RepositoryAnalysis) -> String {
    // Evidence: docs/code-intelligence/code-implementation-roadmap.md:173-270
    // Pattern: Training duration estimation based on evidence count
    let base_time = 5; // minutes
    let evidence_factor = analysis.evidence_spans.len() as f32 / 100.0;
    let config_factor = (config.rank as f32 / 16.0) * (config.epochs as f32 / 3.0);
    
    let total_minutes = (base_time as f32 * (1.0 + evidence_factor + config_factor)) as usize;
    
    if total_minutes < 60 {
        format!("{} minutes", total_minutes)
    } else {
        format!("{} hours {} minutes", total_minutes / 60, total_minutes % 60)
    }
}
