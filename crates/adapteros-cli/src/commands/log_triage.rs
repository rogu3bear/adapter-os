//! Log triage module - categorizes log entries with rule-based remediation hints
//!
//! Analyzes log entries from digest output and categorizes them by issue type,
//! providing remediation hints based on known patterns.
//!
//! # Usage
//!
//! ```bash
//! aosctl log triage ./var/logs
//! aosctl log triage ./var/logs --rules ./custom_rules.json
//! aosctl log triage ./var/logs --json
//! ```

use crate::commands::log_digest::{LogDigest, LogEntry, LogLevel};
use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Log triage command arguments
#[derive(Debug, Args, Clone)]
pub struct LogTriageArgs {
    /// Directory containing log files (uses digest internally)
    #[arg(default_value = "./var/logs")]
    pub log_dir: PathBuf,

    /// Custom rules file (JSON format)
    #[arg(long)]
    pub rules: Option<PathBuf>,

    /// Only include entries from the last N hours/minutes (e.g., "1h", "30m")
    #[arg(long)]
    pub since: Option<String>,

    /// Maximum number of entries to triage
    #[arg(long, default_value = "100")]
    pub max_entries: usize,

    /// Show detailed remediation steps
    #[arg(long)]
    pub detailed: bool,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Triage rule for pattern matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageRule {
    /// Unique identifier for the rule
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Category this rule belongs to
    pub category: IssueCategory,
    /// Patterns to match (any match triggers the rule)
    pub patterns: Vec<String>,
    /// Severity level
    pub severity: Severity,
    /// Short remediation hint
    pub hint: String,
    /// Detailed remediation steps
    #[serde(default)]
    pub detailed_steps: Vec<String>,
    /// Related error codes (if any)
    #[serde(default)]
    pub error_codes: Vec<String>,
    /// Related documentation links
    #[serde(default)]
    pub doc_links: Vec<String>,
}

/// Issue category for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IssueCategory {
    /// Memory-related issues
    Memory,
    /// Database/storage issues
    Database,
    /// Network/connectivity issues
    Network,
    /// Authentication/authorization issues
    Auth,
    /// Configuration issues
    Config,
    /// Hardware/GPU issues
    Hardware,
    /// Policy violation issues
    Policy,
    /// Training-related issues
    Training,
    /// Inference-related issues
    Inference,
    /// Security issues
    Security,
    /// Performance issues
    Performance,
    /// General system issues
    System,
    /// Unknown/uncategorized
    Unknown,
}

impl IssueCategory {
    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            IssueCategory::Memory => "Memory",
            IssueCategory::Database => "Database",
            IssueCategory::Network => "Network",
            IssueCategory::Auth => "Auth",
            IssueCategory::Config => "Config",
            IssueCategory::Hardware => "Hardware",
            IssueCategory::Policy => "Policy",
            IssueCategory::Training => "Training",
            IssueCategory::Inference => "Inference",
            IssueCategory::Security => "Security",
            IssueCategory::Performance => "Performance",
            IssueCategory::System => "System",
            IssueCategory::Unknown => "Unknown",
        }
    }

    /// Get emoji symbol
    pub fn symbol(&self) -> &'static str {
        match self {
            IssueCategory::Memory => "M",
            IssueCategory::Database => "D",
            IssueCategory::Network => "N",
            IssueCategory::Auth => "A",
            IssueCategory::Config => "C",
            IssueCategory::Hardware => "H",
            IssueCategory::Policy => "P",
            IssueCategory::Training => "T",
            IssueCategory::Inference => "I",
            IssueCategory::Security => "S",
            IssueCategory::Performance => "F",
            IssueCategory::System => "Y",
            IssueCategory::Unknown => "?",
        }
    }
}

/// Severity level for triaged issues
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn name(&self) -> &'static str {
        match self {
            Severity::Low => "Low",
            Severity::Medium => "Medium",
            Severity::High => "High",
            Severity::Critical => "Critical",
        }
    }
}

/// Triaged log entry with classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriagedEntry {
    /// Original log entry
    pub entry: LogEntry,
    /// Matched rule (if any)
    pub rule: Option<TriageRule>,
    /// Issue category
    pub category: IssueCategory,
    /// Severity
    pub severity: Severity,
    /// Remediation hint
    pub hint: String,
}

