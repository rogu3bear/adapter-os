//! Production Readiness Policy Pack
//!
//! Enforces code hygiene requirements for production deployments:
//! - Unwrap density limits (panics per 100 LOC)
//! - Unsafe code restrictions (only allowed in FFI modules)
//! - Handler commitment enforcement (no uncommitted handlers)
//! - Bypass cfg-gating requirements (dev bypasses must be cfg-gated)
//! - Forbidden marker detection (XXX, HACK, FIXME:security)
//!
//! ## Configuration
//!
//! The policy supports environment-aware defaults:
//! - **Production**: Strict enforcement with low unwrap density, zero unsafe outside FFI
//! - **Development**: Lenient enforcement for rapid iteration
//!
//! ## Enforcement Points
//!
//! - CI pipeline: Block merges with policy violations
//! - Code review: Surface issues before merge
//! - Promotion gate: Prevent non-compliant code from reaching production
//!
//! ## Integration Example
//!
//! ```ignore
//! use adapteros_policy::packs::production_readiness::{
//!     ProductionReadinessPolicy, ProductionReadinessConfig, CodeAnalysisContext,
//! };
//!
//! let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
//!
//! let ctx = CodeAnalysisContext {
//!     file_path: "src/handlers.rs".to_string(),
//!     total_lines: 500,
//!     unwrap_count: 12,
//!     unsafe_blocks: vec![],
//!     forbidden_markers: vec![],
//!     uncommitted_handlers: vec![],
//!     ungated_bypasses: vec![],
//! };
//!
//! let result = policy.analyze(&ctx)?;
//! if !result.is_compliant {
//!     for violation in result.violations {
//!         eprintln!("Violation: {}", violation.message);
//!     }
//! }
//! ```

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// =============================================================================
// Configuration
// =============================================================================

/// Production readiness policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionReadinessConfig {
    /// Maximum unwrap calls per 100 lines of code
    /// Default: 5.0 for production, 20.0 for development
    pub max_unwrap_density: f32,

    /// Maximum unsafe blocks outside FFI modules
    /// Default: 0 for production (all unsafe must be in FFI)
    pub max_unsafe_outside_ffi: usize,

    /// Require all handlers to be committed (no TODO/unimplemented! handlers)
    /// Default: true in production
    pub require_committed_handlers: bool,

    /// Require all dev bypasses to be cfg-gated (debug_assertions, test, etc.)
    /// Default: true in production
    pub require_bypass_cfg_gated: bool,

    /// Forbidden code markers that indicate incomplete or unsafe code
    /// Default: ["XXX", "HACK", "FIXME:security", "FIXME:prod"]
    pub forbidden_markers: Vec<String>,

    /// Modules considered FFI where unsafe is allowed
    /// Default: ["*_ffi", "*_sys", "ffi", "bindings"]
    pub ffi_module_patterns: Vec<String>,

    /// Patterns that indicate dev bypass code
    /// Default: ["dev_bypass", "skip_auth", "insecure_", "no_verify"]
    pub bypass_patterns: Vec<String>,

    /// Severity for unwrap density violations
    pub unwrap_severity: Severity,

    /// Severity for unsafe outside FFI violations
    pub unsafe_severity: Severity,

    /// Severity for forbidden marker violations
    pub marker_severity: Severity,

    /// Whether this is running in production mode
    pub is_production: bool,
}

impl Default for ProductionReadinessConfig {
    fn default() -> Self {
        Self::development()
    }
}

impl ProductionReadinessConfig {
    /// Create a strict production configuration
    pub fn production() -> Self {
        Self {
            max_unwrap_density: 5.0,
            max_unsafe_outside_ffi: 0,
            require_committed_handlers: true,
            require_bypass_cfg_gated: true,
            forbidden_markers: vec![
                "XXX".to_string(),
                "HACK".to_string(),
                "FIXME:security".to_string(),
                "FIXME:prod".to_string(),
                "TODO:security".to_string(),
                "UNSAFE:".to_string(),
            ],
            ffi_module_patterns: vec![
                "*_ffi".to_string(),
                "*_sys".to_string(),
                "ffi".to_string(),
                "bindings".to_string(),
                "c_api".to_string(),
            ],
            bypass_patterns: vec![
                "dev_bypass".to_string(),
                "skip_auth".to_string(),
                "insecure_".to_string(),
                "no_verify".to_string(),
                "disable_check".to_string(),
                "unsafe_skip".to_string(),
            ],
            unwrap_severity: Severity::High,
            unsafe_severity: Severity::Critical,
            marker_severity: Severity::High,
            is_production: true,
        }
    }

