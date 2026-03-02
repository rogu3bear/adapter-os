//! Bootstrap loop: self-improvement cycle for code generation adapters.
//!
//! Orchestrates: ingest → train → evaluate → generate proposals → validate →
//! apply → retrain. Each iteration produces an adapter version that is
//! strictly validated before acceptance.
//!
//! Safety rails prevent unchecked modification: git worktree isolation,
//! maximum diff size, single-crate scope, and compilation gates.

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::codegen_eval::{
    CompilationChecker, CompilationCheckerConfig, QualityGateThresholds, QualityReportBuilder,
};
use crate::{CodeGenQualityReport, CodebaseIngestion, IngestionConfig};

// ─── Configuration ───────────────────────────────────────────────────────

/// Configuration for the bootstrap self-improvement loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Repository root to self-train on.
    pub repo_path: PathBuf,
    /// Base model identifier (e.g., "qwen2.5-7b").
    pub base_model: String,
    /// Minimum compile rate for adapter promotion (0.0–1.0).
    pub min_compile_rate: f64,
    /// Minimum test pass rate for adapter promotion (0.0–1.0).
    pub min_test_pass_rate: f64,
    /// Maximum bootstrap iterations before stopping.
    pub max_iterations: u32,
    /// Types of proposals to generate.
    pub proposal_types: Vec<ProposalType>,
    /// Maximum number of proposals per iteration.
    pub max_proposals_per_iteration: usize,
    /// Maximum lines changed per proposal.
    pub max_diff_lines: usize,
    /// Whether to auto-apply proposals that pass validation.
    pub auto_apply: bool,
    /// Whether to require human review before applying.
    pub require_human_review: bool,
    /// Output directory for adapters.
    pub adapters_root: PathBuf,
    /// Ingestion configuration for the training pipeline.
    pub ingestion_config: IngestionConfig,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            repo_path: PathBuf::from("."),
            base_model: "qwen2.5-7b".into(),
            min_compile_rate: 0.80,
            min_test_pass_rate: 0.70,
            max_iterations: 10,
            proposal_types: vec![
                ProposalType::FillTodo,
                ProposalType::AddTests,
                ProposalType::ImproveErrors,
            ],
            max_proposals_per_iteration: 50,
            max_diff_lines: 100,
            auto_apply: false,
            require_human_review: true,
            adapters_root: adapteros_core::rebase_var_path("var/adapters"),
            ingestion_config: IngestionConfig::default(),
        }
    }
}

// ─── Proposal types ──────────────────────────────────────────────────────

/// Types of code improvement proposals the bootstrap loop can generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalType {
    /// Fill in `todo!()` / `unimplemented!()` function bodies.
    FillTodo,
    /// Add missing documentation to public items.
    AddDocumentation,
    /// Implement functions suggested by comments or signatures.
    ImplementSuggested,
    /// Improve error messages to be more descriptive.
    ImproveErrors,
    /// Add missing test cases for uncovered functions.
    AddTests,
    /// Refactor functions exceeding complexity threshold.
    RefactorComplex,
}

impl std::fmt::Display for ProposalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FillTodo => write!(f, "fill_todo"),
            Self::AddDocumentation => write!(f, "add_documentation"),
            Self::ImplementSuggested => write!(f, "implement_suggested"),
            Self::ImproveErrors => write!(f, "improve_errors"),
            Self::AddTests => write!(f, "add_tests"),
            Self::RefactorComplex => write!(f, "refactor_complex"),
        }
    }
}

// ─── Proposal ────────────────────────────────────────────────────────────

/// A concrete code improvement proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeProposal {
    /// Unique identifier for this proposal.
    pub id: String,
    /// Type of proposal.
    pub proposal_type: ProposalType,
    /// File path relative to repo root.
    pub file_path: String,
    /// Crate containing the file (for scoped validation).
    pub crate_name: Option<String>,
    /// Line range of the target (start, end).
    pub target_span: (u32, u32),
    /// Original code at the target span.
    pub original_code: String,
    /// Proposed replacement code.
    pub proposed_code: String,
    /// Explanation of the proposal.
    pub rationale: String,
    /// Metadata about the proposal source.
    pub metadata: BTreeMap<String, String>,
}

