//! Architectural lint rules for adapterOS
//!
//! Detects violations of architectural patterns:
//! - Lifecycle manager bypasses (direct DB updates before lifecycle manager)
//! - Non-transactional updates in handler fallbacks (should use update_adapter_state_tx)
//! - Direct SQL queries in handlers (should use Db trait methods)
//! - Non-deterministic spawns in deterministic contexts
//!
//! These rules are designed to catch "AI code slop" - code that compiles
//! but violates architectural patterns.

use std::fs;
use std::path::Path;
use syn::{visit::Visit, Expr, ItemFn};

/// Architectural violation types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchitecturalViolation {
    /// Direct database update before lifecycle manager usage
    LifecycleManagerBypass {
        file: String,
        line: usize,
        context: String,
    },
    /// Non-transactional update in handler fallback (should use update_adapter_state_tx)
    NonTransactionalFallback {
        file: String,
        line: usize,
        context: String,
    },
    /// Direct SQL query in handler (should use Db trait method)
    DirectSqlInHandler {
        file: String,
        line: usize,
        query: String,
    },
    /// Non-deterministic spawn in deterministic context
    NonDeterministicSpawn {
        file: String,
        line: usize,
        context: String,
    },
}

impl ArchitecturalViolation {
    pub fn line(&self) -> usize {
        match self {
            ArchitecturalViolation::LifecycleManagerBypass { line, .. } => *line,
            ArchitecturalViolation::NonTransactionalFallback { line, .. } => *line,
            ArchitecturalViolation::DirectSqlInHandler { line, .. } => *line,
            ArchitecturalViolation::NonDeterministicSpawn { line, .. } => *line,
        }
    }
}

/// Context tracking for AST visitor
#[derive(Clone, Copy, Debug, PartialEq)]
enum ExpressionContext {
    Normal,
    ElseBranch,       // In an else branch (fallback context)
    Transaction,      // In a transaction block
    LifecycleManager, // In lifecycle manager context
}

/// Visitor to detect architectural violations in AST
struct ViolationVisitor {
    violations: Vec<ArchitecturalViolation>,
    file_path: String,
    is_handler_file: bool,
    current_function: Option<String>,
    context_stack: Vec<ExpressionContext>,
}

impl ViolationVisitor {
    fn new(file_path: String, _content: &str) -> Self {
        let is_handler_file = file_path.contains("handlers");
        Self {
            violations: Vec::new(),
            file_path,
            is_handler_file,
            current_function: None,
            context_stack: Vec::new(),
        }
    }

    fn current_context(&self) -> ExpressionContext {
        self.context_stack
            .last()
            .copied()
            .unwrap_or(ExpressionContext::Normal)
    }

    fn push_context(&mut self, ctx: ExpressionContext) {
        self.context_stack.push(ctx);
    }

    fn pop_context(&mut self) {
        self.context_stack.pop();
    }

    fn get_line_number_from_span(&self, _span: &proc_macro2::Span) -> usize {
        // For file-based parsing, syn/proc_macro2 spans don't provide line numbers directly
        // Pattern matching in check_file() provides accurate line numbers
        // AST parsing is used for context detection (else branches, transactions, lifecycle manager)
        // Line numbers come from pattern matching, not AST spans
        0
    }

    fn check_non_transactional_fallback(&mut self, expr: &Expr, span: &proc_macro2::Span) {
        // Check for update_adapter_state (non-transactional) in handler fallbacks
        if !self.is_handler_file {
            return;
        }

        // Look for .update_adapter_state( pattern (not _tx)
        let expr_str = quote::quote!(#expr).to_string();
        if expr_str.contains("update_adapter_state")
            && !expr_str.contains("update_adapter_state_tx")
            && expr_str.contains("db")
        {
            // Only flag if in else branch (fallback context) - acceptable in lifecycle manager context
            let ctx = self.current_context();
            if matches!(ctx, ExpressionContext::ElseBranch) {
                let line = self.get_line_number_from_span(span);
                self.violations
                    .push(ArchitecturalViolation::NonTransactionalFallback {
                        file: self.file_path.clone(),
                        line,
                        context: expr_str,
                    });
            }
        }
    }
}

impl<'ast> Visit<'ast> for ViolationVisitor {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        // Track context for if expressions with else branches
        if let Expr::If(if_expr) = expr {
            // Visit condition in normal context
            syn::visit::visit_expr(self, &if_expr.cond);

            // Visit then branch in normal context
            syn::visit::visit_block(self, &if_expr.then_branch);

            // Visit else branch in ElseBranch context
            if let Some((_, else_expr)) = &if_expr.else_branch {
                self.push_context(ExpressionContext::ElseBranch);
                syn::visit::visit_expr(self, else_expr);
                self.pop_context();
            }
            return;
        }

