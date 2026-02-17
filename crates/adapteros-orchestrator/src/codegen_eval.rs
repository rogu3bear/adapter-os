//! Code generation evaluation harness.
//!
//! Provides infrastructure for evaluating trained code-generation adapters:
//!
//! 1. **Held-out split** — deterministic hash-based split of training pairs
//! 2. **Quality metrics** — edit distance, token overlap, exact match
//! 3. **Compilation checking** — verify generated code compiles in context
//! 4. **Quality report** — structured report with promotion gate integration
//!
//! The eval harness works with [`CodeTrainingPair`]s from [`code_training_gen`]
//! and the promotion gate system in [`adapteros_db::promotions`].

use crate::code_training_gen::{CodeTrainingPair, CodeTrainingStrategy};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Dataset split
// ---------------------------------------------------------------------------

/// Which split a training pair belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatasetSplit {
    /// 85% — used for training.
    Train,
    /// 10% — used for early stopping / hyperparameter tuning.
    Validation,
    /// 5% — never seen during training; used for eval.
    HeldOut,
}

/// Deterministically assign a training pair to a dataset split.
///
/// Uses BLAKE3 hash of the pair's qualified symbol name (from metadata)
/// so the split is stable across regeneration runs as long as the codebase
/// hasn't changed structurally.
pub fn assign_split(pair: &CodeTrainingPair) -> DatasetSplit {
    let key = pair
        .metadata
        .get("symbol_name")
        .map(|s| s.as_str())
        .unwrap_or("");
    split_from_hash(key)
}

/// Deterministic split from a hashable string key.
///
/// The hash → byte → split mapping:
/// - bytes 0..=216   → Train      (~85%)
/// - bytes 217..=241 → Validation  (~10%)
/// - bytes 242..=255 → HeldOut     (~5%)
fn split_from_hash(key: &str) -> DatasetSplit {
    let hash = blake3::hash(key.as_bytes());
    let byte = hash.as_bytes()[0];
    match byte {
        0..=216 => DatasetSplit::Train,
        217..=241 => DatasetSplit::Validation,
        242..=255 => DatasetSplit::HeldOut,
    }
}

/// Split a collection of training pairs into train/validation/held-out sets.
///
/// Returns `(train, validation, held_out)`. The split is deterministic based
/// on each pair's `symbol_name` metadata key.
pub fn split_dataset(
    pairs: Vec<CodeTrainingPair>,
) -> (
    Vec<CodeTrainingPair>,
    Vec<CodeTrainingPair>,
    Vec<CodeTrainingPair>,
) {
    let mut train = Vec::new();
    let mut validation = Vec::new();
    let mut held_out = Vec::new();

    for pair in pairs {
        match assign_split(&pair) {
            DatasetSplit::Train => train.push(pair),
            DatasetSplit::Validation => validation.push(pair),
            DatasetSplit::HeldOut => held_out.push(pair),
        }
    }

    debug!(
        train = train.len(),
        validation = validation.len(),
        held_out = held_out.len(),
        "Dataset split complete"
    );

    (train, validation, held_out)
}

// ---------------------------------------------------------------------------
// Quality metrics
// ---------------------------------------------------------------------------

/// Compute normalized Levenshtein edit distance between two strings.
///
/// Returns a value in `[0.0, 1.0]` where 0.0 = identical, 1.0 = completely different.
pub fn normalized_edit_distance(a: &str, b: &str) -> f64 {
    let dist = levenshtein_distance(a, b);
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 0.0;
    }
    dist as f64 / max_len as f64
}

/// Standard Levenshtein distance (character-level).
///
/// Uses the two-row optimization for O(min(m,n)) memory.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];

    for (j, item) in prev.iter_mut().enumerate().take(n + 1) {
        *item = j;
    }

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Compute Jaccard similarity of whitespace-split token sets.
///
/// Returns a value in `[0.0, 1.0]` where 1.0 = identical token sets.
pub fn token_overlap(a: &str, b: &str) -> f64 {
    let a_tokens: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let b_tokens: std::collections::HashSet<&str> = b.split_whitespace().collect();

    let intersection = a_tokens.intersection(&b_tokens).count();
    let union = a_tokens.union(&b_tokens).count();

    if union == 0 {
        return 1.0; // Both empty = identical
    }
    intersection as f64 / union as f64
}

