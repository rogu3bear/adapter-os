//! Log digest module - summarizes logs extracting WARN/ERROR entries
//!
//! Provides functionality to parse log files and generate summaries
//! focusing on warnings and errors for quick diagnostics.
//!
//! # Usage
//!
//! ```bash
//! aosctl log digest ./var/logs --since 1h
//! aosctl log digest ./var/logs --json
//! aosctl log digest ./var/logs --max-entries 100
//! ```

use crate::output::OutputWriter;
use adapteros_core::{rebase_var_path, AosError, Result};
use chrono::{DateTime, Duration, Utc};
use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Log digest command arguments
#[derive(Debug, Args, Clone)]
pub struct LogDigestArgs {
    /// Directory containing log files
    #[arg(default_value = "./var/logs")]
    pub log_dir: PathBuf,

    /// Only include entries from the last N hours/minutes (e.g., "1h", "30m")
    #[arg(long)]
    pub since: Option<String>,

    /// Maximum number of entries to include in digest
    #[arg(long, default_value = "50")]
    pub max_entries: usize,

    /// Include INFO level entries in addition to WARN/ERROR
    #[arg(long)]
    pub include_info: bool,

    /// Filter by component name
    #[arg(long)]
    pub component: Option<String>,

    /// Output in JSON format
    #[arg(long)]
    pub json: bool,
}

/// Parsed log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp of the log entry
    pub timestamp: Option<DateTime<Utc>>,
    /// Log level (DEBUG, INFO, WARN, ERROR, etc.)
    pub level: LogLevel,
    /// Component or module that generated the log
    pub component: Option<String>,
    /// Log message
    pub message: String,
    /// Source file path
    pub source_file: String,
    /// Line number in source file
    pub line_number: usize,
    /// Additional metadata (from JSON logs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Log level enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Critical,
    Unknown,
}

impl LogLevel {
    /// Parse log level from string
    pub fn parse(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "DEBUG" | "TRACE" => LogLevel::Debug,
            "INFO" => LogLevel::Info,
            "WARN" | "WARNING" => LogLevel::Warn,
            "ERROR" | "ERR" => LogLevel::Error,
            "CRITICAL" | "FATAL" | "CRIT" => LogLevel::Critical,
            _ => LogLevel::Unknown,
        }
    }

    /// Get display symbol for log level
    pub fn symbol(&self) -> &'static str {
        match self {
            LogLevel::Debug => "D",
            LogLevel::Info => "I",
            LogLevel::Warn => "W",
            LogLevel::Error => "E",
            LogLevel::Critical => "C",
            LogLevel::Unknown => "?",
        }
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Critical => "CRITICAL",
            LogLevel::Unknown => "UNKNOWN",
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = AosError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let level = Self::parse(s);
        if level == LogLevel::Unknown {
            Err(AosError::Validation(format!("Invalid log level: {}", s)))
        } else {
            Ok(level)
        }
    }
}

/// Log digest summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogDigest {
    /// Total number of log files scanned
    pub files_scanned: usize,
    /// Total number of entries processed
    pub total_entries: usize,
    /// Number of entries by level
    pub counts_by_level: HashMap<String, usize>,
    /// Number of entries by component
    pub counts_by_component: HashMap<String, usize>,
    /// Filtered entries (WARN/ERROR by default)
    pub entries: Vec<LogEntry>,
    /// Time range of scanned logs
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Filter criteria used
    pub filter_criteria: FilterCriteria,
}

/// Filter criteria used for digest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCriteria {
    /// Minimum log level included
    pub min_level: String,
    /// Time filter (if any)
    pub since: Option<String>,
    /// Component filter (if any)
    pub component: Option<String>,
    /// Maximum entries
    pub max_entries: usize,
}