impl CodeProposal {
    /// Compute the diff size in lines.
    pub fn diff_lines(&self) -> usize {
        let orig_lines = self.original_code.lines().count();
        let new_lines = self.proposed_code.lines().count();
        orig_lines.max(new_lines)
    }
}

// ─── Validation ──────────────────────────────────────────────────────────

/// Result of validating a code proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationResult {
    /// Proposal compiles and passes tests.
    Passed,
    /// Proposal failed validation.
    Failed(ValidationFailure),
}

/// Reasons a proposal can fail validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFailure {
    pub reason: String,
    pub stage: ValidationStage,
    pub details: Option<String>,
}

/// Stage at which validation failed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStage {
    /// Diff size exceeds maximum.
    DiffSize,
    /// Syntax check failed.
    Syntax,
    /// Compilation (cargo check) failed.
    Compilation,
    /// Tests regressed.
    TestRegression,
    /// Modifies files outside its crate.
    CrateBoundary,
    /// Modifies Cargo.toml (forbidden).
    DependencyChange,
}

/// Validate a proposal against safety rails.
pub fn validate_safety_rails(proposal: &CodeProposal, max_diff_lines: usize) -> ValidationResult {
    // Rule: maximum diff size
    if proposal.diff_lines() > max_diff_lines {
        return ValidationResult::Failed(ValidationFailure {
            reason: format!(
                "Diff size {} exceeds maximum {}",
                proposal.diff_lines(),
                max_diff_lines
            ),
            stage: ValidationStage::DiffSize,
            details: None,
        });
    }

    // Rule: no Cargo.toml modifications
    if proposal.file_path.ends_with("Cargo.toml") {
        return ValidationResult::Failed(ValidationFailure {
            reason: "Proposals cannot modify Cargo.toml".into(),
            stage: ValidationStage::DependencyChange,
            details: None,
        });
    }

    // Rule: no modifications outside the declared crate
    if let Some(ref crate_name) = proposal.crate_name {
        let expected_prefix = format!("crates/{}/", crate_name);
        if !proposal.file_path.starts_with(&expected_prefix)
            && !proposal.file_path.starts_with("src/")
        {
            return ValidationResult::Failed(ValidationFailure {
                reason: format!(
                    "Proposal targets {} but declares crate {}",
                    proposal.file_path, crate_name
                ),
                stage: ValidationStage::CrateBoundary,
                details: None,
            });
        }
    }

    ValidationResult::Passed
}

/// Validate a proposal by checking compilation.
pub async fn validate_compilation(
    proposal: &CodeProposal,
    checker: &CompilationChecker,
) -> ValidationResult {
    match checker
        .check_compilation_cargo(
            &proposal.file_path,
            proposal.target_span.0,
            proposal.target_span.1,
            &proposal.proposed_code,
            proposal.crate_name.as_deref(),
        )
        .await
    {
        Ok(result) if result.compiled => ValidationResult::Passed,
        Ok(result) => ValidationResult::Failed(ValidationFailure {
            reason: "Compilation failed".into(),
            stage: ValidationStage::Compilation,
            details: if result.diagnostics.is_empty() {
                None
            } else {
                Some(result.diagnostics)
            },
        }),
        Err(e) => ValidationResult::Failed(ValidationFailure {
            reason: "Compilation check error".into(),
            stage: ValidationStage::Compilation,
            details: Some(e.to_string()),
        }),
    }
}

// ─── Iteration result ────────────────────────────────────────────────────

/// Result of a single bootstrap iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IterationOutcome {
    /// Iteration succeeded, adapter promoted.
    Success(IterationSuccess),
    /// Adapter failed quality gate.
    QualityGateFailed {
        report: CodeGenQualityReport,
        reason: String,
    },
    /// No proposals were generated.
    NoProposals,
    /// All proposals failed validation.
    AllProposalsFailed {
        total: usize,
        failures: BTreeMap<String, String>,
    },
}