/// Check if two strings are exactly equal after normalizing whitespace.
pub fn exact_match(a: &str, b: &str) -> bool {
    normalize_ws(a) == normalize_ws(b)
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Compilation checking
// ---------------------------------------------------------------------------

/// Result of attempting to compile generated code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    /// Whether the code compiled successfully.
    pub compiled: bool,
    /// Compiler diagnostics (errors/warnings).
    pub diagnostics: String,
    /// File that was patched.
    pub file_path: String,
    /// Span that was replaced (start_line, end_line), 1-indexed.
    pub replaced_span: (u32, u32),
}

/// Configuration for the compilation checker.
#[derive(Debug, Clone)]
pub struct CompilationCheckerConfig {
    /// Root of the workspace/project to check against.
    pub workspace_root: PathBuf,
    /// Timeout for `cargo check` in seconds.
    pub timeout_secs: u64,
}

impl Default for CompilationCheckerConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
            timeout_secs: 60,
        }
    }
}

/// Checks whether generated code compiles by patching source files.
///
/// Two modes:
/// - [`check_syntax`](Self::check_syntax): fast, in-memory brace/string balance check
/// - [`check_compilation_cargo`](Self::check_compilation_cargo): full `cargo check` against
///   a patched workspace (expensive, requires exclusive file access)
pub struct CompilationChecker {
    config: CompilationCheckerConfig,
}

impl CompilationChecker {
    pub fn new(config: CompilationCheckerConfig) -> Self {
        Self { config }
    }

    /// Fast in-memory syntax check: balanced braces and unclosed strings.
    ///
    /// This is a rough filter, not a substitute for `cargo check`.
    pub fn check_syntax(
        &self,
        original_source: &str,
        span_start: u32,
        span_end: u32,
        generated_body: &str,
    ) -> CompilationResult {
        let patched = patch_source(original_source, span_start, span_end, generated_body);

        let open = patched.chars().filter(|&c| c == '{').count();
        let close = patched.chars().filter(|&c| c == '}').count();
        let balanced = open == close;
        let has_unclosed = has_unclosed_string_literal(&patched);

        let compiled = balanced && !has_unclosed;
        let diagnostics = if compiled {
            String::new()
        } else {
            let mut diags = Vec::new();
            if !balanced {
                diags.push(format!("Unbalanced braces: {} open, {} close", open, close));
            }
            if has_unclosed {
                diags.push("Unclosed string literal detected".to_string());
            }
            diags.join("; ")
        };

        CompilationResult {
            compiled,
            diagnostics,
            file_path: String::new(),
            replaced_span: (span_start, span_end),
        }
    }