    /// Create a lenient development configuration
    pub fn development() -> Self {
        Self {
            max_unwrap_density: 20.0,
            max_unsafe_outside_ffi: 10,
            require_committed_handlers: false,
            require_bypass_cfg_gated: false,
            forbidden_markers: vec![
                "XXX".to_string(),
                "FIXME:security".to_string(),
            ],
            ffi_module_patterns: vec![
                "*_ffi".to_string(),
                "*_sys".to_string(),
                "ffi".to_string(),
                "bindings".to_string(),
                "c_api".to_string(),
            ],
            bypass_patterns: vec![
                "dev_bypass".to_string(),
                "skip_auth".to_string(),
                "insecure_".to_string(),
                "no_verify".to_string(),
            ],
            unwrap_severity: Severity::Low,
            unsafe_severity: Severity::Medium,
            marker_severity: Severity::Medium,
            is_production: false,
        }
    }

    /// Add a custom forbidden marker
    pub fn with_forbidden_marker(mut self, marker: impl Into<String>) -> Self {
        self.forbidden_markers.push(marker.into());
        self
    }

    /// Add a custom FFI module pattern
    pub fn with_ffi_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.ffi_module_patterns.push(pattern.into());
        self
    }

    /// Set the unwrap density limit
    pub fn with_max_unwrap_density(mut self, density: f32) -> Self {
        self.max_unwrap_density = density;
        self
    }
}

// =============================================================================
// Analysis Context
// =============================================================================

/// Context for analyzing code production readiness
#[derive(Debug, Clone)]
pub struct CodeAnalysisContext {
    /// Path to the file being analyzed
    pub file_path: String,

    /// Total lines of code (excluding blanks and comments)
    pub total_lines: usize,

    /// Count of unwrap() and expect() calls
    pub unwrap_count: usize,

    /// Unsafe blocks found outside FFI modules
    pub unsafe_blocks: Vec<UnsafeBlock>,

    /// Forbidden markers found in code
    pub forbidden_markers: Vec<MarkerLocation>,

    /// Handlers that are not committed (contain unimplemented!, todo!, etc.)
    pub uncommitted_handlers: Vec<UncommittedHandler>,

    /// Dev bypass code that is not cfg-gated
    pub ungated_bypasses: Vec<UngatedBypass>,

    /// Cached metadata for PolicyContext trait
    cached_metadata: std::sync::OnceLock<std::collections::HashMap<String, String>>,
}

impl CodeAnalysisContext {
    /// Create a new empty analysis context for a file
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            total_lines: 0,
            unwrap_count: 0,
            unsafe_blocks: Vec::new(),
            forbidden_markers: Vec::new(),
            uncommitted_handlers: Vec::new(),
            ungated_bypasses: Vec::new(),
            cached_metadata: std::sync::OnceLock::new(),
        }
    }

    /// Calculate unwrap density (per 100 LOC)
    pub fn unwrap_density(&self) -> f32 {
        if self.total_lines == 0 {
            return 0.0;
        }
        (self.unwrap_count as f32 / self.total_lines as f32) * 100.0
    }
}

impl PolicyContext for CodeAnalysisContext {
    fn context_type(&self) -> &str {
        "code_analysis"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn metadata(&self) -> &std::collections::HashMap<String, String> {
        self.cached_metadata.get_or_init(|| {
            let mut map = std::collections::HashMap::new();
            map.insert("file_path".to_string(), self.file_path.clone());
            map.insert("total_lines".to_string(), self.total_lines.to_string());
            map.insert("unwrap_count".to_string(), self.unwrap_count.to_string());
            map.insert(
                "unwrap_density".to_string(),
                format!("{:.2}", self.unwrap_density()),
            );
            map.insert(
                "unsafe_block_count".to_string(),
                self.unsafe_blocks.len().to_string(),
            );
            map.insert(
                "forbidden_marker_count".to_string(),
                self.forbidden_markers.len().to_string(),
            );
            map
        })
    }
}

/// An unsafe block found in non-FFI code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsafeBlock {
    /// Line number where unsafe block starts
    pub line: usize,
    /// The unsafe code snippet (truncated)
    pub snippet: String,
    /// Reason given in SAFETY comment (if any)
    pub safety_comment: Option<String>,
}

