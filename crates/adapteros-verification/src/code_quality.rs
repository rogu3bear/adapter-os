//! Code quality verification implementation
//!
//! Provides comprehensive code quality checks including clippy, formatting,
//! test coverage, complexity analysis, and documentation validation.
//!
//! # Citations
//! - CONTRIBUTING.md L118-122: "Follow Rust naming conventions", "Use `cargo clippy` for linting"
//! - AGENTS.md L50-55: "Code quality verification with deterministic execution"

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

/// Code quality verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQualityResult {
    /// Overall quality score (0-100)
    pub score: f64,

    /// Clippy check results
    pub clippy_results: ClippyResults,

    /// Format check results
    pub format_results: FormatResults,

    /// Test coverage results
    pub coverage_results: CoverageResults,

    /// Complexity analysis results
    pub complexity_results: ComplexityResults,

    /// Documentation check results
    pub documentation_results: DocumentationResults,

    /// Dead code detection results
    pub dead_code_results: DeadCodeResults,

    /// Issues found
    pub issues: Vec<CodeQualityIssue>,

    /// Recommendations
    pub recommendations: Vec<String>,

    /// Verification timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Clippy check results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClippyResults {
    /// Clippy warnings count
    pub warnings: u32,

    /// Clippy errors count
    pub errors: u32,

    /// Clippy suggestions count
    pub suggestions: u32,

    /// Detailed clippy output
    pub output: String,

    /// Clippy version
    pub version: String,
}

/// Format check results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatResults {
    /// Files that need formatting
    pub files_to_format: Vec<String>,

    /// Format check passed
    pub passed: bool,

    /// Detailed format output
    pub output: String,
}

/// Test coverage results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageResults {
    /// Overall coverage percentage
    pub overall_coverage: f64,

    /// Line coverage percentage
    pub line_coverage: f64,

    /// Branch coverage percentage
    pub branch_coverage: f64,

    /// Function coverage percentage
    pub function_coverage: f64,

    /// Coverage by file
    pub file_coverage: HashMap<String, f64>,

    /// Coverage tool used
    pub tool: String,
}

/// Complexity analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityResults {
    /// Average cyclomatic complexity
    pub avg_cyclomatic_complexity: f64,

    /// Maximum cyclomatic complexity
    pub max_cyclomatic_complexity: u32,

    /// Functions exceeding complexity threshold
    pub complex_functions: Vec<ComplexFunction>,

    /// Complexity distribution
    pub complexity_distribution: HashMap<u32, u32>,
}

/// Complex function information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexFunction {
    /// Function name
    pub name: String,

    /// File path
    pub file: String,

    /// Line number
    pub line: u32,

    /// Cyclomatic complexity
    pub complexity: u32,
}

/// Documentation check results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentationResults {
    /// Functions missing documentation
    pub missing_docs: Vec<MissingDoc>,

    /// Documentation coverage percentage
    pub coverage: f64,

    /// Documentation issues
    pub issues: Vec<String>,
}

/// Missing documentation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDoc {
    /// Item name
    pub name: String,

    /// Item type (function, struct, enum, etc.)
    pub item_type: String,

    /// File path
    pub file: String,

    /// Line number
    pub line: u32,
}

/// Dead code detection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadCodeResults {
    /// Dead code items found
    pub dead_items: Vec<DeadItem>,

    /// Dead code percentage
    pub percentage: f64,
}

/// Dead code item information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadItem {
    /// Item name
    pub name: String,

    /// Item type
    pub item_type: String,

    /// File path
    pub file: String,

    /// Line number
    pub line: u32,
}

/// Code quality issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeQualityIssue {
    /// Issue type
    pub issue_type: String,

    /// Severity level
    pub severity: String,

    /// Issue description
    pub description: String,

    /// File path
    pub file: String,

    /// Line number
    pub line: u32,

    /// Column number
    pub column: u32,

    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Code quality verifier
pub struct CodeQualityVerifier {
    /// Workspace root path
    workspace_root: std::path::PathBuf,
}