/// Triage result summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResult {
    /// Total entries triaged
    pub total_entries: usize,
    /// Entries by category
    pub by_category: HashMap<String, usize>,
    /// Entries by severity
    pub by_severity: HashMap<String, usize>,
    /// Top issues (most frequent patterns)
    pub top_issues: Vec<IssueCount>,
    /// Triaged entries
    pub entries: Vec<TriagedEntry>,
    /// Rules that matched
    pub matched_rules: Vec<String>,
}

/// Issue count for top issues
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueCount {
    pub rule_id: String,
    pub rule_name: String,
    pub category: String,
    pub count: usize,
    pub hint: String,
}

/// Run the log triage command
pub async fn run(args: LogTriageArgs, output: &OutputWriter) -> Result<()> {
    // First, run log digest to get entries
    let digest_args = super::log_digest::LogDigestArgs {
        log_dir: args.log_dir.clone(),
        since: args.since.clone(),
        max_entries: args.max_entries,
        include_info: false, // Triage focuses on WARN/ERROR
        component: None,
        json: false,
    };

    // Get digest (we need to run digest first to get entries)
    let digest = run_digest_internal(&digest_args).await?;

    // Load triage rules
    let rules = load_rules(&args.rules)?;

    // Triage entries
    let result = triage_entries(&digest, &rules);

    if args.json {
        output.json(&result)?;
    } else {
        render_triage_text(&result, args.detailed, output);
    }

    Ok(())
}

/// Run digest internally (without output)
async fn run_digest_internal(args: &super::log_digest::LogDigestArgs) -> Result<LogDigest> {
    use chrono::{Duration, Utc};
    use std::collections::HashMap;
    use std::fs::{self, File};
    use std::io::{BufRead, BufReader};

    let log_dir = &args.log_dir;

    if !log_dir.exists() {
        return Err(AosError::Io(format!(
            "Log directory not found: {}",
            log_dir.display()
        )));
    }

    // Parse time filter
    let since_duration = parse_duration_simple(&args.since)?;
    let cutoff_time = since_duration.map(|d| Utc::now() - d);

    // Determine minimum log level
    let min_level = if args.include_info {
        LogLevel::Info
    } else {
        LogLevel::Warn
    };

    // Collect log files
    let log_files = collect_log_files_simple(log_dir)?;

    if log_files.is_empty() {
        return Ok(LogDigest {
            files_scanned: 0,
            total_entries: 0,
            counts_by_level: HashMap::new(),
            counts_by_component: HashMap::new(),
            entries: Vec::new(),
            time_range: None,
            filter_criteria: super::log_digest::FilterCriteria {
                min_level: min_level.name().to_string(),
                since: args.since.clone(),
                component: args.component.clone(),
                max_entries: args.max_entries,
            },
        });
    }

    // Parse and filter entries
    let mut all_entries = Vec::new();
    let mut files_scanned = 0;
    let mut total_entries = 0;
    let mut counts_by_level: HashMap<String, usize> = HashMap::new();
    let mut counts_by_component: HashMap<String, usize> = HashMap::new();

    for log_file in &log_files {
        files_scanned += 1;
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

            // Try to parse
            let entry = if let Some(e) = try_parse_json(&line, &source_file, line_number + 1) {
                e
            } else if let Some(e) = try_parse_text(&line, &source_file, line_number + 1) {
                e
            } else {
                continue;
            };

            total_entries += 1;

            // Update counts
            *counts_by_level
                .entry(entry.level.name().to_string())
                .or_insert(0) += 1;

            if let Some(ref comp) = entry.component {
                *counts_by_component.entry(comp.clone()).or_insert(0) += 1;
            }

            // Apply filters
            if entry.level < min_level {
                continue;
            }

            if let Some(cutoff) = cutoff_time {
                if let Some(ts) = entry.timestamp {
                    if ts < cutoff {
                        continue;
                    }
                }
            }

            all_entries.push(entry);
        }
    }

    // Sort and limit
    all_entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all_entries.truncate(args.max_entries);

    Ok(LogDigest {
        files_scanned,
        total_entries,
        counts_by_level,
        counts_by_component,
        entries: all_entries,
        time_range: None,
        filter_criteria: super::log_digest::FilterCriteria {
            min_level: min_level.name().to_string(),
            since: args.since.clone(),
            component: args.component.clone(),
            max_entries: args.max_entries,
        },
    })
}