/// Run the log digest command
pub async fn run(args: LogDigestArgs, output: &OutputWriter) -> Result<()> {
    let log_dir = rebase_var_path(&args.log_dir);

    if !log_dir.exists() {
        return Err(AosError::Io(format!(
            "Log directory not found: {}",
            log_dir.display()
        )));
    }

    // Parse time filter
    let since_duration = parse_duration(&args.since)?;
    let cutoff_time = since_duration.map(|d| Utc::now() - d);

    // Determine minimum log level
    let min_level = if args.include_info {
        LogLevel::Info
    } else {
        LogLevel::Warn
    };

    // Collect log files
    let log_files = collect_log_files(&log_dir)?;

    if log_files.is_empty() {
        output.warning("No log files found in the specified directory");
        return Ok(());
    }

    // Parse and filter entries
    let mut all_entries = Vec::new();
    let mut files_scanned = 0;
    let mut total_entries = 0;
    let mut counts_by_level: HashMap<String, usize> = HashMap::new();
    let mut counts_by_component: HashMap<String, usize> = HashMap::new();
    let mut earliest: Option<DateTime<Utc>> = None;
    let mut latest: Option<DateTime<Utc>> = None;

    for log_file in &log_files {
        files_scanned += 1;
        let entries = parse_log_file(log_file)?;

        for entry in entries {
            total_entries += 1;

            // Update level counts
            *counts_by_level
                .entry(entry.level.name().to_string())
                .or_insert(0) += 1;

            // Update component counts
            if let Some(ref comp) = entry.component {
                *counts_by_component.entry(comp.clone()).or_insert(0) += 1;
            }

            // Update time range
            if let Some(ts) = entry.timestamp {
                earliest = Some(earliest.map_or(ts, |e| e.min(ts)));
                latest = Some(latest.map_or(ts, |l| l.max(ts)));
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

            if let Some(ref filter_comp) = args.component {
                if let Some(ref entry_comp) = entry.component {
                    if !entry_comp
                        .to_lowercase()
                        .contains(&filter_comp.to_lowercase())
                    {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            all_entries.push(entry);
        }
    }

    // Sort by timestamp (most recent first) and limit
    all_entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all_entries.truncate(args.max_entries);

    let digest = LogDigest {
        files_scanned,
        total_entries,
        counts_by_level,
        counts_by_component,
        entries: all_entries,
        time_range: earliest.zip(latest),
        filter_criteria: FilterCriteria {
            min_level: min_level.name().to_string(),
            since: args.since.clone(),
            component: args.component.clone(),
            max_entries: args.max_entries,
        },
    };

    if args.json {
        output.json(&digest)?;
    } else {
        render_digest_text(&digest, output);
    }

    Ok(())
}

/// Parse duration string like "1h", "30m", "2d"
fn parse_duration(s: &Option<String>) -> Result<Option<Duration>> {
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
        .map_err(|_| AosError::Validation(format!("Invalid duration format: {}", s)))?;

    let duration = match unit {
        "s" => Duration::seconds(num),
        "m" => Duration::minutes(num),
        "h" => Duration::hours(num),
        "d" => Duration::days(num),
        _ => {
            return Err(AosError::Validation(format!(
                "Invalid duration unit '{}'. Use s, m, h, or d",
                unit
            )))
        }
    };

    Ok(Some(duration))
}

/// Collect all log files from a directory recursively
fn collect_log_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut log_files = Vec::new();

    if dir.is_file() {
        if is_log_file(dir) {
            log_files.push(dir.to_path_buf());
        }
        return Ok(log_files);
    }

    let entries =
        fs::read_dir(dir).map_err(|e| AosError::Io(format!("Cannot read directory: {}", e)))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let sub_files = collect_log_files(&path)?;
            log_files.extend(sub_files);
        } else if is_log_file(&path) {
            log_files.push(path);
        }
    }

    // Sort by modification time (most recent first)
    log_files.sort_by(|a, b| {
        let a_modified = fs::metadata(a)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        let b_modified = fs::metadata(b)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        b_modified.cmp(&a_modified)
    });

    Ok(log_files)
}

/// Check if a file is a log file by extension or name
fn is_log_file(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Check by extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "log" | "txt" | "out" | "err" | "ndjson" => return true,
            _ => {}
        }
    }

    // Check by filename patterns
    let lower_name = file_name.to_lowercase();
    lower_name.contains("log")
        || lower_name.contains("error")
        || lower_name.contains("debug")
        || lower_name.contains("trace")
        || lower_name.starts_with("aos-")
        || lower_name.starts_with("adapteros-")
}