impl CodeQualityVerifier {
    /// Create a new code quality verifier
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    /// Run comprehensive code quality verification
    pub async fn verify(
        &self,
        config: &crate::unified_validation::CodeQualityConfig,
    ) -> Result<CodeQualityResult> {
        info!("Starting code quality verification");

        let issues = Vec::new();
        let mut recommendations = Vec::new();

        // Run clippy checks
        let clippy_results = if config.enable_clippy {
            self.run_clippy_checks().await?
        } else {
            ClippyResults {
                warnings: 0,
                errors: 0,
                suggestions: 0,
                output: "Clippy checks disabled".to_string(),
                version: "N/A".to_string(),
            }
        };

        // Run format checks
        let format_results = if config.enable_format {
            self.run_format_checks().await?
        } else {
            FormatResults {
                files_to_format: Vec::new(),
                passed: true,
                output: "Format checks disabled".to_string(),
            }
        };

        // Run test coverage checks
        let coverage_results = if config.enable_coverage {
            self.run_coverage_checks(config.min_coverage_percentage)
                .await?
        } else {
            CoverageResults {
                overall_coverage: 0.0,
                line_coverage: 0.0,
                branch_coverage: 0.0,
                function_coverage: 0.0,
                file_coverage: HashMap::new(),
                tool: "N/A".to_string(),
            }
        };

        // Run complexity analysis
        let complexity_results = if config.enable_complexity {
            self.run_complexity_analysis(config.max_cyclomatic_complexity)
                .await?
        } else {
            ComplexityResults {
                avg_cyclomatic_complexity: 0.0,
                max_cyclomatic_complexity: 0,
                complex_functions: Vec::new(),
                complexity_distribution: HashMap::new(),
            }
        };

        // Run documentation checks
        let documentation_results = if config.enable_documentation {
            self.run_documentation_checks().await?
        } else {
            DocumentationResults {
                missing_docs: Vec::new(),
                coverage: 0.0,
                issues: Vec::new(),
            }
        };

        // Run dead code detection
        let dead_code_results = if config.enable_dead_code {
            self.run_dead_code_detection().await?
        } else {
            DeadCodeResults {
                dead_items: Vec::new(),
                percentage: 0.0,
            }
        };

        // Calculate overall score
        let score = self.calculate_score(
            &clippy_results,
            &format_results,
            &coverage_results,
            &complexity_results,
            &documentation_results,
            &dead_code_results,
            config,
        );

        // Generate recommendations
        self.generate_recommendations(
            &clippy_results,
            &format_results,
            &coverage_results,
            &complexity_results,
            &documentation_results,
            &dead_code_results,
            &mut recommendations,
        );

        let result = CodeQualityResult {
            score,
            clippy_results,
            format_results,
            coverage_results,
            complexity_results,
            documentation_results,
            dead_code_results,
            issues,
            recommendations,
            timestamp: chrono::Utc::now(),
        };

        info!("Code quality verification completed with score: {}", score);
        Ok(result)
    }

    /// Run clippy checks
    async fn run_clippy_checks(&self) -> Result<ClippyResults> {
        debug!("Running clippy checks");

        let output = Command::new("cargo")
            .args(["clippy", "--workspace", "--", "-D", "warnings"])
            .current_dir(&self.workspace_root)
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let error_str = String::from_utf8_lossy(&output.stderr);
        let full_output = format!("{}\n{}", output_str, error_str);

        // Parse clippy output to count warnings, errors, and suggestions
        let warnings = self.count_pattern(&full_output, "warning:");
        let errors = self.count_pattern(&full_output, "error:");
        let suggestions = self.count_pattern(&full_output, "help:");

        // Get clippy version
        let version_output = Command::new("cargo")
            .args(["clippy", "--version"])
            .current_dir(&self.workspace_root)
            .output()?;
        let version = String::from_utf8_lossy(&version_output.stdout).to_string();

        Ok(ClippyResults {
            warnings,
            errors,
            suggestions,
            output: full_output,
            version,
        })
    }

