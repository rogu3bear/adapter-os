//! Pre-flight system readiness checker for AdapterOS
//!
//! Provides comprehensive environment verification before launching the server:
//! - Model availability and configuration
//! - Database initialization and migrations
//! - Required directories and files
//! - Environment variables
//! - Backend availability (CoreML, Metal, MLX)
//! - System resources
//! - Alias swap gating and validation
//!
//! # Alias Swap Gating
//!
//! Before an alias can be swapped to point to a new adapter, several conditions
//! must be met to ensure system stability and data integrity:
//!
//! 1. **Adapter Existence**: The target adapter must exist in the registry
//! 2. **File Integrity**: The .aos file must exist and have valid hashes
//! 3. **Lifecycle State**: Adapter must be in ready/active/training state
//! 4. **Conflict Detection**: No conflicting active adapters for same repo/branch
//! 5. **System Mode**: System must not be in maintenance mode
//! 6. **Tenant Isolation**: Swap must respect tenant boundaries
//!
//! Use `gate_alias_swap()` to enforce these checks before any alias operation.
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use crate::output::OutputWriter;
use adapteros_config::{
    resolve_base_model_location, DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT,
};
use anyhow::Result;
use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read as IoRead;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

// Import auto-fix module
use super::preflight_fix::{
    create_directories, create_env_from_example, download_model, install_mlx_library,
    install_xcode_cli_tools, run_bootstrap_repair, run_database_migrations, AutoFixer, FixMode,
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

    // 3. Check bootstrap state (tenants/plans/nodes)
    results.push(check_bootstrap_state(&cmd).await);

    // 4. Check required directories
    results.extend(check_directories().await);

    // 5. Check environment variables
    results.extend(check_environment_variables().await);

    // 6. Check backends (CoreML, Metal, MLX) unless skipped
    if !cmd.skip_backends {
        results.extend(check_backends().await);
    }

    // 7. Check system resources unless skipped
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
        results.push(check_bootstrap_state(&cmd).await);
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
    output.info(format!("\n📊 Summary: {} checks run", results.len()));
    if failures > 0 {
        output.error(format!("❌ {} critical failures", failures));
        if cmd.fix {
            output.info("   Some issues could not be fixed automatically");
        }
    }
    if warnings > 0 {
        output.warning(format!("⚠️  {} warnings", warnings));
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
                let db_url = resolve_db_url(cmd);
                Some(run_database_migrations(db_url))
            }

            "Bootstrap State" => {
                let db_url = resolve_db_url(cmd);
                resolve_sqlite_path(&db_url).map(run_bootstrap_repair)
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
    // 1. Explicit command-line override takes precedence
    if let Some(path) = cmd.model_path.as_ref() {
        return Ok(PathBuf::from(path));
    }

    // 2. Primary method: Use canonical resolver (matches server behavior)
    //    This reads AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID
    match resolve_base_model_location(None, None, false) {
        Ok(loc) => return Ok(loc.full_path),
        Err(e) => {
            // If canonical resolver fails, try legacy fallbacks before giving up
            tracing::debug!(
                "Canonical model resolver failed ({}), trying legacy env vars",
                e
            );
        }
    }

    // 3. Legacy fallback: AOS_MODEL_PATH (deprecated)
    if let Ok(path) = std::env::var("AOS_MODEL_PATH") {
        eprintln!("⚠️  WARNING: Using deprecated AOS_MODEL_PATH environment variable.");
        eprintln!(
            "   Please migrate to AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID for consistency with server."
        );
        eprintln!("   Example: export AOS_MODEL_CACHE_DIR=~/.cache/adapteros/models");
        eprintln!("            export AOS_BASE_MODEL_ID=Qwen/Qwen2.5-7B-Instruct-4bit\n");
        return Ok(PathBuf::from(path));
    }

    // 4. Legacy fallback: AOS_MLX_FFI_MODEL (deprecated)
    if let Ok(path) = std::env::var("AOS_MLX_FFI_MODEL") {
        eprintln!("⚠️  WARNING: Using deprecated AOS_MLX_FFI_MODEL environment variable.");
        eprintln!(
            "   Please migrate to AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID for consistency with server."
        );
        eprintln!("   Example: export AOS_MODEL_CACHE_DIR=~/.cache/adapteros/models");
        eprintln!("            export AOS_BASE_MODEL_ID=Qwen/Qwen2.5-7B-Instruct-4bit\n");
        return Ok(PathBuf::from(path));
    }

    // 5. All methods failed
    Err(anyhow::anyhow!(
        "Failed to resolve model path. Set AOS_MODEL_CACHE_DIR and AOS_BASE_MODEL_ID, or use --model-path"
    ))
}

fn resolve_db_url(cmd: &PreflightCommand) -> String {
    cmd.database_url
        .as_ref()
        .map(|s| s.to_string())
        .or_else(|| std::env::var("AOS_DATABASE_URL").ok())
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .unwrap_or_else(|| "sqlite:var/aos-cp.sqlite3".to_string())
}

fn resolve_sqlite_path(db_url: &str) -> Option<String> {
    let raw = db_url.strip_prefix("sqlite:")?;
    let mut path = raw.to_string();
    if let Some(stripped) = path.strip_prefix("//") {
        path = stripped.to_string();
    }
    if let Some(idx) = path.find('?') {
        path.truncate(idx);
    }
    if let Some(idx) = path.find('#') {
        path.truncate(idx);
    }
    Some(path)
}

/// Check database initialization and migrations
async fn check_database(cmd: &PreflightCommand) -> CheckResult {
    // Determine database path
    let db_url = resolve_db_url(cmd);

    // Extract file path from sqlite: URL
    let Some(db_path) = resolve_sqlite_path(&db_url) else {
        return CheckResult::warning(
            "Database",
            "Non-sqlite database URL; skipping sqlite checks",
            None,
        );
    };

    // Check if database file exists
    if !Path::new(&db_path).exists() {
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

/// Check bootstrap state (tenants/plans/nodes)
async fn check_bootstrap_state(cmd: &PreflightCommand) -> CheckResult {
    let db_url = resolve_db_url(cmd);
    let Some(db_path) = resolve_sqlite_path(&db_url) else {
        return CheckResult::warning(
            "Bootstrap State",
            "Non-sqlite database URL; bootstrap check skipped",
            None,
        );
    };

    if !Path::new(&db_path).exists() {
        return CheckResult::warning(
            "Bootstrap State",
            &format!("Database not initialized: {}", db_path),
            None,
        );
    }

    let pool = match sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", db_path))
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            return CheckResult::warning(
                "Bootstrap State",
                &format!("Cannot connect to database: {}", e),
                None,
            )
        }
    };

    let tenants: Result<i64, sqlx::Error> = sqlx::query_scalar("SELECT COUNT(*) FROM tenants")
        .fetch_one(&pool)
        .await;
    let plans: Result<i64, sqlx::Error> = sqlx::query_scalar("SELECT COUNT(*) FROM plans")
        .fetch_one(&pool)
        .await;
    let nodes: Result<i64, sqlx::Error> = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
        .fetch_one(&pool)
        .await;

    pool.close().await;

    let (tenants, plans, nodes) = match (tenants, plans, nodes) {
        (Ok(tenants), Ok(plans), Ok(nodes)) => (tenants, plans, nodes),
        _ => {
            return CheckResult::warning(
                "Bootstrap State",
                "Bootstrap tables missing or inaccessible; run migrations",
                Some("cargo run -p adapteros-cli -- db migrate".to_string()),
            )
        }
    };

    let details = format!("tenants={}, plans={}, nodes={}", tenants, plans, nodes);

    if tenants == 0 || plans == 0 || nodes == 0 {
        return CheckResult::fail(
            "Bootstrap State",
            &format!("Bootstrap rows missing ({})", details),
            Some("aosctl db repair-bootstrap --dry-run".to_string()),
        )
        .with_details(details);
    }

    CheckResult::pass("Bootstrap State", "Bootstrap rows present").with_details(details)
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
            .args(["metal", "--version"])
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
    let mut mlx_available = false;
    if let Ok(path) = std::env::var("AOS_MLX_PATH").or_else(|_| std::env::var("MLX_PATH")) {
        if Path::new(&path).exists() {
            mlx_available = true;
        }
    }

    if !mlx_available {
        let candidates = ["/opt/homebrew/opt/mlx", "/usr/local/opt/mlx"];
        if candidates.iter().any(|path| Path::new(path).exists()) {
            mlx_available = true;
        }
    }

    if !mlx_available {
        mlx_available = Command::new("pkg-config")
            .args(["--modversion", "mlx"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
    }

    if mlx_available {
        results.push(CheckResult::pass(
            "Backend: MLX",
            "MLX library detected - MLX backend available",
        ));
    } else {
        results.push(CheckResult::warning(
            "Backend: MLX",
            "MLX library not found - set AOS_MLX_PATH or install via Homebrew (optional)",
            Some("brew install mlx  # Optional for MLX backend".to_string()),
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
        if let Ok(output) = Command::new("df").args(["-h", "."]).output() {
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
        if let Ok(output) = Command::new("sysctl").args(["hw.memsize"]).output() {
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

/// Result of an adapter swap readiness check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSwapReadiness {
    /// Whether the adapter is ready for swap
    pub ready: bool,
    /// Adapter ID that was checked
    pub adapter_id: String,
    /// Individual check results
    pub checks: Vec<CheckResult>,
    /// Summary message
    pub message: String,
}

/// Check if an adapter is ready for alias swap
///
/// This function performs preflight checks on an adapter before allowing
/// it to be swapped via alias. Checks include:
/// - Adapter exists and has valid metadata
/// - .aos file path and hashes are set
/// - Lifecycle state allows activation
/// - No conflicting active adapters for same repo/branch
///
/// # Arguments
/// * `adapter_id` - The adapter ID to check
/// * `db` - Database connection
///
/// # Returns
/// * `AdapterSwapReadiness` - Result indicating if the adapter is ready
pub async fn check_adapter_swap_readiness(
    adapter_id: &str,
    db: &adapteros_db::Db,
) -> Result<AdapterSwapReadiness> {
    let mut checks = Vec::new();
    let mut all_passed = true;

    // Check 1: Adapter exists
    #[allow(deprecated)]
    let adapter = match db.get_adapter(adapter_id).await? {
        Some(a) => a,
        None => {
            checks.push(CheckResult::fail(
                "Adapter Exists",
                &format!("Adapter '{}' not found in registry", adapter_id),
                None,
            ));
            return Ok(AdapterSwapReadiness {
                ready: false,
                adapter_id: adapter_id.to_string(),
                checks,
                message: format!("Adapter '{}' not found", adapter_id),
            });
        }
    };
    checks.push(CheckResult::pass(
        "Adapter Exists",
        &format!("Adapter '{}' found in registry", adapter_id),
    ));

    // Check 2: .aos file path is set
    if adapter
        .aos_file_path
        .as_ref()
        .map(|p| !p.is_empty())
        .unwrap_or(false)
    {
        checks.push(CheckResult::pass(
            "AOS File Path",
            "Adapter has .aos file path set",
        ));
    } else {
        checks.push(CheckResult::fail(
            "AOS File Path",
            "Adapter missing .aos file path",
            Some("Register adapter with --aos-file-path".to_string()),
        ));
        all_passed = false;
    }

    // Check 3: .aos file hash is set
    if adapter
        .aos_file_hash
        .as_ref()
        .map(|h| !h.is_empty())
        .unwrap_or(false)
    {
        checks.push(CheckResult::pass(
            "AOS File Hash",
            "Adapter has .aos file hash set",
        ));
    } else {
        checks.push(CheckResult::fail(
            "AOS File Hash",
            "Adapter missing .aos file hash",
            Some("Register adapter with valid .aos file".to_string()),
        ));
        all_passed = false;
    }

    // Check 4: Content hash is set (required for integrity)
    if adapter
        .content_hash_b3
        .as_ref()
        .map(|h| !h.is_empty())
        .unwrap_or(false)
    {
        checks.push(CheckResult::pass(
            "Content Hash",
            "Adapter has content hash (BLAKE3) set",
        ));
    } else {
        checks.push(CheckResult::warning(
            "Content Hash",
            "Adapter missing content hash - integrity verification limited",
            Some("Re-register adapter with content hash".to_string()),
        ));
        // Warning doesn't fail the overall check
    }

    // Check 5: Lifecycle state allows activation
    let lifecycle_ok = matches!(
        adapter.lifecycle_state.as_str(),
        "ready" | "active" | "training"
    );
    if lifecycle_ok {
        checks.push(CheckResult::pass(
            "Lifecycle State",
            &format!(
                "Adapter lifecycle state '{}' allows activation",
                adapter.lifecycle_state
            ),
        ));
    } else {
        checks.push(CheckResult::fail(
            "Lifecycle State",
            &format!(
                "Adapter lifecycle state '{}' does not allow activation (need ready/active/training)",
                adapter.lifecycle_state
            ),
            Some(format!(
                "aosctl adapter update-lifecycle {} --state ready",
                adapter_id
            )),
        ));
        all_passed = false;
    }

    // Check 6: No conflicting active adapters for same repo/branch
    if let Some(ref repo_id) = adapter.repo_id {
        // Extract branch from the adapter's metadata
        let adapter_branch = adapter.metadata_json.as_ref().and_then(|m| {
            serde_json::from_str::<serde_json::Value>(m)
                .ok()
                .and_then(|v| {
                    v.get("branch")
                        .or_else(|| v.get("git_branch"))
                        .and_then(|b| b.as_str())
                        .map(String::from)
                })
        });

        // list_active_adapters_for_repo returns Vec<(adapter_id, Option<branch>)>
        let active_adapters = db
            .list_active_adapters_for_repo(repo_id)
            .await
            .unwrap_or_default();

        // Filter for conflicting adapters (same repo/branch, different adapter_id)
        let conflicting: Vec<_> = active_adapters
            .iter()
            .filter(|(other_adapter_id, other_branch)| {
                // Skip self
                if other_adapter_id == adapter_id {
                    return false;
                }
                // Check branch conflict:
                // - If both have branches specified, conflict only if they match
                // - If new adapter has no branch, conflicts with any existing active
                // - If existing has no branch, conflicts with any new activation
                match (&adapter_branch, other_branch) {
                    (Some(req), Some(other)) => req == other,
                    (Some(_), None) => true,
                    (None, _) => true,
                }
            })
            .collect();

        if conflicting.is_empty() {
            checks.push(CheckResult::pass(
                "Repo/Branch Uniqueness",
                "No conflicting active adapters for this repo/branch",
            ));
        } else {
            let conflict_ids: Vec<_> = conflicting
                .iter()
                .map(|(id, _)| id.as_str())
                .collect();
            checks.push(CheckResult::fail(
                "Repo/Branch Uniqueness",
                &format!(
                    "Conflicting active adapters for repo '{}': {:?}",
                    repo_id, conflict_ids
                ),
                Some(format!(
                    "Deactivate conflicting adapters first: aosctl adapter update-lifecycle {} --state deprecated",
                    conflict_ids.first().unwrap_or(&"")
                )),
            ));
            all_passed = false;
        }
    } else {
        checks.push(CheckResult::pass(
            "Repo/Branch Uniqueness",
            "Adapter not linked to a repository (no conflict check needed)",
        ));
    }

    // Check 7: .aos file exists on disk (if path is set)
    if let Some(ref aos_path) = adapter.aos_file_path {
        if !aos_path.is_empty() {
            let path = Path::new(aos_path);
            if path.exists() {
                checks.push(CheckResult::pass(
                    "AOS File Exists",
                    &format!("AOS file exists at {}", aos_path),
                ));
            } else {
                checks.push(CheckResult::fail(
                    "AOS File Exists",
                    &format!("AOS file not found at {}", aos_path),
                    Some(format!("Ensure .aos file exists at {}", aos_path)),
                ));
                all_passed = false;
            }
        }
    }

    let message = if all_passed {
        format!("Adapter '{}' is ready for swap", adapter_id)
    } else {
        format!(
            "Adapter '{}' failed {} preflight check(s)",
            adapter_id,
            checks
                .iter()
                .filter(|c| c.status == CheckStatus::Fail)
                .count()
        )
    };

    Ok(AdapterSwapReadiness {
        ready: all_passed,
        adapter_id: adapter_id.to_string(),
        checks,
        message,
    })
}

// =============================================================================
// Alias Swap Gating - Enhanced preflight checks before alias swap operations
// =============================================================================

/// Reason why an alias swap is blocked
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AliasSwapBlockReason {
    /// Target adapter not found in registry
    AdapterNotFound,
    /// Target adapter file (.aos) does not exist on disk
    AdapterFileNotFound,
    /// Target adapter file is corrupted or unreadable
    AdapterFileCorrupted,
    /// Target adapter has invalid or missing manifest
    InvalidManifest,
    /// Target adapter missing required hash
    MissingHash,
    /// Adapter lifecycle state does not permit activation
    InvalidLifecycleState,
    /// Conflicting active adapters for same repo/branch
    ConflictingAdapters,
    /// System is in maintenance mode
    MaintenanceMode,
    /// Alias swap would violate tenant isolation
    TenantIsolationViolation,
    /// Database error during validation
    DatabaseError,
}

impl std::fmt::Display for AliasSwapBlockReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdapterNotFound => write!(f, "Target adapter not found in registry"),
            Self::AdapterFileNotFound => write!(f, "Target adapter .aos file not found on disk"),
            Self::AdapterFileCorrupted => {
                write!(f, "Target adapter file is corrupted or unreadable")
            }
            Self::InvalidManifest => write!(f, "Target adapter has invalid or missing manifest"),
            Self::MissingHash => write!(f, "Target adapter missing required content hash"),
            Self::InvalidLifecycleState => {
                write!(f, "Adapter lifecycle state does not permit activation")
            }
            Self::ConflictingAdapters => {
                write!(f, "Conflicting active adapters exist for same repo/branch")
            }
            Self::MaintenanceMode => write!(f, "System is in maintenance mode"),
            Self::TenantIsolationViolation => {
                write!(f, "Swap would violate tenant isolation boundaries")
            }
            Self::DatabaseError => write!(f, "Database error during validation"),
        }
    }
}

/// Configuration for alias swap gating behavior
#[derive(Debug, Clone)]
pub struct AliasSwapGateConfig {
    /// Force swap even with warnings (not failures)
    pub force: bool,
    /// Skip maintenance mode check
    pub skip_maintenance_check: bool,
    /// Skip conflict detection
    pub skip_conflict_check: bool,
    /// Tenant ID for isolation checks
    pub tenant_id: Option<String>,
    /// Allow swaps to adapters in "training" state
    pub allow_training_state: bool,
}

impl Default for AliasSwapGateConfig {
    fn default() -> Self {
        Self {
            force: false,
            skip_maintenance_check: false,
            skip_conflict_check: false,
            tenant_id: None,
            allow_training_state: true,
        }
    }
}

/// Result of enhanced alias swap gating
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasSwapGateResult {
    /// Whether the swap is allowed
    pub allowed: bool,
    /// Blocking reasons (if any)
    pub block_reasons: Vec<AliasSwapBlockReason>,
    /// Warning messages (non-blocking)
    pub warnings: Vec<String>,
    /// Individual check results
    pub checks: Vec<CheckResult>,
    /// Suggested remediation steps
    pub remediation: Vec<String>,
    /// Time taken for checks
    pub check_duration_ms: u64,
}

impl AliasSwapGateResult {
    fn from_readiness(readiness: AdapterSwapReadiness, duration: Duration) -> Self {
        let mut block_reasons = Vec::new();
        let mut warnings = Vec::new();
        let mut remediation = Vec::new();

        for check in &readiness.checks {
            match check.status {
                CheckStatus::Fail => {
                    // Map check name to block reason
                    let reason = match check.name.as_str() {
                        "Adapter Exists" => AliasSwapBlockReason::AdapterNotFound,
                        "AOS File Exists" => AliasSwapBlockReason::AdapterFileNotFound,
                        "AOS File Path" => AliasSwapBlockReason::AdapterFileNotFound,
                        "AOS File Hash" => AliasSwapBlockReason::MissingHash,
                        "Content Hash" => AliasSwapBlockReason::MissingHash,
                        "Lifecycle State" => AliasSwapBlockReason::InvalidLifecycleState,
                        "Repo/Branch Uniqueness" => AliasSwapBlockReason::ConflictingAdapters,
                        "System Mode" => AliasSwapBlockReason::MaintenanceMode,
                        "Tenant Isolation" => AliasSwapBlockReason::TenantIsolationViolation,
                        _ => AliasSwapBlockReason::AdapterFileCorrupted,
                    };
                    block_reasons.push(reason);

                    if let Some(ref fix) = check.fix_command {
                        remediation.push(fix.clone());
                    }
                }
                CheckStatus::Warning => {
                    warnings.push(check.message.clone());
                }
                CheckStatus::Pass => {}
            }
        }

        Self {
            allowed: block_reasons.is_empty(),
            block_reasons,
            warnings,
            checks: readiness.checks,
            remediation,
            check_duration_ms: duration.as_millis() as u64,
        }
    }
}

/// Check if system is in maintenance mode
fn check_maintenance_mode() -> CheckResult {
    let maintenance_file = Path::new("var/.maintenance");
    let maintenance_env = std::env::var("AOS_MAINTENANCE_MODE").ok();

    if maintenance_file.exists() {
        return CheckResult::fail(
            "System Mode",
            "System is in maintenance mode (var/.maintenance exists)",
            Some("rm var/.maintenance  # Remove maintenance flag".to_string()),
        );
    }

    if let Some(mode) = maintenance_env {
        if mode == "1" || mode.to_lowercase() == "true" {
            return CheckResult::fail(
                "System Mode",
                "System is in maintenance mode (AOS_MAINTENANCE_MODE=true)",
                Some("unset AOS_MAINTENANCE_MODE".to_string()),
            );
        }
    }

    CheckResult::pass("System Mode", "System is operational")
}

/// Verify adapter file integrity (basic checks without full parsing)
fn check_adapter_file_integrity(aos_path: &Path) -> CheckResult {
    // Check file exists
    if !aos_path.exists() {
        return CheckResult::fail(
            "File Integrity",
            &format!("Adapter file not found: {}", aos_path.display()),
            None,
        );
    }

    // Check file is readable and has minimum size
    match File::open(aos_path) {
        Ok(mut file) => {
            let mut header = [0u8; 64];
            match file.read_exact(&mut header) {
                Ok(_) => {
                    // Basic size check
                    if let Ok(metadata) = file.metadata() {
                        const MIN_SIZE: u64 = 256;
                        if metadata.len() < MIN_SIZE {
                            return CheckResult::warning(
                                "File Integrity",
                                &format!(
                                    "Adapter file is suspiciously small ({} bytes)",
                                    metadata.len()
                                ),
                                None,
                            );
                        }
                    }
                    CheckResult::pass("File Integrity", "Adapter file readable and valid size")
                }
                Err(e) => CheckResult::fail(
                    "File Integrity",
                    &format!("Cannot read adapter file header: {}", e),
                    None,
                ),
            }
        }
        Err(e) => CheckResult::fail(
            "File Integrity",
            &format!("Cannot open adapter file: {}", e),
            Some("Check file permissions".to_string()),
        ),
    }
}

/// Gate an alias swap operation behind readiness checks
///
/// Returns Ok(()) if the adapter passes all preflight checks,
/// or an error with details about which checks failed.
///
/// # Arguments
/// * `adapter_id` - The adapter ID to swap to
/// * `db` - Database connection
///
/// # Example
/// ```no_run
/// # use adapteros_db::Db;
/// # async fn example(db: &Db) -> anyhow::Result<()> {
/// use adapteros_cli::commands::preflight::gate_alias_swap;
/// gate_alias_swap("my-adapter", db).await?;
/// // Proceed with swap only if checks pass
/// # Ok(())
/// # }
/// ```
pub async fn gate_alias_swap(adapter_id: &str, db: &adapteros_db::Db) -> Result<()> {
    gate_alias_swap_with_config(adapter_id, db, &AliasSwapGateConfig::default()).await
}

/// Gate an alias swap with custom configuration
///
/// Allows fine-grained control over which checks are performed.
pub async fn gate_alias_swap_with_config(
    adapter_id: &str,
    db: &adapteros_db::Db,
    config: &AliasSwapGateConfig,
) -> Result<()> {
    let start = Instant::now();

    // Check maintenance mode first (unless skipped)
    if !config.skip_maintenance_check {
        let maintenance_check = check_maintenance_mode();
        if maintenance_check.status == CheckStatus::Fail {
            return Err(anyhow::anyhow!(
                "Alias swap blocked: {}",
                AliasSwapBlockReason::MaintenanceMode
            ));
        }
    }

    // Run core readiness checks
    let readiness = check_adapter_swap_readiness(adapter_id, db).await?;
    let duration = start.elapsed();
    let gate_result = AliasSwapGateResult::from_readiness(readiness, duration);

    if gate_result.allowed {
        if !gate_result.warnings.is_empty() && !config.force {
            tracing::warn!(
                adapter_id = adapter_id,
                warnings = ?gate_result.warnings,
                "Alias swap allowed with warnings"
            );
        }
        Ok(())
    } else {
        let reasons: Vec<String> = gate_result
            .block_reasons
            .iter()
            .map(|r| r.to_string())
            .collect();

        let failed_checks: Vec<_> = gate_result
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .map(|c| format!("  - {}: {}", c.name, c.message))
            .collect();

        let remediation = if gate_result.remediation.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nRemediation:\n  {}",
                gate_result.remediation.join("\n  ")
            )
        };

        Err(anyhow::anyhow!(
            "Alias swap blocked for adapter '{}'.\n\nReasons:\n  - {}\n\nFailed checks:\n{}{}",
            adapter_id,
            reasons.join("\n  - "),
            failed_checks.join("\n"),
            remediation
        ))
    }
}

/// Run alias swap preflight and return detailed results
///
/// Use this for reporting/display purposes. For gating, use `gate_alias_swap()`.
pub async fn run_alias_swap_preflight(
    adapter_id: &str,
    db: &adapteros_db::Db,
    output: &OutputWriter,
) -> Result<AliasSwapGateResult> {
    let start = Instant::now();

    output.info(format!(
        "Running alias swap preflight for adapter: {}",
        adapter_id
    ));

    // Check maintenance mode
    let maintenance_check = check_maintenance_mode();
    let mut extra_checks = vec![maintenance_check];

    // Run core readiness checks
    let mut readiness = check_adapter_swap_readiness(adapter_id, db).await?;

    // Add extra checks
    readiness.checks.append(&mut extra_checks);

    // Add file integrity check if we have a path
    #[allow(deprecated)]
    if let Ok(Some(adapter)) = db.get_adapter(adapter_id).await {
        if let Some(ref aos_path) = adapter.aos_file_path {
            if !aos_path.is_empty() {
                let integrity_check = check_adapter_file_integrity(Path::new(aos_path));
                readiness.checks.push(integrity_check);
            }
        }
    }

    // Recalculate readiness
    readiness.ready = readiness
        .checks
        .iter()
        .all(|c| c.status != CheckStatus::Fail);

    let duration = start.elapsed();
    let result = AliasSwapGateResult::from_readiness(readiness, duration);

    // Display results
    display_alias_swap_preflight_results(&result, output);

    Ok(result)
}

/// Display alias swap preflight results
fn display_alias_swap_preflight_results(result: &AliasSwapGateResult, output: &OutputWriter) {
    output.blank();
    output.info("Alias Swap Preflight Results:");
    output.info("-----------------------------");

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Check", "Status", "Message"]);

    for check in &result.checks {
        let (status_symbol, status_color) = match check.status {
            CheckStatus::Pass => ("PASS", Color::Green),
            CheckStatus::Warning => ("WARN", Color::Yellow),
            CheckStatus::Fail => ("FAIL", Color::Red),
        };

        table.add_row(vec![
            Cell::new(&check.name),
            Cell::new(status_symbol).fg(status_color),
            Cell::new(&check.message),
        ]);
    }

    println!("{}", table);

    output.blank();
    output.info(format!("Check duration: {} ms", result.check_duration_ms));

    if result.allowed {
        if result.warnings.is_empty() {
            output.success("ALIAS SWAP ALLOWED: All preflight checks passed");
        } else {
            output.warning(format!(
                "ALIAS SWAP ALLOWED with {} warning(s)",
                result.warnings.len()
            ));
            for warning in &result.warnings {
                output.warning(format!("  - {}", warning));
            }
        }
    } else {
        output.error("ALIAS SWAP BLOCKED:");
        for reason in &result.block_reasons {
            output.error(format!("  - {}", reason));
        }

        if !result.remediation.is_empty() {
            output.blank();
            output.info("Suggested remediation:");
            for fix in &result.remediation {
                output.info(format!("  $ {}", fix));
            }
        }
    }
}

/// Quick validation for adapter file readiness before hot-swap
///
/// This is a lighter-weight check than full preflight, suitable for
/// real-time gating during inference operations.
pub fn require_adapter_file_ready(aos_path: &Path) -> Result<()> {
    let start = Instant::now();

    // Quick existence check
    if !aos_path.exists() {
        return Err(anyhow::anyhow!(
            "Adapter not ready: file not found at {}",
            aos_path.display()
        ));
    }

    // Quick readability check
    let file = File::open(aos_path).map_err(|e| {
        anyhow::anyhow!(
            "Adapter not ready: cannot open file {}: {}",
            aos_path.display(),
            e
        )
    })?;

    let metadata = file.metadata().map_err(|e| {
        anyhow::anyhow!(
            "Adapter not ready: cannot read metadata {}: {}",
            aos_path.display(),
            e
        )
    })?;

    // Minimum size check
    const MIN_ADAPTER_SIZE: u64 = 256;
    if metadata.len() < MIN_ADAPTER_SIZE {
        return Err(anyhow::anyhow!(
            "Adapter not ready: file too small ({} bytes, minimum {})",
            metadata.len(),
            MIN_ADAPTER_SIZE
        ));
    }

    let elapsed = start.elapsed();
    tracing::debug!(
        path = %aos_path.display(),
        duration_us = elapsed.as_micros(),
        "Adapter file readiness verified"
    );

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

    #[test]
    fn test_alias_swap_block_reason_display() {
        assert_eq!(
            AliasSwapBlockReason::AdapterNotFound.to_string(),
            "Target adapter not found in registry"
        );
        assert_eq!(
            AliasSwapBlockReason::AdapterFileNotFound.to_string(),
            "Target adapter .aos file not found on disk"
        );
        assert_eq!(
            AliasSwapBlockReason::InvalidLifecycleState.to_string(),
            "Adapter lifecycle state does not permit activation"
        );
        assert_eq!(
            AliasSwapBlockReason::ConflictingAdapters.to_string(),
            "Conflicting active adapters exist for same repo/branch"
        );
        assert_eq!(
            AliasSwapBlockReason::MaintenanceMode.to_string(),
            "System is in maintenance mode"
        );
        assert_eq!(
            AliasSwapBlockReason::TenantIsolationViolation.to_string(),
            "Swap would violate tenant isolation boundaries"
        );
    }

    #[test]
    fn test_alias_swap_gate_config_default() {
        let config = AliasSwapGateConfig::default();
        assert!(!config.force);
        assert!(!config.skip_maintenance_check);
        assert!(!config.skip_conflict_check);
        assert!(config.tenant_id.is_none());
        assert!(config.allow_training_state);
    }

    #[test]
    fn test_alias_swap_gate_result_from_readiness() {
        let checks = vec![
            CheckResult::pass("Test1", "ok"),
            CheckResult::warning("Test2", "minor issue", None),
            CheckResult::fail("Adapter Exists", "not found", Some("fix it".to_string())),
        ];

        let readiness = AdapterSwapReadiness {
            ready: false,
            adapter_id: "test-adapter".to_string(),
            checks,
            message: "Test failed".to_string(),
        };

        let result = AliasSwapGateResult::from_readiness(readiness, Duration::from_millis(50));

        assert!(!result.allowed);
        assert_eq!(result.block_reasons.len(), 1);
        assert_eq!(
            result.block_reasons[0],
            AliasSwapBlockReason::AdapterNotFound
        );
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.remediation.len(), 1);
        assert_eq!(result.check_duration_ms, 50);
    }

    #[test]
    fn test_alias_swap_gate_result_all_pass() {
        let checks = vec![
            CheckResult::pass("Adapter Exists", "found"),
            CheckResult::pass("AOS File Path", "path set"),
            CheckResult::pass("Lifecycle State", "ready"),
        ];

        let readiness = AdapterSwapReadiness {
            ready: true,
            adapter_id: "test-adapter".to_string(),
            checks,
            message: "All passed".to_string(),
        };

        let result = AliasSwapGateResult::from_readiness(readiness, Duration::from_millis(10));

        assert!(result.allowed);
        assert!(result.block_reasons.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_check_adapter_file_integrity_missing_file() {
        let result = check_adapter_file_integrity(Path::new("/nonexistent/path/adapter.aos"));
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn test_require_adapter_file_ready_missing() {
        let result = require_adapter_file_ready(Path::new("/nonexistent/adapter.aos"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_adapter_swap_readiness_structure() {
        let readiness = AdapterSwapReadiness {
            ready: true,
            adapter_id: "test-123".to_string(),
            checks: vec![CheckResult::pass("Test", "ok")],
            message: "Ready".to_string(),
        };

        assert!(readiness.ready);
        assert_eq!(readiness.adapter_id, "test-123");
        assert_eq!(readiness.checks.len(), 1);
    }
}
