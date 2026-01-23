//! Progress indicators for CLI operations
//!
//! Provides spinners for indeterminate operations (HTTP requests, connecting)
//! and progress bars for determinate operations (training, batch processing).
//!
//! All progress indicators respect OutputWriter modes:
//! - Text mode: Full progress display with animations
//! - JSON mode: No output (silent)
//! - Quiet mode: No output (silent)

use crate::output::OutputWriter;
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use std::borrow::Cow;
use std::time::Duration;

/// Pre-defined templates for common operations
#[derive(Debug, Clone)]
pub enum ProgressTemplate {
    /// Training progress: [00:15:32] 42/100 epochs (loss: 0.0234)
    Training,
    /// Deployment: [00:01:23] 3/5 adapters (deploying: python-lora)
    Deployment,
    /// Verification: [00:00:45] 150/500 files (validating hashes)
    Verification,
    /// Generic items: [00:00:12] 42/100 items
    Items,
    /// Custom template string (indicatif format)
    Custom(String),
}

impl ProgressTemplate {
    fn to_style(&self) -> ProgressStyle {
        // Default fallback template if custom template is invalid
        const FALLBACK_TEMPLATE: &str =
            "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({msg})";

        let template = match self {
            Self::Training => {
                "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} epochs ({msg})"
            }
            Self::Deployment => {
                "{spinner:.green} [{elapsed_precise}] {bar:40.green/blue} {pos}/{len} adapters ({msg})"
            }
            Self::Verification => {
                "{spinner:.yellow} [{elapsed_precise}] {bar:40.yellow/blue} {pos}/{len} files ({msg})"
            }
            Self::Items => {
                "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} items ({msg})"
            }
            Self::Custom(t) => t.as_str(),
        };

        // Try the requested template, fall back to default if invalid
        ProgressStyle::with_template(template)
            .or_else(|_| ProgressStyle::with_template(FALLBACK_TEMPLATE))
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=>-")
    }
}

/// Spinner for indeterminate operations (HTTP requests, connecting, etc.)
///
/// Automatically disabled in JSON/Quiet modes. Provides animated feedback
/// for operations where the total duration is unknown.
///
/// # Example
///
/// ```ignore
/// let spinner = Spinner::new(&output, "Connecting to server...");
/// let result = client.connect().await;
/// match result {
///     Ok(_) => spinner.finish_success("Connected"),
///     Err(e) => spinner.finish_error(&format!("Connection failed: {}", e)),
/// }
/// ```
pub struct Spinner {
    pb: Option<ProgressBar>,
}

impl Spinner {
    /// Create a new spinner with the given message.
    ///
    /// Returns a no-op spinner if output mode is JSON or Quiet.
    pub fn new(output: &OutputWriter, message: impl Into<String>) -> Self {
        if output.is_json() || output.is_quiet() {
            return Self { pb: None };
        }

        let pb = ProgressBar::new_spinner();
        let style = ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);
        pb.set_style(style);
        pb.set_message(message.into());
        pb.enable_steady_tick(Duration::from_millis(80));

        Self { pb: Some(pb) }
    }

    /// Update the spinner message.
    pub fn set_message(&self, msg: impl Into<Cow<'static, str>>) {
        if let Some(pb) = &self.pb {
            pb.set_message(msg);
        }
    }

    /// Finish with a success message (green checkmark).
    pub fn finish_success(&self, msg: impl AsRef<str>) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
            println!("{} {}", "✓".green(), msg.as_ref());
        }
    }

    /// Finish with an error message (red X).
    pub fn finish_error(&self, msg: impl AsRef<str>) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
            eprintln!("{} {}", "✗".red(), msg.as_ref().red());
        }
    }

    /// Finish with a warning message (yellow warning).
    pub fn finish_warning(&self, msg: impl AsRef<str>) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
            eprintln!("{} {}", "⚠".yellow(), msg.as_ref().yellow());
        }
    }

    /// Finish and clear without any message.
    pub fn finish_and_clear(&self) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
        }
    }

    /// Finish with a custom message (no prefix).
    pub fn finish(&self, msg: impl AsRef<str>) {
        if let Some(pb) = &self.pb {
            pb.finish_with_message(msg.as_ref().to_string());
        }
    }
}

/// Progress bar for determinate operations (training, batch processing, etc.)
///
/// Automatically disabled in JSON/Quiet modes. Provides visual feedback
/// for operations where the total count is known.
///
/// # Example
///
/// ```ignore
/// let progress = Progress::new(&output, adapters.len() as u64, ProgressTemplate::Deployment);
/// for adapter in adapters {
///     progress.set_message(format!("deploying: {}", adapter.name));
///     deploy_adapter(&adapter).await?;
///     progress.inc(1);
/// }
/// progress.finish("Deployment complete");
/// ```
pub struct Progress {
    pb: Option<ProgressBar>,
}

impl Progress {
    /// Create a new progress bar for the given total count.
    ///
    /// Returns a no-op progress bar if output mode is JSON or Quiet.
    pub fn new(output: &OutputWriter, total: u64, template: ProgressTemplate) -> Self {
        if output.is_json() || output.is_quiet() {
            return Self { pb: None };
        }

        let pb = ProgressBar::new(total);
        pb.set_style(template.to_style());
        pb.enable_steady_tick(Duration::from_millis(100));

        Self { pb: Some(pb) }
    }

    /// Increment the progress by the given amount.
    pub fn inc(&self, delta: u64) {
        if let Some(pb) = &self.pb {
            pb.inc(delta);
        }
    }

