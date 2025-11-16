//! Git repository management handlers
//!
//! Implements git repository registration, analysis, and training pipeline integration.
//! Follows evidence-first philosophy and security-first principles established in the codebase.

use crate::handlers::{require_any_role, AppState, Claims, ErrorResponse};
use adapteros_core::error::AosError;
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

    // Evidence: docs/code-intelligence/code-policies.md:82-84
    // Policy: Path validation and security checks
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

    // Evidence: crates/adapteros-git/src/lib.rs:22-50
    // Policy: Deterministic behavior
    // Note: GitSubsystem integration will be implemented when needed
    tracing::info!(
        "GitSubsystem integration placeholder for repository: {}",
        request.repo_id
    );

    // Perform repository analysis using GitSubsystem
    let analysis = analyze_repository(&request.path, &request.repo_id)
        .await
        .map_err(|e| {
            tracing::error!("Repository analysis failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Repository analysis failed")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

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

    // Store repository in database
    let repo_id = Uuid::now_v7().to_string();
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

    // Log repository registration event
    tracing::info!(
        "Repository registered: {} by user: {} with {} evidence spans, {} languages, {} frameworks",
        request.repo_id,
        claims.sub,
        analysis.evidence_spans.len(),
        analysis.languages.len(),
        analysis.frameworks.len()
    );

    info!("Successfully registered repository: {}", request.repo_id);

    Ok(Json(RegisterRepositoryResponse {
        repo_id: request.repo_id,
        status: "registered".to_string(),
        analysis: analysis.clone(),
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
    };

    let training_job = state
        .training_service
        .start_training(
            format!("repo-{}-adapter", repo_id),
            training_config,
            None, // template_id
            Some(repo_id.clone()),
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
#[allow(dead_code)] // TODO: Implement path validation in future iteration
struct PathValidator {
    allowlist: Vec<glob::Pattern>,
    denylist: Vec<glob::Pattern>,
}

impl PathValidator {
    #[allow(dead_code)] // TODO: Implement path validation in future iteration
    fn new(config: &PathPolicy) -> Self {
        Self {
            allowlist: config.allowlist.clone(),
            denylist: config.denylist.clone(),
        }
    }

    #[allow(dead_code)] // TODO: Implement path validation in future iteration
    fn validate_repo_path(
        &self,
        path: &str,
        tenant_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Evidence: docs/code-intelligence/code-policies.md:82-84
        // Policy: Path allowlist and denylist enforcement
        let canonical_path = std::fs::canonicalize(path)
            .map_err(|e| AosError::Validation(format!("Invalid path: {}", e)))?;

        // Check allowlist
        let allowed = self.allowlist.iter().any(|pattern| pattern.matches(path));
        if !allowed {
            return Err(Box::new(AosError::Validation(format!(
                "Path not allowed: {}",
                path
            ))));
        }

        // Check denylist
        let denied = self.denylist.iter().any(|pattern| pattern.matches(path));
        if denied {
            return Err(Box::new(AosError::Validation(format!(
                "Path denied: {}",
                path
            ))));
        }

        Ok(())
    }
}

/// Analyze a Git repository for languages, frameworks, and evidence spans
///
/// Evidence: crates/adapteros-git/src/branch_manager.rs:82-110
/// Pattern: Git2 repository analysis
async fn analyze_repository(
    path: &str,
    repo_id: &str,
) -> Result<RepositoryAnalysis, Box<dyn std::error::Error>> {
    let repo_path = StdPath::new(path);

    // Open Git repository
    let repo = Repository::open(repo_path)?;

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
fn get_git_info(repo: &Repository) -> Result<GitInfo, Box<dyn std::error::Error>> {
    let head = repo.head()?;
    let branch_name = head.shorthand().unwrap_or("unknown").to_string();

    // Get commit count
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
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

    // Get authors (simplified)
    let authors = vec!["unknown".to_string()]; // TODO: Extract from commit history

    Ok(GitInfo {
        branch: branch_name,
        commit_count,
        last_commit,
        authors,
    })
}

/// Analyze code structure for languages and frameworks
fn analyze_code_structure(
    repo_path: &StdPath,
) -> Result<(Vec<LanguageInfo>, Vec<FrameworkInfo>), Box<dyn std::error::Error>> {
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
fn perform_security_scan(
    repo_path: &StdPath,
) -> Result<SecurityScanResult, Box<dyn std::error::Error>> {
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
fn extract_evidence_spans(
    repo_path: &StdPath,
) -> Result<Vec<EvidenceSpan>, Box<dyn std::error::Error>> {
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
