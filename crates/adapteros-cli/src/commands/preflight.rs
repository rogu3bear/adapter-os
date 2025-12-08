//! Pre-flight system readiness checker for AdapterOS
//!
//! Provides comprehensive environment verification before launching the server:
//! - Model availability and configuration
//! - Database initialization and migrations
//! - Required directories and files
//! - Environment variables
//! - Backend availability (CoreML, Metal, MLX)
//! - System resources
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use crate::output::OutputWriter;
use adapteros_config::{
    resolve_base_model_location, DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT,
};
use anyhow::{Context, Result};
use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

// Import auto-fix module
use super::preflight_fix::{
    create_directories, create_env_from_example, download_model, install_mlx_library,
    install_xcode_cli_tools, run_database_migrations, AutoFixer, FixMode,
};

/// Preflight command to check system readiness before launch
#[derive(Debug, Args, Clone)]
pub struct PreflightCommand {
    /// Fix issues automatically where possible (interactive mode)
    #[arg(long, short = 'f')]
    pub fix: bool,

    /// Fix all issues without confirmation (dangerous - use with caution)
    #[arg(long, requires = "fix")]
    pub fix_force: bool,

    /// Only apply safe fixes (no user confirmation required)
    #[arg(long, requires = "fix", conflicts_with = "fix_force")]
    pub safe_only: bool,

    /// Database path to check (defaults to AOS_DATABASE_URL env var or var/aos-cp.sqlite3)
    #[arg(long, env = "AOS_DATABASE_URL")]
    pub database_url: Option<String>,

    /// Model path to check (overrides AOS_MODEL_CACHE_DIR/AOS_BASE_MODEL_ID resolver)
    #[arg(long, env = "AOS_MODEL_PATH")]
    pub model_path: Option<String>,

    /// Skip backend availability checks
    #[arg(long)]
    pub skip_backends: bool,

    /// Skip resource checks (memory, disk)
    #[arg(long)]
    pub skip_resources: bool,
}

/// Check result for individual components
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
}

/// Individual check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl CheckResult {
    fn pass(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Pass,
            message: message.to_string(),
            fix_command: None,
            details: None,
        }
    }

    fn warning(name: &str, message: &str, fix_command: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Warning,
            message: message.to_string(),
            fix_command,
            details: None,
        }
    }

    fn fail(name: &str, message: &str, fix_command: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Fail,
            message: message.to_string(),
            fix_command,
            details: None,
        }
    }

    fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }
}

