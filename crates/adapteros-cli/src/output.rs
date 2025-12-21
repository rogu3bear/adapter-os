//! Output formatting utilities with CI detection
//!
//! Provides consistent output formatting across all CLI commands,
//! with automatic quiet mode when running in CI environments.

// Re-export Table for use by commands
pub use comfy_table::Table;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use std::env;
#[cfg(test)]
use std::sync::{Arc, Mutex};
use tracing::{error, warn};

/// Output mode for CLI commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Standard text output
    Text,
    /// JSON formatted output
    Json,
    /// Quiet mode - minimal output
    Quiet,
}

impl OutputMode {
    /// Detect output mode from environment
    ///
    /// Returns Quiet mode when running in CI environments (detected via standard
    /// CI environment variables), otherwise returns Text mode for interactive use.
    pub fn from_env() -> Self {
        if is_ci() {
            Self::Quiet
        } else {
            Self::Text
        }
    }

    /// Create output mode from flags
    pub fn from_flags(json: bool, quiet: bool) -> Self {
        if json {
            Self::Json
        } else if quiet {
            Self::Quiet
        } else {
            Self::Text
        }
    }

    /// Check if mode is text (verbose)
    pub fn is_verbose(&self) -> bool {
        matches!(self, Self::Text)
    }

    /// Check if mode is quiet
    pub fn is_quiet(&self) -> bool {
        matches!(self, Self::Quiet)
    }

    /// Check if mode is JSON
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Output writer that handles different output formats
#[derive(Debug, Clone)]
pub struct OutputWriter {
    mode: OutputMode,
    verbose: bool,
    #[cfg(test)]
    sink: Option<Arc<Mutex<Vec<String>>>>,
}

impl OutputWriter {
    /// Create a new output writer
    pub fn new(mode: OutputMode, verbose: bool) -> Self {
        Self {
            mode,
            verbose,
            #[cfg(test)]
            sink: None,
        }
    }

    /// Create an output writer that records messages (tests only).
    #[cfg(test)]
    pub fn with_sink(mode: OutputMode, verbose: bool, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            mode,
            verbose,
            sink: Some(sink),
        }
    }

    #[cfg(test)]
    fn record(&self, msg: impl AsRef<str>) {
        if let Some(sink) = &self.sink {
            if let Ok(mut guard) = sink.lock() {
                guard.push(msg.as_ref().to_string());
            }
        }
    }

    /// Get the output mode
    pub fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Check if verbose flag is set
    pub fn is_verbose(&self) -> bool {
        self.verbose || self.mode.is_verbose()
    }

    /// Check if quiet mode
    pub fn is_quiet(&self) -> bool {
        self.mode.is_quiet()
    }

    /// Print a progress message (suppressed in quiet/json mode)
    pub fn progress(&self, msg: impl AsRef<str>) {
        #[cfg(test)]
        self.record(msg.as_ref());
        if self.is_verbose() && !self.mode.is_json() {
            println!("  {}", msg.as_ref());
        }
    }

    /// Print a progress completion message
    pub fn progress_done(&self, success: bool) {
        #[cfg(test)]
        self.record(if success {
            "progress:done"
        } else {
            "progress:failed"
        });
        if self.is_verbose() && !self.mode.is_json() {
            if success {
                println!("  ✓ Done");
            } else {
                println!("  ✗ Failed");
            }
        }
    }

    /// Print verbose message (only in verbose mode)
    pub fn verbose(&self, msg: impl AsRef<str>) {
        #[cfg(test)]
        self.record(msg.as_ref());
        if self.is_verbose() && !self.mode.is_json() {
            println!("  {}", msg.as_ref());
        }
    }