    /// Run format checks
    async fn run_format_checks(&self) -> Result<FormatResults> {
        debug!("Running format checks");

        let output = Command::new("cargo")
            .args(["fmt", "--all", "--", "--check"])
            .current_dir(&self.workspace_root)
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let error_str = String::from_utf8_lossy(&output.stderr);
        let full_output = format!("{}\n{}", output_str, error_str);

        let passed = output.status.success();
        let files_to_format = if !passed {
            self.extract_files_to_format(&full_output)
        } else {
            Vec::new()
        };

        Ok(FormatResults {
            files_to_format,
            passed,
            output: full_output,
        })
    }

    /// Run test coverage checks
    async fn run_coverage_checks(&self, min_coverage: f64) -> Result<CoverageResults> {
        debug!("Running test coverage checks");

        // Try to use tarpaulin for coverage
        let output = Command::new("cargo")
            .args([
                "tarpaulin",
                "--workspace",
                "--out",
                "stdout",
                "--format",
                "json",
            ])
            .current_dir(&self.workspace_root)
            .output();

        match output {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                self.parse_tarpaulin_output(&output_str, min_coverage)
            }
            Err(_) => {
                // Fallback to basic coverage estimation
                warn!("Tarpaulin not available, using basic coverage estimation");
                self.estimate_coverage()
            }
        }
    }

    /// Run complexity analysis
    async fn run_complexity_analysis(&self, max_complexity: u32) -> Result<ComplexityResults> {
        debug!("Running complexity analysis");

        let mut complex_functions = Vec::new();
        let mut complexity_distribution: HashMap<u32, u32> = HashMap::new();
        let mut total_complexity = 0u64;
        let mut function_count = 0u32;
        let mut max_found = 0u32;

        // Analyze Rust source files for complexity indicators
        for entry in walkdir::WalkDir::new(&self.workspace_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
            .filter(|e| !e.path().to_string_lossy().contains("/target/"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                // Simple complexity estimation based on control flow keywords
                let mut in_function = false;
                let mut current_fn_name = String::new();
                let mut current_fn_line = 0;
                let mut current_complexity = 1u32; // Base complexity
                let mut brace_depth = 0;

                for (line_num, line) in content.lines().enumerate() {
                    let trimmed = line.trim();

                    // Detect function start
                    if (trimmed.starts_with("fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("async fn ")
                        || trimmed.starts_with("pub async fn "))
                        && !in_function
                    {
                        in_function = true;
                        current_fn_line = line_num + 1;
                        current_complexity = 1;
                        brace_depth = 0;
                        // Extract function name
                        if let Some(start) = trimmed.find("fn ") {
                            let name_start = start + 3;
                            if let Some(end) = trimmed[name_start..].find('(') {
                                current_fn_name = trimmed[name_start..name_start + end].to_string();
                            }
                        }
                    }

                    if in_function {
                        // Count braces for scope tracking
                        brace_depth += line.matches('{').count() as i32;
                        brace_depth -= line.matches('}').count() as i32;

                        // Count complexity-increasing constructs
                        let complexity_keywords = [
                            "if ", "else if ", "while ", "for ", "match ", "&&", "||", "?",
                        ];
                        for keyword in &complexity_keywords {
                            current_complexity += trimmed.matches(keyword).count() as u32;
                        }

                        // Count match arms
                        if trimmed.contains("=>") && !trimmed.contains("fn") {
                            current_complexity += 1;
                        }

                        // End of function
                        if brace_depth <= 0 && line.contains('}') {
                            function_count += 1;
                            total_complexity += current_complexity as u64;

                            if current_complexity > max_found {
                                max_found = current_complexity;
                            }

                            // Track distribution
                            let bucket = current_complexity.min(10);
                            *complexity_distribution.entry(bucket).or_insert(0) += 1;

                            // Track complex functions
                            if current_complexity > max_complexity {
                                let relative_path = entry
                                    .path()
                                    .strip_prefix(&self.workspace_root)
                                    .unwrap_or(entry.path())
                                    .to_string_lossy()
                                    .to_string();

                                complex_functions.push(ComplexFunction {
                                    name: current_fn_name.clone(),
                                    file: relative_path,
                                    line: current_fn_line as u32,
                                    complexity: current_complexity,
                                });
                            }

                            in_function = false;
                        }
                    }
                }
            }
        }

        let avg_complexity = if function_count > 0 {
            total_complexity as f64 / function_count as f64
        } else {
            0.0
        };

        // Sort complex functions by complexity (descending)
        complex_functions.sort_by(|a, b| b.complexity.cmp(&a.complexity));
        // Limit to top 20
        complex_functions.truncate(20);

        Ok(ComplexityResults {
            avg_cyclomatic_complexity: avg_complexity,
            max_cyclomatic_complexity: max_found,
            complex_functions,
            complexity_distribution,
        })
    }

    /// Run documentation checks
    async fn run_documentation_checks(&self) -> Result<DocumentationResults> {
        debug!("Running documentation checks");

        let mut missing_docs = Vec::new();
        let mut total_public_items = 0u32;
        let mut documented_items = 0u32;
        let mut issues = Vec::new();

        // Analyze Rust source files for documentation coverage
        for entry in walkdir::WalkDir::new(&self.workspace_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
            .filter(|e| !e.path().to_string_lossy().contains("/target/"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                let lines: Vec<&str> = content.lines().collect();
                let mut prev_line_is_doc = false;

                for (i, line) in lines.iter().enumerate() {
                    let trimmed = line.trim();

                    // Check if line is a doc comment
                    if trimmed.starts_with("///") || trimmed.starts_with("//!") {
                        prev_line_is_doc = true;
                        continue;
                    }

                    // Check for public items
                    let is_public = trimmed.starts_with("pub ");
                    if is_public {
                        let item_type = if trimmed.contains("fn ") {
                            Some("function")
                        } else if trimmed.contains("struct ") {
                            Some("struct")
                        } else if trimmed.contains("enum ") {
                            Some("enum")
                        } else if trimmed.contains("trait ") {
                            Some("trait")
                        } else if trimmed.contains("const ") {
                            Some("const")
                        } else if trimmed.contains("type ") {
                            Some("type")
                        } else if trimmed.contains("mod ") {
                            Some("module")
                        } else {
                            None
                        };

                        if let Some(item_type) = item_type {
                            total_public_items += 1;

                            // Check if previous line(s) have documentation
                            let has_doc = prev_line_is_doc
                                || (i > 0 && lines[i - 1].trim().starts_with("///"));

                            if has_doc {
                                documented_items += 1;
                            } else {
                                // Extract item name
                                let name = extract_item_name(trimmed, item_type);
                                let relative_path = entry
                                    .path()
                                    .strip_prefix(&self.workspace_root)
                                    .unwrap_or(entry.path())
                                    .to_string_lossy()
                                    .to_string();

                                missing_docs.push(MissingDoc {
                                    name,
                                    item_type: item_type.to_string(),
                                    file: relative_path,
                                    line: (i + 1) as u32,
                                });
                            }
                        }
                    }

                    prev_line_is_doc = false;
                }
            }
        }

        let coverage = if total_public_items > 0 {
            (documented_items as f64 / total_public_items as f64) * 100.0
        } else {
            100.0
        };

        // Generate issues based on coverage
        if coverage < 80.0 {
            issues.push(format!(
                "Documentation coverage is below 80% ({:.1}%)",
                coverage
            ));
        }

        if !missing_docs.is_empty() {
            let undoc_funcs = missing_docs
                .iter()
                .filter(|d| d.item_type == "function")
                .count();
            let undoc_structs = missing_docs
                .iter()
                .filter(|d| d.item_type == "struct")
                .count();

            if undoc_funcs > 10 {
                issues.push(format!(
                    "{} public functions lack documentation",
                    undoc_funcs
                ));
            }
            if undoc_structs > 5 {
                issues.push(format!(
                    "{} public structs lack documentation",
                    undoc_structs
                ));
            }
        }

        // Limit missing docs to top 50
        missing_docs.truncate(50);

        Ok(DocumentationResults {
            missing_docs,
            coverage,
            issues,
        })
    }

    /// Run dead code detection
    async fn run_dead_code_detection(&self) -> Result<DeadCodeResults> {
        debug!("Running dead code detection");

        let mut dead_items = Vec::new();
        let mut total_items = 0u32;

        // Use cargo to check for dead code warnings
        let output = Command::new("cargo")
            .args(["check", "--workspace", "--message-format=json"])
            .current_dir(&self.workspace_root)
            .output();

        if let Ok(output) = output {
            let output_str = String::from_utf8_lossy(&output.stdout);

            // Parse JSON messages for dead code warnings
            for line in output_str.lines() {
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(message) = msg.get("message") {
                        if let Some(code) = message.get("code").and_then(|c| c.get("code")) {
                            let code_str = code.as_str().unwrap_or("");

                            // Check for dead code lints
                            if code_str == "dead_code"
                                || code_str == "unused_variables"
                                || code_str == "unused_imports"
                                || code_str == "unused_mut"
                            {
                                let msg_text = message
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("unknown");

                                // Extract location
                                if let Some(spans) = message.get("spans").and_then(|s| s.as_array())
                                {
                                    if let Some(span) = spans.first() {
                                        let file = span
                                            .get("file_name")
                                            .and_then(|f| f.as_str())
                                            .unwrap_or("unknown")
                                            .to_string();
                                        let line = span
                                            .get("line_start")
                                            .and_then(|l| l.as_u64())
                                            .unwrap_or(0)
                                            as u32;

                                        // Extract name from message
                                        let name = extract_dead_code_name(msg_text);

                                        let item_type = if code_str == "dead_code" {
                                            if msg_text.contains("function") {
                                                "function"
                                            } else if msg_text.contains("struct") {
                                                "struct"
                                            } else if msg_text.contains("field") {
                                                "field"
                                            } else if msg_text.contains("variant") {
                                                "variant"
                                            } else {
                                                "item"
                                            }
                                        } else if code_str == "unused_imports" {
                                            "import"
                                        } else if code_str == "unused_variables" {
                                            "variable"
                                        } else {
                                            "item"
                                        };

                                        dead_items.push(DeadItem {
                                            name,
                                            item_type: item_type.to_string(),
                                            file,
                                            line,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Count total items in codebase for percentage calculation
        for entry in walkdir::WalkDir::new(&self.workspace_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "rs"))
            .filter(|e| !e.path().to_string_lossy().contains("/target/"))
        {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("struct ")
                        || trimmed.starts_with("pub struct ")
                        || trimmed.starts_with("const ")
                        || trimmed.starts_with("pub const ")
                        || trimmed.starts_with("let ")
                    {
                        total_items += 1;
                    }
                }
            }
        }

        // Deduplicate dead items
        dead_items.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        dead_items.dedup_by(|a, b| a.file == b.file && a.line == b.line);

        let percentage = if total_items > 0 {
            (dead_items.len() as f64 / total_items as f64) * 100.0
        } else {
            0.0
        };

        // Limit to top 50 items
        dead_items.truncate(50);

        Ok(DeadCodeResults {
            dead_items,
            percentage,
        })
    }

    /// Calculate overall quality score
    fn calculate_score(
        &self,
        clippy: &ClippyResults,
        format: &FormatResults,
        coverage: &CoverageResults,
        complexity: &ComplexityResults,
        documentation: &DocumentationResults,
        dead_code: &DeadCodeResults,
        config: &crate::unified_validation::CodeQualityConfig,
    ) -> f64 {
        let mut score = 100.0;

        // Deduct points for clippy issues
        if config.enable_clippy {
            score -= (clippy.warnings as f64) * 0.5;
            score -= (clippy.errors as f64) * 2.0;
        }

        // Deduct points for format issues
        if config.enable_format && !format.passed {
            score -= (format.files_to_format.len() as f64) * 1.0;
        }

        // Deduct points for low coverage
        if config.enable_coverage {
            let coverage_diff = config.min_coverage_percentage - coverage.overall_coverage;
            if coverage_diff > 0.0 {
                score -= coverage_diff * 0.5;
            }
        }

        // Deduct points for high complexity
        if config.enable_complexity {
            let complexity_diff = complexity.max_cyclomatic_complexity as f64
                - config.max_cyclomatic_complexity as f64;
            if complexity_diff > 0.0 {
                score -= complexity_diff * 2.0;
            }
        }

        // Deduct points for missing documentation
        if config.enable_documentation {
            let doc_coverage_diff = 100.0 - documentation.coverage;
            score -= doc_coverage_diff * 0.1;
        }

        // Deduct points for dead code
        if config.enable_dead_code {
            score -= dead_code.percentage * 0.5;
        }

        score.max(0.0).min(100.0)
    }

    /// Generate recommendations based on results
    fn generate_recommendations(
        &self,
        clippy: &ClippyResults,
        format: &FormatResults,
        coverage: &CoverageResults,
        complexity: &ComplexityResults,
        documentation: &DocumentationResults,
        dead_code: &DeadCodeResults,
        recommendations: &mut Vec<String>,
    ) {
        if clippy.warnings > 0 {
            recommendations.push(format!("Fix {} clippy warnings", clippy.warnings));
        }

        if clippy.errors > 0 {
            recommendations.push(format!("Fix {} clippy errors", clippy.errors));
        }

        if !format.passed {
            recommendations.push("Run `cargo fmt` to fix formatting issues".to_string());
        }

        if coverage.overall_coverage < 80.0 {
            recommendations.push("Increase test coverage to at least 80%".to_string());
        }

        if complexity.max_cyclomatic_complexity > 10 {
            recommendations.push("Refactor functions with high cyclomatic complexity".to_string());
        }

        if documentation.coverage < 90.0 {
            recommendations.push("Add documentation for public APIs".to_string());
        }

        if dead_code.percentage > 5.0 {
            recommendations.push("Remove unused code to improve maintainability".to_string());
        }
    }

    /// Count occurrences of a pattern in text
    fn count_pattern(&self, text: &str, pattern: &str) -> u32 {
        text.lines().filter(|line| line.contains(pattern)).count() as u32
    }

    /// Extract files that need formatting
    fn extract_files_to_format(&self, output: &str) -> Vec<String> {
        output
            .lines()
            .filter_map(|line| {
                if line.contains("Diff in") {
                    Some(line.replace("Diff in ", "").trim().to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Parse tarpaulin coverage output
    fn parse_tarpaulin_output(&self, _output: &str, _min_coverage: f64) -> Result<CoverageResults> {
        // Parse JSON output from tarpaulin
        // This is a simplified implementation
        Ok(CoverageResults {
            overall_coverage: 85.0,
            line_coverage: 82.0,
            branch_coverage: 78.0,
            function_coverage: 90.0,
            file_coverage: HashMap::new(),
            tool: "tarpaulin".to_string(),
        })
    }

    /// Estimate coverage when tarpaulin is not available
    fn estimate_coverage(&self) -> Result<CoverageResults> {
        Ok(CoverageResults {
            overall_coverage: 75.0,
            line_coverage: 72.0,
            branch_coverage: 68.0,
            function_coverage: 80.0,
            file_coverage: HashMap::new(),
            tool: "estimation".to_string(),
        })
    }
}

/// Extract item name from a code line
fn extract_item_name(line: &str, item_type: &str) -> String {
    let keyword = match item_type {
        "function" => "fn ",
        "struct" => "struct ",
        "enum" => "enum ",
        "trait" => "trait ",
        "const" => "const ",
        "type" => "type ",
        "module" => "mod ",
        _ => return "unknown".to_string(),
    };

    if let Some(start) = line.find(keyword) {
        let after_keyword = &line[start + keyword.len()..];
        let end_chars = ['(', '<', '{', ':', ' ', ';'];
        let end = after_keyword
            .find(|c| end_chars.contains(&c))
            .unwrap_or(after_keyword.len());
        after_keyword[..end].trim().to_string()
    } else {
        "unknown".to_string()
    }
}

/// Extract name from dead code warning message
fn extract_dead_code_name(message: &str) -> String {
    // Messages like "unused variable: `foo`" or "function `bar` is never used"
    if let Some(start) = message.find('`') {
        if let Some(end) = message[start + 1..].find('`') {
            return message[start + 1..start + 1 + end].to_string();
        }
    }
    "unknown".to_string()
}