    /// Full compilation check using `cargo check`.
    ///
    /// **Warning**: This temporarily modifies the source file on disk (backs up
    /// and restores). Do not run concurrently on the same file.
    pub async fn check_compilation_cargo(
        &self,
        file_path: &str,
        span_start: u32,
        span_end: u32,
        generated_body: &str,
        crate_name: Option<&str>,
    ) -> Result<CompilationResult> {
        let abs_path = self.config.workspace_root.join(file_path);
        let original = std::fs::read_to_string(&abs_path)
            .map_err(|e| AosError::Io(format!("Failed to read {}: {}", abs_path.display(), e)))?;

        let patched = patch_source(&original, span_start, span_end, generated_body);

        let backup = format!("{}.codegen-eval-backup", abs_path.display());
        std::fs::copy(&abs_path, &backup)
            .map_err(|e| AosError::Io(format!("Failed to backup {}: {}", abs_path.display(), e)))?;

        // Write patched version
        let write_result = std::fs::write(&abs_path, &patched);
        if let Err(e) = write_result {
            // Restore from backup before returning error
            let _ = std::fs::copy(&backup, &abs_path);
            let _ = std::fs::remove_file(&backup);
            return Err(AosError::Io(format!(
                "Failed to write patched {}: {}",
                abs_path.display(),
                e
            )));
        }

        // Run cargo check
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("check").arg("--message-format=short");
        if let Some(name) = crate_name {
            cmd.arg("-p").arg(name);
        }
        cmd.current_dir(&self.config.workspace_root);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let timeout = std::time::Duration::from_secs(self.config.timeout_secs);
        let result = tokio::time::timeout(timeout, cmd.output()).await;

        // Restore original file immediately
        if let Err(e) = std::fs::copy(&backup, &abs_path) {
            warn!(error = %e, path = %abs_path.display(),
                  "Failed to restore backup — manual intervention needed");
        }
        let _ = std::fs::remove_file(&backup);

        match result {
            Ok(Ok(output)) => {
                let compiled = output.status.success();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(CompilationResult {
                    compiled,
                    diagnostics: stderr,
                    file_path: file_path.to_string(),
                    replaced_span: (span_start, span_end),
                })
            }
            Ok(Err(e)) => Err(AosError::Internal(format!(
                "cargo check failed to execute: {}",
                e
            ))),
            Err(_) => Ok(CompilationResult {
                compiled: false,
                diagnostics: format!("cargo check timed out after {}s", self.config.timeout_secs),
                file_path: file_path.to_string(),
                replaced_span: (span_start, span_end),
            }),
        }
    }
}