    /// Set the current position.
    pub fn set_position(&self, pos: u64) {
        if let Some(pb) = &self.pb {
            pb.set_position(pos);
        }
    }

    /// Update the progress message.
    pub fn set_message(&self, msg: impl Into<Cow<'static, str>>) {
        if let Some(pb) = &self.pb {
            pb.set_message(msg);
        }
    }

    /// Update the total count (useful when the total is discovered during processing).
    pub fn set_length(&self, len: u64) {
        if let Some(pb) = &self.pb {
            pb.set_length(len);
        }
    }

    /// Finish with a success message.
    pub fn finish(&self, msg: impl AsRef<str>) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
            println!("{} {}", "✓".green(), msg.as_ref());
        }
    }

    /// Finish with an error message.
    pub fn finish_error(&self, msg: impl AsRef<str>) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
            eprintln!("{} {}", "✗".red(), msg.as_ref().red());
        }
    }

    /// Finish and clear without any message.
    pub fn finish_and_clear(&self) {
        if let Some(pb) = &self.pb {
            pb.finish_and_clear();
        }
    }

    /// Abandon the progress bar (for error cases where you want to preserve output).
    pub fn abandon(&self) {
        if let Some(pb) = &self.pb {
            pb.abandon();
        }
    }

    /// Check if progress is active (not in JSON/Quiet mode).
    pub fn is_active(&self) -> bool {
        self.pb.is_some()
    }
}

/// Multi-step progress tracker for operations with distinct phases.
///
/// Useful for training workflows that have multiple stages:
/// data loading, training, validation, export, etc.
///
/// # Example
///
/// ```ignore
/// let steps = MultiStepProgress::new(&output, &["Loading data", "Training", "Validation", "Export"]);
/// steps.start_step(0)?;
/// load_data().await?;
/// steps.complete_step(0);
/// steps.start_step(1)?;
/// // ... training loop with inner progress ...
/// ```
pub struct MultiStepProgress {
    steps: Vec<String>,
    current: Option<usize>,
    output_active: bool,
}

impl MultiStepProgress {
    /// Create a new multi-step progress tracker.
    pub fn new(output: &OutputWriter, steps: &[&str]) -> Self {
        let output_active = !output.is_json() && !output.is_quiet();

        if output_active {
            println!();
            for (i, step) in steps.iter().enumerate() {
                println!("  {} {}", format!("[{}]", i + 1).dimmed(), step.dimmed());
            }
            println!();
        }

        Self {
            steps: steps.iter().map(|s| s.to_string()).collect(),
            current: None,
            output_active,
        }
    }

    /// Start a step (0-indexed).
    pub fn start_step(&mut self, index: usize) {
        if !self.output_active || index >= self.steps.len() {
            return;
        }

        self.current = Some(index);
        println!(
            "  {} {}",
            format!("[{}]", index + 1).cyan(),
            self.steps[index]
        );
    }

    /// Complete the current step with success.
    pub fn complete_step(&mut self, index: usize) {
        if !self.output_active || index >= self.steps.len() {
            return;
        }

        println!(
            "  {} {} {}",
            "✓".green(),
            format!("[{}]", index + 1).green(),
            self.steps[index].green()
        );
    }

    /// Mark a step as failed.
    pub fn fail_step(&mut self, index: usize, error: &str) {
        if !self.output_active || index >= self.steps.len() {
            return;
        }

        eprintln!(
            "  {} {} {} - {}",
            "✗".red(),
            format!("[{}]", index + 1).red(),
            self.steps[index].red(),
            error.red()
        );
    }

    /// Get the current step index.
    pub fn current_step(&self) -> Option<usize> {
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputMode;

    #[test]
    fn test_spinner_noop_in_json_mode() {
        let output = OutputWriter::new(OutputMode::Json, false);
        let spinner = Spinner::new(&output, "test");
        assert!(spinner.pb.is_none());
    }

    #[test]
    fn test_spinner_noop_in_quiet_mode() {
        let output = OutputWriter::new(OutputMode::Quiet, false);
        let spinner = Spinner::new(&output, "test");
        assert!(spinner.pb.is_none());
    }

    #[test]
    fn test_progress_noop_in_json_mode() {
        let output = OutputWriter::new(OutputMode::Json, false);
        let progress = Progress::new(&output, 100, ProgressTemplate::Items);
        assert!(progress.pb.is_none());
        assert!(!progress.is_active());
    }

    #[test]
    fn test_progress_noop_in_quiet_mode() {
        let output = OutputWriter::new(OutputMode::Quiet, false);
        let progress = Progress::new(&output, 100, ProgressTemplate::Items);
        assert!(progress.pb.is_none());
    }

    #[test]
    fn test_progress_template_styles() {
        // Ensure all templates compile without panic
        let _ = ProgressTemplate::Training.to_style();
        let _ = ProgressTemplate::Deployment.to_style();
        let _ = ProgressTemplate::Verification.to_style();
        let _ = ProgressTemplate::Items.to_style();
        let _ = ProgressTemplate::Custom("{msg}".to_string()).to_style();
    }

    #[test]
    fn test_multi_step_progress_bounds() {
        let output = OutputWriter::new(OutputMode::Quiet, false);
        let mut steps = MultiStepProgress::new(&output, &["Step 1", "Step 2"]);

        // Should not panic on out-of-bounds
        steps.start_step(5);
        steps.complete_step(5);
        steps.fail_step(5, "error");
    }
}