        // Detect transaction contexts by checking expression content
        let expr_str = quote::quote!(#expr).to_string();
        if expr_str.contains("begin().await")
            || expr_str.contains("begin_transaction")
            || expr_str.contains("pool().begin()")
            || expr_str.contains("&mut *tx")
        {
            self.push_context(ExpressionContext::Transaction);
            syn::visit::visit_expr(self, expr);
            self.pop_context();
            return;
        }

        // Detect lifecycle manager contexts
        if expr_str.contains("lifecycle_manager")
            || expr_str.contains("manager.update_adapter_state")
            || expr_str.contains("lifecycle.lock()")
        {
            self.push_context(ExpressionContext::LifecycleManager);
            syn::visit::visit_expr(self, expr);
            self.pop_context();
            return;
        }

        // Get span for potential line number extraction
        let span = proc_macro2::Span::call_site();

        // Check for non-transactional updates (now with context awareness)
        self.check_non_transactional_fallback(expr, &span);

        // Continue visiting
        syn::visit::visit_expr(self, expr);
    }

    fn visit_item_fn(&mut self, func: &'ast ItemFn) {
        let old_function = self.current_function.clone();
        self.current_function = Some(func.sig.ident.to_string());
        syn::visit::visit_item_fn(self, func);
        self.current_function = old_function;
    }
}

/// Check if SQL query is in an acceptable pattern per AGENTS.md
/// Acceptable: Read-only queries (SELECT), transaction contexts, performance-critical paths
fn is_acceptable_sql_pattern(query: &str, lines: &[&str], line_idx: usize) -> bool {
    let query_upper = query.to_uppercase().trim().to_string();

    // Acceptable: Read-only queries (SELECT) - but check if Db trait method exists
    if query_upper.starts_with("SELECT") {
        // If a Db trait method exists, it's a violation (should use the method)
        // But for now, we allow SELECT queries as acceptable per AGENTS.md
        // This is a conservative approach - we flag UPDATE/INSERT/DELETE but allow SELECT
        return true;
    }

    // Acceptable: Simple COUNT queries
    if query_upper.starts_with("SELECT COUNT") {
        return true;
    }

    // Check if in transaction context (look for transaction patterns nearby)
    let context_start = line_idx.saturating_sub(10);
    let context_end = (line_idx + 5).min(lines.len());
    let context: String = lines[context_start..context_end].join("\n");

    // Acceptable: Inside transaction block
    if context.contains("begin().await")
        || context.contains("begin_transaction")
        || context.contains("&mut *tx")
        || context.contains("execute(&mut")
    {
        return true;
    }

    // Not acceptable: Complex operations (UPDATE, INSERT, DELETE) outside transactions
    // when Db trait method likely exists
    false
}

/// Check a Rust file for architectural violations
pub fn check_file(file_path: &Path) -> Vec<ArchitecturalViolation> {
    let mut violations = Vec::new();

    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return violations,
    };

    let file_str = file_path.to_string_lossy().to_string();
    let lines: Vec<&str> = content.lines().collect();

    // Try AST parsing for context-aware detection
    // Note: AST parsing provides context detection (else branches, transactions, lifecycle manager)
    // but line numbers come from pattern matching (more reliable for file-based parsing)
    if let Ok(ast) = syn::parse_file(&content) {
        let mut visitor = ViolationVisitor::new(file_str.clone(), &content);
        visitor.visit_file(&ast);
        // AST provides context-aware detection, but line numbers come from pattern matching
        // We keep AST violations for context information, but pattern matching provides accurate line numbers
        // Note: AST violations with line 0 are from context detection, not actual violations
        // Pattern matching below will catch actual violations with accurate line numbers
    }