fn parse_duration_simple(s: &Option<String>) -> Result<Option<chrono::Duration>> {
    let s = match s {
        Some(s) => s,
        None => return Ok(None),
    };

    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return Ok(None);
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str
        .parse()
        .map_err(|_| AosError::Validation(format!("Invalid duration: {}", s)))?;

    let duration = match unit {
        "s" => chrono::Duration::seconds(num),
        "m" => chrono::Duration::minutes(num),
        "h" => chrono::Duration::hours(num),
        "d" => chrono::Duration::days(num),
        _ => {
            return Err(AosError::Validation(format!(
                "Invalid duration unit: {}",
                unit
            )))
        }
    };

    Ok(Some(duration))
}

fn collect_log_files_simple(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
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
        .map(LogLevel::from_str)
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

/// Load triage rules from file or use defaults
fn load_rules(rules_path: &Option<PathBuf>) -> Result<Vec<TriageRule>> {
    if let Some(path) = rules_path {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AosError::Io(format!("Cannot read rules file: {}", e)))?;
        let rules: Vec<TriageRule> = serde_json::from_str(&content)
            .map_err(|e| AosError::Parse(format!("Invalid rules JSON: {}", e)))?;
        Ok(rules)
    } else {
        Ok(default_rules())
    }
}

/// Default triage rules for adapterOS
fn default_rules() -> Vec<TriageRule> {
    vec![
        // Memory issues
        TriageRule {
            id: "MEM001".to_string(),
            name: "Out of Memory".to_string(),
            category: IssueCategory::Memory,
            patterns: vec![
                "out of memory".to_string(),
                "OOM".to_string(),
                "memory allocation failed".to_string(),
                "cannot allocate".to_string(),
            ],
            severity: Severity::Critical,
            hint: "System ran out of memory. Consider reducing batch size or model size."
                .to_string(),
            detailed_steps: vec![
                "Check current memory usage with: aosctl status".to_string(),
                "Reduce batch size in inference config".to_string(),
                "Consider evicting unused adapters".to_string(),
                "Run: aosctl adapter evict --oldest".to_string(),
            ],
            error_codes: vec!["E9001".to_string()],
            doc_links: vec![],
        },
        TriageRule {
            id: "MEM002".to_string(),
            name: "Memory Pressure".to_string(),
            category: IssueCategory::Memory,
            patterns: vec![
                "memory pressure".to_string(),
                "high memory usage".to_string(),
                "memory_usage".to_string(),
            ],
            severity: Severity::High,
            hint: "System is experiencing memory pressure. Monitor and consider scaling."
                .to_string(),
            detailed_steps: vec![
                "Monitor memory with: aosctl doctor".to_string(),
                "Review adapter memory usage".to_string(),
                "Consider enabling memory limits".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
        // Database issues
        TriageRule {
            id: "DB001".to_string(),
            name: "Database Connection Failed".to_string(),
            category: IssueCategory::Database,
            patterns: vec![
                "database connection".to_string(),
                "sqlite error".to_string(),
                "SQLITE_".to_string(),
                "connection refused".to_string(),
            ],
            severity: Severity::Critical,
            hint: "Database connection failed. Check database path and permissions.".to_string(),
            detailed_steps: vec![
                "Verify database exists: ls -la var/aos-cp.sqlite3".to_string(),
                "Check permissions on database file".to_string(),
                "Run migrations: aosctl db migrate".to_string(),
            ],
            error_codes: vec!["E8003".to_string()],
            doc_links: vec![],
        },
        TriageRule {
            id: "DB002".to_string(),
            name: "Database Locked".to_string(),
            category: IssueCategory::Database,
            patterns: vec![
                "database is locked".to_string(),
                "SQLITE_BUSY".to_string(),
                "SQLITE_LOCKED".to_string(),
            ],
            severity: Severity::High,
            hint: "Database is locked. Check for concurrent access or stale locks.".to_string(),
            detailed_steps: vec![
                "Check for other processes using the database".to_string(),
                "Wait and retry the operation".to_string(),
                "Restart the server if lock persists".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
        // Hardware issues
        TriageRule {
            id: "HW001".to_string(),
            name: "Metal Device Not Found".to_string(),
            category: IssueCategory::Hardware,
            patterns: vec![
                "Metal device".to_string(),
                "no Metal".to_string(),
                "GPU not found".to_string(),
                "metal error".to_string(),
            ],
            severity: Severity::Critical,
            hint: "Metal GPU device not available. Check hardware and drivers.".to_string(),
            detailed_steps: vec![
                "Run diagnostics: aosctl diag run --system".to_string(),
                "Verify macOS version supports Metal".to_string(),
                "Check system_profiler SPDisplaysDataType".to_string(),
            ],
            error_codes: vec!["E3004".to_string()],
            doc_links: vec![],
        },
        TriageRule {
            id: "HW002".to_string(),
            name: "Kernel Library Missing".to_string(),
            category: IssueCategory::Hardware,
            patterns: vec![
                "metallib not found".to_string(),
                "kernel library".to_string(),
                "aos_kernels.metallib".to_string(),
            ],
            severity: Severity::Critical,
            hint: "Metal kernel library missing. Build kernels with: cd metal && ./build.sh"
                .to_string(),
            detailed_steps: vec![
                "Navigate to metal directory".to_string(),
                "Run build script: ./build.sh".to_string(),
                "Verify aos_kernels.metallib was created".to_string(),
            ],
            error_codes: vec!["E3001".to_string()],
            doc_links: vec![],
        },
        // Policy issues
        TriageRule {
            id: "POL001".to_string(),
            name: "Policy Violation".to_string(),
            category: IssueCategory::Policy,
            patterns: vec![
                "policy violation".to_string(),
                "policy denied".to_string(),
                "blocked by policy".to_string(),
            ],
            severity: Severity::High,
            hint: "Operation blocked by policy. Review policy configuration.".to_string(),
            detailed_steps: vec![
                "List policies: aosctl policy list".to_string(),
                "Check policy pack configuration".to_string(),
                "Review adapter permissions".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
        // Network issues
        TriageRule {
            id: "NET001".to_string(),
            name: "Network Connection Failed".to_string(),
            category: IssueCategory::Network,
            patterns: vec![
                "connection refused".to_string(),
                "connection timed out".to_string(),
                "network error".to_string(),
                "socket error".to_string(),
            ],
            severity: Severity::High,
            hint: "Network connection failed. Check service availability and firewall.".to_string(),
            detailed_steps: vec![
                "Verify service is running".to_string(),
                "Check network connectivity".to_string(),
                "Review firewall rules".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
        // Config issues
        TriageRule {
            id: "CFG001".to_string(),
            name: "Configuration Error".to_string(),
            category: IssueCategory::Config,
            patterns: vec![
                "config error".to_string(),
                "invalid configuration".to_string(),
                "missing config".to_string(),
                "configuration failed".to_string(),
            ],
            severity: Severity::High,
            hint: "Configuration error. Verify config file syntax and values.".to_string(),
            detailed_steps: vec![
                "Check config file: cat configs/cp.toml".to_string(),
                "Validate TOML syntax".to_string(),
                "Review environment variables".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
        // Training issues
        TriageRule {
            id: "TRN001".to_string(),
            name: "Training Failed".to_string(),
            category: IssueCategory::Training,
            patterns: vec![
                "training failed".to_string(),
                "training error".to_string(),
                "gradient overflow".to_string(),
                "NaN loss".to_string(),
            ],
            severity: Severity::High,
            hint: "Training failed. Check dataset and hyperparameters.".to_string(),
            detailed_steps: vec![
                "Verify dataset format and content".to_string(),
                "Reduce learning rate".to_string(),
                "Check for data quality issues".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
        // Inference issues
        TriageRule {
            id: "INF001".to_string(),
            name: "Inference Timeout".to_string(),
            category: IssueCategory::Inference,
            patterns: vec![
                "inference timeout".to_string(),
                "request timed out".to_string(),
                "inference took too long".to_string(),
            ],
            severity: Severity::Medium,
            hint: "Inference request timed out. Consider increasing timeout or optimizing."
                .to_string(),
            detailed_steps: vec![
                "Review inference parameters".to_string(),
                "Check system load".to_string(),
                "Consider reducing max_tokens".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
        // Security issues
        TriageRule {
            id: "SEC001".to_string(),
            name: "Authentication Failed".to_string(),
            category: IssueCategory::Auth,
            patterns: vec![
                "authentication failed".to_string(),
                "invalid token".to_string(),
                "unauthorized".to_string(),
                "401".to_string(),
            ],
            severity: Severity::High,
            hint: "Authentication failed. Check credentials and token validity.".to_string(),
            detailed_steps: vec![
                "Verify API key or token".to_string(),
                "Check token expiration".to_string(),
                "Re-authenticate if needed".to_string(),
            ],
            error_codes: vec![],
            doc_links: vec![],
        },
    ]
}

/// Triage log entries using rules
fn triage_entries(digest: &LogDigest, rules: &[TriageRule]) -> TriageResult {
    let mut triaged_entries = Vec::new();
    let mut by_category: HashMap<String, usize> = HashMap::new();
    let mut by_severity: HashMap<String, usize> = HashMap::new();
    let mut rule_counts: HashMap<String, usize> = HashMap::new();
    let mut matched_rules: Vec<String> = Vec::new();

    for entry in &digest.entries {
        let (rule, category, severity, hint) = match_rule(entry, rules);

        // Update counts
        *by_category.entry(category.name().to_string()).or_insert(0) += 1;
        *by_severity.entry(severity.name().to_string()).or_insert(0) += 1;

        if let Some(ref r) = rule {
            *rule_counts.entry(r.id.clone()).or_insert(0) += 1;
            if !matched_rules.contains(&r.id) {
                matched_rules.push(r.id.clone());
            }
        }

        triaged_entries.push(TriagedEntry {
            entry: entry.clone(),
            rule: rule.cloned(),
            category,
            severity,
            hint,
        });
    }

    // Build top issues
    let mut top_issues: Vec<IssueCount> = rule_counts
        .iter()
        .filter_map(|(rule_id, count)| {
            rules.iter().find(|r| &r.id == rule_id).map(|r| IssueCount {
                rule_id: r.id.clone(),
                rule_name: r.name.clone(),
                category: r.category.name().to_string(),
                count: *count,
                hint: r.hint.clone(),
            })
        })
        .collect();
    top_issues.sort_by(|a, b| b.count.cmp(&a.count));

    TriageResult {
        total_entries: triaged_entries.len(),
        by_category,
        by_severity,
        top_issues,
        entries: triaged_entries,
        matched_rules,
    }
}

/// Match a log entry against rules
fn match_rule<'a>(
    entry: &LogEntry,
    rules: &'a [TriageRule],
) -> (Option<&'a TriageRule>, IssueCategory, Severity, String) {
    let message_lower = entry.message.to_lowercase();

    for rule in rules {
        for pattern in &rule.patterns {
            if message_lower.contains(&pattern.to_lowercase()) {
                return (Some(rule), rule.category, rule.severity, rule.hint.clone());
            }
        }
    }

    // Default classification based on log level
    let (category, severity, hint) = match entry.level {
        LogLevel::Critical => (
            IssueCategory::System,
            Severity::Critical,
            "Critical system issue. Investigate immediately.".to_string(),
        ),
        LogLevel::Error => (
            IssueCategory::Unknown,
            Severity::High,
            "Error occurred. Review log message for details.".to_string(),
        ),
        LogLevel::Warn => (
            IssueCategory::Unknown,
            Severity::Medium,
            "Warning issued. May require attention.".to_string(),
        ),
        _ => (
            IssueCategory::Unknown,
            Severity::Low,
            "Informational message.".to_string(),
        ),
    };

    (None, category, severity, hint)
}

/// Render triage result as text
fn render_triage_text(result: &TriageResult, detailed: bool, output: &OutputWriter) {
    output.section("Log Triage Summary");

    output.kv("Total entries", &result.total_entries.to_string());
    output.kv("Matched rules", &result.matched_rules.len().to_string());

    // By severity
    output.section("By Severity");
    let mut severities: Vec<_> = result.by_severity.iter().collect();
    severities.sort_by(|a, b| b.1.cmp(a.1));
    for (severity, count) in severities {
        output.kv(severity, &count.to_string());
    }

    // By category
    output.section("By Category");
    let mut categories: Vec<_> = result.by_category.iter().collect();
    categories.sort_by(|a, b| b.1.cmp(a.1));
    for (category, count) in categories {
        output.kv(category, &count.to_string());
    }

    // Top issues
    if !result.top_issues.is_empty() {
        output.section("Top Issues");

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(vec!["Rule", "Category", "Count", "Hint"]);

        for issue in result.top_issues.iter().take(10) {
            let hint = if issue.hint.len() > 50 {
                format!("{}...", &issue.hint[..47])
            } else {
                issue.hint.clone()
            };

            table.add_row(vec![
                Cell::new(&issue.rule_name),
                Cell::new(&issue.category),
                Cell::new(&issue.count.to_string()),
                Cell::new(&hint),
            ]);
        }

        output
            .table(&table as &dyn std::fmt::Display, None::<&()>)
            .ok();

        // Show detailed steps if requested
        if detailed {
            output.section("Remediation Steps");
            for issue in result.top_issues.iter().take(5) {
                if let Some(rule) = default_rules().iter().find(|r| r.id == issue.rule_id) {
                    output.kv(&rule.name, "");
                    for step in &rule.detailed_steps {
                        output.print(&format!("  - {}", step));
                    }
                    output.blank();
                }
            }
        }
    }

    // Sample entries
    if !result.entries.is_empty() {
        output.section("Sample Entries");

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(vec!["Severity", "Category", "Rule", "Message"]);

        for entry in result.entries.iter().take(20) {
            let rule_name = entry.rule.as_ref().map(|r| r.name.as_str()).unwrap_or("-");

            let message = if entry.entry.message.len() > 60 {
                format!("{}...", &entry.entry.message[..57])
            } else {
                entry.entry.message.clone()
            };

            table.add_row(vec![
                Cell::new(entry.severity.name()),
                Cell::new(entry.category.name()),
                Cell::new(rule_name),
                Cell::new(&message),
            ]);
        }

        output
            .table(&table as &dyn std::fmt::Display, None::<&()>)
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_rules_load() {
        let rules = default_rules();
        assert!(!rules.is_empty());
        assert!(rules.iter().any(|r| r.id == "MEM001"));
        assert!(rules.iter().any(|r| r.id == "DB001"));
    }

    #[test]
    fn test_match_rule_memory() {
        let rules = default_rules();
        let entry = LogEntry {
            timestamp: None,
            level: LogLevel::Error,
            component: None,
            message: "System ran out of memory during inference".to_string(),
            source_file: "test.log".to_string(),
            line_number: 1,
            metadata: None,
        };

        let (rule, category, severity, _) = match_rule(&entry, &rules);
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().id, "MEM001");
        assert_eq!(category, IssueCategory::Memory);
        assert_eq!(severity, Severity::Critical);
    }

    #[test]
    fn test_match_rule_database() {
        let rules = default_rules();
        let entry = LogEntry {
            timestamp: None,
            level: LogLevel::Error,
            component: None,
            message: "Database connection failed: SQLITE_BUSY".to_string(),
            source_file: "test.log".to_string(),
            line_number: 1,
            metadata: None,
        };

        let (rule, category, _, _) = match_rule(&entry, &rules);
        assert!(rule.is_some());
        assert_eq!(category, IssueCategory::Database);
    }

    #[test]
    fn test_match_rule_no_match() {
        let rules = default_rules();
        let entry = LogEntry {
            timestamp: None,
            level: LogLevel::Warn,
            component: None,
            message: "Some generic warning message".to_string(),
            source_file: "test.log".to_string(),
            line_number: 1,
            metadata: None,
        };

        let (rule, category, severity, _) = match_rule(&entry, &rules);
        assert!(rule.is_none());
        assert_eq!(category, IssueCategory::Unknown);
        assert_eq!(severity, Severity::Medium);
    }

    #[test]
    fn test_issue_category_name() {
        assert_eq!(IssueCategory::Memory.name(), "Memory");
        assert_eq!(IssueCategory::Database.name(), "Database");
        assert_eq!(IssueCategory::Unknown.name(), "Unknown");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }
}
