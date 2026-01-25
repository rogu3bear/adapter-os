//! Log prompt module - builds LLM prompts from triage output
//!
//! Generates structured prompts for LLM analysis of log entries,
//! useful for getting AI-assisted diagnosis of complex issues.
//!
//! # Usage
//!
//! ```bash
//! aosctl log prompt ./var/logs
//! aosctl log prompt ./var/logs --format markdown
//! aosctl log prompt ./var/logs --output prompt.txt
//! ```

use crate::commands::log_digest::{LogEntry, LogLevel};
use crate::commands::log_triage::{IssueCategory, Severity, TriageResult, TriagedEntry};
use crate::output::OutputWriter;
use adapteros_core::{rebase_var_path, AosError, Result};
use chrono::Utc;
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Log prompt command arguments
#[derive(Debug, Args, Clone)]
pub struct LogPromptArgs {
    /// Directory containing log files
    #[arg(default_value = "./var/logs")]
    pub log_dir: PathBuf,

    /// Only include entries from the last N hours/minutes (e.g., "1h", "30m")
    #[arg(long)]
    pub since: Option<String>,

    /// Maximum number of entries to include
    #[arg(long, default_value = "30")]
    pub max_entries: usize,

    /// Output format: text, markdown, json
    #[arg(long, default_value = "markdown")]
    pub format: String,

    /// Write prompt to file instead of stdout
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Include system context (hardware, OS info)
    #[arg(long)]
    pub include_system: bool,

    /// Focus on a specific category (memory, database, network, etc.)
    #[arg(long)]
    pub focus: Option<String>,

    /// Output in JSON format (for programmatic use)
    #[arg(long)]
    pub json: bool,
}

/// Generated prompt structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedPrompt {
    /// The generated prompt text
    pub prompt: String,
    /// Format used
    pub format: String,
    /// Number of entries included
    pub entry_count: usize,
    /// Categories included
    pub categories: Vec<String>,
    /// Severities included
    pub severities: Vec<String>,
    /// Generation timestamp
    pub generated_at: String,
}

/// System context information
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemContext {
    os: String,
    os_version: String,
    hostname: String,
    memory_total_gb: f64,
    cpu_count: usize,
}

/// Run the log prompt command
pub async fn run(args: LogPromptArgs, output: &OutputWriter) -> Result<()> {
    // Run triage to get categorized entries
    let log_dir = rebase_var_path(&args.log_dir);
    let triage_args = super::log_triage::LogTriageArgs {
        log_dir,
        rules: None,
        since: args.since.clone(),
        max_entries: args.max_entries,
        detailed: false,
        json: false,
    };

    // Get triage result
    let triage_result = run_triage_internal(&triage_args).await?;

    // Apply focus filter if specified
    let filtered_entries = if let Some(ref focus) = args.focus {
        let focus_category = parse_category(focus)?;
        triage_result
            .entries
            .iter()
            .filter(|e| e.category == focus_category)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        triage_result.entries.clone()
    };

    // Get system context if requested
    let system_context = if args.include_system {
        Some(collect_system_context())
    } else {
        None
    };

    // Generate prompt
    let prompt_text = match args.format.to_lowercase().as_str() {
        "markdown" | "md" => {
            generate_markdown_prompt(&filtered_entries, &triage_result, system_context.as_ref())
        }
        "text" | "txt" => {
            generate_text_prompt(&filtered_entries, &triage_result, system_context.as_ref())
        }
        "json" => generate_json_prompt(&filtered_entries, &triage_result, system_context.as_ref())?,
        _ => {
            return Err(AosError::Validation(format!(
                "Unknown format: {}. Use: markdown, text, or json",
                args.format
            )))
        }
    };

    let generated = GeneratedPrompt {
        prompt: prompt_text.clone(),
        format: args.format.clone(),
        entry_count: filtered_entries.len(),
        categories: triage_result.by_category.keys().cloned().collect(),
        severities: triage_result.by_severity.keys().cloned().collect(),
        generated_at: Utc::now().to_rfc3339(),
    };

    // Output
    if args.json {
        output.json(&generated)?;
    } else if let Some(output_path) = args.output {
        std::fs::write(&output_path, &prompt_text).map_err(|e| {
            AosError::Io(format!("Cannot write to {}: {}", output_path.display(), e))
        })?;
        output.success(format!("Prompt written to: {}", output_path.display()));
    } else {
        output.print(&prompt_text);
    }

    Ok(())
}