/// A forbidden marker found in code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkerLocation {
    /// Line number where marker was found
    pub line: usize,
    /// The marker that was found
    pub marker: String,
    /// Full line content (truncated)
    pub context: String,
}

/// A handler that is not fully implemented
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncommittedHandler {
    /// Handler/function name
    pub name: String,
    /// Line number
    pub line: usize,
    /// Type of uncommitted code (unimplemented!, todo!, panic!)
    pub uncommitted_type: String,
}

/// Dev bypass code that is not cfg-gated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UngatedBypass {
    /// Line number
    pub line: usize,
    /// The bypass pattern found
    pub pattern: String,
    /// Full line content (truncated)
    pub context: String,
}

// =============================================================================
// Analysis Result
// =============================================================================

/// Result of production readiness analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// Whether the code is production-ready
    pub is_compliant: bool,

    /// Violations found
    pub violations: Vec<ReadinessViolation>,

    /// Warnings (non-blocking issues)
    pub warnings: Vec<String>,

    /// Computed metrics
    pub metrics: ReadinessMetrics,
}

/// A specific production readiness violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessViolation {
    /// Violation category
    pub category: ViolationCategory,

    /// Severity level
    pub severity: Severity,

    /// Human-readable message
    pub message: String,

    /// File path
    pub file_path: String,

    /// Line number (if applicable)
    pub line: Option<usize>,

    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Categories of production readiness violations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationCategory {
    /// Excessive use of unwrap/expect
    UnwrapDensity,
    /// Unsafe code outside FFI modules
    UnsafeOutsideFfi,
    /// Uncommitted handler code
    UncommittedHandler,
    /// Ungated dev bypass code
    UngatedBypass,
    /// Forbidden marker in code
    ForbiddenMarker,
}

impl ViolationCategory {
    /// Get the category name for display
    pub fn name(&self) -> &'static str {
        match self {
            ViolationCategory::UnwrapDensity => "Unwrap Density",
            ViolationCategory::UnsafeOutsideFfi => "Unsafe Outside FFI",
            ViolationCategory::UncommittedHandler => "Uncommitted Handler",
            ViolationCategory::UngatedBypass => "Ungated Bypass",
            ViolationCategory::ForbiddenMarker => "Forbidden Marker",
        }
    }
}

/// Computed metrics from analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessMetrics {
    /// Unwrap density (per 100 LOC)
    pub unwrap_density: f32,

    /// Total unsafe blocks outside FFI
    pub unsafe_outside_ffi_count: usize,

    /// Total forbidden markers found
    pub forbidden_marker_count: usize,

    /// Total uncommitted handlers
    pub uncommitted_handler_count: usize,

    /// Total ungated bypasses
    pub ungated_bypass_count: usize,
}

// =============================================================================
// Policy Implementation
// =============================================================================

/// Production readiness policy enforcer
pub struct ProductionReadinessPolicy {
    config: ProductionReadinessConfig,
}

impl ProductionReadinessPolicy {
    /// Create a new production readiness policy
    pub fn new(config: ProductionReadinessConfig) -> Self {
        Self { config }
    }

    /// Get the current configuration
    pub fn config(&self) -> &ProductionReadinessConfig {
        &self.config
    }

