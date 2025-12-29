//! Auto-fix implementations for preflight checks
//!
//! Provides safe, automated fixes for common pre-flight issues with
//! three-tier safety classification system.
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Safety level for auto-fix operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixSafety {
    /// Safe to execute without user confirmation
    Safe,
    /// Requires user confirmation before execution
    RequiresConfirm,
    /// Cannot be auto-fixed (manual intervention required)
    Unsafe,
}

/// Auto-fix mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixMode {
    /// Ask for confirmation before fixes that require it
    Interactive,
    /// Only execute safe fixes automatically
    SafeOnly,
    /// Execute all fixable operations without confirmation (dangerous)
    Force,
}

/// A fixable issue with associated fix function
type FixFn = Box<dyn FnOnce(&OutputWriter) -> Result<()>>;

pub struct FixableIssue {
    pub check_name: String,
    pub issue: String,
    pub fix_description: String,
    pub safety: FixSafety,
    pub fix_fn: FixFn,
}

impl FixableIssue {
    pub fn new<F>(
        check_name: String,
        issue: String,
        fix_description: String,
        safety: FixSafety,
        fix_fn: F,
    ) -> Self
    where
        F: FnOnce(&OutputWriter) -> Result<()> + 'static,
    {
        Self {
            check_name,
            issue,
            fix_description,
            safety,
            fix_fn: Box::new(fix_fn),
        }
    }
}

/// Auto-fixer for preflight issues
pub struct AutoFixer {
    mode: FixMode,
    output: OutputWriter,
}

impl AutoFixer {
    pub fn new(mode: FixMode, output: OutputWriter) -> Self {
        Self { mode, output }
    }

    /// Attempt to fix an issue based on safety level and mode
    pub fn try_fix(&mut self, issue: FixableIssue) -> Result<bool> {
        match (self.mode, issue.safety) {
            // Always skip unsafe operations
            (_, FixSafety::Unsafe) => {
                self.output.warning(format!(
                    "⚠️  {} requires manual intervention",
                    issue.check_name
                ));
                self.output.info(format!("   {}", issue.fix_description));
                Ok(false)
            }

            // Safe operations: execute in all modes
            (_, FixSafety::Safe) => {
                self.output.info(format!("🔧 Fixing: {}", issue.check_name));
                self.output.info(format!("   {}", issue.fix_description));

                match (issue.fix_fn)(&self.output) {
                    Ok(()) => {
                        self.output
                            .success(format!("✅ Fixed: {}", issue.check_name));
                        Ok(true)
                    }
                    Err(e) => {
                        self.output
                            .error(format!("❌ Failed to fix {}: {}", issue.check_name, e));
                        Ok(false)
                    }
                }
            }

            // Confirmation required: depends on mode
            (FixMode::Force, FixSafety::RequiresConfirm) => {
                self.output.warning(format!(
                    "🔧 Force-fixing: {} (no confirmation)",
                    issue.check_name
                ));
                self.output.info(format!("   {}", issue.fix_description));

                match (issue.fix_fn)(&self.output) {
                    Ok(()) => {
                        self.output
                            .success(format!("✅ Fixed: {}", issue.check_name));
                        Ok(true)
                    }
                    Err(e) => {
                        self.output
                            .error(format!("❌ Failed to fix {}: {}", issue.check_name, e));
                        Ok(false)
                    }
                }
            }

            (FixMode::Interactive, FixSafety::RequiresConfirm) => {
                self.output
                    .warning(format!("⚠️  {} requires confirmation", issue.check_name));
                self.output.info(format!("   {}", issue.fix_description));

                // Ask for confirmation
                use dialoguer::Confirm;
                let confirmed = Confirm::new()
                    .with_prompt(format!("Fix {}?", issue.check_name))
                    .default(false)
                    .interact()
                    .unwrap_or(false);

                if !confirmed {
                    self.output.info("   Skipped");
                    return Ok(false);
                }

                match (issue.fix_fn)(&self.output) {
                    Ok(()) => {
                        self.output
                            .success(format!("✅ Fixed: {}", issue.check_name));
                        Ok(true)
                    }
                    Err(e) => {
                        self.output
                            .error(format!("❌ Failed to fix {}: {}", issue.check_name, e));
                        Ok(false)
                    }
                }
            }

            (FixMode::SafeOnly, FixSafety::RequiresConfirm) => {
                self.output.warning(format!(
                    "⚠️  {} requires confirmation (use --fix without --safe-only)",
                    issue.check_name
                ));
                self.output.info(format!("   {}", issue.fix_description));
                Ok(false)
            }
        }
    }
}