/// Parse a log file and extract entries
fn parse_log_file(path: &Path) -> Result<Vec<LogEntry>> {
    let file =
        File::open(path).map_err(|e| AosError::Io(format!("Cannot open log file: {}", e)))?;
    let reader = BufReader::new(file);
    let source_file = path.display().to_string();

    let mut entries = Vec::new();

    for (line_number, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if line.trim().is_empty() {
            continue;
        }

        // Try to parse as JSON first (tracing JSON format)
        if let Some(entry) = try_parse_json_log(&line, &source_file, line_number + 1) {
            entries.push(entry);
            continue;
        }

        // Try to parse as standard text log
        if let Some(entry) = try_parse_text_log(&line, &source_file, line_number + 1) {
            entries.push(entry);
        }
    }

    Ok(entries)
}

/// Try to parse a JSON-formatted log line (tracing JSON format)
fn try_parse_json_log(line: &str, source_file: &str, line_number: usize) -> Option<LogEntry> {
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
        .or_else(|| json.get("ts"))
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let component = json
        .get("target")
        .or_else(|| json.get("component"))
        .or_else(|| json.get("module"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract metadata (everything except standard fields)
    let metadata = {
        let mut meta = json.clone();
        if let Some(obj) = meta.as_object_mut() {
            obj.remove("level");
            obj.remove("message");
            obj.remove("msg");
            obj.remove("timestamp");
            obj.remove("ts");
            obj.remove("target");
            obj.remove("component");
            obj.remove("module");
        }
        if meta.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            None
        } else {
            Some(meta)
        }
    };

    Some(LogEntry {
        timestamp,
        level,
        component,
        message,
        source_file: source_file.to_string(),
        line_number,
        metadata,
    })
}

/// Try to parse a standard text log line
fn try_parse_text_log(line: &str, source_file: &str, line_number: usize) -> Option<LogEntry> {
    // Common patterns:
    // 2024-01-15T10:30:00.000Z INFO component: message
    // [2024-01-15 10:30:00] WARN - message
    // ERROR: message
    // 2024-01-15 10:30:00 | ERROR | component | message

    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Try to extract level
    let level = if line.contains(" ERROR ") || line.starts_with("ERROR") || line.contains("|ERROR|")
    {
        LogLevel::Error
    } else if line.contains(" WARN ") || line.starts_with("WARN") || line.contains("|WARN|") {
        LogLevel::Warn
    } else if line.contains(" INFO ") || line.starts_with("INFO") || line.contains("|INFO|") {
        LogLevel::Info
    } else if line.contains(" DEBUG ") || line.starts_with("DEBUG") || line.contains("|DEBUG|") {
        LogLevel::Debug
    } else if line.contains(" CRITICAL ")
        || line.starts_with("CRITICAL")
        || line.contains("|CRITICAL|")
    {
        LogLevel::Critical
    } else {
        LogLevel::Unknown
    };

    // Try to extract timestamp (ISO 8601 format)
    let timestamp = extract_timestamp(line);

    Some(LogEntry {
        timestamp,
        level,
        component: None,
        message: line.to_string(),
        source_file: source_file.to_string(),
        line_number,
        metadata: None,
    })
}

/// Extract timestamp from a log line
fn extract_timestamp(line: &str) -> Option<DateTime<Utc>> {
    // Try common timestamp patterns
    // ISO 8601: 2024-01-15T10:30:00.000Z
    // With brackets: [2024-01-15 10:30:00]
    // Space separated: 2024-01-15 10:30:00

    // Find potential timestamp substring
    let patterns = [
        // ISO 8601 with T separator
        r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)",
        // Space separated
        r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?)",
    ];

    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(line) {
                if let Some(ts_str) = caps.get(1) {
                    // Try parsing with various formats
                    if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str.as_str()) {
                        return Some(dt.with_timezone(&Utc));
                    }
                    if let Ok(dt) =
                        chrono::NaiveDateTime::parse_from_str(ts_str.as_str(), "%Y-%m-%d %H:%M:%S")
                    {
                        return Some(dt.and_utc());
                    }
                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
                        ts_str.as_str(),
                        "%Y-%m-%d %H:%M:%S%.f",
                    ) {
                        return Some(dt.and_utc());
                    }
                }
            }
        }
    }

    None
}