    /// Check if a module path matches FFI patterns
    pub fn is_ffi_module(&self, path: &str) -> bool {
        let path_lower = path.to_lowercase();
        for pattern in &self.config.ffi_module_patterns {
            if let Some(suffix) = pattern.strip_prefix('*') {
                if path_lower.ends_with(suffix) || path_lower.contains(&format!("{}/", suffix)) {
                    return true;
                }
            } else if path_lower.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Analyze code for production readiness
    pub fn analyze(&self, ctx: &CodeAnalysisContext) -> Result<AnalysisResult> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Check unwrap density
        let density = ctx.unwrap_density();
        if density > self.config.max_unwrap_density {
            violations.push(ReadinessViolation {
                category: ViolationCategory::UnwrapDensity,
                severity: self.config.unwrap_severity,
                message: format!(
                    "Unwrap density {:.2} exceeds limit of {:.2} per 100 LOC",
                    density, self.config.max_unwrap_density
                ),
                file_path: ctx.file_path.clone(),
                line: None,
                suggestion: Some(
                    "Replace unwrap() calls with proper error handling using ? or match".to_string(),
                ),
            });
        }

        // Check unsafe blocks outside FFI
        if !self.is_ffi_module(&ctx.file_path) {
            let unsafe_count = ctx.unsafe_blocks.len();
            if unsafe_count > self.config.max_unsafe_outside_ffi {
                for block in &ctx.unsafe_blocks {
                    violations.push(ReadinessViolation {
                        category: ViolationCategory::UnsafeOutsideFfi,
                        severity: self.config.unsafe_severity,
                        message: format!(
                            "Unsafe block outside FFI module: {}",
                            truncate_string(&block.snippet, 60)
                        ),
                        file_path: ctx.file_path.clone(),
                        line: Some(block.line),
                        suggestion: if block.safety_comment.is_none() {
                            Some("Add a SAFETY comment explaining why unsafe is required".to_string())
                        } else {
                            Some("Consider moving unsafe code to a dedicated FFI module".to_string())
                        },
                    });
                }
            }
        }

        // Check forbidden markers
        for marker_loc in &ctx.forbidden_markers {
            violations.push(ReadinessViolation {
                category: ViolationCategory::ForbiddenMarker,
                severity: self.config.marker_severity,
                message: format!(
                    "Forbidden marker '{}' found: {}",
                    marker_loc.marker,
                    truncate_string(&marker_loc.context, 60)
                ),
                file_path: ctx.file_path.clone(),
                line: Some(marker_loc.line),
                suggestion: Some(format!(
                    "Resolve the '{}' issue before deploying to production",
                    marker_loc.marker
                )),
            });
        }

        // Check uncommitted handlers
        if self.config.require_committed_handlers {
            for handler in &ctx.uncommitted_handlers {
                violations.push(ReadinessViolation {
                    category: ViolationCategory::UncommittedHandler,
                    severity: Severity::High,
                    message: format!(
                        "Uncommitted handler '{}' contains {}",
                        handler.name, handler.uncommitted_type
                    ),
                    file_path: ctx.file_path.clone(),
                    line: Some(handler.line),
                    suggestion: Some("Implement the handler or remove it".to_string()),
                });
            }
        } else if !ctx.uncommitted_handlers.is_empty() {
            warnings.push(format!(
                "{} uncommitted handlers found (not blocking in dev mode)",
                ctx.uncommitted_handlers.len()
            ));
        }

        // Check ungated bypasses
        if self.config.require_bypass_cfg_gated {
            for bypass in &ctx.ungated_bypasses {
                violations.push(ReadinessViolation {
                    category: ViolationCategory::UngatedBypass,
                    severity: Severity::Critical,
                    message: format!(
                        "Ungated dev bypass '{}': {}",
                        bypass.pattern,
                        truncate_string(&bypass.context, 60)
                    ),
                    file_path: ctx.file_path.clone(),
                    line: Some(bypass.line),
                    suggestion: Some(
                        "Gate bypass code with #[cfg(debug_assertions)] or #[cfg(test)]".to_string(),
                    ),
                });
            }
        } else if !ctx.ungated_bypasses.is_empty() {
            warnings.push(format!(
                "{} ungated bypasses found (not blocking in dev mode)",
                ctx.ungated_bypasses.len()
            ));
        }

        let metrics = ReadinessMetrics {
            unwrap_density: density,
            unsafe_outside_ffi_count: if self.is_ffi_module(&ctx.file_path) {
                0
            } else {
                ctx.unsafe_blocks.len()
            },
            forbidden_marker_count: ctx.forbidden_markers.len(),
            uncommitted_handler_count: ctx.uncommitted_handlers.len(),
            ungated_bypass_count: ctx.ungated_bypasses.len(),
        };

        Ok(AnalysisResult {
            is_compliant: violations.is_empty(),
            violations,
            warnings,
            metrics,
        })
    }

    /// Validate that a set of forbidden markers are not present in content
    pub fn check_forbidden_markers(&self, content: &str) -> Vec<String> {
        let mut found = Vec::new();
        for marker in &self.config.forbidden_markers {
            if content.contains(marker) {
                found.push(marker.clone());
            }
        }
        found
    }

    /// Check if content contains any bypass patterns
    pub fn contains_bypass_pattern(&self, content: &str) -> Option<&str> {
        let content_lower = content.to_lowercase();
        self.config
            .bypass_patterns
            .iter()
            .find(|pattern| content_lower.contains(&pattern.to_lowercase()))
            .map(|s| s.as_str())
    }

    /// Get all configured forbidden markers
    pub fn forbidden_markers(&self) -> &[String] {
        &self.config.forbidden_markers
    }

    /// Get all configured bypass patterns
    pub fn bypass_patterns(&self) -> &[String] {
        &self.config.bypass_patterns
    }
}

impl Policy for ProductionReadinessPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::ProductionReadiness
    }

    fn name(&self) -> &'static str {
        "Production Readiness"
    }

    fn severity(&self) -> Severity {
        if self.config.is_production {
            Severity::Critical
        } else {
            Severity::Medium
        }
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        // Try to downcast to CodeAnalysisContext
        if let Some(code_ctx) = ctx.as_any().downcast_ref::<CodeAnalysisContext>() {
            let result = self.analyze(code_ctx)?;

            if result.is_compliant {
                return Ok(Audit::passed(self.id()).with_warnings(result.warnings));
            }

            let violations: Vec<Violation> = result
                .violations
                .iter()
                .map(|v| Violation {
                    severity: v.severity,
                    message: v.message.clone(),
                    details: v.suggestion.clone(),
                })
                .collect();

            return Ok(Audit::failed(self.id(), violations).with_warnings(result.warnings));
        }

        // Fallback to metadata-based enforcement
        let metadata = ctx.metadata();
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check unwrap density from metadata
        if let Some(density_str) = metadata.get("unwrap_density") {
            if let Ok(density) = density_str.parse::<f32>() {
                if density > self.config.max_unwrap_density {
                    violations.push(Violation {
                        severity: self.config.unwrap_severity,
                        message: format!(
                            "Unwrap density {:.2} exceeds limit of {:.2}",
                            density, self.config.max_unwrap_density
                        ),
                        details: Some(
                            "Replace unwrap() with proper error handling".to_string(),
                        ),
                    });
                }
            }
        }

        // Check unsafe count from metadata
        if let Some(unsafe_str) = metadata.get("unsafe_block_count") {
            if let Ok(count) = unsafe_str.parse::<usize>() {
                if count > self.config.max_unsafe_outside_ffi {
                    violations.push(Violation {
                        severity: self.config.unsafe_severity,
                        message: format!(
                            "Found {} unsafe blocks outside FFI (limit: {})",
                            count, self.config.max_unsafe_outside_ffi
                        ),
                        details: Some(
                            "Move unsafe code to dedicated FFI modules".to_string(),
                        ),
                    });
                }
            }
        }

        // Check forbidden markers from metadata
        if let Some(marker_str) = metadata.get("forbidden_marker_count") {
            if let Ok(count) = marker_str.parse::<usize>() {
                if count > 0 {
                    violations.push(Violation {
                        severity: self.config.marker_severity,
                        message: format!("Found {} forbidden markers in code", count),
                        details: Some(
                            "Remove XXX, HACK, FIXME:security markers before production".to_string(),
                        ),
                    });
                }
            }
        }

        if violations.is_empty() {
            Ok(Audit::passed(self.id()).with_warnings(warnings))
        } else {
            Ok(Audit::failed(self.id(), violations).with_warnings(warnings))
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Truncate a string with ellipsis
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Scan source code for production readiness issues
///
/// This is a simple scanner that can be used to build CodeAnalysisContext
/// from raw source code. For more sophisticated analysis, use tree-sitter
/// or syn-based parsing.
pub fn scan_source_code(file_path: &str, content: &str, config: &ProductionReadinessConfig) -> CodeAnalysisContext {
    let mut ctx = CodeAnalysisContext::new(file_path);

    let lines: Vec<&str> = content.lines().collect();
    let mut code_lines = 0;
    let mut in_multiline_comment = false;

    // Build a set of forbidden markers for efficient lookup
    let forbidden_set: HashSet<&str> = config.forbidden_markers.iter().map(|s| s.as_str()).collect();

    for (line_num, line) in lines.iter().enumerate() {
        let line_number = line_num + 1;
        let trimmed = line.trim();

        // Track multiline comments
        if trimmed.starts_with("/*") {
            in_multiline_comment = true;
        }
        if trimmed.ends_with("*/") {
            in_multiline_comment = false;
            continue;
        }
        if in_multiline_comment {
            continue;
        }

        // Skip blank lines and single-line comments
        if trimmed.is_empty() || trimmed.starts_with("//") {
            // But still check comments for forbidden markers
            for marker in &forbidden_set {
                if trimmed.contains(marker) {
                    ctx.forbidden_markers.push(MarkerLocation {
                        line: line_number,
                        marker: marker.to_string(),
                        context: trimmed.to_string(),
                    });
                }
            }
            continue;
        }

        code_lines += 1;

        // Count unwrap and expect calls
        let unwrap_count = line.matches(".unwrap()").count()
            + line.matches(".unwrap_or_else").count()
            + line.matches(".expect(").count();
        ctx.unwrap_count += unwrap_count;

        // Check for unsafe blocks
        if trimmed.contains("unsafe {") || trimmed.starts_with("unsafe ") {
            // Look for SAFETY comment above
            let safety_comment = if line_num > 0 {
                let prev_line = lines[line_num - 1].trim();
                if prev_line.contains("SAFETY:") || prev_line.contains("// SAFETY") {
                    Some(prev_line.to_string())
                } else {
                    None
                }
            } else {
                None
            };

            ctx.unsafe_blocks.push(UnsafeBlock {
                line: line_number,
                snippet: trimmed.to_string(),
                safety_comment,
            });
        }

        // Check for forbidden markers in code
        for marker in &forbidden_set {
            if line.contains(marker) {
                ctx.forbidden_markers.push(MarkerLocation {
                    line: line_number,
                    marker: marker.to_string(),
                    context: trimmed.to_string(),
                });
            }
        }

        // Check for uncommitted handlers
        if trimmed.contains("unimplemented!()") || trimmed.contains("todo!()") {
            // Try to find function name
            let name = find_enclosing_function(&lines, line_num).unwrap_or_else(|| "<unknown>".to_string());
            ctx.uncommitted_handlers.push(UncommittedHandler {
                name,
                line: line_number,
                uncommitted_type: if trimmed.contains("unimplemented!()") {
                    "unimplemented!()".to_string()
                } else {
                    "todo!()".to_string()
                },
            });
        }

        // Check for ungated bypasses
        for pattern in &config.bypass_patterns {
            if line.to_lowercase().contains(&pattern.to_lowercase()) {
                // Check if the line or surrounding lines have cfg attribute
                let has_cfg_gate = line.contains("#[cfg(") ||
                    (line_num > 0 && lines[line_num - 1].contains("#[cfg("));

                if !has_cfg_gate {
                    ctx.ungated_bypasses.push(UngatedBypass {
                        line: line_number,
                        pattern: pattern.clone(),
                        context: trimmed.to_string(),
                    });
                }
            }
        }
    }

    ctx.total_lines = code_lines;
    ctx
}

/// Try to find the enclosing function name for a given line
fn find_enclosing_function(lines: &[&str], target_line: usize) -> Option<String> {
    for i in (0..target_line).rev() {
        let line = lines[i].trim();
        if line.starts_with("fn ") || line.starts_with("pub fn ") || line.starts_with("async fn ") || line.starts_with("pub async fn ") {
            // Extract function name
            if let Some(start) = line.find("fn ") {
                let after_fn = &line[start + 3..];
                if let Some(end) = after_fn.find('(') {
                    return Some(after_fn[..end].trim().to_string());
                }
            }
        }
    }
    None
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_readiness_policy_creation() {
        let config = ProductionReadinessConfig::production();
        let policy = ProductionReadinessPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::ProductionReadiness);
        assert_eq!(policy.name(), "Production Readiness");
        assert_eq!(policy.severity(), Severity::Critical);
    }

    #[test]
    fn test_development_config_is_lenient() {
        let config = ProductionReadinessConfig::development();
        assert_eq!(config.max_unwrap_density, 20.0);
        assert_eq!(config.max_unsafe_outside_ffi, 10);
        assert!(!config.require_committed_handlers);
        assert!(!config.require_bypass_cfg_gated);
        assert!(!config.is_production);
    }

    #[test]
    fn test_production_config_is_strict() {
        let config = ProductionReadinessConfig::production();
        assert_eq!(config.max_unwrap_density, 5.0);
        assert_eq!(config.max_unsafe_outside_ffi, 0);
        assert!(config.require_committed_handlers);
        assert!(config.require_bypass_cfg_gated);
        assert!(config.is_production);
    }

    #[test]
    fn test_ffi_module_detection() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());

        assert!(policy.is_ffi_module("crates/foo_ffi/src/lib.rs"));
        assert!(policy.is_ffi_module("src/ffi/wrapper.rs"));
        assert!(policy.is_ffi_module("bindings/python.rs"));
        assert!(policy.is_ffi_module("src/c_api/mod.rs"));

        assert!(!policy.is_ffi_module("src/handlers.rs"));
        assert!(!policy.is_ffi_module("src/main.rs"));
    }

    #[test]
    fn test_unwrap_density_calculation() {
        let mut ctx = CodeAnalysisContext::new("test.rs");
        ctx.total_lines = 100;
        ctx.unwrap_count = 5;
        assert!((ctx.unwrap_density() - 5.0).abs() < 0.001);

        ctx.total_lines = 200;
        assert!((ctx.unwrap_density() - 2.5).abs() < 0.001);

        ctx.total_lines = 0;
        assert!((ctx.unwrap_density()).abs() < 0.001);
    }

    #[test]
    fn test_analyze_compliant_code() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "src/handlers.rs".to_string(),
            total_lines: 100,
            unwrap_count: 3, // 3% density, under 5% limit
            unsafe_blocks: vec![],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![],
            ungated_bypasses: vec![],
        };

        let result = policy.analyze(&ctx).unwrap();
        assert!(result.is_compliant);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn test_analyze_high_unwrap_density() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "src/handlers.rs".to_string(),
            total_lines: 100,
            unwrap_count: 10, // 10% density, over 5% limit
            unsafe_blocks: vec![],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![],
            ungated_bypasses: vec![],
        };

        let result = policy.analyze(&ctx).unwrap();
        assert!(!result.is_compliant);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].category, ViolationCategory::UnwrapDensity);
    }

    #[test]
    fn test_analyze_unsafe_outside_ffi() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "src/handlers.rs".to_string(), // Not FFI
            total_lines: 100,
            unwrap_count: 0,
            unsafe_blocks: vec![UnsafeBlock {
                line: 42,
                snippet: "unsafe { ptr::read(p) }".to_string(),
                safety_comment: None,
            }],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![],
            ungated_bypasses: vec![],
        };

        let result = policy.analyze(&ctx).unwrap();
        assert!(!result.is_compliant);
        assert_eq!(result.violations[0].category, ViolationCategory::UnsafeOutsideFfi);
    }

    #[test]
    fn test_analyze_unsafe_in_ffi_allowed() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "crates/my_ffi/src/lib.rs".to_string(), // FFI module
            total_lines: 100,
            unwrap_count: 0,
            unsafe_blocks: vec![UnsafeBlock {
                line: 42,
                snippet: "unsafe { ptr::read(p) }".to_string(),
                safety_comment: Some("// SAFETY: pointer is valid".to_string()),
            }],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![],
            ungated_bypasses: vec![],
        };

        let result = policy.analyze(&ctx).unwrap();
        assert!(result.is_compliant);
    }

    #[test]
    fn test_analyze_forbidden_markers() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "src/handlers.rs".to_string(),
            total_lines: 100,
            unwrap_count: 0,
            unsafe_blocks: vec![],
            forbidden_markers: vec![
                MarkerLocation {
                    line: 10,
                    marker: "XXX".to_string(),
                    context: "// XXX: fix this later".to_string(),
                },
                MarkerLocation {
                    line: 25,
                    marker: "FIXME:security".to_string(),
                    context: "// FIXME:security validate input".to_string(),
                },
            ],
            uncommitted_handlers: vec![],
            ungated_bypasses: vec![],
        };

        let result = policy.analyze(&ctx).unwrap();
        assert!(!result.is_compliant);
        assert_eq!(result.violations.len(), 2);
        assert!(result.violations.iter().all(|v| v.category == ViolationCategory::ForbiddenMarker));
    }

    #[test]
    fn test_analyze_uncommitted_handlers() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "src/handlers.rs".to_string(),
            total_lines: 100,
            unwrap_count: 0,
            unsafe_blocks: vec![],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![UncommittedHandler {
                name: "handle_request".to_string(),
                line: 50,
                uncommitted_type: "todo!()".to_string(),
            }],
            ungated_bypasses: vec![],
        };

        let result = policy.analyze(&ctx).unwrap();
        assert!(!result.is_compliant);
        assert_eq!(result.violations[0].category, ViolationCategory::UncommittedHandler);
    }

    #[test]
    fn test_analyze_ungated_bypasses() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "src/auth.rs".to_string(),
            total_lines: 100,
            unwrap_count: 0,
            unsafe_blocks: vec![],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![],
            ungated_bypasses: vec![UngatedBypass {
                line: 30,
                pattern: "skip_auth".to_string(),
                context: "if skip_auth { return Ok(()); }".to_string(),
            }],
        };

        let result = policy.analyze(&ctx).unwrap();
        assert!(!result.is_compliant);
        assert_eq!(result.violations[0].category, ViolationCategory::UngatedBypass);
    }

    #[test]
    fn test_dev_mode_warnings_instead_of_violations() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::development());
        let ctx = CodeAnalysisContext {
            file_path: "src/handlers.rs".to_string(),
            total_lines: 100,
            unwrap_count: 0,
            unsafe_blocks: vec![],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![UncommittedHandler {
                name: "handle_request".to_string(),
                line: 50,
                uncommitted_type: "todo!()".to_string(),
            }],
            ungated_bypasses: vec![UngatedBypass {
                line: 30,
                pattern: "skip_auth".to_string(),
                context: "if skip_auth { return Ok(()); }".to_string(),
            }],
        };

        let result = policy.analyze(&ctx).unwrap();
        // In dev mode, uncommitted handlers and ungated bypasses are warnings, not violations
        assert!(result.is_compliant);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_check_forbidden_markers() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());

        let found = policy.check_forbidden_markers("// XXX: this is broken");
        assert_eq!(found, vec!["XXX"]);

        let found = policy.check_forbidden_markers("// HACK around the issue");
        assert_eq!(found, vec!["HACK"]);

        let found = policy.check_forbidden_markers("// Normal comment");
        assert!(found.is_empty());
    }

    #[test]
    fn test_contains_bypass_pattern() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());

        assert_eq!(
            policy.contains_bypass_pattern("if dev_bypass { skip }"),
            Some("dev_bypass")
        );
        assert_eq!(
            policy.contains_bypass_pattern("SKIP_AUTH=true"),
            Some("skip_auth")
        );
        assert!(policy.contains_bypass_pattern("normal code").is_none());
    }

    #[test]
    fn test_scan_source_code() {
        let config = ProductionReadinessConfig::production();
        let source = r#"
fn main() {
    let x = some_result().unwrap();
    let y = another().expect("failed");
    // XXX: fix this
}

fn handler() {
    todo!()
}

unsafe {
    ptr::read(p)
}
"#;

        let ctx = scan_source_code("test.rs", source, &config);

        assert!(ctx.unwrap_count >= 2); // unwrap and expect
        assert_eq!(ctx.forbidden_markers.len(), 1); // XXX
        assert_eq!(ctx.uncommitted_handlers.len(), 1); // todo!()
        assert_eq!(ctx.unsafe_blocks.len(), 1); // unsafe block
    }

    #[test]
    fn test_config_builder_methods() {
        let config = ProductionReadinessConfig::production()
            .with_max_unwrap_density(10.0)
            .with_forbidden_marker("DANGER")
            .with_ffi_pattern("native_*");

        assert_eq!(config.max_unwrap_density, 10.0);
        assert!(config.forbidden_markers.contains(&"DANGER".to_string()));
        assert!(config.ffi_module_patterns.contains(&"native_*".to_string()));
    }

    #[test]
    fn test_violation_category_names() {
        assert_eq!(ViolationCategory::UnwrapDensity.name(), "Unwrap Density");
        assert_eq!(ViolationCategory::UnsafeOutsideFfi.name(), "Unsafe Outside FFI");
        assert_eq!(ViolationCategory::UncommittedHandler.name(), "Uncommitted Handler");
        assert_eq!(ViolationCategory::UngatedBypass.name(), "Ungated Bypass");
        assert_eq!(ViolationCategory::ForbiddenMarker.name(), "Forbidden Marker");
    }

    #[test]
    fn test_policy_enforce_with_context() {
        let policy = ProductionReadinessPolicy::new(ProductionReadinessConfig::production());
        let ctx = CodeAnalysisContext {
            file_path: "src/handlers.rs".to_string(),
            total_lines: 100,
            unwrap_count: 2,
            unsafe_blocks: vec![],
            forbidden_markers: vec![],
            uncommitted_handlers: vec![],
            ungated_bypasses: vec![],
        };

        let audit = policy.enforce(&ctx).unwrap();
        assert!(audit.passed);
        assert!(audit.violations.is_empty());
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("this is a very long string", 10), "this is...");
    }
}