/// Run triage internally
async fn run_triage_internal(args: &super::log_triage::LogTriageArgs) -> Result<TriageResult> {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let log_dir = rebase_var_path(&args.log_dir);

    if !log_dir.exists() {
        return Ok(TriageResult {
            total_entries: 0,
            by_category: HashMap::new(),
            by_severity: HashMap::new(),
            top_issues: Vec::new(),
            entries: Vec::new(),
            matched_rules: Vec::new(),
        });
    }

    // Collect entries
    let mut all_entries = Vec::new();
    let log_files = collect_log_files_simple(&log_dir)?;

    for log_file in &log_files {
        let file = match File::open(log_file) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        let source_file = log_file.display().to_string();

        for (line_number, line) in reader.lines().enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            if line.trim().is_empty() {
                continue;
            }

            let entry = if let Some(e) = try_parse_json(&line, &source_file, line_number + 1) {
                e
            } else if let Some(e) = try_parse_text(&line, &source_file, line_number + 1) {
                e
            } else {
                continue;
            };

            // Only include WARN/ERROR
            if entry.level >= LogLevel::Warn {
                all_entries.push(entry);
            }
        }
    }

    // Sort by timestamp and limit
    all_entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all_entries.truncate(args.max_entries);

    // Simple triage without full rule matching
    let mut by_category: HashMap<String, usize> = HashMap::new();
    let mut by_severity: HashMap<String, usize> = HashMap::new();
    let mut triaged_entries = Vec::new();

    for entry in all_entries {
        let (category, severity) = categorize_entry(&entry);

        *by_category.entry(category.name().to_string()).or_insert(0) += 1;
        *by_severity.entry(severity.name().to_string()).or_insert(0) += 1;

        triaged_entries.push(TriagedEntry {
            entry,
            rule: None,
            category,
            severity,
            hint: String::new(),
        });
    }

    Ok(TriageResult {
        total_entries: triaged_entries.len(),
        by_category,
        by_severity,
        top_issues: Vec::new(),
        entries: triaged_entries,
        matched_rules: Vec::new(),
    })
}

fn collect_log_files_simple(dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    use std::fs;

    let mut log_files = Vec::new();

    if dir.is_file() {
        log_files.push(dir.to_path_buf());
        return Ok(log_files);
    }

    let entries = fs::read_dir(dir).map_err(|e| AosError::Io(e.to_string()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub) = collect_log_files_simple(&path) {
                log_files.extend(sub);
            }
        } else {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "log" || ext == "ndjson" || name.contains("log") {
                log_files.push(path);
            }
        }
    }

    Ok(log_files)
}