    /// Print a blank line
    pub fn blank(&self) {
        #[cfg(test)]
        self.record("");
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!();
        }
    }

    /// Print a success message (suppressed in quiet/json mode)
    pub fn success(&self, msg: impl AsRef<str>) {
        #[cfg(test)]
        self.record(format!("success:{}", msg.as_ref()));
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("✓ {}", msg.as_ref());
        }
    }

    /// Print a result message (always shown unless JSON)
    pub fn result(&self, msg: impl AsRef<str>) {
        #[cfg(test)]
        self.record(msg.as_ref());
        if !self.mode.is_json() {
            println!("{}", msg.as_ref());
        }
    }

    /// Print an error message (always shown)
    pub fn error(&self, msg: impl AsRef<str>) {
        #[cfg(test)]
        self.record(format!("error:{}", msg.as_ref()));
        error!(message = %msg.as_ref(), "CLI error");
    }

    /// Print a warning message (always shown unless quiet)
    pub fn warning(&self, msg: impl AsRef<str>) {
        #[cfg(test)]
        self.record(format!("warn:{}", msg.as_ref()));
        if !self.mode.is_quiet() {
            warn!(message = %msg.as_ref(), "CLI warning");
        }
    }

    /// Print a fatal error with code and exit
    pub fn fatal_with_code(&mut self, code: &str, msg: &str) -> ! {
        let event_id = self.emit_cli_error(code, msg);
        self.error(format!(
            "{} – see: aosctl explain {} (event: {})",
            msg, code, event_id
        ));
        std::process::exit(20);
    }

    /// Emit CLI error event and return event ID
    fn emit_cli_error(&self, _code: &str, _msg: &str) -> String {
        // Call into mplora-telemetry if linked; else return "-"
        // For now, return placeholder since we can't do async here
        // In a real implementation, this would be handled differently
        "-".to_string()
    }

    /// Print a section header
    pub fn section(&self, title: impl AsRef<str>) {
        let title = title.as_ref();
        #[cfg(test)]
        self.record(format!("section:{}", title));
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("\n🔧 {}", title);
            println!("{}", "─".repeat(title.len() + 3));
        }
    }

    /// Print an info message
    pub fn info(&self, msg: impl AsRef<str>) {
        #[cfg(test)]
        self.record(format!("info:{}", msg.as_ref()));
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("ℹ️  {}", msg.as_ref());
        }
    }

    /// Print key-value pair
    pub fn kv(&self, key: &str, value: &str) {
        #[cfg(test)]
        self.record(format!("{key}:{value}"));
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("  {}: {}", key, value);
        }
    }

    /// Check if output mode is JSON
    pub fn is_json(&self) -> bool {
        self.mode.is_json()
    }

    /// Output JSON data
    pub fn json<T: serde::Serialize>(&self, data: &T) -> Result<(), serde_json::Error> {
        if self.mode.is_json() {
            println!("{}", serde_json::to_string_pretty(data)?);
        }
        Ok(())
    }

    /// Print a simple message
    pub fn print(&self, msg: impl AsRef<str>) {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("{}", msg.as_ref());
        }
    }

    /// Print a line (alias for print that returns Result for consistency)
    pub fn print_line(&self, msg: impl AsRef<str>) -> Result<(), std::io::Error> {
        if !self.mode.is_quiet() && !self.mode.is_json() {
            println!("{}", msg.as_ref());
        }
        Ok(())
    }

    /// Print JSON data (alias for json method)
    pub fn print_json<T: serde::Serialize>(&self, data: &T) -> Result<(), serde_json::Error> {
        println!("{}", serde_json::to_string_pretty(data)?);
        Ok(())
    }

    /// Print a warning message (alias for warning)
    pub fn warn(&self, msg: impl AsRef<str>) -> Result<(), std::io::Error> {
        if !self.mode.is_quiet() {
            warn!(message = %msg.as_ref(), "CLI warning");
        }
        Ok(())
    }

    /// Print a table (human) or JSON (machine)
    pub fn table<T: serde::Serialize>(
        &self,
        table: &dyn std::fmt::Display,
        json_data: Option<&T>,
    ) -> Result<(), serde_json::Error> {
        if self.mode.is_json() {
            if let Some(data) = json_data {
                println!("{}", serde_json::to_string_pretty(data)?);
            }
        } else if !self.mode.is_quiet() {
            println!("{}", table);
        }
        Ok(())
    }
}

/// Create a new table with standard AdapterOS CLI styling.
///
/// This function creates a comfy-table with UTF-8 borders and rounded corners,
/// which is the standard style used throughout the CLI.
///
/// # Example
///
/// ```ignore
/// use adapteros_cli::output::create_styled_table;
///
/// let mut table = create_styled_table();
/// table.set_header(vec!["Name", "Status", "Version"]);
/// table.add_row(vec!["adapter-1", "active", "1.0.0"]);
/// println!("{}", table);
/// ```
pub fn create_styled_table() -> Table {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.apply_modifier(UTF8_ROUND_CORNERS);
    table
}

/// Detect if running in CI environment
///
/// Checks common CI environment variables:
/// - CI (generic, set by most CI systems)
/// - GITHUB_ACTIONS (GitHub Actions)
/// - JENKINS_URL (Jenkins)
/// - CIRCLECI (CircleCI)
/// - TRAVIS (Travis CI)
/// - GITLAB_CI (GitLab CI)
/// - BUILDKITE (Buildkite)
/// - TEAMCITY_VERSION (TeamCity)
/// - BITBUCKET_PIPELINE (Bitbucket Pipelines)
/// - AZURE_PIPELINES (Azure Pipelines)
pub fn is_ci() -> bool {
    env::var("CI")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
        || env::var("GITHUB_ACTIONS").is_ok()
        || env::var("JENKINS_URL").is_ok()
        || env::var("CIRCLECI").is_ok()
        || env::var("TRAVIS").is_ok()
        || env::var("GITLAB_CI").is_ok()
        || env::var("BUILDKITE").is_ok()
        || env::var("TEAMCITY_VERSION").is_ok()
        || env::var("BITBUCKET_BUILD_NUMBER").is_ok()
        || env::var("TF_BUILD").is_ok()
}

/// Print a command header (convenience function for legacy code)
pub fn command_header(mode: &OutputMode, title: &str) {
    if !mode.is_quiet() && !mode.is_json() {
        println!("\n🔧 {}", title);
        println!("{}", "─".repeat(title.len() + 3));
    }
}

/// Print a progress message (convenience function for legacy code)
pub fn progress(mode: &OutputMode, msg: &str) {
    if mode.is_verbose() && !mode.is_json() {
        println!("  {}", msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ci_detection() {
        // This will vary based on test environment
        // Just ensure it doesn't panic
        let _ = is_ci();
    }

    #[test]
    fn test_output_mode_text() {
        let mode = OutputMode::Text;
        assert!(mode.is_verbose());
        assert!(!mode.is_quiet());
        assert!(!mode.is_json());
    }

    #[test]
    fn test_output_mode_quiet() {
        let mode = OutputMode::Quiet;
        assert!(!mode.is_verbose());
        assert!(mode.is_quiet());
        assert!(!mode.is_json());
    }

    #[test]
    fn test_output_mode_json() {
        let mode = OutputMode::Json;
        assert!(!mode.is_verbose());
        assert!(!mode.is_quiet());
        assert!(mode.is_json());
    }
}