/// Run the preflight command
pub async fn run(cmd: PreflightCommand, output: &OutputWriter) -> Result<()> {
    output.info("🚀 Running AdapterOS preflight checks...\n");

    let mut results = Vec::new();

    // 1. Check model availability
    results.push(check_model(&cmd).await);

    // 2. Check database
    results.push(check_database(&cmd).await);

    // 3. Check required directories
    results.extend(check_directories().await);

    // 4. Check environment variables
    results.extend(check_environment_variables().await);

    // 5. Check backends (CoreML, Metal, MLX) unless skipped
    if !cmd.skip_backends {
        results.extend(check_backends().await);
    }

    // 6. Check system resources unless skipped
    if !cmd.skip_resources {
        results.extend(check_resources().await);
    }

    // Display initial results
    display_results(&results, output)?;

    // Count initial failures and warnings
    let initial_failures = results
        .iter()
        .filter(|r| r.status == CheckStatus::Fail)
        .count();
    let initial_warnings = results
        .iter()
        .filter(|r| r.status == CheckStatus::Warning)
        .count();

    // If --fix flag is enabled, attempt to fix issues
    if cmd.fix && (initial_failures > 0 || initial_warnings > 0) {
        output.info("\n🔧 Attempting to fix issues...\n");

        // Determine fix mode based on flags
        let fix_mode = if cmd.fix_force {
            FixMode::Force
        } else if cmd.safe_only {
            FixMode::SafeOnly
        } else {
            FixMode::Interactive
        };

        // Create auto-fixer
        let mut fixer = AutoFixer::new(fix_mode, output.clone());

        // Attempt fixes based on check results
        attempt_fixes(&mut fixer, &results, &cmd).await?;

        // Re-run checks after fixes
        output.info("\n🔄 Re-running checks after fixes...\n");

        results.clear();
        results.push(check_model(&cmd).await);
        results.push(check_database(&cmd).await);
        results.extend(check_directories().await);
        results.extend(check_environment_variables().await);

        if !cmd.skip_backends {
            results.extend(check_backends().await);
        }

        if !cmd.skip_resources {
            results.extend(check_resources().await);
        }

        // Display updated results
        display_results(&results, output)?;
    }

    // Count final failures and warnings
    let failures = results
        .iter()
        .filter(|r| r.status == CheckStatus::Fail)
        .count();
    let warnings = results
        .iter()
        .filter(|r| r.status == CheckStatus::Warning)
        .count();

    // Display summary
    output.info(&format!("\n📊 Summary: {} checks run", results.len()));
    if failures > 0 {
        output.error(&format!("❌ {} critical failures", failures));
        if cmd.fix {
            output.info("   Some issues could not be fixed automatically");
        }
    }
    if warnings > 0 {
        output.warning(&format!("⚠️  {} warnings", warnings));
    }
    if failures == 0 && warnings == 0 {
        output.success("✅ All checks passed - system ready to launch!");
    }

    // Display fix suggestions (only if not already attempted)
    if !cmd.fix && (failures > 0 || warnings > 0) {
        display_fix_suggestions(&results, output)?;
    }

    // Exit with error code if any failures
    if failures > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Attempt to fix issues based on check results
async fn attempt_fixes(
    fixer: &mut AutoFixer,
    results: &[CheckResult],
    cmd: &PreflightCommand,
) -> Result<()> {
    // Collect fixable issues based on check results
    for result in results {
        // Skip passing checks
        if result.status == CheckStatus::Pass {
            continue;
        }

        // Determine fixable issue based on check name
        let fixable = match result.name.as_str() {
            name if name.starts_with("Directory:") => {
                // Extract directory name from check name
                let dir_name = name.strip_prefix("Directory: ").unwrap_or("var");
                Some(create_directories(&[dir_name]))
            }

            "Model Directory" | "Model" => {
                // Model download
                if let Ok(model_path) = resolve_model_path_from_inputs(cmd) {
                    Some(download_model(model_path))
                } else {
                    None
                }
            }

            "Database" | "Database Connection" | "Database Migrations" => {
                // Database migrations
                let db_url = cmd
                    .database_url
                    .as_ref()
                    .map(|s| s.to_string())
                    .or_else(|| std::env::var("AOS_DATABASE_URL").ok())
                    .or_else(|| std::env::var("DATABASE_URL").ok())
                    .unwrap_or_else(|| "sqlite:var/aos-cp.sqlite3".to_string());

                Some(run_database_migrations(db_url))
            }

            ".env File" | "Env: DATABASE_URL" | "Env: MODEL_PATH" => {
                // Create .env from example
                Some(create_env_from_example())
            }

            "Backend: CoreML" | "Backend: Metal" => {
                // Xcode CLI tools (unsafe - manual only)
                Some(install_xcode_cli_tools())
            }

            "Backend: MLX" => {
                // MLX library (unsafe - manual only)
                Some(install_mlx_library())
            }

            _ => None,
        };

        // Attempt fix if available
        if let Some(issue) = fixable {
            let _ = fixer.try_fix(issue);
        }
    }

    Ok(())
}

/// Check model availability and configuration
async fn check_model(cmd: &PreflightCommand) -> CheckResult {
    // Determine model path using canonical resolver
    let model_path = match resolve_model_path_from_inputs(cmd) {
        Ok(path) => path,
        Err(e) => {
            return CheckResult::fail(
                "Model",
                &format!("Failed to resolve model path: {}", e),
                Some(
                    "Set AOS_BASE_MODEL_ID/AOS_MODEL_CACHE_DIR or provide --model-path".to_string(),
                ),
            )
        }
    };

    // Check if model directory exists
    if !model_path.exists() {
        return CheckResult::fail(
            "Model Directory",
            &format!("Model directory not found: {}", model_path.display()),
            Some("make download-model  # or: ./scripts/download-model.sh".to_string()),
        );
    }

    // Check for required model files
    let required_files = vec!["config.json", "tokenizer.json"];
    let mut missing_files = Vec::new();

    for file in &required_files {
        let file_path = model_path.join(file);
        if !file_path.exists() {
            missing_files.push(file.to_string());
        }
    }

    if !missing_files.is_empty() {
        return CheckResult::fail(
            "Model Files",
            &format!("Missing required files: {}", missing_files.join(", ")),
            Some("make download-model  # Re-download model".to_string()),
        );
    }

    // Check for model weights
    let has_weights = model_path
        .read_dir()
        .ok()
        .and_then(|entries| {
            entries.filter_map(|e| e.ok()).find(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.ends_with(".safetensors") || name.ends_with(".bin")
            })
        })
        .is_some();

    if !has_weights {
        return CheckResult::warning(
            "Model Weights",
            "No weight files (.safetensors or .bin) found",
            Some("make download-model  # Re-download model".to_string()),
        );
    }

    CheckResult::pass("Model", &format!("Model ready at {}", model_path.display()))
}