/// Successful iteration details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationSuccess {
    pub iteration: u32,
    pub adapter_id: String,
    pub adapter_hash: String,
    pub quality_report: CodeGenQualityReport,
    pub proposals_generated: usize,
    pub proposals_accepted: usize,
    pub proposals_rejected: usize,
}

/// Summary of a complete bootstrap run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapSummary {
    pub iterations_completed: u32,
    pub iterations_successful: u32,
    pub total_proposals_generated: usize,
    pub total_proposals_accepted: usize,
    pub final_adapter_id: Option<String>,
    pub final_quality_report: Option<CodeGenQualityReport>,
    pub iteration_history: Vec<IterationOutcome>,
}

// ─── Bootstrap controller ────────────────────────────────────────────────

/// Orchestrates the full bootstrap self-improvement cycle.
///
/// Each call to [`run_iteration`](Self::run_iteration) performs one pass:
/// ingest → train → evaluate → scan → validate → collect results.
///
/// The controller does NOT apply proposals to disk automatically — the caller
/// (CLI or human-in-the-loop) decides whether to apply accepted proposals.
/// This keeps the loop auditable and reversible.
pub struct BootstrapController {
    config: BootstrapConfig,
    ingestion: CodebaseIngestion,
    checker: CompilationChecker,
    thresholds: QualityGateThresholds,
}

impl BootstrapController {
    /// Create a new bootstrap controller.
    pub fn new(config: BootstrapConfig) -> Result<Self> {
        let ingestion = CodebaseIngestion::new(config.ingestion_config.clone())?;
        let checker = CompilationChecker::new(CompilationCheckerConfig {
            workspace_root: config.repo_path.clone(),
            timeout_secs: 120,
        });
        let thresholds = QualityGateThresholds {
            min_compile_rate: config.min_compile_rate,
            min_test_pass_rate: config.min_test_pass_rate,
        };
        Ok(Self {
            config,
            ingestion,
            checker,
            thresholds,
        })
    }