fn try_parse_json(line: &str, source_file: &str, line_number: usize) -> Option<LogEntry> {
    let json: serde_json::Value = serde_json::from_str(line).ok()?;

    let level = json
        .get("level")
        .and_then(|v| v.as_str())
        .map(LogLevel::parse)
        .unwrap_or(LogLevel::Unknown);

    let message = json
        .get("message")
        .or_else(|| json.get("msg"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let timestamp = json
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let component = json
        .get("target")
        .or_else(|| json.get("component"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Some(LogEntry {
        timestamp,
        level,
        component,
        message,
        source_file: source_file.to_string(),
        line_number,
        metadata: None,
    })
}

fn try_parse_text(line: &str, source_file: &str, line_number: usize) -> Option<LogEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let level = if line.contains("ERROR") {
        LogLevel::Error
    } else if line.contains("WARN") {
        LogLevel::Warn
    } else if line.contains("INFO") {
        LogLevel::Info
    } else {
        LogLevel::Unknown
    };

    Some(LogEntry {
        timestamp: None,
        level,
        component: None,
        message: line.to_string(),
        source_file: source_file.to_string(),
        line_number,
        metadata: None,
    })
}

fn categorize_entry(entry: &LogEntry) -> (IssueCategory, Severity) {
    let msg = entry.message.to_lowercase();

    // Categorize by keywords
    let category = if msg.contains("memory") || msg.contains("oom") || msg.contains("alloc") {
        IssueCategory::Memory
    } else if msg.contains("database") || msg.contains("sqlite") || msg.contains("sql") {
        IssueCategory::Database
    } else if msg.contains("network") || msg.contains("connection") || msg.contains("socket") {
        IssueCategory::Network
    } else if msg.contains("auth") || msg.contains("token") || msg.contains("unauthorized") {
        IssueCategory::Auth
    } else if msg.contains("config") || msg.contains("configuration") {
        IssueCategory::Config
    } else if msg.contains("metal") || msg.contains("gpu") || msg.contains("hardware") {
        IssueCategory::Hardware
    } else if msg.contains("policy") {
        IssueCategory::Policy
    } else if msg.contains("training") || msg.contains("train") {
        IssueCategory::Training
    } else if msg.contains("inference") || msg.contains("infer") {
        IssueCategory::Inference
    } else if msg.contains("security") {
        IssueCategory::Security
    } else {
        IssueCategory::Unknown
    };

    let severity = match entry.level {
        LogLevel::Critical => Severity::Critical,
        LogLevel::Error => Severity::High,
        LogLevel::Warn => Severity::Medium,
        _ => Severity::Low,
    };

    (category, severity)
}

fn parse_category(s: &str) -> Result<IssueCategory> {
    match s.to_lowercase().as_str() {
        "memory" | "mem" => Ok(IssueCategory::Memory),
        "database" | "db" => Ok(IssueCategory::Database),
        "network" | "net" => Ok(IssueCategory::Network),
        "auth" | "authentication" => Ok(IssueCategory::Auth),
        "config" | "configuration" => Ok(IssueCategory::Config),
        "hardware" | "hw" | "gpu" => Ok(IssueCategory::Hardware),
        "policy" => Ok(IssueCategory::Policy),
        "training" | "train" => Ok(IssueCategory::Training),
        "inference" | "infer" => Ok(IssueCategory::Inference),
        "security" | "sec" => Ok(IssueCategory::Security),
        "performance" | "perf" => Ok(IssueCategory::Performance),
        "system" | "sys" => Ok(IssueCategory::System),
        _ => Err(AosError::Validation(format!(
            "Unknown category: {}. Use: memory, database, network, auth, config, hardware, policy, training, inference, security, performance, system",
            s
        ))),
    }
}

fn collect_system_context() -> SystemContext {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    SystemContext {
        os: System::name().unwrap_or_else(|| "unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|| "unknown".to_string()),
        hostname: System::host_name().unwrap_or_else(|| "unknown".to_string()),
        memory_total_gb: sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
        cpu_count: sys.cpus().len(),
    }
}

/// Generate markdown-formatted prompt
fn generate_markdown_prompt(
    entries: &[TriagedEntry],
    result: &TriageResult,
    system_context: Option<&SystemContext>,
) -> String {
    let mut prompt = String::new();

    prompt.push_str("# Log Analysis Request\n\n");
    prompt.push_str(
        "Please analyze the following log entries from an AdapterOS deployment and provide:\n",
    );
    prompt.push_str("1. Root cause analysis for the errors\n");
    prompt.push_str("2. Recommended remediation steps\n");
    prompt.push_str("3. Prevention strategies\n\n");

    if let Some(ctx) = system_context {
        prompt.push_str("## System Context\n\n");
        prompt.push_str(&format!("- **OS**: {} {}\n", ctx.os, ctx.os_version));
        prompt.push_str(&format!("- **Hostname**: {}\n", ctx.hostname));
        prompt.push_str(&format!("- **Memory**: {:.1} GB\n", ctx.memory_total_gb));
        prompt.push_str(&format!("- **CPUs**: {}\n", ctx.cpu_count));
        prompt.push('\n');
    }

    prompt.push_str("## Summary\n\n");
    prompt.push_str(&format!("- **Total Entries**: {}\n", result.total_entries));

    if !result.by_severity.is_empty() {
        prompt.push_str("- **By Severity**:\n");
        for (severity, count) in &result.by_severity {
            prompt.push_str(&format!("  - {}: {}\n", severity, count));
        }
    }

    if !result.by_category.is_empty() {
        prompt.push_str("- **By Category**:\n");
        for (category, count) in &result.by_category {
            prompt.push_str(&format!("  - {}: {}\n", category, count));
        }
    }

    prompt.push_str("\n## Log Entries\n\n");

    for (i, entry) in entries.iter().enumerate() {
        prompt.push_str(&format!("### Entry {}\n\n", i + 1));
        prompt.push_str(&format!("- **Level**: {}\n", entry.entry.level.name()));
        prompt.push_str(&format!("- **Category**: {}\n", entry.category.name()));
        prompt.push_str(&format!("- **Severity**: {}\n", entry.severity.name()));

        if let Some(ts) = entry.entry.timestamp {
            prompt.push_str(&format!("- **Time**: {}\n", ts.format("%Y-%m-%d %H:%M:%S")));
        }

        if let Some(ref comp) = entry.entry.component {
            prompt.push_str(&format!("- **Component**: {}\n", comp));
        }

        prompt.push_str(&format!(
            "- **Source**: {}:{}\n",
            entry.entry.source_file, entry.entry.line_number
        ));
        prompt.push_str(&format!("\n```\n{}\n```\n\n", entry.entry.message));

        if let Some(ref rule) = entry.rule {
            prompt.push_str(&format!(
                "**Matched Rule**: {} - {}\n\n",
                rule.id, rule.name
            ));
        }
    }

    prompt.push_str("## Questions\n\n");
    prompt.push_str("1. What is the most likely root cause of these errors?\n");
    prompt.push_str("2. Are there any patterns or correlations between the errors?\n");
    prompt.push_str("3. What immediate actions should be taken?\n");
    prompt.push_str("4. What configuration changes might prevent these issues?\n");

    prompt
}

/// Generate plain text prompt
fn generate_text_prompt(
    entries: &[TriagedEntry],
    result: &TriageResult,
    system_context: Option<&SystemContext>,
) -> String {
    let mut prompt = String::new();

    prompt.push_str("LOG ANALYSIS REQUEST\n");
    prompt.push_str("====================\n\n");

    prompt.push_str("Please analyze the following log entries from an AdapterOS deployment.\n\n");

    if let Some(ctx) = system_context {
        prompt.push_str("SYSTEM CONTEXT:\n");
        prompt.push_str(&format!("  OS: {} {}\n", ctx.os, ctx.os_version));
        prompt.push_str(&format!("  Hostname: {}\n", ctx.hostname));
        prompt.push_str(&format!("  Memory: {:.1} GB\n", ctx.memory_total_gb));
        prompt.push_str(&format!("  CPUs: {}\n\n", ctx.cpu_count));
    }

    prompt.push_str(&format!("SUMMARY: {} entries\n", result.total_entries));

    for (severity, count) in &result.by_severity {
        prompt.push_str(&format!("  {}: {}\n", severity, count));
    }

    prompt.push_str("\nLOG ENTRIES:\n");
    prompt.push_str("-----------\n\n");

    for (i, entry) in entries.iter().enumerate() {
        prompt.push_str(&format!(
            "[{}] {} | {} | {}\n",
            i + 1,
            entry.entry.level.name(),
            entry.category.name(),
            entry.severity.name()
        ));

        if let Some(ts) = entry.entry.timestamp {
            prompt.push_str(&format!("    Time: {}\n", ts.format("%Y-%m-%d %H:%M:%S")));
        }

        prompt.push_str(&format!("    Message: {}\n\n", entry.entry.message));
    }

    prompt.push_str("\nQUESTIONS:\n");
    prompt.push_str("1. What is the root cause?\n");
    prompt.push_str("2. What actions should be taken?\n");
    prompt.push_str("3. How can these be prevented?\n");

    prompt
}

/// Generate JSON-formatted prompt
fn generate_json_prompt(
    entries: &[TriagedEntry],
    result: &TriageResult,
    system_context: Option<&SystemContext>,
) -> Result<String> {
    #[derive(Serialize)]
    struct JsonPrompt {
        task: String,
        system_context: Option<SystemContext>,
        summary: PromptSummary,
        entries: Vec<PromptEntry>,
        questions: Vec<String>,
    }

    #[derive(Serialize)]
    struct PromptSummary {
        total_entries: usize,
        by_severity: std::collections::HashMap<String, usize>,
        by_category: std::collections::HashMap<String, usize>,
    }

    #[derive(Serialize)]
    struct PromptEntry {
        index: usize,
        level: String,
        category: String,
        severity: String,
        timestamp: Option<String>,
        component: Option<String>,
        message: String,
        matched_rule: Option<String>,
    }

    let json_prompt = JsonPrompt {
        task: "Analyze these log entries and provide root cause analysis, remediation steps, and prevention strategies.".to_string(),
        system_context: system_context.cloned(),
        summary: PromptSummary {
            total_entries: result.total_entries,
            by_severity: result.by_severity.clone(),
            by_category: result.by_category.clone(),
        },
        entries: entries
            .iter()
            .enumerate()
            .map(|(i, e)| PromptEntry {
                index: i + 1,
                level: e.entry.level.name().to_string(),
                category: e.category.name().to_string(),
                severity: e.severity.name().to_string(),
                timestamp: e.entry.timestamp.map(|ts| ts.to_rfc3339()),
                component: e.entry.component.clone(),
                message: e.entry.message.clone(),
                matched_rule: e.rule.as_ref().map(|r| format!("{}: {}", r.id, r.name)),
            })
            .collect(),
        questions: vec![
            "What is the most likely root cause?".to_string(),
            "Are there patterns or correlations?".to_string(),
            "What immediate actions should be taken?".to_string(),
            "How can these issues be prevented?".to_string(),
        ],
    };

    serde_json::to_string_pretty(&json_prompt)
        .map_err(|e| AosError::Internal(format!("JSON serialization failed: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_category() {
        assert_eq!(parse_category("memory").unwrap(), IssueCategory::Memory);
        assert_eq!(parse_category("db").unwrap(), IssueCategory::Database);
        assert_eq!(parse_category("network").unwrap(), IssueCategory::Network);
        assert!(parse_category("invalid").is_err());
    }

    #[test]
    fn test_categorize_entry() {
        let entry = LogEntry {
            timestamp: None,
            level: LogLevel::Error,
            component: None,
            message: "Out of memory during inference".to_string(),
            source_file: "test.log".to_string(),
            line_number: 1,
            metadata: None,
        };

        let (category, severity) = categorize_entry(&entry);
        assert_eq!(category, IssueCategory::Memory);
        assert_eq!(severity, Severity::High);
    }

    #[test]
    fn test_generate_markdown_prompt() {
        let entries = vec![TriagedEntry {
            entry: LogEntry {
                timestamp: None,
                level: LogLevel::Error,
                component: Some("test".to_string()),
                message: "Test error".to_string(),
                source_file: "test.log".to_string(),
                line_number: 1,
                metadata: None,
            },
            rule: None,
            category: IssueCategory::Unknown,
            severity: Severity::High,
            hint: String::new(),
        }];

        let result = TriageResult {
            total_entries: 1,
            by_category: [("Unknown".to_string(), 1)].into_iter().collect(),
            by_severity: [("High".to_string(), 1)].into_iter().collect(),
            top_issues: Vec::new(),
            entries: entries.clone(),
            matched_rules: Vec::new(),
        };

        let prompt = generate_markdown_prompt(&entries, &result, None);
        assert!(prompt.contains("# Log Analysis Request"));
        assert!(prompt.contains("Test error"));
        assert!(prompt.contains("Root cause analysis"));
    }

    #[test]
    fn test_generate_text_prompt() {
        let entries = vec![];
        let result = TriageResult {
            total_entries: 0,
            by_category: std::collections::HashMap::new(),
            by_severity: std::collections::HashMap::new(),
            top_issues: Vec::new(),
            entries: Vec::new(),
            matched_rules: Vec::new(),
        };

        let prompt = generate_text_prompt(&entries, &result, None);
        assert!(prompt.contains("LOG ANALYSIS REQUEST"));
        assert!(prompt.contains("SUMMARY: 0 entries"));
    }
}
