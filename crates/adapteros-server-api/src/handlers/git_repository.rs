//! Git repository management handlers
//!
//! Implements git repository registration, analysis, and training pipeline integration.
//! Follows evidence-first philosophy and security-first principles established in the codebase.

use crate::handlers::{require_any_role, AppState, Claims, ErrorResponse};
use adapteros_core::{error::AosError, Result};
use adapteros_db::users::Role;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path as StdPath;
use tokio::time::{timeout, Duration};
use tracing::info;
use uuid::Uuid;
use walkdir::WalkDir;

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
    pub fallback: bool,
}

/// Repository analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryAnalysis {
    pub repo_id: String,
    pub languages: Vec<LanguageInfo>,
    pub frameworks: Vec<FrameworkInfo>,
    pub security_scan: SecurityScanResult,
    pub git_info: GitInfo,
    pub evidence_spans: Vec<EvidenceSpan>,
}

/// Language detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub name: String,
    pub files: usize,
    pub lines: usize,
    pub percentage: f32,
}

/// Framework detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkInfo {
    pub name: String,
    pub version: Option<String>,
    pub confidence: f32,
    pub files: Vec<String>,
}

/// Security scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanResult {
    pub violations: Vec<SecurityViolation>,
    pub scan_timestamp: String,
    pub status: String,
}

/// Security violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityViolation {
    pub file_path: String,
    pub pattern: String,
    pub line_number: Option<usize>,
    pub severity: String,
}

/// Git repository information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: String,
    pub commit_count: usize,
    pub last_commit: String,
    pub authors: Vec<String>,
}