// =============================================================================
// Safe Fix Implementations
// =============================================================================

/// Create missing directories (SAFE)
pub fn create_directories(dirs: &[&str]) -> FixableIssue {
    let dirs_vec: Vec<String> = dirs.iter().map(|s| s.to_string()).collect();

    FixableIssue::new(
        "Missing Directories".to_string(),
        format!("Missing required directories: {}", dirs.join(", ")),
        format!("Create directories: {}", dirs.join(", ")),
        FixSafety::Safe,
        move |output| {
            for dir in &dirs_vec {
                let path = Path::new(dir);
                if !path.exists() {
                    std::fs::create_dir_all(path)
                        .map_err(|e| AosError::Io(format!("Failed to create {}: {}", dir, e)))?;
                    output.info(format!("   Created: {}", dir));
                }
            }
            Ok(())
        },
    )
}

/// Create .env from .env.example if missing (SAFE)
pub fn create_env_from_example() -> FixableIssue {
    FixableIssue::new(
        ".env File".to_string(),
        "Missing .env file".to_string(),
        "Copy .env.example to .env".to_string(),
        FixSafety::Safe,
        |output| {
            let env_path = Path::new(".env");
            let example_path = Path::new(".env.example");

            if env_path.exists() {
                output.info("   .env already exists");
                return Ok(());
            }

            if !example_path.exists() {
                return Err(AosError::Config(
                    ".env.example not found - cannot create .env".to_string(),
                ));
            }

            std::fs::copy(example_path, env_path)
                .map_err(|e| AosError::Io(format!("Failed to copy .env.example: {}", e)))?;

            output.info("   Created .env from .env.example");
            output.warning("   ⚠️  Please review and update .env with your settings");
            Ok(())
        },
    )
}

// =============================================================================
// Confirmation-Required Fix Implementations
// =============================================================================

/// Run database migrations (REQUIRES CONFIRMATION)
pub fn run_database_migrations(db_path: String) -> FixableIssue {
    FixableIssue::new(
        "Database Migrations".to_string(),
        "Database not initialized or migrations not applied".to_string(),
        format!("Run migrations on {}", db_path),
        FixSafety::RequiresConfirm,
        move |output| {
            output.info("   Running database migrations...");

            // Run migrations using the CLI command
            let status = Command::new("cargo")
                .args(["run", "-p", "adapteros-cli", "--", "db", "migrate"])
                .status()
                .map_err(|e| AosError::Database(format!("Failed to run migrations: {}", e)))?;

            if !status.success() {
                return Err(AosError::Database("Migration failed".to_string()));
            }

            output.info("   Migrations completed");
            Ok(())
        },
    )
}

/// Repair bootstrap state (REQUIRES CONFIRMATION)
pub fn run_bootstrap_repair(db_path: String) -> FixableIssue {
    FixableIssue::new(
        "Bootstrap State".to_string(),
        "Bootstrap records missing or incomplete".to_string(),
        format!("Repair bootstrap state in {}", db_path),
        FixSafety::RequiresConfirm,
        move |output| {
            output.info("   Repairing bootstrap state...");

            let mut command = Command::new("cargo");
            command.args(["run", "-p", "adapteros-cli", "--", "db", "repair-bootstrap"]);
            if !db_path.trim().is_empty() {
                command.args(["--db-path", db_path.as_str()]);
            }

            let status = command
                .status()
                .map_err(|e| AosError::Database(format!("Failed to repair bootstrap: {}", e)))?;

            if !status.success() {
                return Err(AosError::Database("Bootstrap repair failed".to_string()));
            }

            output.info("   Bootstrap repair completed");
            Ok(())
        },
    )
}