/// Replace lines `[span_start..=span_end]` (1-indexed) with `replacement`.
fn patch_source(source: &str, span_start: u32, span_end: u32, replacement: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let start_idx = (span_start as usize).saturating_sub(1);
    let end_idx = (span_end as usize).min(lines.len());

    let mut result = String::new();
    for line in &lines[..start_idx] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(replacement);
    if !replacement.ends_with('\n') {
        result.push('\n');
    }
    for line in &lines[end_idx..] {
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Rough check for unclosed string literals (does not handle raw strings or
/// char literals — only used as a pre-filter before `cargo check`).
fn has_unclosed_string_literal(source: &str) -> bool {
    let mut in_string = false;
    let mut prev = '\0';
    for ch in source.chars() {
        if ch == '"' && prev != '\\' {
            in_string = !in_string;
        }
        if ch == '\n' && in_string {
            return true;
        }
        prev = ch;
    }
    false
}

// ---------------------------------------------------------------------------
// Quality report
// ---------------------------------------------------------------------------

/// Comprehensive quality report for a code generation evaluation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGenQualityReport {
    /// Adapter being evaluated.
    pub adapter_id: String,
    /// BLAKE3 hash of the adapter weights.
    pub adapter_hash: String,
    /// ISO 8601 timestamp of the evaluation.
    pub timestamp: String,
    /// Number of held-out functions evaluated.
    pub held_out_count: usize,
    /// Fraction of generated functions that compile (`[0.0, 1.0]`).
    pub compile_rate: f64,
    /// Fraction where crate tests still pass (`[0.0, 1.0]`).
    pub test_pass_rate: f64,
    /// Fraction matching the original exactly (whitespace-normalized).
    pub exact_match_rate: f64,
    /// Mean normalized edit distance (`0.0` = identical).
    pub avg_edit_distance: f64,
    /// Mean token overlap (Jaccard, `1.0` = identical).
    pub avg_token_overlap: f64,
    /// Per-strategy metric breakdown.
    pub strategy_breakdown: BTreeMap<String, StrategyMetrics>,
    /// Whether the adapter passed the promotion quality gate.
    pub passed_promotion_gate: bool,
}

/// Quality metrics for a single code-generation strategy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyMetrics {
    pub count: usize,
    pub compile_rate: f64,
    pub exact_match_rate: f64,
    pub avg_edit_distance: f64,
    pub avg_token_overlap: f64,
}

/// Thresholds for the code generation quality promotion gate.
#[derive(Debug, Clone)]
pub struct QualityGateThresholds {
    /// Minimum compile rate to pass (default: 0.80).
    pub min_compile_rate: f64,
    /// Minimum test pass rate to pass (default: 0.70).
    pub min_test_pass_rate: f64,
}

impl Default for QualityGateThresholds {
    fn default() -> Self {
        Self {
            min_compile_rate: 0.80,
            min_test_pass_rate: 0.70,
        }
    }
}

impl CodeGenQualityReport {
    /// Check if this report passes the promotion quality gate.
    pub fn passes_gate(&self, thresholds: &QualityGateThresholds) -> bool {
        self.compile_rate >= thresholds.min_compile_rate
            && self.test_pass_rate >= thresholds.min_test_pass_rate
    }

    /// Build a promotion gate details JSON for recording via
    /// [`adapteros_db::promotions::RecordGateParams`].
    pub fn gate_details(&self) -> serde_json::Value {
        serde_json::json!({
            "adapter_id": self.adapter_id,
            "adapter_hash": self.adapter_hash,
            "held_out_count": self.held_out_count,
            "compile_rate": self.compile_rate,
            "test_pass_rate": self.test_pass_rate,
            "exact_match_rate": self.exact_match_rate,
            "avg_edit_distance": self.avg_edit_distance,
            "avg_token_overlap": self.avg_token_overlap,
            "strategy_breakdown": self.strategy_breakdown,
        })
    }
}

// ---------------------------------------------------------------------------
// Report builder
// ---------------------------------------------------------------------------

/// A single evaluation sample (generated code vs original).
#[derive(Debug, Clone)]
pub struct EvalSample {
    /// Strategy that produced this pair.
    pub strategy: CodeTrainingStrategy,
    /// The original (ground truth) code.
    pub original: String,
    /// The adapter-generated code.
    pub generated: String,
    /// Whether the generated code compiled.
    pub compiled: bool,
    /// Whether the crate tests still passed after replacement.
    pub tests_passed: bool,
}

/// Accumulates [`EvalSample`]s and produces a [`CodeGenQualityReport`].
pub struct QualityReportBuilder {
    adapter_id: String,
    adapter_hash: String,
    results: Vec<EvalSample>,
}

impl QualityReportBuilder {
    pub fn new(adapter_id: String, adapter_hash: String) -> Self {
        Self {
            adapter_id,
            adapter_hash,
            results: Vec::new(),
        }
    }

    pub fn add_sample(&mut self, sample: EvalSample) {
        self.results.push(sample);
    }

    pub fn sample_count(&self) -> usize {
        self.results.len()
    }

    /// Build the final quality report.
    pub fn build(self, thresholds: &QualityGateThresholds) -> CodeGenQualityReport {
        let held_out_count = self.results.len();
        if held_out_count == 0 {
            return CodeGenQualityReport {
                adapter_id: self.adapter_id,
                adapter_hash: self.adapter_hash,
                timestamp: chrono::Utc::now().to_rfc3339(),
                held_out_count: 0,
                compile_rate: 0.0,
                test_pass_rate: 0.0,
                exact_match_rate: 0.0,
                avg_edit_distance: 1.0,
                avg_token_overlap: 0.0,
                strategy_breakdown: BTreeMap::new(),
                passed_promotion_gate: false,
            };
        }

        let mut compiled_count = 0usize;
        let mut tests_passed_count = 0usize;
        let mut exact_count = 0usize;
        let mut total_edit_dist = 0.0f64;
        let mut total_token_overlap = 0.0f64;

        // Per-strategy accumulators: (count, compiled, exact, edit_sum, overlap_sum)
        let mut strat_acc: BTreeMap<String, (usize, usize, usize, f64, f64)> = BTreeMap::new();

        for sample in &self.results {
            if sample.compiled {
                compiled_count += 1;
            }
            if sample.tests_passed {
                tests_passed_count += 1;
            }
            let is_exact = exact_match(&sample.original, &sample.generated);
            if is_exact {
                exact_count += 1;
            }
            let edit_dist = normalized_edit_distance(&sample.original, &sample.generated);
            let tok_overlap = token_overlap(&sample.original, &sample.generated);
            total_edit_dist += edit_dist;
            total_token_overlap += tok_overlap;

            let strat_key = format!("{:?}", sample.strategy);
            let entry = strat_acc.entry(strat_key).or_insert((0, 0, 0, 0.0, 0.0));
            entry.0 += 1;
            if sample.compiled {
                entry.1 += 1;
            }
            if is_exact {
                entry.2 += 1;
            }
            entry.3 += edit_dist;
            entry.4 += tok_overlap;
        }

        let n = held_out_count as f64;

        let strategy_breakdown = strat_acc
            .into_iter()
            .map(|(strat, (count, compiled, exact, edit_sum, overlap_sum))| {
                let c = count as f64;
                (
                    strat,
                    StrategyMetrics {
                        count,
                        compile_rate: compiled as f64 / c,
                        exact_match_rate: exact as f64 / c,
                        avg_edit_distance: edit_sum / c,
                        avg_token_overlap: overlap_sum / c,
                    },
                )
            })
            .collect();

        let mut report = CodeGenQualityReport {
            adapter_id: self.adapter_id,
            adapter_hash: self.adapter_hash,
            timestamp: chrono::Utc::now().to_rfc3339(),
            held_out_count,
            compile_rate: compiled_count as f64 / n,
            test_pass_rate: tests_passed_count as f64 / n,
            exact_match_rate: exact_count as f64 / n,
            avg_edit_distance: total_edit_dist / n,
            avg_token_overlap: total_token_overlap / n,
            strategy_breakdown,
            passed_promotion_gate: false,
        };

        report.passed_promotion_gate = report.passes_gate(thresholds);
        report
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Split tests --

    #[test]
    fn test_split_deterministic() {
        let key = "crate::foo::bar::my_function";
        let s1 = split_from_hash(key);
        let s2 = split_from_hash(key);
        assert_eq!(s1, s2, "Same key must produce same split");
    }

    #[test]
    fn test_split_different_keys() {
        // Different keys should (usually) produce different splits or at least
        // the split function doesn't panic on varied input.
        let splits: Vec<DatasetSplit> = (0..100)
            .map(|i| split_from_hash(&format!("symbol_{}", i)))
            .collect();

        let train = splits.iter().filter(|s| **s == DatasetSplit::Train).count();
        let val = splits
            .iter()
            .filter(|s| **s == DatasetSplit::Validation)
            .count();
        let held = splits
            .iter()
            .filter(|s| **s == DatasetSplit::HeldOut)
            .count();

        assert_eq!(train + val + held, 100);
        // With 100 samples, expect rough distribution. Train should dominate.
        assert!(train > 50, "Train should be majority, got {}", train);
    }

    #[test]
    fn test_split_dataset() {
        let pairs: Vec<CodeTrainingPair> = (0..200)
            .map(|i| {
                let mut meta = BTreeMap::new();
                meta.insert("symbol_name".to_string(), format!("ns::func_{}", i));
                CodeTrainingPair {
                    prompt: format!("prompt_{}", i),
                    completion: format!("completion_{}", i),
                    strategy: CodeTrainingStrategy::SignatureToBody,
                    metadata: meta,
                    weight: 1.0,
                }
            })
            .collect();

        let (train, val, held) = split_dataset(pairs);
        let total = train.len() + val.len() + held.len();
        assert_eq!(total, 200);
        assert!(
            train.len() > 100,
            "Train should be ~170, got {}",
            train.len()
        );
        assert!(held.len() > 0, "Held-out should have some samples");
    }

    // -- Edit distance tests --

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein_distance("", "hello"), 5);
        assert_eq!(levenshtein_distance("hello", ""), 5);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    #[test]
    fn test_levenshtein_known_values() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("saturday", "sunday"), 3);
    }

    #[test]
    fn test_normalized_edit_distance() {
        assert!((normalized_edit_distance("hello", "hello") - 0.0).abs() < 1e-9);
        assert!((normalized_edit_distance("", "") - 0.0).abs() < 1e-9);
        // "a" vs "b" = 1 edit, max_len = 1
        assert!((normalized_edit_distance("a", "b") - 1.0).abs() < 1e-9);
    }

    // -- Token overlap tests --

    #[test]
    fn test_token_overlap_identical() {
        assert!((token_overlap("let x = 1;", "let x = 1;") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_token_overlap_disjoint() {
        assert!((token_overlap("hello world", "foo bar") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_token_overlap_partial() {
        // "let x" vs "let y" => intersection={let}, union={let, x, y} => 1/3
        let overlap = token_overlap("let x", "let y");
        assert!((overlap - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_token_overlap_empty() {
        assert!((token_overlap("", "") - 1.0).abs() < 1e-9);
    }

    // -- Exact match tests --

    #[test]
    fn test_exact_match_identical() {
        assert!(exact_match("fn foo() { 42 }", "fn foo() { 42 }"));
    }

    #[test]
    fn test_exact_match_whitespace_normalized() {
        assert!(exact_match("fn  foo()  {  42  }", "fn foo() { 42 }"));
        assert!(exact_match("  hello  world  ", "hello world"));
    }

    #[test]
    fn test_exact_match_different() {
        assert!(!exact_match("fn foo() { 42 }", "fn foo() { 43 }"));
    }

    // -- Patch source tests --

    #[test]
    fn test_patch_source() {
        let source = "line1\nline2\nline3\nline4\nline5\n";
        // Replace lines 2-3 (1-indexed) with "REPLACED"
        let patched = patch_source(source, 2, 3, "REPLACED");
        assert!(patched.contains("line1"));
        assert!(patched.contains("REPLACED"));
        assert!(patched.contains("line4"));
        assert!(patched.contains("line5"));
        assert!(!patched.contains("line2"));
        assert!(!patched.contains("line3"));
    }

    #[test]
    fn test_patch_source_first_line() {
        let source = "fn old() {}\nother_code\n";
        let patched = patch_source(source, 1, 1, "fn new() {}");
        assert!(patched.contains("fn new() {}"));
        assert!(patched.contains("other_code"));
        assert!(!patched.contains("fn old()"));
    }

    // -- Syntax check tests --

    #[test]
    fn test_syntax_check_balanced() {
        let checker = CompilationChecker::new(CompilationCheckerConfig::default());
        let source = "fn foo() {\n    42\n}\n";
        let result = checker.check_syntax(source, 1, 3, "fn foo() {\n    42\n}");
        assert!(result.compiled);
    }

    #[test]
    fn test_syntax_check_unbalanced() {
        let checker = CompilationChecker::new(CompilationCheckerConfig::default());
        let source = "fn foo() {\n    42\n}\n";
        // Replace with unbalanced braces
        let result = checker.check_syntax(source, 1, 3, "fn foo() {\n    42\n");
        assert!(!result.compiled);
        assert!(result.diagnostics.contains("Unbalanced"));
    }

    // -- Quality report tests --

    #[test]
    fn test_quality_report_empty() {
        let builder = QualityReportBuilder::new("test-adapter".into(), "abc123".into());
        let report = builder.build(&QualityGateThresholds::default());
        assert_eq!(report.held_out_count, 0);
        assert!(!report.passed_promotion_gate);
    }

    #[test]
    fn test_quality_report_all_passing() {
        let mut builder = QualityReportBuilder::new("test-adapter".into(), "abc123".into());

        for i in 0..10 {
            builder.add_sample(EvalSample {
                strategy: CodeTrainingStrategy::SignatureToBody,
                original: format!("let x = {};", i),
                generated: format!("let x = {};", i),
                compiled: true,
                tests_passed: true,
            });
        }

        let report = builder.build(&QualityGateThresholds::default());
        assert_eq!(report.held_out_count, 10);
        assert!((report.compile_rate - 1.0).abs() < 1e-9);
        assert!((report.test_pass_rate - 1.0).abs() < 1e-9);
        assert!((report.exact_match_rate - 1.0).abs() < 1e-9);
        assert!((report.avg_edit_distance - 0.0).abs() < 1e-9);
        assert!(report.passed_promotion_gate);
    }

    #[test]
    fn test_quality_report_below_threshold() {
        let mut builder = QualityReportBuilder::new("test-adapter".into(), "abc123".into());

        // 3 compile, 7 don't → 30% compile rate (below 80% threshold)
        for i in 0..10 {
            builder.add_sample(EvalSample {
                strategy: CodeTrainingStrategy::SignatureToBody,
                original: "let x = 1;".to_string(),
                generated: "let x = 1;".to_string(),
                compiled: i < 3,
                tests_passed: i < 3,
            });
        }

        let report = builder.build(&QualityGateThresholds::default());
        assert!((report.compile_rate - 0.3).abs() < 1e-9);
        assert!(!report.passed_promotion_gate);
    }

    #[test]
    fn test_quality_report_strategy_breakdown() {
        let mut builder = QualityReportBuilder::new("test-adapter".into(), "abc123".into());

        builder.add_sample(EvalSample {
            strategy: CodeTrainingStrategy::SignatureToBody,
            original: "fn a() { 1 }".to_string(),
            generated: "fn a() { 1 }".to_string(),
            compiled: true,
            tests_passed: true,
        });
        builder.add_sample(EvalSample {
            strategy: CodeTrainingStrategy::FillInTheMiddle,
            original: "fn b() { 2 }".to_string(),
            generated: "fn b() { 3 }".to_string(),
            compiled: true,
            tests_passed: false,
        });

        let report = builder.build(&QualityGateThresholds::default());
        assert_eq!(report.strategy_breakdown.len(), 2);
        assert!(report.strategy_breakdown.contains_key("SignatureToBody"));
        assert!(report.strategy_breakdown.contains_key("FillInTheMiddle"));
    }

    #[test]
    fn test_passes_gate_custom_thresholds() {
        let report = CodeGenQualityReport {
            adapter_id: "test".into(),
            adapter_hash: "abc".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
            held_out_count: 10,
            compile_rate: 0.75,
            test_pass_rate: 0.60,
            exact_match_rate: 0.5,
            avg_edit_distance: 0.3,
            avg_token_overlap: 0.7,
            strategy_breakdown: BTreeMap::new(),
            passed_promotion_gate: false,
        };

        // Default thresholds: 80% compile, 70% test → should fail
        assert!(!report.passes_gate(&QualityGateThresholds::default()));

        // Relaxed thresholds
        let relaxed = QualityGateThresholds {
            min_compile_rate: 0.70,
            min_test_pass_rate: 0.50,
        };
        assert!(report.passes_gate(&relaxed));
    }

    #[test]
    fn test_gate_details_json() {
        let report = CodeGenQualityReport {
            adapter_id: "test-adapter".into(),
            adapter_hash: "hash123".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
            held_out_count: 5,
            compile_rate: 0.8,
            test_pass_rate: 0.6,
            exact_match_rate: 0.4,
            avg_edit_distance: 0.3,
            avg_token_overlap: 0.7,
            strategy_breakdown: BTreeMap::new(),
            passed_promotion_gate: false,
        };

        let details = report.gate_details();
        assert_eq!(details["adapter_id"], "test-adapter");
        assert_eq!(details["held_out_count"], 5);
        assert_eq!(details["compile_rate"], 0.8);
    }

    #[test]
    fn test_unclosed_string_detection() {
        assert!(!has_unclosed_string_literal("let x = \"hello\";"));
        assert!(has_unclosed_string_literal("let x = \"hello\nworld\";"));
        assert!(!has_unclosed_string_literal("let x = \"hello\\\"world\";"));
    }
}