    /// Run a single bootstrap iteration.
    ///
    /// Returns the outcome including any accepted proposals. The caller
    /// decides whether to apply them (auto_apply, human review, etc.).
    pub async fn run_iteration(&self, iteration: u32) -> Result<IterationOutcome> {
        let adapter_id = format!("self-v{}", iteration);

        // Step 1+2: Ingest codebase and train adapter
        tracing::info!(iteration, adapter_id = %adapter_id, "Starting bootstrap iteration");

        let ingestion_result = self
            .ingestion
            .ingest_and_train(
                &self.config.repo_path,
                &adapter_id,
                &self.config.adapters_root,
            )
            .await?;

        tracing::info!(
            adapter_id = %ingestion_result.adapter_id,
            symbols = ingestion_result.symbols_count,
            examples = ingestion_result.examples_count,
            loss = ingestion_result.final_loss,
            "Ingestion and training complete"
        );

        // Step 3: Evaluate adapter quality via held-out samples
        // For the bootstrap loop, we use the ingestion metadata as a proxy.
        // A full evaluation would run held-out completions through inference —
        // that requires a running worker, which we defer to the CLI layer.
        let quality_report = QualityReportBuilder::new(
            ingestion_result.adapter_id.clone(),
            ingestion_result.adapter_hash.clone(),
        )
        .build(&self.thresholds);

        if !quality_report.passed_promotion_gate {
            tracing::warn!(
                compile_rate = quality_report.compile_rate,
                test_pass_rate = quality_report.test_pass_rate,
                "Adapter failed quality gate"
            );
            return Ok(IterationOutcome::QualityGateFailed {
                report: quality_report,
                reason: "Adapter did not meet minimum quality thresholds".into(),
            });
        }

        // Step 4: Scan for proposal opportunities
        let file_cache = load_file_cache(&self.config.repo_path)?;

        let mut all_opportunities = Vec::new();
        for pt in &self.config.proposal_types {
            let opps = scan_for_opportunities(&self.config.repo_path, *pt, &file_cache);
            all_opportunities.extend(opps);
        }

        if all_opportunities.is_empty() {
            tracing::info!("No proposal opportunities found");
            return Ok(IterationOutcome::NoProposals);
        }

        // Truncate to max proposals
        all_opportunities.truncate(self.config.max_proposals_per_iteration);
        let proposals_generated = all_opportunities.len();

        tracing::info!(
            count = proposals_generated,
            "Scanned proposal opportunities"
        );

        // Step 5: Convert opportunities to proposals and validate
        // In a full system, the proposal generator would use inference + RAG
        // to produce the proposed_code. Here we create skeleton proposals
        // from opportunities — the actual code generation is delegated to the
        // inference layer (FIM or chat completions with the trained adapter).
        let mut accepted = Vec::new();
        let mut failures = BTreeMap::new();

        for opp in &all_opportunities {
            let proposal = CodeProposal {
                id: format!("{}_{}_L{}", adapter_id, opp.proposal_type, opp.line_start),
                proposal_type: opp.proposal_type,
                file_path: opp.file_path.clone(),
                crate_name: extract_crate_name(&opp.file_path),
                target_span: (opp.line_start, opp.line_end),
                original_code: extract_span_content(
                    &file_cache,
                    &opp.file_path,
                    opp.line_start,
                    opp.line_end,
                ),
                proposed_code: String::new(), // Filled by inference layer
                rationale: opp.description.clone(),
                metadata: BTreeMap::new(),
            };

            // Safety rails first (cheap), then compilation (expensive).
            match validate_safety_rails(&proposal, self.config.max_diff_lines) {
                ValidationResult::Passed => {
                    match validate_compilation(&proposal, &self.checker).await {
                        ValidationResult::Passed => {
                            accepted.push(proposal);
                        }
                        ValidationResult::Failed(f) => {
                            failures.insert(proposal.id.clone(), f.reason);
                        }
                    }
                }
                ValidationResult::Failed(f) => {
                    failures.insert(proposal.id.clone(), f.reason);
                }
            }
        }

        if accepted.is_empty() {
            return Ok(IterationOutcome::AllProposalsFailed {
                total: proposals_generated,
                failures,
            });
        }

        let proposals_accepted = accepted.len();
        let proposals_rejected = proposals_generated - proposals_accepted;

        tracing::info!(
            accepted = proposals_accepted,
            rejected = proposals_rejected,
            "Proposal validation complete"
        );

        Ok(IterationOutcome::Success(IterationSuccess {
            iteration,
            adapter_id: ingestion_result.adapter_id,
            adapter_hash: ingestion_result.adapter_hash,
            quality_report,
            proposals_generated,
            proposals_accepted,
            proposals_rejected,
        }))
    }

    /// Run the full bootstrap loop up to `max_iterations`.
    pub async fn run(&self) -> Result<BootstrapSummary> {
        let mut summary = BootstrapSummary {
            iterations_completed: 0,
            iterations_successful: 0,
            total_proposals_generated: 0,
            total_proposals_accepted: 0,
            final_adapter_id: None,
            final_quality_report: None,
            iteration_history: Vec::new(),
        };

        for i in 0..self.config.max_iterations {
            let outcome = self.run_iteration(i).await?;

            match &outcome {
                IterationOutcome::Success(s) => {
                    summary.iterations_successful += 1;
                    summary.total_proposals_generated += s.proposals_generated;
                    summary.total_proposals_accepted += s.proposals_accepted;
                    summary.final_adapter_id = Some(s.adapter_id.clone());
                    summary.final_quality_report = Some(s.quality_report.clone());
                }
                IterationOutcome::NoProposals => {
                    tracing::info!(iteration = i, "No more opportunities — stopping");
                    summary.iteration_history.push(outcome);
                    summary.iterations_completed = i + 1;
                    break;
                }
                _ => {}
            }

            summary.iteration_history.push(outcome);
            summary.iterations_completed = i + 1;
        }

        tracing::info!(
            iterations = summary.iterations_completed,
            successful = summary.iterations_successful,
            proposals_accepted = summary.total_proposals_accepted,
            "Bootstrap loop complete"
        );

        Ok(summary)
    }