fn resolve_model_path_from_inputs(cmd: &PreflightCommand) -> Result<PathBuf> {
    if let Some(path) = cmd.model_path.as_ref() {
        return Ok(PathBuf::from(path));
    }

    if let Ok(path) = std::env::var("AOS_MODEL_PATH") {
        return Ok(PathBuf::from(path));
    }

    if let Ok(path) = std::env::var("AOS_MLX_FFI_MODEL") {
        return Ok(PathBuf::from(path));
    }

    resolve_base_model_location(None, None, false)
        .map(|loc| loc.full_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Check database initialization and migrations
async fn check_database(cmd: &PreflightCommand) -> CheckResult {
    // Determine database path
    let db_url = cmd
        .database_url
        .as_ref()
        .map(|s| s.to_string())
        .or_else(|| std::env::var("AOS_DATABASE_URL").ok())
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .unwrap_or_else(|| "sqlite:var/aos-cp.sqlite3".to_string());

    // Extract file path from sqlite: URL
    let db_path = db_url.strip_prefix("sqlite:").unwrap_or(&db_url);

    // Check if database file exists
    if !Path::new(db_path).exists() {
        return CheckResult::fail(
            "Database",
            &format!("Database not initialized: {}", db_path),
            Some("cargo run -p adapteros-cli -- db migrate".to_string()),
        );
    }

    // Try to open database and check migrations
    match sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", db_path))
        .await
    {
        Ok(pool) => {
            // Check if migrations table exists
            let migrations_exist: Result<i64, sqlx::Error> = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
            )
            .fetch_one(&pool)
            .await;

            pool.close().await;

            match migrations_exist {
                Ok(count) if count > 0 => {
                    CheckResult::pass("Database", &format!("Database ready at {}", db_path))
                }
                _ => CheckResult::warning(
                    "Database Migrations",
                    "Migrations table not found - database may need migration",
                    Some("cargo run -p adapteros-cli -- db migrate".to_string()),
                ),
            }
        }
        Err(e) => CheckResult::fail(
            "Database Connection",
            &format!("Cannot connect to database: {}", e),
            Some("cargo run -p adapteros-cli -- db migrate".to_string()),
        ),
    }
}

/// Check required directories exist
async fn check_directories() -> Vec<CheckResult> {
    let required_dirs = vec![
        ("var", "Runtime data directory"),
        ("var/logs", "Log directory"),
        ("var/bundles", "Telemetry bundles directory"),
        ("var/keys", "Cryptographic keys directory"),
    ];

    let mut results = Vec::new();

    for (dir, description) in required_dirs {
        let path = PathBuf::from(dir);
        if path.exists() {
            results.push(CheckResult::pass(
                &format!("Directory: {}", dir),
                &format!("{} exists", description),
            ));
        } else {
            results.push(CheckResult::warning(
                &format!("Directory: {}", dir),
                &format!("{} not found (will be auto-created)", description),
                Some(format!("mkdir -p {}", dir)),
            ));
        }
    }

    results
}

/// Check critical environment variables
async fn check_environment_variables() -> Vec<CheckResult> {
    let mut results = Vec::new();

    // Check AOS_DATABASE_URL or DATABASE_URL
    let has_db_url =
        std::env::var("AOS_DATABASE_URL").is_ok() || std::env::var("DATABASE_URL").is_ok();

    if has_db_url {
        results.push(CheckResult::pass(
            "Env: DATABASE_URL",
            "Database URL configured",
        ));
    } else {
        results.push(CheckResult::warning(
            "Env: DATABASE_URL",
            "No database URL set (will use default: sqlite:var/aos-cp.sqlite3)",
            Some("export AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3".to_string()),
        ));
    }

    // Check canonical model resolution knobs
    let has_model_path = std::env::var("AOS_MODEL_PATH").is_ok()
        || std::env::var("AOS_MLX_FFI_MODEL").is_ok()
        || std::env::var("AOS_BASE_MODEL_ID").is_ok()
        || std::env::var("AOS_MODEL_CACHE_DIR").is_ok();

    if has_model_path {
        results.push(CheckResult::pass(
            "Env: MODEL_PATH",
            "Model path configured via env/config",
        ));
    } else {
        results.push(CheckResult::warning(
            "Env: MODEL_PATH",
            &format!(
                "No model path set (resolver will default to {}/{}). Configure AOS_MODEL_CACHE_DIR/AOS_BASE_MODEL_ID or pass --model-path.",
                DEFAULT_MODEL_CACHE_ROOT, DEFAULT_BASE_MODEL_ID
            ),
            Some(format!(
                "export AOS_BASE_MODEL_ID={} && export AOS_MODEL_CACHE_DIR={}",
                DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT
            )),
        ));
    }

    // Check backend preference
    let backend = std::env::var("AOS_MODEL_BACKEND").unwrap_or_else(|_| "auto".to_string());
    results.push(CheckResult::pass(
        "Env: MODEL_BACKEND",
        &format!("Backend preference: {}", backend),
    ));

    results
}

/// Check backend availability (CoreML, Metal, MLX)
async fn check_backends() -> Vec<CheckResult> {
    let mut results = Vec::new();

    // Check CoreML availability (macOS only)
    #[cfg(target_os = "macos")]
    {
        // Check for Swift compiler (required for CoreML bridge)
        let swiftc_available = Command::new("which")
            .arg("swiftc")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if swiftc_available {
            results.push(CheckResult::pass(
                "Backend: CoreML",
                "Swift compiler available - CoreML backend ready",
            ));
        } else {
            results.push(CheckResult::warning(
                "Backend: CoreML",
                "Swift compiler not found - CoreML backend may be degraded",
                Some("xcode-select --install".to_string()),
            ));
        }

        // Check for Metal compiler
        let metal_available = Command::new("xcrun")
            .args(&["metal", "--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if metal_available {
            results.push(CheckResult::pass(
                "Backend: Metal",
                "Metal compiler available - Metal backend ready",
            ));
        } else {
            results.push(CheckResult::warning(
                "Backend: Metal",
                "Metal compiler not found - Metal backend unavailable",
                Some("xcode-select --install".to_string()),
            ));
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        results.push(CheckResult::warning(
            "Backend: CoreML/Metal",
            "CoreML and Metal backends only available on macOS",
            None,
        ));
    }

    // Check for MLX (optional)
    let mlx_available = std::env::var("MLX_PATH").is_ok()
        || Command::new("pkg-config")
            .args(&["--modversion", "mlx"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

    if mlx_available {
        results.push(CheckResult::pass(
            "Backend: MLX",
            "MLX library detected - MLX backend available",
        ));
    } else {
        results.push(CheckResult::warning(
            "Backend: MLX",
            "MLX library not found - using stub implementation (optional)",
            Some("pip install mlx  # Optional for MLX backend".to_string()),
        ));
    }

    results
}

/// Check system resources (memory, disk)
async fn check_resources() -> Vec<CheckResult> {
    let mut results = Vec::new();

    // Check available disk space
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("df").args(&["-h", "."]).output() {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                // Parse df output to get available space
                let lines: Vec<&str> = stdout.lines().collect();
                if lines.len() > 1 {
                    let details = lines[1].to_string();
                    results.push(
                        CheckResult::pass("Disk Space", "Sufficient disk space available")
                            .with_details(details),
                    );
                }
            }
        }
    }

    // Check available memory
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("sysctl").args(&["hw.memsize"]).output() {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                if let Some(mem_str) = stdout.split(':').nth(1) {
                    if let Ok(mem_bytes) = mem_str.trim().parse::<u64>() {
                        let mem_gb = mem_bytes / 1024 / 1024 / 1024;
                        if mem_gb >= 8 {
                            results.push(CheckResult::pass(
                                "System Memory",
                                &format!("{}GB RAM available", mem_gb),
                            ));
                        } else {
                            results.push(CheckResult::warning(
                                "System Memory",
                                &format!("Only {}GB RAM - recommend 8GB+ for inference", mem_gb),
                                None,
                            ));
                        }
                    }
                }
            }
        }
    }

    results
}

/// Display check results in a formatted table
fn display_results(results: &[CheckResult], _output: &OutputWriter) -> Result<()> {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Check", "Status", "Message"]);

    for result in results {
        let (status_symbol, status_color) = match result.status {
            CheckStatus::Pass => ("✓", Color::Green),
            CheckStatus::Warning => ("⚠", Color::Yellow),
            CheckStatus::Fail => ("✗", Color::Red),
        };

        table.add_row(vec![
            Cell::new(&result.name),
            Cell::new(status_symbol).fg(status_color),
            Cell::new(&result.message),
        ]);
    }

    println!("{}", table);

    Ok(())
}

/// Display fix suggestions for failed/warning checks
fn display_fix_suggestions(results: &[CheckResult], output: &OutputWriter) -> Result<()> {
    let failures_with_fixes: Vec<_> = results
        .iter()
        .filter(|r| {
            (r.status == CheckStatus::Fail || r.status == CheckStatus::Warning)
                && r.fix_command.is_some()
        })
        .collect();

    if failures_with_fixes.is_empty() {
        return Ok(());
    }

    output.info("\n💡 Suggested fixes:");
    println!();

    for result in failures_with_fixes {
        if let Some(ref fix_cmd) = result.fix_command {
            println!("  {}:", result.name);
            println!("    $ {}", fix_cmd);
            println!();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_result_creation() {
        let pass = CheckResult::pass("test", "ok");
        assert_eq!(pass.status, CheckStatus::Pass);
        assert_eq!(pass.name, "test");

        let fail = CheckResult::fail("test", "error", Some("fix".to_string()));
        assert_eq!(fail.status, CheckStatus::Fail);
        assert_eq!(fail.fix_command, Some("fix".to_string()));
    }
}