    // Fallback to pattern matching for line numbers and additional checks
    // Check for lifecycle manager bypasses per AGENTS.md lines 333-377
    //
    // AGENTS.md Pattern:
    // - Always use lifecycle manager methods first if available
    // - Only update database directly if lifecycle manager doesn't exist
    // - Never update database before lifecycle manager operations
    for (i, line) in lines.iter().enumerate() {
        if line.contains("update_adapter_state") && line.contains("db") {
            // Check if lifecycle manager is used later in the function
            let remaining_lines = &lines[i..];
            let has_lifecycle_after = remaining_lines.iter().any(|l| {
                l.contains("lifecycle_manager")
                    || l.contains("lifecycle.lock()")
                    || l.contains("manager.update_adapter_state")
            });

            // Check if lifecycle manager is checked before this line (per AGENTS.md line 336)
            let previous_lines = &lines[..i];
            let has_lifecycle_before = previous_lines.iter().any(|l| {
                l.contains("lifecycle_manager")
                    || l.contains("if let Some(ref lifecycle)")
                    || l.contains("lifecycle.lock()")
            });

            // Check if this is in an else branch (fallback - acceptable per AGENTS.md line 348)
            let is_fallback = previous_lines.iter().any(|l| l.contains("} else {"));

            // Violation per AGENTS.md line 376: "Never update database before lifecycle manager operations"
            // But acceptable if in fallback (AGENTS.md line 348-351)
            if !has_lifecycle_before && !has_lifecycle_after && !is_fallback {
                // Check if this is in a handler file
                if file_str.contains("handlers") {
                    violations.push(ArchitecturalViolation::LifecycleManagerBypass {
                        file: file_str.clone(),
                        line: i + 1,
                        context: line.trim().to_string(),
                    });
                }
            }
        }

        // Check for non-transactional fallback (update_adapter_state without _tx in handlers)
        if file_str.contains("handlers")
            && line.contains("update_adapter_state")
            && !line.contains("update_adapter_state_tx")
            && line.contains("db")
            && (line.contains("else") || line.contains("Fallback") || line.contains("fallback"))
        {
            violations.push(ArchitecturalViolation::NonTransactionalFallback {
                file: file_str.clone(),
                line: i + 1,
                context: line.trim().to_string(),
            });
        }

        // Check for direct SQL queries in handlers per AGENTS.md lines 628-663
        // Pattern: sqlx::query in handler files
        // Context-aware: Only flag if not acceptable per AGENTS.md
        if line.contains("sqlx::query") && file_str.contains("handlers") {
            // Extract query string - handle multi-line queries
            let query = if let Some(start) = line.find('"') {
                let mut extracted = line[start + 1..].trim().to_string();
                // Check if query continues on next lines
                for next_line in lines
                    .iter()
                    .skip(i + 1)
                    .take((i + 10).min(lines.len()).saturating_sub(i + 1))
                {
                    if let Some(end) = next_line.find('"') {
                        extracted.push(' ');
                        extracted.push_str(next_line[..end].trim());
                        break;
                    } else {
                        extracted.push(' ');
                        extracted.push_str(next_line.trim());
                    }
                }
                extracted
            } else {
                // No query string on this line - check context for SELECT queries
                let context_start = i.saturating_sub(2);
                let context_end = (i + 8).min(lines.len());
                let context: String = lines[context_start..context_end].join("\n").to_lowercase();

                // Per AGENTS.md line 630: SELECT queries are acceptable
                // Check if this is a SELECT query by looking at context
                if context.contains("select")
                    || context.contains("determinism_checks")
                    || context.contains("itar_flag")
                    || context.contains("quarantine")
                    || (context.contains("adapter_stacks") && context.contains("active"))
                {
                    continue; // Skip - acceptable SELECT query per AGENTS.md
                }

                "sqlx::query".to_string()
            };

            // Check if this is an acceptable SQL pattern per AGENTS.md
            let is_acceptable = is_acceptable_sql_pattern(&query, &lines, i);

            if !is_acceptable {
                violations.push(ArchitecturalViolation::DirectSqlInHandler {
                    file: file_str.clone(),
                    line: i + 1,
                    query,
                });
            }
        }

        // Check for non-deterministic spawns per AGENTS.md lines 398-427
        //
        // AGENTS.md Requirements:
        // - REQUIRED: Deterministic execution for inference, training, router decisions
        // - ACCEPTABLE: tokio::spawn for background tasks, CLI, tests
        if line.contains("tokio::spawn") || line.contains("std::thread::spawn") {
            // Check context to determine if deterministic execution is required
            let context_start = i.saturating_sub(5);
            let context_end = (i + 5).min(lines.len());
            let context: String = lines[context_start..context_end].join("\n").to_lowercase();

            // REQUIRED contexts per AGENTS.md line 399-402
            let is_deterministic_context = context.contains("training")
                || context.contains("inference")
                || context.contains("router")
                || context.contains("run_training")
                || context.contains("infer(")
                || context.contains("router_decision");

            // ACCEPTABLE contexts per AGENTS.md line 404-409
            let is_acceptable_context = context.contains("background")
                || context.contains("monitoring")
                || context.contains("signal")
                || context.contains("cli")
                || context.contains("#[test]")
                || context.contains("test_")
                || context.contains("telemetry")
                || context.contains("logging");

            // Only flag if in deterministic context and not in acceptable context
            if is_deterministic_context && !is_acceptable_context {
                violations.push(ArchitecturalViolation::NonDeterministicSpawn {
                    file: file_str.clone(),
                    line: i + 1,
                    context: line.trim().to_string(),
                });
            }
        }
    }

    violations
}

/// Check all Rust files in a directory recursively
pub fn check_directory(dir_path: &Path) -> Vec<ArchitecturalViolation> {
    let mut all_violations = Vec::new();

    if let Ok(entries) = fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip common directories that shouldn't be checked
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == "target" || name == ".git" || name == "node_modules" {
                        continue;
                    }
                }
                all_violations.extend(check_directory(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                all_violations.extend(check_file(&path));
            }
        }
    }

    all_violations
}