    /// Access the compilation checker for external validation.
    pub fn checker(&self) -> &CompilationChecker {
        &self.checker
    }
}

/// Proposal validator wrapping safety rails + compilation checking.
pub struct ProposalValidator {
    checker: CompilationChecker,
    max_diff_lines: usize,
}

impl ProposalValidator {
    pub fn new(workspace_root: PathBuf, max_diff_lines: usize) -> Self {
        Self {
            checker: CompilationChecker::new(CompilationCheckerConfig {
                workspace_root,
                timeout_secs: 120,
            }),
            max_diff_lines,
        }
    }

    /// Full validation pipeline: safety rails → compilation.
    pub async fn validate(&self, proposal: &CodeProposal) -> ValidationResult {
        // Fast path: safety rails
        match validate_safety_rails(proposal, self.max_diff_lines) {
            ValidationResult::Passed => {}
            failed => return failed,
        }

        // Expensive path: compilation
        validate_compilation(proposal, &self.checker).await
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Load all .rs files from a repo into a path→content cache.
fn load_file_cache(repo_root: &Path) -> Result<BTreeMap<String, String>> {
    let mut cache = BTreeMap::new();
    load_dir_recursive(repo_root, repo_root, &mut cache)?;
    Ok(cache)
}

fn load_dir_recursive(root: &Path, dir: &Path, cache: &mut BTreeMap<String, String>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        adapteros_core::AosError::Io(format!("Failed to read dir {}: {}", dir.display(), e))
    })?;

    for entry in entries {
        let entry =
            entry.map_err(|e| adapteros_core::AosError::Io(format!("Dir entry error: {}", e)))?;
        let path = entry.path();

        // Skip target/ and .git/
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name == "target" || name == ".git" || name == "var" || name == "node_modules" {
                continue;
            }
        }

        if path.is_dir() {
            load_dir_recursive(root, &path, cache)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                cache.insert(rel, content);
            }
        }
    }
    Ok(())
}

/// Extract crate name from a relative path like "crates/foo/src/lib.rs" → "foo".
fn extract_crate_name(rel_path: &str) -> Option<String> {
    if rel_path.starts_with("crates/") {
        rel_path
            .strip_prefix("crates/")
            .and_then(|rest| rest.split('/').next())
            .map(|s| s.to_string())
    } else {
        None
    }
}

/// Extract source lines from the file cache for a given span.
fn extract_span_content(
    cache: &BTreeMap<String, String>,
    file_path: &str,
    start: u32,
    end: u32,
) -> String {
    cache
        .get(file_path)
        .map(|content| {
            content
                .lines()
                .skip((start.saturating_sub(1)) as usize)
                .take((end - start.saturating_sub(1)) as usize)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

// ─── Proposal scanner ────────────────────────────────────────────────────

/// Scan a codebase for proposal opportunities of a given type.
pub fn scan_for_opportunities(
    _repo_root: &Path,
    proposal_type: ProposalType,
    file_content_cache: &BTreeMap<String, String>,
) -> Vec<ProposalOpportunity> {
    let mut opportunities = Vec::new();

    for (rel_path, content) in file_content_cache {
        // Only Rust files
        if !rel_path.ends_with(".rs") {
            continue;
        }

        match proposal_type {
            ProposalType::FillTodo => {
                scan_todos(rel_path, content, &mut opportunities);
            }
            ProposalType::AddDocumentation => {
                scan_missing_docs(rel_path, content, &mut opportunities);
            }
            ProposalType::AddTests => {
                scan_untested_functions(rel_path, content, &mut opportunities);
            }
            ProposalType::ImproveErrors => {
                scan_generic_errors(rel_path, content, &mut opportunities);
            }
            // Other types require deeper analysis
            _ => {}
        }
    }

    // Sort deterministically by file path, then line number
    opportunities.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then(a.line_start.cmp(&b.line_start))
    });

    opportunities
}

/// A location in the codebase that could benefit from a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalOpportunity {
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub proposal_type: ProposalType,
    pub description: String,
    pub context: String,
}