/// Render digest as text output
fn render_digest_text(digest: &LogDigest, output: &OutputWriter) {
    output.section("Log Digest");

    output.kv("Files scanned", &digest.files_scanned.to_string());
    output.kv("Total entries", &digest.total_entries.to_string());
    output.kv("Filtered entries", &digest.entries.len().to_string());

    if let Some((start, end)) = digest.time_range {
        output.kv(
            "Time range",
            &format!(
                "{} to {}",
                start.format("%Y-%m-%d %H:%M"),
                end.format("%Y-%m-%d %H:%M")
            ),
        );
    }

    // Show level breakdown
    output.section("Entries by Level");
    let mut levels: Vec<_> = digest.counts_by_level.iter().collect();
    levels.sort_by(|a, b| b.1.cmp(a.1));
    for (level, count) in levels {
        output.kv(level, &count.to_string());
    }

    // Show top components
    if !digest.counts_by_component.is_empty() {
        output.section("Top Components");
        let mut components: Vec<_> = digest.counts_by_component.iter().collect();
        components.sort_by(|a, b| b.1.cmp(a.1));
        for (component, count) in components.iter().take(10) {
            output.kv(component, &count.to_string());
        }
    }

    // Show entries
    if !digest.entries.is_empty() {
        output.section("Log Entries");

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(vec!["Time", "Level", "Component", "Message"]);

        for entry in &digest.entries {
            let time_str = entry
                .timestamp
                .map(|ts| ts.format("%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "-".to_string());

            let component = entry.component.as_deref().unwrap_or("-");

            // Truncate message for display
            let message = if entry.message.len() > 80 {
                format!("{}...", &entry.message[..77])
            } else {
                entry.message.clone()
            };

            table.add_row(vec![
                Cell::new(&time_str),
                Cell::new(entry.level.name()),
                Cell::new(component),
                Cell::new(&message),
            ]);
        }

        output
            .table(&table as &dyn std::fmt::Display, None::<&()>)
            .ok();
    } else {
        output.info("No entries matching the filter criteria");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert!(parse_duration(&None).unwrap().is_none());
        assert!(parse_duration(&Some("".to_string())).unwrap().is_none());

        let d = parse_duration(&Some("1h".to_string())).unwrap().unwrap();
        assert_eq!(d.num_hours(), 1);

        let d = parse_duration(&Some("30m".to_string())).unwrap().unwrap();
        assert_eq!(d.num_minutes(), 30);

        let d = parse_duration(&Some("2d".to_string())).unwrap().unwrap();
        assert_eq!(d.num_days(), 2);

        let d = parse_duration(&Some("60s".to_string())).unwrap().unwrap();
        assert_eq!(d.num_seconds(), 60);
    }

    #[test]
    fn test_log_level_parse() {
        assert_eq!(LogLevel::parse("ERROR"), LogLevel::Error);
        assert_eq!(LogLevel::parse("error"), LogLevel::Error);
        assert_eq!(LogLevel::parse("WARN"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("WARNING"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("INFO"), LogLevel::Info);
        assert_eq!(LogLevel::parse("DEBUG"), LogLevel::Debug);
        assert_eq!(LogLevel::parse("CRITICAL"), LogLevel::Critical);
        assert_eq!(LogLevel::parse("unknown_level"), LogLevel::Unknown);
    }

    #[test]
    fn test_try_parse_json_log() {
        let json_line = r#"{"timestamp":"2024-01-15T10:30:00Z","level":"ERROR","message":"Test error","target":"test_component"}"#;
        let entry = try_parse_json_log(json_line, "test.log", 1).unwrap();

        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.message, "Test error");
        assert_eq!(entry.component, Some("test_component".to_string()));
        assert!(entry.timestamp.is_some());
    }

    #[test]
    fn test_try_parse_text_log() {
        let text_line = "2024-01-15 10:30:00 ERROR some error message";
        let entry = try_parse_text_log(text_line, "test.log", 1).unwrap();

        assert_eq!(entry.level, LogLevel::Error);
        assert!(entry.message.contains("ERROR"));
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
        assert!(LogLevel::Error < LogLevel::Critical);
    }

    #[test]
    fn test_is_log_file() {
        assert!(is_log_file(Path::new("test.log")));
        assert!(is_log_file(Path::new("error.txt")));
        assert!(is_log_file(Path::new("aos-server.log")));
        assert!(is_log_file(Path::new("events.ndjson")));
        assert!(!is_log_file(Path::new("config.toml")));
        assert!(!is_log_file(Path::new("data.json")));
    }
}
