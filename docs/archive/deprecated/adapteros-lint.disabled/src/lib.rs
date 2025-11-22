//! AdapterOS Determinism Lint Rules
//!
//! This crate provides custom lint rules to prevent developers from introducing
//! nondeterminism into the AdapterOS codebase. It detects common patterns that
//! violate determinism guarantees:
//!
//! - `tokio::task::spawn_blocking` calls
//! - Wall-clock time usage (`SystemTime::now()`, `Instant::now()`)
//! - Random number generation without proper seeding
//! - File I/O operations
//! - System calls
//!
//! # Usage
//!
//! Add to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! adapteros-lint = { path = "crates/adapteros-lint" }
//! ```
//!
//! Then run:
//! ```bash
//! cargo clippy -- -W adapteros-lint::nondeterminism
//! ```

pub mod runtime_guards;
pub mod strict_mode;

use rustc_lint::{EarlyLintPass, LintContext};
use rustc_ast::ast::*;
use rustc_errors::Applicability;
use rustc_span::{sym, Span};

declare_lint! {
    /// ### What it does
    /// Detects `tokio::task::spawn_blocking` calls that can introduce nondeterminism
    /// ### Why is this bad?
    /// `spawn_blocking` uses a thread pool with non-deterministic scheduling
    /// ### Example
    /// ```rust
    /// tokio::task::spawn_blocking(|| {
    ///     // This runs on a thread pool with non-deterministic scheduling
    /// });
    /// ```
    /// ### Solution
    /// Use deterministic execution patterns or ensure the operation is truly necessary
    /// and documented as non-deterministic
    pub NONDETERMINISTIC_SPAWN_BLOCKING,
    Warn,
    "use of `tokio::task::spawn_blocking` which can introduce nondeterminism"
}

declare_lint! {
    /// ### What it does
    /// Detects wall-clock time usage (`SystemTime::now()`, `Instant::now()`)
    /// ### Why is this bad?
    /// Wall-clock time introduces non-determinism across runs
    /// ### Example
    /// ```rust
    /// let now = std::time::SystemTime::now();
    /// let instant = std::time::Instant::now();
    /// ```
    /// ### Solution
    /// Use deterministic time sources or ensure the operation is documented as non-deterministic
    pub NONDETERMINISTIC_WALL_CLOCK_TIME,
    Warn,
    "use of wall-clock time functions which introduce nondeterminism"
}

declare_lint! {
    /// ### What it does
    /// Detects random number generation without proper seeding
    /// ### Why is this bad?
    /// Unseeded RNG introduces non-determinism
    /// ### Example
    /// ```rust
    /// let mut rng = rand::thread_rng();
    /// let value = rng.gen::<u32>();
    /// ```
    /// ### Solution
    /// Use `DeterministicRng` or ensure proper HKDF seeding
    pub NONDETERMINISTIC_RANDOM,
    Warn,
    "use of unseeded random number generation"
}

declare_lint! {
    /// ### What it does
    /// Detects file I/O operations that can introduce nondeterminism
    /// ### Why is this bad?
    /// File I/O timing and content can vary between runs
    /// ### Example
    /// ```rust
    /// std::fs::read_to_string("file.txt")?;
    /// std::fs::write("output.txt", data)?;
    /// ```
    /// ### Solution
    /// Use deterministic I/O patterns or ensure the operation is documented as non-deterministic
    pub NONDETERMINISTIC_FILE_IO,
    Warn,
    "use of file I/O operations which can introduce nondeterminism"
}

declare_lint! {
    /// ### What it does
    /// Detects system calls that can introduce nondeterminism
    /// ### Why is this bad?
    /// System calls can have non-deterministic timing and results
    /// ### Example
    /// ```rust
    /// std::process::Command::new("ls").output()?;
    /// ```
    /// ### Solution
    /// Use deterministic alternatives or ensure the operation is documented as non-deterministic
    pub NONDETERMINISTIC_SYSCALL,
    Warn,
    "use of system calls which can introduce nondeterminism"
}

declare_lint_pass!(DeterminismLint => [
    NONDETERMINISTIC_SPAWN_BLOCKING,
    NONDETERMINISTIC_WALL_CLOCK_TIME,
    NONDETERMINISTIC_RANDOM,
    NONDETERMINISTIC_FILE_IO,
    NONDETERMINISTIC_SYSCALL,
]);

impl EarlyLintPass for DeterminismLint {
    fn check_expr(&mut self, cx: &EarlyContext<'_>, expr: &Expr) {
        match &expr.kind {
            ExprKind::Call(call_expr, args) => {
                self.check_call_expr(cx, call_expr, args, expr.span);
            }
            ExprKind::MethodCall(method_call) => {
                self.check_method_call(cx, method_call, expr.span);
            }
            _ => {}
        }
    }

    fn check_item(&mut self, cx: &EarlyContext<'_>, item: &Item) {
        // Check for imports of problematic functions
        if let ItemKind::Use(use_tree) = &item.kind {
            self.check_use_tree(cx, use_tree, item.span);
        }
    }
}

impl DeterminismLint {
    fn check_call_expr(&self, cx: &EarlyContext<'_>, call_expr: &Expr, args: &[Expr], span: Span) {
        if let ExprKind::Path(qpath) = &call_expr.kind {
            if let Some(path) = self.get_path_string(qpath) {
                self.check_problematic_path(cx, &path, span);
            }
        }
    }