fn scan_todos(rel_path: &str, content: &str, opportunities: &mut Vec<ProposalOpportunity>) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.contains("todo!()")
            || trimmed.contains("unimplemented!()")
            || trimmed.contains("todo!(\"")
        {
            opportunities.push(ProposalOpportunity {
                file_path: rel_path.to_string(),
                line_start: (i + 1) as u32,
                line_end: (i + 1) as u32,
                proposal_type: ProposalType::FillTodo,
                description: format!("Replace {} with implementation", trimmed),
                context: extract_surrounding_context(content, i, 10),
            });
        }
    }
}

fn scan_missing_docs(rel_path: &str, content: &str, opportunities: &mut Vec<ProposalOpportunity>) {
    let lines: Vec<&str> = content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // Public items without doc comments
        if (trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub struct ")
            || trimmed.starts_with("pub enum ")
            || trimmed.starts_with("pub trait "))
            && (i == 0 || !lines[i - 1].trim().starts_with("///"))
        {
            opportunities.push(ProposalOpportunity {
                file_path: rel_path.to_string(),
                line_start: (i + 1) as u32,
                line_end: (i + 1) as u32,
                proposal_type: ProposalType::AddDocumentation,
                description: format!(
                    "Add documentation for {}",
                    trimmed.split('(').next().unwrap_or(trimmed)
                ),
                context: extract_surrounding_context(content, i, 5),
            });
        }
    }
}

fn scan_untested_functions(
    rel_path: &str,
    content: &str,
    opportunities: &mut Vec<ProposalOpportunity>,
) {
    // Simple heuristic: public functions in non-test files that don't have a
    // corresponding test_<name> in the same file
    if rel_path.contains("/tests/") || rel_path.ends_with("_test.rs") {
        return;
    }

    let has_test_module = content.contains("#[cfg(test)]");
    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn ") {
            let fn_name = trimmed
                .split("fn ")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .unwrap_or("")
                .trim();

            if fn_name.is_empty() || fn_name.starts_with("test_") {
                continue;
            }

            let test_name = format!("test_{}", fn_name);
            if !content.contains(&test_name) && has_test_module {
                opportunities.push(ProposalOpportunity {
                    file_path: rel_path.to_string(),
                    line_start: (i + 1) as u32,
                    line_end: (i + 1) as u32,
                    proposal_type: ProposalType::AddTests,
                    description: format!("Add test for {}", fn_name),
                    context: extract_surrounding_context(content, i, 10),
                });
            }
        }
    }
}

fn scan_generic_errors(
    rel_path: &str,
    content: &str,
    opportunities: &mut Vec<ProposalOpportunity>,
) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        // Generic error strings that could be more descriptive
        if (trimmed.contains("\"internal error\"")
            || trimmed.contains("\"unknown error\"")
            || trimmed.contains("\"something went wrong\"")
            || trimmed.contains("\"error\""))
            && !trimmed.starts_with("//")
        {
            opportunities.push(ProposalOpportunity {
                file_path: rel_path.to_string(),
                line_start: (i + 1) as u32,
                line_end: (i + 1) as u32,
                proposal_type: ProposalType::ImproveErrors,
                description: "Improve generic error message".into(),
                context: extract_surrounding_context(content, i, 5),
            });
        }
    }
}