/// Evidence span for repository analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    info!(
        "Registering git repository: {} at path: {}",
        request.repo_id, request.path
    );

    // Validate repository path for security (path traversal prevention)
    let default_policy = PathPolicy {
        allowlist: vec![],
        denylist: vec![],
    };
    let validator = PathValidator::new(&default_policy);

    validator
        .validate_repo_path(&request.path, &claims.tenant_id)
        .map_err(|e| {
            tracing::warn!("Path validation failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Path validation failed")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Validate branch name if provided
    if let Some(ref branch) = request.branch {
        PathValidator::validate_branch_path(branch).map_err(|e| {
            tracing::warn!("Branch validation failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Branch validation failed")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    }

    // Check if path exists
    if !std::path::Path::new(&request.path).exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("repository path does not exist")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!("Path: {}", request.path)),
            ),
        ));
    }

    let enabled = state
        .plugin_registry
        .is_enabled_for_tenant("git", &claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Plugin registry error").with_string_details(e.to_string()),
                ),
            )
        })?;

    let (analysis, fallback) = if !enabled {
        let empty_analysis = RepositoryAnalysis {
            repo_id: request.repo_id.clone(),
            languages: vec![],
            frameworks: vec![],
            security_scan: SecurityScanResult {
                violations: vec![],
                scan_timestamp: chrono::Utc::now().to_rfc3339(),
                status: "skipped".to_string(),
            },
            git_info: GitInfo {
                branch: request.branch.clone().unwrap_or_else(|| "main".to_string()),
                commit_count: 0,
                last_commit: "plugin_disabled".to_string(),
                authors: vec![],
            },
            evidence_spans: vec![],
        };
        (empty_analysis, true)
    } else {
        let analysis_result = timeout(
            Duration::from_secs(30),
            analyze_repository(&request.path, &request.repo_id),
        )
        .await;

        match analysis_result {
            Ok(Ok(analysis)) => (analysis, false),
            _ => {
                tracing::warn!("Full analysis timed out or failed, falling back to basic analysis");
                match basic_analyze_repository(&request.path, &request.repo_id).await {
                    Ok(basic) => (basic, true),
                    Err(e) => {
                        tracing::error!("Basic analysis also failed: {}", e);
                        let empty_analysis = RepositoryAnalysis {
                            repo_id: request.repo_id.clone(),
                            languages: vec![],
                            frameworks: vec![],
                            security_scan: SecurityScanResult {
                                violations: vec![],
                                scan_timestamp: chrono::Utc::now().to_rfc3339(),
                                status: "failed".to_string(),
                            },
                            git_info: GitInfo {
                                branch: request
                                    .branch
                                    .clone()
                                    .unwrap_or_else(|| "main".to_string()),
                                commit_count: 0,
                                last_commit: "analysis_failed".to_string(),
                                authors: vec![],
                            },
                            evidence_spans: vec![],
                        };
                        (empty_analysis, true)
                    }
                }
            }
        }
    };

    let analysis_json = serde_json::to_string(&analysis).map_err(|e| {
        tracing::error!("Failed to serialize analysis: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to serialize analysis")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Store repository in database
    let repo_id = Uuid::now_v7().to_string();
    state
        .db
        .create_git_repository(
            &repo_id,
            &request.repo_id,
            &request.path,
            &request.branch.unwrap_or_else(|| "main".to_string()),
            &analysis_json,
            &claims.sub,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create git repository: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create git repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Evidence validation only if not fallback
    if !fallback {
        // Evidence: crates/adapteros-policy/src/packs/evidence.rs:126-172
        // Policy: Evidence Ruleset #4 - Mandatory open-book grounding
        if analysis.evidence_spans.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("Evidence Ruleset #4 violation").with_code("INTERNAL_ERROR").with_string_details("Repository analysis must include at least one evidence span for open-book grounding")),
            ));
        }

        // Validate evidence spans meet minimum requirements
        let min_relevance_score = 0.5; // Policy threshold
        let valid_evidence_count = analysis
            .evidence_spans
            .iter()
            .filter(|span| span.relevance_score >= min_relevance_score)
            .count();

        if valid_evidence_count == 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Evidence Ruleset #4 violation")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!(
                            "No evidence spans meet minimum relevance score of {}",
                            min_relevance_score
                        )),
                ),
            ));
        }
    }

    // Log repository registration event
    tracing::info!(
        "Repository registered: {} by user: {} with {} evidence spans, {} languages, {} frameworks, fallback={}",
        request.repo_id,
        claims.sub,
        analysis.evidence_spans.len(),
        analysis.languages.len(),
        analysis.frameworks.len(),
        fallback
    );

    info!("Successfully registered repository: {}", request.repo_id);

    Ok(Json(RegisterRepositoryResponse {
        repo_id: request.repo_id,
        status: "registered".to_string(),
        analysis: analysis.clone(),
        evidence_count: analysis.evidence_spans.len(),
        fallback,
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
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    info!("Retrieving analysis for repository: {}", repo_id);

    // Retrieve repository from database
    let git_repo = state.db.get_git_repository(&repo_id).await.map_err(|e| {
        tracing::error!("Failed to get git repository: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get git repository")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let git_repo = git_repo.ok_or_else(|| {
        tracing::warn!("Repository not found: {}", repo_id);
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Repository not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Repository ID: {}", repo_id)),
            ),
        )
    })?;

    // Parse analysis from JSON
    let analysis: RepositoryAnalysis =
        serde_json::from_str(&git_repo.analysis_json).map_err(|e| {
            tracing::error!("Failed to parse analysis JSON: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to parse analysis")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
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
    require_any_role(&claims, &[Role::Admin, Role::Operator])?;

    info!("Starting adapter training for repository: {}", repo_id);

    // Get repository analysis from database
    let git_repo = state.db.get_git_repository(&repo_id).await.map_err(|e| {
        tracing::error!("Failed to get git repository: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get git repository")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let git_repo = git_repo.ok_or_else(|| {
        tracing::warn!("Repository not found: {}", repo_id);
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("Repository not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Repository ID: {}", repo_id)),
            ),
        )
    })?;

    // Parse analysis from JSON
    let analysis: RepositoryAnalysis =
        serde_json::from_str(&git_repo.analysis_json).map_err(|e| {
            tracing::error!("Failed to parse analysis JSON: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to parse analysis")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Evidence: docs/code-intelligence/code-policies.md:45-78
    // Policy: Evidence requirements for training
    if analysis.evidence_spans.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Insufficient evidence")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("Repository must have evidence spans for training"),
            ),
        ));
    }

    // Evidence: docs/code-intelligence/code-implementation-roadmap.md:173-270
    // Pattern: Training pipeline with evidence-based adapter creation
    let training_id = Uuid::now_v7().to_string();
    let estimated_duration = estimate_training_duration(&request.config, &analysis);

    // Start training job using TrainingService
    let training_config = adapteros_orchestrator::TrainingConfig {
        rank: request.config.rank as u32,
        alpha: request.config.alpha as u32,
        targets: request.config.targets.clone(),
        epochs: request.config.epochs as u32,
        learning_rate: request.config.learning_rate,
        batch_size: request.config.batch_size as u32,
        warmup_steps: None,
        max_seq_length: None,
        gradient_accumulation_steps: None,
        weight_group_config: None,
        lr_schedule: None,
        final_lr: None,
        early_stopping: None,
        patience: None,
        min_delta: None,
        checkpoint_frequency: None,
        max_checkpoints: None,
    };

    let training_job = state
        .training_service
        .start_training(
            format!("repo-{}-adapter", repo_id),
            training_config,
            None, // template_id
            Some(repo_id.clone()),
            None,                           // dataset_id
            Some(claims.tenant_id.clone()), // tenant_id (6th parameter)
            Some(claims.sub.clone()),       // initiated_by (7th parameter)
            Some(claims.role.clone()),      // initiated_by_role (8th parameter)
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to start training job: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to start training job")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Store training job in database
    let training_config_json = serde_json::to_string(&training_job.config).map_err(|e| {
        tracing::error!("Failed to serialize training config: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to serialize training config")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    state
        .db
        .create_training_job(&repo_id, &training_config_json, &claims.sub)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create training job record: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create training job record")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Log training job start event
    tracing::info!(
        "Training job started: {} for repository: {} by user: {} with adapter: {}",
        training_job.id,
        repo_id,
        claims.sub,
        training_job.adapter_name
    );

    info!(
        "Started training job: {} for repository: {}",
        training_job.id, repo_id
    );

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
pub struct PathValidator {
    allowlist: Vec<glob::Pattern>,
    denylist: Vec<glob::Pattern>,
}

impl PathValidator {
    /// Create a new PathValidator from a PathPolicy configuration
    pub fn new(config: &PathPolicy) -> Self {
        Self {
            allowlist: config.allowlist.clone(),
            denylist: config.denylist.clone(),
        }
    }

    /// Validate a repository path for security and policy compliance
    ///
    /// Prevents path traversal attacks and enforces allowlist/denylist policies
    pub fn validate_repo_path(&self, path: &str, _tenant_id: &str) -> Result<()> {
        // Evidence: docs/code-intelligence/code-policies.md:82-84
        // Policy: Path allowlist and denylist enforcement

        // Check for path traversal attempts
        if contains_path_traversal(path) {
            return Err(AosError::Validation(format!(
                "Path traversal attempt detected: {}",
                path
            )));
        }

        // Canonicalize to resolve symlinks and get absolute path
        let canonical_path = std::fs::canonicalize(path)
            .map_err(|e| AosError::Validation(format!("Invalid path: {}", e)))?;

        let canonical_str = canonical_path.to_string_lossy();

        // Check allowlist (if not empty)
        if !self.allowlist.is_empty() {
            let allowed = self
                .allowlist
                .iter()
                .any(|pattern| pattern.matches(&canonical_str));
            if !allowed {
                return Err(AosError::Validation(format!(
                    "Path not in allowlist: {}",
                    path
                )));
            }
        }

        // Check denylist
        let denied = self
            .denylist
            .iter()
            .any(|pattern| pattern.matches(&canonical_str));
        if denied {
            return Err(AosError::Validation(format!("Path denied: {}", path)));
        }

        Ok(())
    }

    /// Validate a branch name for security
    ///
    /// Prevents injection attacks via malicious branch names
    pub fn validate_branch_path(branch: &str) -> Result<()> {
        // Check for empty branch name
        if branch.is_empty() {
            return Err(AosError::Validation(
                "Branch name cannot be empty".to_string(),
            ));
        }

        // Check for path traversal in branch name
        if contains_path_traversal(branch) {
            return Err(AosError::Validation(format!(
                "Path traversal in branch name: {}",
                branch
            )));
        }

        // Check for null bytes
        if branch.contains('\0') {
            return Err(AosError::Validation(format!(
                "Null byte in branch name: {}",
                branch
            )));
        }

        // Disallow control characters
        if branch.chars().any(|c| c.is_control()) {
            return Err(AosError::Validation(format!(
                "Control characters in branch name: {}",
                branch
            )));
        }

        // Disallow shell metacharacters that could be used for injection
        let forbidden_chars = ['|', '&', ';', '$', '`', '>', '<', '!', '{', '}'];
        if branch.chars().any(|c| forbidden_chars.contains(&c)) {
            return Err(AosError::Validation(format!(
                "Forbidden characters in branch name: {}",
                branch
            )));
        }

        // Branch name length limit
        if branch.len() > 255 {
            return Err(AosError::Validation(
                "Branch name too long (max 255 characters)".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate a file path within a repository
    ///
    /// Ensures the file path is safe and doesn't escape the repository root
    pub fn validate_file_path(repo_root: &str, file_path: &str) -> Result<()> {
        // Check for path traversal attempts
        if contains_path_traversal(file_path) {
            return Err(AosError::Validation(format!(
                "Path traversal in file path: {}",
                file_path
            )));
        }

        // Check for null bytes
        if file_path.contains('\0') {
            return Err(AosError::Validation(format!(
                "Null byte in file path: {}",
                file_path
            )));
        }

        // Check for absolute paths (should be relative to repo)
        if file_path.starts_with('/') || file_path.starts_with('\\') {
            return Err(AosError::Validation(format!(
                "File path must be relative: {}",
                file_path
            )));
        }

        // Construct full path and canonicalize
        let full_path = std::path::Path::new(repo_root).join(file_path);

        // Canonicalize repo root
        let canonical_root = std::fs::canonicalize(repo_root)
            .map_err(|e| AosError::Validation(format!("Invalid repo root: {}", e)))?;

        // Canonicalize full path (if it exists)
        let canonical_full = if full_path.exists() {
            std::fs::canonicalize(&full_path)
                .map_err(|e| AosError::Validation(format!("Invalid file path: {}", e)))?
        } else {
            // For non-existent files, normalize the path manually
            let mut normalized = canonical_root.clone();
            for component in std::path::Path::new(file_path).components() {
                match component {
                    std::path::Component::Normal(part) => normalized.push(part),
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => {
                        return Err(AosError::Validation(format!(
                            "Path traversal in file path: {}",
                            file_path
                        )));
                    }
                    _ => {
                        return Err(AosError::Validation(format!(
                            "Invalid path component in: {}",
                            file_path
                        )));
                    }
                }
            }
            normalized
        };

        // Ensure the resolved path is within the repo root
        if !canonical_full.starts_with(&canonical_root) {
            return Err(AosError::Validation(format!(
                "File path escapes repository: {}",
                file_path
            )));
        }

        Ok(())
    }
}

/// Check if a path contains traversal sequences
fn contains_path_traversal(path: &str) -> bool {
    // Check for common traversal patterns
    let traversal_patterns = [
        "..",     // Parent directory
        "./.",    // Hidden current directory tricks
        "..\\",   // Windows-style traversal
        "..%2f",  // URL-encoded forward slash
        "..%5c",  // URL-encoded backslash
        "%2e%2e", // URL-encoded dots
        "....//", // Double-dot variations
        "..;/",   // Semicolon bypass
    ];

    let path_lower = path.to_lowercase();
    for pattern in traversal_patterns {
        if path_lower.contains(pattern) {
            return true;
        }
    }

    // Check for consecutive dots that could indicate traversal
    if path.contains("...") {
        return true;
    }

    // Check path components for exact ".." matches
    for component in std::path::Path::new(path).components() {
        if let std::path::Component::ParentDir = component {
            return true;
        }
    }

    false
}

/// Analyze a Git repository for languages, frameworks, and evidence spans
///
/// Evidence: crates/adapteros-git/src/branch_manager.rs:82-110
/// Pattern: Git2 repository analysis
async fn analyze_repository(path: &str, repo_id: &str) -> Result<RepositoryAnalysis> {
    let repo_path = StdPath::new(path);

    // Open Git repository
    let repo = Repository::open(repo_path)
        .map_err(|e| AosError::Io(format!("Failed to open git repository at {}: {}", path, e)))?;

    // Get Git information
    let git_info = get_git_info(&repo)?;

    // Analyze languages and frameworks
    let (languages, frameworks) = analyze_code_structure(repo_path)?;

    // Perform security scan
    let security_scan = perform_security_scan(repo_path)?;

    // Extract evidence spans
    let evidence_spans = extract_evidence_spans(repo_path)?;

    Ok(RepositoryAnalysis {
        repo_id: repo_id.to_string(),
        languages,
        frameworks,
        security_scan,
        git_info,
        evidence_spans,
    })
}

/// Get Git repository information
fn get_git_info(repo: &Repository) -> Result<GitInfo> {
    let head = repo
        .head()
        .map_err(|e| AosError::Io(format!("Failed to get git HEAD: {}", e)))?;
    let branch_name = head.shorthand().unwrap_or("unknown").to_string();

    // Get commit count
    let mut revwalk = repo
        .revwalk()
        .map_err(|e| AosError::Io(format!("Failed to create git revwalk: {}", e)))?;
    revwalk
        .push_head()
        .map_err(|e| AosError::Io(format!("Failed to push HEAD to revwalk: {}", e)))?;
    let commit_count = revwalk.count();

    // Get last commit
    let last_commit = if let Some(oid) = head.target() {
        if let Ok(commit) = repo.find_commit(oid) {
            commit.summary().unwrap_or("No message").to_string()
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };

    // Extract unique authors from commit history
    let authors = {
        let mut author_set = std::collections::HashSet::new();
        let mut revwalk = repo.revwalk().map_err(|e| {
            AosError::Io(format!("Failed to create git revwalk for authors: {}", e))
        })?;
        revwalk.push_head().map_err(|e| {
            AosError::Io(format!("Failed to push HEAD to revwalk for authors: {}", e))
        })?;

        for oid in revwalk {
            if let Ok(oid) = oid {
                if let Ok(commit) = repo.find_commit(oid) {
                    let author_name = commit.author().name().unwrap_or("unknown").to_string();
                    author_set.insert(author_name);
                }
            }
        }

        let mut authors: Vec<String> = author_set.into_iter().collect();
        authors.sort();

        if authors.is_empty() {
            vec!["unknown".to_string()]
        } else {
            authors
        }
    };

    Ok(GitInfo {
        branch: branch_name,
        commit_count,
        last_commit,
        authors,
    })
}

/// Analyze code structure for languages and frameworks
fn analyze_code_structure(repo_path: &StdPath) -> Result<(Vec<LanguageInfo>, Vec<FrameworkInfo>)> {
    let mut language_counts: HashMap<String, usize> = HashMap::new();
    let mut framework_hints: HashMap<String, Vec<String>> = HashMap::new();

    for entry in WalkDir::new(repo_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Some(extension) = entry.path().extension() {
            if let Some(ext_str) = extension.to_str() {
                let language = match ext_str {
                    "rs" => "Rust",
                    "py" => "Python",
                    "js" => "JavaScript",
                    "ts" => "TypeScript",
                    "go" => "Go",
                    "java" => "Java",
                    "cpp" | "cc" | "cxx" => "C++",
                    "c" => "C",
                    _ => "Other",
                };

                *language_counts.entry(language.to_string()).or_insert(0) += 1;

                // Detect frameworks
                if ext_str == "py" && entry.path().to_string_lossy().contains("django") {
                    framework_hints
                        .entry("Django".to_string())
                        .or_default()
                        .push(entry.path().to_string_lossy().to_string());
                } else if ext_str == "js" && entry.path().to_string_lossy().contains("react") {
                    framework_hints
                        .entry("React".to_string())
                        .or_default()
                        .push(entry.path().to_string_lossy().to_string());
                }
            }
        }
    }

    // Convert to LanguageInfo
    let total_files: usize = language_counts.values().sum();
    let languages: Vec<LanguageInfo> = language_counts
        .into_iter()
        .map(|(name, files)| LanguageInfo {
            name,
            files,
            lines: files * 50, // Estimate
            percentage: (files as f32 / total_files as f32) * 100.0,
        })
        .collect();

    // Convert to FrameworkInfo
    let frameworks: Vec<FrameworkInfo> = framework_hints
        .into_iter()
        .map(|(name, files)| FrameworkInfo {
            name,
            version: None,
            confidence: 0.8, // High confidence for detected frameworks
            files,
        })
        .collect();

    Ok((languages, frameworks))
}

/// Perform security scan on repository
fn perform_security_scan(repo_path: &StdPath) -> Result<SecurityScanResult> {
    let mut violations = Vec::new();

    // Simple security scan for common patterns
    for entry in WalkDir::new(repo_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            // Check for hardcoded secrets
            if content.contains("password") && content.contains("=") {
                violations.push(SecurityViolation {
                    file_path: entry.path().to_string_lossy().to_string(),
                    pattern: "hardcoded_password".to_string(),
                    line_number: None,
                    severity: "medium".to_string(),
                });
            }

            // Check for debug statements
            if content.contains("console.log") || content.contains("print(") {
                violations.push(SecurityViolation {
                    file_path: entry.path().to_string_lossy().to_string(),
                    pattern: "debug_statement".to_string(),
                    line_number: None,
                    severity: "low".to_string(),
                });
            }
        }
    }

    Ok(SecurityScanResult {
        violations,
        scan_timestamp: chrono::Utc::now().to_rfc3339(),
        status: "completed".to_string(),
    })
}

/// Extract evidence spans from repository
fn extract_evidence_spans(repo_path: &StdPath) -> Result<Vec<EvidenceSpan>> {
    let mut evidence_spans = Vec::new();

    // Extract function definitions as evidence spans
    for entry in WalkDir::new(repo_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Some(extension) = entry.path().extension() {
            if let Some(ext_str) = extension.to_str() {
                if ext_str == "rs" || ext_str == "py" || ext_str == "js" || ext_str == "ts" {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        let lines: Vec<&str> = content.lines().collect();

                        for (line_num, line) in lines.iter().enumerate() {
                            // Look for function definitions
                            if line.contains("fn ")
                                || line.contains("def ")
                                || line.contains("function ")
                            {
                                let file_name = entry
                                    .path()
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "unknown".to_string());

                                evidence_spans.push(EvidenceSpan {
                                    span_id: format!("{}-{}", file_name, line_num),
                                    evidence_type: "function_definition".to_string(),
                                    file_path: entry.path().to_string_lossy().to_string(),
                                    line_range: (line_num + 1, line_num + 1),
                                    relevance_score: 0.8,
                                    content: line.trim().to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(evidence_spans)
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
        format!(
            "{} hours {} minutes",
            total_minutes / 60,
            total_minutes % 60
        )
    }
}

/// Basic analysis without Git2 operations for fallback cases
async fn basic_analyze_repository(path: &str, repo_id: &str) -> Result<RepositoryAnalysis> {
    let repo_path = StdPath::new(path);

    let git_info = GitInfo {
        branch: "fallback".to_string(),
        commit_count: 0,
        last_commit: "basic_analysis".to_string(),
        authors: vec![],
    };

    let (languages, frameworks) = analyze_code_structure(repo_path)
        .map_err(|e| AosError::Io(format!("Failed to analyze code structure: {}", e)))?;

    let security_scan = SecurityScanResult {
        violations: vec![],
        scan_timestamp: chrono::Utc::now().to_rfc3339(),
        status: "basic".to_string(),
    };

    let evidence_spans = extract_evidence_spans(repo_path)
        .map_err(|e| AosError::Io(format!("Failed to extract evidence spans: {}", e)))?;

    Ok(RepositoryAnalysis {
        repo_id: repo_id.to_string(),
        languages,
        frameworks,
        security_scan,
        git_info,
        evidence_spans,
    })
}