    fn check_method_call(&self, cx: &EarlyContext<'_>, method_call: &MethodCall, span: Span) {
        let method_name = method_call.ident.name.to_string();
        
        // Check for problematic method calls
        match method_name.as_str() {
            "spawn_blocking" => {
                cx.lint(
                    NONDETERMINISTIC_SPAWN_BLOCKING,
                    "`tokio::task::spawn_blocking` can introduce nondeterminism",
                    |lint| {
                        lint.span_label(span, "nondeterministic spawn_blocking call")
                            .note("Consider using deterministic execution patterns")
                            .help("If this is intentional, add `#[allow(adapteros_lint::nondeterministic_spawn_blocking)]`")
                    }
                );
            }
            "now" => {
                // Check if it's SystemTime::now() or Instant::now()
                if let Some(receiver) = &method_call.receiver {
                    if let ExprKind::Path(qpath) = &receiver.kind {
                        if let Some(path) = self.get_path_string(qpath) {
                            if path.contains("SystemTime") || path.contains("Instant") {
                                cx.lint(
                                    NONDETERMINISTIC_WALL_CLOCK_TIME,
                                    "Wall-clock time functions introduce nondeterminism",
                                    |lint| {
                                        lint.span_label(span, "wall-clock time usage")
                                            .note("Consider using deterministic time sources")
                                            .help("If this is intentional, add `#[allow(adapteros_lint::nondeterministic_wall_clock_time)]`")
                                    }
                                );
                            }
                        }
                    }
                }
            }
            "gen" | "thread_rng" => {
                cx.lint(
                    NONDETERMINISTIC_RANDOM,
                    "Random number generation without proper seeding",
                    |lint| {
                        lint.span_label(span, "unseeded random number generation")
                            .note("Consider using `DeterministicRng` with HKDF seeding")
                            .help("If this is intentional, add `#[allow(adapteros_lint::nondeterministic_random)]`")
                    }
                );
            }
            _ => {}
        }
    }

    fn check_use_tree(&self, cx: &EarlyContext<'_>, use_tree: &UseTree, span: Span) {
        if let UseTreeKind::Simple(ident, _) = &use_tree.kind {
            let name = ident.name.to_string();
            
            // Check for problematic imports
            match name.as_str() {
                "spawn_blocking" => {
                    cx.lint(
                        NONDETERMINISTIC_SPAWN_BLOCKING,
                        "Import of `spawn_blocking` detected",
                        |lint| {
                            lint.span_label(span, "nondeterministic function import")
                                .note("Consider using deterministic alternatives")
                        }
                    );
                }
                "SystemTime" | "Instant" => {
                    cx.lint(
                        NONDETERMINISTIC_WALL_CLOCK_TIME,
                        "Import of wall-clock time types detected",
                        |lint| {
                            lint.span_label(span, "wall-clock time type import")
                                .note("Consider using deterministic time sources")
                        }
                    );
                }
                "thread_rng" | "random" => {
                    cx.lint(
                        NONDETERMINISTIC_RANDOM,
                        "Import of random number generation detected",
                        |lint| {
                            lint.span_label(span, "random number generation import")
                                .note("Consider using `DeterministicRng` with HKDF seeding")
                        }
                    );
                }
                _ => {}
            }
        }
    }

    fn check_problematic_path(&self, cx: &EarlyContext<'_>, path: &str, span: Span) {
        if path.contains("std::fs::") {
            cx.lint(
                NONDETERMINISTIC_FILE_IO,
                "File I/O operations can introduce nondeterminism",
                |lint| {
                    lint.span_label(span, "file I/O operation")
                        .note("Consider using deterministic I/O patterns")
                        .help("If this is intentional, add `#[allow(adapteros_lint::nondeterministic_file_io)]`")
                }
            );
        } else if path.contains("std::process::") {
            cx.lint(
                NONDETERMINISTIC_SYSCALL,
                "System calls can introduce nondeterminism",
                |lint| {
                    lint.span_label(span, "system call")
                        .note("Consider using deterministic alternatives")
                        .help("If this is intentional, add `#[allow(adapteros_lint::nondeterministic_syscall)]`")
                }
            );
        }
    }

    fn get_path_string(&self, qpath: &QPath) -> Option<String> {
        match qpath {
            QPath::Resolved(_, path) => {
                let segments: Vec<String> = path.segments.iter()
                    .map(|seg| seg.ident.name.to_string())
                    .collect();
                Some(segments.join("::"))
            }
            QPath::TypeRelative(ty, segment) => {
                // Handle cases like `tokio::task::spawn_blocking`
                Some(format!("{}::{}", "unknown", segment.ident.name))
            }
            QPath::LangItem(_, _) => None,
        }
    }
}

/// Register the lint pass with the compiler
#[no_mangle]
pub fn register_lints(_sess: &rustc_session::Session, lint_store: &mut rustc_lint::LintStore) {
    lint_store.register_lints(&[&NONDETERMINISTIC_SPAWN_BLOCKING]);
    lint_store.register_lints(&[&NONDETERMINISTIC_WALL_CLOCK_TIME]);
    lint_store.register_lints(&[&NONDETERMINISTIC_RANDOM]);
    lint_store.register_lints(&[&NONDETERMINISTIC_FILE_IO]);
    lint_store.register_lints(&[&NONDETERMINISTIC_SYSCALL]);
    
    lint_store.register_early_pass(|| Box::new(DeterminismLint));
}

/// Initialize the plugin
#[no_mangle]
pub fn __rustc_plugin_registrar(reg: &mut rustc_plugin::Registry) {
    reg.register_early_lint_pass(Box::new(DeterminismLint));
}