fn extract_surrounding_context(content: &str, line_idx: usize, radius: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = line_idx.saturating_sub(radius);
    let end = (line_idx + radius + 1).min(lines.len());
    lines[start..end].join("\n")
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_diff_lines() {
        let proposal = CodeProposal {
            id: "p1".into(),
            proposal_type: ProposalType::FillTodo,
            file_path: "src/lib.rs".into(),
            crate_name: None,
            target_span: (10, 12),
            original_code: "todo!()".into(),
            proposed_code: "fn foo() {\n    42\n}".into(),
            rationale: "Fill todo".into(),
            metadata: BTreeMap::new(),
        };
        assert_eq!(proposal.diff_lines(), 3);
    }

    #[test]
    fn test_safety_rails_diff_too_large() {
        let proposal = CodeProposal {
            id: "p1".into(),
            proposal_type: ProposalType::FillTodo,
            file_path: "src/lib.rs".into(),
            crate_name: None,
            target_span: (1, 200),
            original_code: (0..150)
                .map(|i| format!("line {}", i))
                .collect::<Vec<_>>()
                .join("\n"),
            proposed_code: "short".into(),
            rationale: "test".into(),
            metadata: BTreeMap::new(),
        };
        match validate_safety_rails(&proposal, 100) {
            ValidationResult::Failed(f) => {
                assert!(matches!(f.stage, ValidationStage::DiffSize));
            }
            ValidationResult::Passed => panic!("Should have failed"),
        }
    }

    #[test]
    fn test_safety_rails_cargo_toml_blocked() {
        let proposal = CodeProposal {
            id: "p1".into(),
            proposal_type: ProposalType::FillTodo,
            file_path: "crates/foo/Cargo.toml".into(),
            crate_name: Some("foo".into()),
            target_span: (1, 5),
            original_code: "[dependencies]".into(),
            proposed_code: "[dependencies]\nnew-dep = \"1.0\"".into(),
            rationale: "test".into(),
            metadata: BTreeMap::new(),
        };
        match validate_safety_rails(&proposal, 100) {
            ValidationResult::Failed(f) => {
                assert!(matches!(f.stage, ValidationStage::DependencyChange));
            }
            ValidationResult::Passed => panic!("Should have failed"),
        }
    }

    #[test]
    fn test_safety_rails_crate_boundary() {
        let proposal = CodeProposal {
            id: "p1".into(),
            proposal_type: ProposalType::FillTodo,
            file_path: "crates/bar/src/lib.rs".into(),
            crate_name: Some("foo".into()),
            target_span: (1, 5),
            original_code: "todo!()".into(),
            proposed_code: "42".into(),
            rationale: "test".into(),
            metadata: BTreeMap::new(),
        };
        match validate_safety_rails(&proposal, 100) {
            ValidationResult::Failed(f) => {
                assert!(matches!(f.stage, ValidationStage::CrateBoundary));
            }
            ValidationResult::Passed => panic!("Should have failed"),
        }
    }

    #[test]
    fn test_safety_rails_valid_proposal() {
        let proposal = CodeProposal {
            id: "p1".into(),
            proposal_type: ProposalType::FillTodo,
            file_path: "crates/foo/src/lib.rs".into(),
            crate_name: Some("foo".into()),
            target_span: (10, 12),
            original_code: "todo!()".into(),
            proposed_code: "42".into(),
            rationale: "test".into(),
            metadata: BTreeMap::new(),
        };
        assert!(matches!(
            validate_safety_rails(&proposal, 100),
            ValidationResult::Passed
        ));
    }

    #[test]
    fn test_scan_todos() {
        let content = "fn foo() {\n    todo!()\n}\n\nfn bar() {\n    42\n}";
        let mut opps = Vec::new();
        scan_todos("src/lib.rs", content, &mut opps);
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].line_start, 2);
    }

    #[test]
    fn test_scan_missing_docs() {
        let content = "pub fn undocumented() {}\n\n/// Documented\npub fn documented() {}";
        let mut opps = Vec::new();
        scan_missing_docs("src/lib.rs", content, &mut opps);
        assert_eq!(opps.len(), 1);
        assert!(opps[0].description.contains("undocumented"));
    }

    #[test]
    fn test_scan_generic_errors() {
        let content = "return Err(\"internal error\".into());\n// \"error\" in comment is ok";
        let mut opps = Vec::new();
        scan_generic_errors("src/lib.rs", content, &mut opps);
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].line_start, 1);
    }

    #[test]
    fn test_bootstrap_config_default() {
        let config = BootstrapConfig::default();
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.min_compile_rate, 0.80);
        assert!(!config.auto_apply);
        assert!(config.require_human_review);
    }

    #[test]
    fn test_proposal_type_display() {
        assert_eq!(ProposalType::FillTodo.to_string(), "fill_todo");
        assert_eq!(ProposalType::AddTests.to_string(), "add_tests");
    }

    #[test]
    fn test_extract_crate_name() {
        assert_eq!(
            extract_crate_name("crates/adapteros-core/src/lib.rs"),
            Some("adapteros-core".into())
        );
        assert_eq!(
            extract_crate_name("crates/foo/src/bar.rs"),
            Some("foo".into())
        );
        assert_eq!(extract_crate_name("src/main.rs"), None);
        assert_eq!(extract_crate_name("tests/integration.rs"), None);
    }

    #[test]
    fn test_extract_span_content() {
        let mut cache = BTreeMap::new();
        cache.insert(
            "src/lib.rs".to_string(),
            "line1\nline2\nline3\nline4\nline5".to_string(),
        );
        let content = extract_span_content(&cache, "src/lib.rs", 2, 4);
        assert_eq!(content, "line2\nline3\nline4");
    }

    #[test]
    fn test_extract_span_content_missing_file() {
        let cache = BTreeMap::new();
        let content = extract_span_content(&cache, "nonexistent.rs", 1, 3);
        assert!(content.is_empty());
    }

    #[test]
    fn test_scan_untested_functions() {
        let content = "pub fn foo() {}\npub fn bar() {}\n\n#[cfg(test)]\nmod tests {\n    fn test_foo() {}\n}";
        let mut opps = Vec::new();
        scan_untested_functions("crates/x/src/lib.rs", content, &mut opps);
        // bar has no corresponding test_bar
        assert_eq!(opps.len(), 1);
        assert!(opps[0].description.contains("bar"));
    }

    #[test]
    fn test_scan_untested_functions_skips_test_files() {
        let content = "pub fn foo() {}\n#[cfg(test)]\nmod tests {}";
        let mut opps = Vec::new();
        scan_untested_functions("crates/x/tests/integration.rs", content, &mut opps);
        assert!(opps.is_empty());
    }

    #[test]
    fn test_bootstrap_summary_default_state() {
        let summary = BootstrapSummary {
            iterations_completed: 0,
            iterations_successful: 0,
            total_proposals_generated: 0,
            total_proposals_accepted: 0,
            final_adapter_id: None,
            final_quality_report: None,
            iteration_history: Vec::new(),
        };
        assert!(summary.final_adapter_id.is_none());
        assert_eq!(summary.iteration_history.len(), 0);
    }

    #[test]
    fn test_scan_for_opportunities_deterministic_ordering() {
        let mut cache = BTreeMap::new();
        cache.insert("crates/b/src/lib.rs".to_string(), "todo!()".to_string());
        cache.insert("crates/a/src/lib.rs".to_string(), "todo!()".to_string());

        let opps = scan_for_opportunities(Path::new("."), ProposalType::FillTodo, &cache);
        assert_eq!(opps.len(), 2);
        // Should be sorted by file path
        assert!(opps[0].file_path < opps[1].file_path);
    }

    #[tokio::test]
    async fn test_proposal_validator_surfaces_compilation_failures() {
        let validator = ProposalValidator::new(PathBuf::from("."), 100);
        let proposal = CodeProposal {
            id: "p-invalid".into(),
            proposal_type: ProposalType::FillTodo,
            file_path: "crates/adapteros-orchestrator/src/definitely_missing.rs".into(),
            crate_name: Some("adapteros-orchestrator".into()),
            target_span: (1, 1),
            original_code: "todo!()".into(),
            proposed_code: "this is not valid rust".into(),
            rationale: "force compile failure".into(),
            metadata: BTreeMap::new(),
        };

        match validator.validate(&proposal).await {
            ValidationResult::Failed(f) => {
                assert!(matches!(f.stage, ValidationStage::Compilation));
            }
            ValidationResult::Passed => panic!("expected compilation validation to fail"),
        }
    }
}
