use crate::architectural::{check_directory, ArchitecturalViolation};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Run in rustc/clippy driver mode.
/// This is intended to be used via `RUSTC_WORKSPACE_WRAPPER=adapteros-lint`
/// so diagnostics appear in rust-analyzer / VS Code as standard warnings.
pub fn run_driver(args: Vec<String>) -> i32 {
    let json_mode = args.iter().any(|arg| arg.contains("error-format=json"));

    // Emit architectural lint diagnostics before delegating to the real compiler.
    emit_architectural_diagnostics(json_mode);

    // Forward to clippy-driver if available, otherwise rustc.
    let rustc_cmd = env::var("ADAPTEROS_LINT_UNDERLYING")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "clippy-driver".to_string());

    invoke_underlying(&rustc_cmd, &args[1..])
}

fn invoke_underlying(cmd: &str, args: &[String]) -> i32 {
    // Try preferred command first; fall back to rustc if not found.
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match status {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) if cmd != "rustc" => Command::new("rustc")
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.code().unwrap_or(1))
            .unwrap_or(1),
        Err(_) => 1,
    }
}

fn emit_architectural_diagnostics(json_mode: bool) {
    let manifest_dir = match env::var("CARGO_MANIFEST_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => return, // Outside Cargo context; skip lint emission.
    };

    // Prefer scanning src/ to avoid pulling in generated or target artifacts.
    let candidate = manifest_dir.join("src");
    let scan_root = if candidate.exists() {
        candidate
    } else {
        manifest_dir
    };

    let violations = check_directory(&scan_root);
    for violation in violations {
        emit_violation(&violation, json_mode);
    }
}

#[derive(Clone, Serialize)]
struct DiagnosticCode {
    code: String,
    explanation: Option<String>,
}

#[derive(Clone, Serialize)]
struct DiagnosticSpanText {
    text: String,
    highlight_start: usize,
    highlight_end: usize,
}

#[derive(Clone, Serialize)]
struct DiagnosticSpan {
    file_name: String,
    byte_start: usize,
    byte_end: usize,
    line_start: usize,
    line_end: usize,
    column_start: usize,
    column_end: usize,
    is_primary: bool,
    text: Vec<DiagnosticSpanText>,
    label: Option<String>,
    suggested_replacement: Option<String>,
    suggestion_applicability: Option<String>,
    expansion: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct Diagnostic {
    message: String,
    code: Option<DiagnosticCode>,
    level: String,
    spans: Vec<DiagnosticSpan>,
    children: Vec<serde_json::Value>,
    rendered: Option<String>,
}

fn emit_violation(violation: &ArchitecturalViolation, json_mode: bool) {
    let (message, label) = violation_message_and_label(violation);
    let span = build_span(violation, label.clone());

    if json_mode {
        let diagnostic = Diagnostic {
            message: message.to_string(),
            code: Some(DiagnosticCode {
                code: "adapteros::architectural".to_string(),
                explanation: None,
            }),
            level: "warning".to_string(),
            spans: vec![span.clone()],
            children: Vec::new(),
            rendered: Some(rendered_warning(message, &span)),
        };

        if let Ok(json) = serde_json::to_string(&diagnostic) {
            eprintln!("{json}");
        }
    } else {
        eprintln!("{}", rendered_warning(message, &span));
        if let Some(label) = label {
            eprintln!("note: {label}");
        }
    }
}

fn build_span(violation: &ArchitecturalViolation, label: Option<String>) -> DiagnosticSpan {
    let file = match violation_file(violation) {
        Some(f) => f,
        None => "<unknown>".to_string(),
    };
    let line = violation.line().max(1);
    let content = fs::read_to_string(&file).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = line.saturating_sub(1);
    let line_text = lines.get(line_idx).copied().unwrap_or("");

    let first_non_ws = line_text
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(idx, _)| idx + 1)
        .unwrap_or(1);
    let column_start = first_non_ws;
    let column_end = std::cmp::max(column_start, line_text.len());

    let mut byte_start = 0usize;
    for l in &lines[..line_idx.min(lines.len())] {
        // +1 for newline separator that was removed by split_lines
        byte_start += l.len() + 1;
    }
    byte_start += column_start.saturating_sub(1);
    let byte_end = byte_start + (column_end.saturating_sub(column_start) + 1);

    DiagnosticSpan {
        file_name: file,
        byte_start,
        byte_end,
        line_start: line,
        line_end: line,
        column_start,
        column_end,
        is_primary: true,
        text: vec![DiagnosticSpanText {
            text: line_text.to_string(),
            highlight_start: column_start,
            highlight_end: column_end,
        }],
        label,
        suggested_replacement: None,
        suggestion_applicability: None,
        expansion: None,
    }
}

fn violation_file(violation: &ArchitecturalViolation) -> Option<String> {
    match violation {
        ArchitecturalViolation::LifecycleManagerBypass { file, .. }
        | ArchitecturalViolation::NonTransactionalFallback { file, .. }
        | ArchitecturalViolation::DirectSqlInHandler { file, .. }
        | ArchitecturalViolation::NonDeterministicSpawn { file, .. } => Some(file.clone()),
    }
}

fn violation_message_and_label(
    violation: &ArchitecturalViolation,
) -> (&'static str, Option<String>) {
    match violation {
        ArchitecturalViolation::LifecycleManagerBypass { context, .. } => (
            "Lifecycle manager bypass: invoke lifecycle manager before direct DB updates",
            Some(format!("context: {context}")),
        ),
        ArchitecturalViolation::NonTransactionalFallback { context, .. } => (
            "Handler fallback must use transactional update_adapter_state_tx",
            Some(format!("context: {context}")),
        ),
        ArchitecturalViolation::DirectSqlInHandler { query, .. } => (
            "Direct SQL in handler; prefer Db trait methods or transactional context",
            Some(format!("query: {query}")),
        ),
        ArchitecturalViolation::NonDeterministicSpawn { context, .. } => (
            "Non-deterministic spawn in deterministic context; use spawn_deterministic",
            Some(format!("context: {context}")),
        ),
    }
}

fn rendered_warning(message: &str, span: &DiagnosticSpan) -> String {
    let line_no = span.line_start;
    let col_no = span.column_start;
    let line_src = span.text.first().map(|t| t.text.as_str()).unwrap_or("");
    let gutter_pad = format!("{line_no} ").len();
    let caret_indent = " ".repeat(gutter_pad + 2 + col_no.saturating_sub(1));

    format!(
        "warning: {message}\n --> {}:{}:{}\n  |\n{line_no:>4} | {}\n  |{caret}^\n",
        span.file_name,
        line_no,
        col_no,
        line_src,
        caret = caret_indent
    )
}