/// Download model (REQUIRES CONFIRMATION - large download)
pub fn download_model(model_path: PathBuf) -> FixableIssue {
    FixableIssue::new(
        "Model Download".to_string(),
        format!("Model not found at {}", model_path.display()),
        "Download Qwen 2.5 7B Instruct model (~3.8GB)".to_string(),
        FixSafety::RequiresConfirm,
        |output| {
            output.warning("   ⚠️  This will download ~3.8GB of data");
            output.info("   Running download script...");

            let script_path = Path::new("./scripts/download-model.sh");
            if !script_path.exists() {
                return Err(AosError::Config(
                    "Download script not found: ./scripts/download-model.sh".to_string(),
                ));
            }

            let status = Command::new(script_path)
                .status()
                .map_err(|e| AosError::Internal(format!("Failed to run download script: {}", e)))?;

            if !status.success() {
                return Err(AosError::Internal("Model download failed".to_string()));
            }

            output.success("   Model downloaded successfully");
            Ok(())
        },
    )
}

/// Initialize default tenant (REQUIRES CONFIRMATION)
pub fn create_default_tenant() -> FixableIssue {
    FixableIssue::new(
        "Default Tenant".to_string(),
        "No tenants configured".to_string(),
        "Create default tenant with uid=1000, gid=1000".to_string(),
        FixSafety::RequiresConfirm,
        |output| {
            output.info("   Creating default tenant...");

            let status = Command::new("cargo")
                .args([
                    "run",
                    "-p",
                    "adapteros-cli",
                    "--",
                    "init-tenant",
                    "--id",
                    "default",
                    "--uid",
                    "1000",
                    "--gid",
                    "1000",
                ])
                .status()
                .map_err(|e| AosError::Http(format!("Failed to create tenant: {}", e)))?;

            if !status.success() {
                return Err(AosError::Internal("Tenant creation failed".to_string()));
            }

            output.info("   Default tenant created");
            Ok(())
        },
    )
}

// =============================================================================
// Unsafe Fix Implementations (Manual Only)
// =============================================================================

/// Install Xcode Command Line Tools (UNSAFE - requires sudo)
pub fn install_xcode_cli_tools() -> FixableIssue {
    FixableIssue::new(
        "Xcode CLI Tools".to_string(),
        "Xcode Command Line Tools not installed".to_string(),
        "Install Xcode Command Line Tools: xcode-select --install".to_string(),
        FixSafety::Unsafe,
        |_output| {
            // This should never be called due to Unsafe safety level
            Err(AosError::Config(
                "Xcode CLI Tools must be installed manually".to_string(),
            ))
        },
    )
}

/// Install MLX library (UNSAFE - requires pip/brew)
pub fn install_mlx_library() -> FixableIssue {
    FixableIssue::new(
        "MLX Library".to_string(),
        "MLX library not installed".to_string(),
        "Install MLX library: pip install mlx (optional)".to_string(),
        FixSafety::Unsafe,
        |_output| {
            // This should never be called due to Unsafe safety level
            Err(AosError::Config(
                "MLX must be installed manually".to_string(),
            ))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fix_safety_levels() {
        assert_eq!(FixSafety::Safe, FixSafety::Safe);
        assert_ne!(FixSafety::Safe, FixSafety::Unsafe);
    }

    #[test]
    fn test_fix_mode() {
        assert_eq!(FixMode::Interactive, FixMode::Interactive);
        assert_ne!(FixMode::SafeOnly, FixMode::Force);
    }
}
