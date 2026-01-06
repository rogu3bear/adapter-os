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
//! - **Activation gating (preflight before lifecycle activation)**
//!
//! # Activation Gating (Preflight Before Activation)
//!
//! Before an adapter can be activated (transitioned to 'active' lifecycle state),
//! preflight checks must pass. This ensures:
//!
//! 1. **Adapter Existence**: The adapter must exist in the registry
//! 2. **Lifecycle State**: Adapter must be in 'ready' state (preflight completed)
//! 3. **File Path Set**: .aos file path must be configured

#![allow(clippy::field_reassign_with_default)]
//! 4. **File Exists**: .aos file must exist on disk
//! 5. **File Hash Set**: .aos file hash must be computed for integrity
//! 6. **Content Hash Set**: content_hash_b3 must be present for reproducibility
//! 7. **File Integrity**: .aos file must be readable and valid
//! 8. **Training Evidence**: training snapshot evidence must exist
//! 9. **No Conflicts**: No other active adapters for same repo/branch
//! 10. **System Mode**: System must not be in maintenance mode
//!
//! Use `gate_activation()` to enforce these checks before activating an adapter:
//!
//! ```ignore
//! use adapteros_cli::commands::preflight::gate_activation;
//!
//! // This will fail if preflight hasn't passed
//! gate_activation("my-adapter", &db).await?;
//!
//! // Only reaches here if checks passed - safe to activate
//! db.update_adapter_lifecycle_state("my-adapter", LifecycleState::Active).await?;
//! ```
//!
//! # Alias Swap Gating
//!
//! Before an alias can be swapped to point to a new adapter, several conditions
//! must be met to ensure system stability and data integrity:
//!
//! 1. **Adapter Existence**: The target adapter must exist in the registry
//! 2. **File Integrity**: The .aos file must exist and have valid file/content/manifest hashes
//! 3. **Lifecycle State**: Adapter must be in ready/active state (training allowed only when configured)
//! 4. **Conflict Detection**: No conflicting active adapters for same repo/branch
//! 5. **System Mode**: System must not be in maintenance mode
//! 6. **Tenant Isolation**: Swap must respect tenant boundaries
//!
//! Use `gate_alias_swap()` to enforce these checks before any alias operation.
//!
//! ## Integration Points
//!
//! Alias swap preflight is enforced at multiple levels:
//!
//! - **CLI**: `aosctl adapter swap` uses `gate_alias_swap()` (see `adapter.rs`, `adapter_swap.rs`)
//! - **Server API**: `POST /v1/adapters/swap` runs `run_adapter_swap_preflight()` internally
//!   (see `adapteros-server-api/src/handlers/adapters/swap.rs`)
//! - **Hot-swap**: Low-level hot-swap operations use the same gating logic
//!
//! ## Hash Requirements for Alias Swap
//!
//! As of the preflight hardening update, adapters must have both hash fields populated:
//!
//! - **content_hash_b3**: `BLAKE3(manifest_bytes || canonical_segment_payload)`
//!   Used for integrity verification and content-addressable routing.
//!
//! - **manifest_hash**: `BLAKE3(manifest_bytes)`
//!   Used for deterministic routing and caching.
//!
//! Adapters registered before these fields were mandatory may be missing one or both.
//! To repair legacy adapters, use:
//!
//! ```bash
//! # Repair a single adapter
//! aosctl adapter repair-hashes --adapter-id <adapter-id>
//!
//! # Repair all adapters for a tenant
//! aosctl adapter repair-hashes --tenant-id <tenant-id>
//!
//! # Preview changes without updating
//! aosctl adapter repair-hashes --adapter-id <adapter-id> --dry-run
//! ```
//!
//! ## Error Codes
//!
//! Preflight failures include structured error codes for programmatic handling:
//!
//! - `PREFLIGHT_MISSING_CONTENT_HASH`: content_hash_b3 is NULL or empty
//! - `PREFLIGHT_MISSING_MANIFEST_HASH`: manifest_hash is NULL or empty
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use crate::output::OutputWriter;
use adapteros_config::{
    resolve_base_model_location, DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT,
};
use adapteros_core::lifecycle::{
    validate_alias_swap, validate_transition_with_context, LifecycleState, LifecycleTransition,
    PreflightStatus, ValidationContext,
};
use adapteros_core::preflight::PreflightErrorCode;
use adapteros_normalization::extract_repo_identifier_from_metadata;
use anyhow::Result;
use clap::Args;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read as IoRead;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
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
    /// Structured error code for programmatic handling (CLI/API consistency)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl CheckResult {
    fn pass(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Pass,
            message: message.to_string(),
            fix_command: None,
            details: None,
            error_code: None,
        }
    }

    fn warning(name: &str, message: &str, fix_command: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Warning,
            message: message.to_string(),
            fix_command,
            details: None,
            error_code: None,
        }
    }

    fn fail(name: &str, message: &str, fix_command: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Fail,
            message: message.to_string(),
            fix_command,
            details: None,
            error_code: None,
        }
    }

    /// Create a failure result with a structured error code for programmatic handling
    fn fail_with_code(
        name: &str,
        message: &str,
        fix_command: Option<String>,
        error_code: PreflightErrorCode,
    ) -> Self {
        Self {
            name: name.to_string(),
            status: CheckStatus::Fail,
            message: message.to_string(),
            fix_command,
            details: None,
            error_code: Some(error_code.as_str().to_string()),
        }
    }

    fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }
}

fn extract_branch_from_metadata(metadata_json: &Option<String>) -> Option<String> {
    metadata_json.as_ref().and_then(|raw| {
        let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
        parsed
            .get("scope_branch")
            .or_else(|| parsed.get("repo_branch"))
            .or_else(|| parsed.get("branch"))
            .or_else(|| parsed.get("git_branch"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })
}

// extract_repo_id_from_metadata moved to adapteros_normalization crate

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
            if let Err(e) = fixer.try_fix(issue) {
                tracing::debug!(error = %e, check = %result.name, "Auto-fix attempt failed (non-fatal)");
            }
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
            return CheckResult::fail_with_code(
                "Model",
                &format!("Failed to resolve model path: {}", e),
                Some(
                    "Set AOS_BASE_MODEL_ID/AOS_MODEL_CACHE_DIR or provide --model-path".to_string(),
                ),
                PreflightErrorCode::ModelPathResolutionFailed,
            )
        }
    };

    // Check if model directory exists
    if !model_path.exists() {
        return CheckResult::fail_with_code(
            "Model Directory",
            &format!("Model directory not found: {}", model_path.display()),
            Some("make download-model  # or: aosctl models seed".to_string()),
            PreflightErrorCode::ModelNotFound,
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
        return CheckResult::fail_with_code(
            "Model Files",
            &format!("Missing required files: {}", missing_files.join(", ")),
            Some("make download-model  # or: aosctl models seed".to_string()),
            PreflightErrorCode::ModelFileMissing,
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
        return CheckResult::fail_with_code(
            "Model Weights",
            "No weight files (.safetensors or .bin) found",
            Some("make download-model  # or: aosctl models seed".to_string()),
            PreflightErrorCode::ModelWeightsMissing,
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
    check_adapter_swap_readiness_with_config(adapter_id, db, &AliasSwapGateConfig::default()).await
}

/// Check if an adapter is ready for alias swap with configuration overrides
pub async fn check_adapter_swap_readiness_with_config(
    adapter_id: &str,
    db: &adapteros_db::Db,
    config: &AliasSwapGateConfig,
) -> Result<AdapterSwapReadiness> {
    let mut checks = Vec::new();
    let mut all_passed = true;
    let mut lifecycle_state: Option<LifecycleState> = None;
    let mut has_aos_path = false;
    let mut has_aos_hash = false;
    let mut has_content_hash = false;
    let mut has_manifest_hash = false;
    let mut aos_file_exists = false;
    let mut file_readiness_failed = false;
    let mut has_training_evidence = false;
    let mut conflicting_adapter_ids: Vec<String> = Vec::new();

    // Check 1: Adapter exists
    #[allow(deprecated)]
    let adapter = match config.tenant_id.as_deref() {
        Some(tenant_id) => db.get_adapter_for_tenant(tenant_id, adapter_id).await?,
        None => db.get_adapter(adapter_id).await?,
    };
    let adapter = match adapter {
        Some(a) => a,
        None => {
            checks.push(CheckResult::fail_with_code(
                "Adapter Exists",
                &format!("Adapter '{}' not found in registry", adapter_id),
                None,
                PreflightErrorCode::AdapterNotFound,
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

    // Check 2: Tenant isolation (if configured)
    if let Some(ref tenant_id) = config.tenant_id {
        if adapter.tenant_id != *tenant_id {
            checks.push(CheckResult::fail_with_code(
                "Tenant Isolation",
                &format!(
                    "Adapter tenant '{}' does not match requested tenant '{}'",
                    adapter.tenant_id, tenant_id
                ),
                Some(format!(
                    "Use a tenant-scoped alias swap for '{}'",
                    adapter.tenant_id
                )),
                PreflightErrorCode::TenantIsolationViolation,
            ));
            all_passed = false;
        } else {
            checks.push(CheckResult::pass(
                "Tenant Isolation",
                "Adapter tenant matches requested tenant",
            ));
        }
    }

    // Check 3: .aos file path is set
    if adapter
        .aos_file_path
        .as_ref()
        .map(|p| !p.is_empty())
        .unwrap_or(false)
    {
        has_aos_path = true;
        checks.push(CheckResult::pass(
            "AOS File Path",
            "Adapter has .aos file path set",
        ));
    } else {
        checks.push(CheckResult::fail_with_code(
            "AOS File Path",
            "Adapter missing .aos file path",
            Some("Register adapter with --aos-file-path".to_string()),
            PreflightErrorCode::AdapterFileNotFound,
        ));
        all_passed = false;
    }

    // Check 4: .aos file hash is set
    if adapter
        .aos_file_hash
        .as_ref()
        .map(|h| !h.is_empty())
        .unwrap_or(false)
    {
        has_aos_hash = true;
        checks.push(CheckResult::pass(
            "AOS File Hash",
            "Adapter has .aos file hash set",
        ));
    } else {
        checks.push(CheckResult::fail_with_code(
            "AOS File Hash",
            "Adapter missing .aos file hash",
            Some("Register adapter with valid .aos file".to_string()),
            PreflightErrorCode::MissingAosFileHash,
        ));
        all_passed = false;
    }

    // Check 5: Content hash is set (required for integrity verification)
    //
    // content_hash_b3 = BLAKE3(manifest_bytes || canonical_segment_payload)
    // This hash uniquely identifies the adapter's content for integrity checks
    // and routing decisions. Adapters registered before this field was mandatory
    // may be missing it.
    if adapter
        .content_hash_b3
        .as_ref()
        .map(|h| !h.is_empty())
        .unwrap_or(false)
    {
        has_content_hash = true;
        checks.push(CheckResult::pass(
            "Content Hash",
            "Adapter has content_hash_b3 (BLAKE3 integrity hash) set",
        ));
    } else {
        let aos_path_hint = adapter
            .aos_file_path
            .as_ref()
            .filter(|p| !p.is_empty())
            .map(|p| format!(" (.aos path: {})", p))
            .unwrap_or_default();

        let remediation = format!(
            "Run: aosctl adapter repair-hashes --adapter-id {}{}",
            adapter_id, aos_path_hint
        );

        checks.push(CheckResult::fail_with_code(
            "Content Hash",
            &format!(
                "Adapter missing content_hash_b3 (required for integrity verification){}",
                aos_path_hint
            ),
            Some(remediation),
            PreflightErrorCode::MissingContentHash,
        ));
        all_passed = false;
    }

    // Check 6: Manifest hash is set (required for deterministic routing)
    //
    // manifest_hash = BLAKE3(manifest_bytes)
    // This hash identifies the adapter's manifest for routing and caching.
    // Adapters registered before this field was mandatory may be missing it.
    if adapter
        .manifest_hash
        .as_ref()
        .map(|h| !h.is_empty())
        .unwrap_or(false)
    {
        has_manifest_hash = true;
        checks.push(CheckResult::pass(
            "Manifest Hash",
            "Adapter has manifest_hash (BLAKE3 manifest hash) set",
        ));
    } else {
        let aos_path_hint = adapter
            .aos_file_path
            .as_ref()
            .filter(|p| !p.is_empty())
            .map(|p| format!(" (.aos path: {})", p))
            .unwrap_or_default();

        let remediation = format!(
            "Run: aosctl adapter repair-hashes --adapter-id {}{}",
            adapter_id, aos_path_hint
        );

        checks.push(CheckResult::fail_with_code(
            "Manifest Hash",
            &format!(
                "Adapter missing manifest_hash (required for deterministic routing){}",
                aos_path_hint
            ),
            Some(remediation),
            PreflightErrorCode::MissingManifestHash,
        ));
        all_passed = false;
    }

    // Check 7: Lifecycle state parsing (constraint validation runs later)
    match LifecycleState::from_str(&adapter.lifecycle_state) {
        Ok(state) => {
            if state.is_terminal() {
                let recovery_hint = if state == LifecycleState::Retired {
                    "Retired adapters cannot be reactivated. Create a new adapter version instead."
                } else {
                    "Failed adapters cannot be reactivated. Investigate the failure cause and retrain."
                };
                checks.push(CheckResult::fail_with_code(
                    "Lifecycle State",
                    &format!(
                        "Adapter in terminal state '{}' - cannot be activated",
                        adapter.lifecycle_state
                    ),
                    Some(recovery_hint.to_string()),
                    PreflightErrorCode::InvalidLifecycleState,
                ));
                all_passed = false;
            } else {
                lifecycle_state = Some(state);
            }
        }
        Err(_) => {
            checks.push(CheckResult::fail_with_code(
                "Lifecycle State",
                &format!(
                    "Adapter lifecycle state '{}' is not recognized",
                    adapter.lifecycle_state
                ),
                None,
                PreflightErrorCode::InvalidLifecycleState,
            ));
            all_passed = false;
        }
    }

    // Check 8: No conflicting active adapters for same repo/branch
    if config.skip_conflict_check {
        checks.push(CheckResult::pass(
            "Repo/Branch Uniqueness",
            "Conflict check skipped by configuration",
        ));
    } else {
        let adapter_branch = extract_branch_from_metadata(&adapter.metadata_json);

        let repo_id = adapter
            .repo_id
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.to_string())
            .or_else(|| extract_repo_identifier_from_metadata(adapter.metadata_json.as_deref()));
        let repo_path = adapter
            .repo_path
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.to_string());
        let codebase_scope = adapter
            .codebase_scope
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.to_string());

        if repo_id.is_none() && repo_path.is_none() && codebase_scope.is_none() {
            checks.push(CheckResult::pass(
                "Repo/Branch Uniqueness",
                "Adapter not linked to a repository (no conflict check needed)",
            ));
        } else {
            match db
                .validate_active_uniqueness(
                    adapter_id,
                    repo_id.clone(),
                    repo_path.clone(),
                    codebase_scope.clone(),
                    adapter_branch.clone(),
                )
                .await
            {
                Ok(result) => {
                    conflicting_adapter_ids = result.conflicting_adapters.clone();
                    if result.is_valid {
                        checks.push(CheckResult::pass(
                            "Repo/Branch Uniqueness",
                            "No conflicting active adapters for this repo scope",
                        ));
                    } else {
                        let conflict_ids = result.conflicting_adapters.join(", ");
                        let reason = result.conflict_reason.unwrap_or_else(|| {
                            format!("Conflicting active adapters detected: {}", conflict_ids)
                        });
                        let remediation = result
                            .conflicting_adapters
                            .first()
                            .map(|id| {
                                format!(
                                    "Deactivate conflicting adapters first: aosctl adapter update-lifecycle {} --state deprecated",
                                    id
                                )
                            });
                        checks.push(CheckResult::fail_with_code(
                            "Repo/Branch Uniqueness",
                            &reason,
                            remediation,
                            PreflightErrorCode::ConflictingActiveAdapters,
                        ));
                        all_passed = false;
                    }
                }
                Err(e) => {
                    checks.push(CheckResult::fail_with_code(
                        "Repo/Branch Uniqueness",
                        &format!("Failed to validate active uniqueness: {}", e),
                        None,
                        PreflightErrorCode::ConflictingActiveAdapters,
                    ));
                    all_passed = false;
                }
            }
        }
    }

    // Check 9: .aos file exists on disk (if path is set)
    if let Some(ref aos_path) = adapter.aos_file_path {
        if !aos_path.is_empty() {
            let path = Path::new(aos_path);
            if path.exists() {
                aos_file_exists = true;
                checks.push(CheckResult::pass(
                    "AOS File Exists",
                    &format!("AOS file exists at {}", aos_path),
                ));
            } else {
                checks.push(CheckResult::fail_with_code(
                    "AOS File Exists",
                    &format!("AOS file not found at {}", aos_path),
                    Some(format!("Ensure .aos file exists at {}", aos_path)),
                    PreflightErrorCode::AdapterFileNotFound,
                ));
                all_passed = false;
            }
        }
    }

    if aos_file_exists {
        let integrity_check = check_adapter_file_integrity(
            Path::new(adapter.aos_file_path.as_deref().unwrap_or("")),
            adapter.aos_file_hash.as_deref(),
        );
        if integrity_check.status == CheckStatus::Fail {
            all_passed = false;
            file_readiness_failed = true;
        }
        checks.push(integrity_check);
    }

    if lifecycle_state.is_some() {
        match db.get_adapter_training_snapshot(adapter_id).await {
            Ok(snapshot) => {
                has_training_evidence = snapshot.is_some();
                if has_training_evidence {
                    checks.push(CheckResult::pass(
                        "Training Evidence",
                        "Training snapshot evidence found",
                    ));
                } else {
                    checks.push(CheckResult::fail_with_code(
                        "Training Evidence",
                        "Training snapshot evidence missing",
                        Some("Re-run training to record snapshot evidence".to_string()),
                        PreflightErrorCode::MissingTrainingEvidence,
                    ));
                    all_passed = false;
                }
            }
            Err(e) => {
                checks.push(CheckResult::fail_with_code(
                    "Training Evidence",
                    &format!("Failed to load training evidence: {}", e),
                    None,
                    PreflightErrorCode::MissingTrainingEvidence,
                ));
                all_passed = false;
            }
        }
    }

    let has_artifact =
        has_aos_path && aos_file_exists && has_aos_hash && has_content_hash && has_manifest_hash;
    let preflight_status = if !has_artifact {
        PreflightStatus::Pending
    } else if file_readiness_failed {
        PreflightStatus::Failed
    } else {
        PreflightStatus::Passed
    };

    match preflight_status {
        PreflightStatus::Passed => {
            checks.push(CheckResult::pass(
                "Preflight Status",
                "Preflight checks passed",
            ));
        }
        PreflightStatus::Failed => {
            checks.push(CheckResult::fail(
                "Preflight Status",
                "Preflight checks failed",
                None,
            ));
            all_passed = false;
        }
        _ => {
            checks.push(CheckResult::fail(
                "Preflight Status",
                "Preflight checks have not been completed",
                None,
            ));
            all_passed = false;
        }
    }

    if let Some(state) = lifecycle_state {
        let ctx = ValidationContext::new()
            .with_tier(adapter.tier.clone())
            .with_preflight_status(preflight_status)
            .with_artifact(has_artifact)
            .with_training_evidence(has_training_evidence)
            .with_conflicting_adapters(conflicting_adapter_ids.clone());
        match validate_alias_swap(state, config.allow_training_state, &ctx) {
            Ok(()) => {
                checks.push(CheckResult::pass(
                    "Lifecycle State",
                    &format!(
                        "Adapter lifecycle state '{}' passes constraint checks",
                        adapter.lifecycle_state
                    ),
                ));
            }
            Err(violations) => {
                let details = violations
                    .iter()
                    .map(|v| v.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ");
                checks.push(CheckResult::fail(
                    "Lifecycle State",
                    &format!("Lifecycle constraints failed: {}", details),
                    None,
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
    /// Preflight checks have not been completed
    PreflightNotCompleted,
    /// Preflight checks failed
    PreflightFailed,
    /// Target adapter missing required hash
    MissingHash,
    /// Training evidence snapshot missing
    MissingTrainingEvidence,
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
            Self::PreflightNotCompleted => write!(f, "Preflight checks have not been completed"),
            Self::PreflightFailed => write!(f, "Preflight checks failed"),
            Self::MissingHash => write!(f, "Target adapter missing required content hash"),
            Self::MissingTrainingEvidence => write!(f, "Training evidence snapshot missing"),
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
#[derive(Debug, Clone, Default)]
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
                        "Manifest Hash" => AliasSwapBlockReason::InvalidManifest,
                        "Preflight Status" => {
                            if check.message.to_ascii_lowercase().contains("failed") {
                                AliasSwapBlockReason::PreflightFailed
                            } else {
                                AliasSwapBlockReason::PreflightNotCompleted
                            }
                        }
                        "Lifecycle State" => AliasSwapBlockReason::InvalidLifecycleState,
                        "Repo/Branch Uniqueness" => AliasSwapBlockReason::ConflictingAdapters,
                        "System Mode" => AliasSwapBlockReason::MaintenanceMode,
                        "Tenant Isolation" => AliasSwapBlockReason::TenantIsolationViolation,
                        "Training Evidence" => AliasSwapBlockReason::MissingTrainingEvidence,
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

const AOS_HASH_BUFFER_SIZE: usize = 64 * 1024;

/// Verify adapter file integrity (basic checks without full parsing)
fn check_adapter_file_integrity(aos_path: &Path, expected_hash: Option<&str>) -> CheckResult {
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

                    if let Some(expected) = expected_hash.and_then(normalize_hash) {
                        match compute_aos_file_hash(aos_path) {
                            Ok(actual) => {
                                if actual != expected {
                                    return CheckResult::fail(
                                        "File Integrity",
                                        "Adapter file hash does not match registry",
                                        Some(
                                            "Re-register adapter to refresh file hash".to_string(),
                                        ),
                                    );
                                }
                            }
                            Err(e) => {
                                return CheckResult::fail(
                                    "File Integrity",
                                    &format!("Failed to compute adapter file hash: {}", e),
                                    None,
                                );
                            }
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

fn normalize_hash(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.trim_start_matches("b3:").to_ascii_lowercase())
}

fn compute_aos_file_hash(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; AOS_HASH_BUFFER_SIZE];
    let mut hasher = blake3::Hasher::new();

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
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

/// Gate an alias update operation behind readiness checks.
///
/// Alias updates that change routing should reuse the alias swap preflight checks
/// to ensure the target adapter is ready before any alias mutation.
pub async fn gate_alias_update_preflight(adapter_id: &str, db: &adapteros_db::Db) -> Result<()> {
    gate_alias_update_preflight_with_config(adapter_id, db, &AliasSwapGateConfig::default()).await
}

/// Gate an alias update with custom configuration.
pub async fn gate_alias_update_preflight_with_config(
    adapter_id: &str,
    db: &adapteros_db::Db,
    config: &AliasSwapGateConfig,
) -> Result<()> {
    gate_alias_swap_with_config(adapter_id, db, config).await
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

    // Run core readiness checks
    let mut readiness = check_adapter_swap_readiness_with_config(adapter_id, db, config).await?;
    if !config.skip_maintenance_check {
        readiness.checks.push(check_maintenance_mode());
    }

    readiness.ready = readiness
        .checks
        .iter()
        .all(|c| c.status != CheckStatus::Fail);
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

    // Run core readiness checks
    let mut readiness =
        check_adapter_swap_readiness_with_config(adapter_id, db, &AliasSwapGateConfig::default())
            .await?;
    readiness.checks.push(check_maintenance_mode());

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

// =============================================================================
// Activation Gating - Preflight checks before lifecycle activation
// =============================================================================

/// Reason why activation is blocked
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActivationBlockReason {
    /// Preflight checks have not been completed
    PreflightNotCompleted,
    /// Preflight checks failed
    PreflightFailed,
    /// Target adapter not found in registry
    AdapterNotFound,
    /// Target adapter file (.aos) does not exist on disk
    AdapterFileNotFound,
    /// Target adapter file is corrupted or unreadable
    AdapterFileCorrupted,
    /// Target adapter has invalid or missing manifest
    InvalidManifest,
    /// Target adapter missing required file hash
    MissingHash,
    /// Target adapter missing required content hash
    MissingContentHash,
    /// Training evidence snapshot missing
    MissingTrainingEvidence,
    /// Adapter not in valid state for activation (must be ready)
    InvalidLifecycleState,
    /// Conflicting active adapters for same repo/branch
    ConflictingAdapters,
    /// System is in maintenance mode
    MaintenanceMode,
    /// Activation would violate tenant isolation
    TenantIsolationViolation,
    /// Database error during validation
    DatabaseError,
    /// File readiness check failed
    FileReadinessCheckFailed,
}

impl std::fmt::Display for ActivationBlockReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreflightNotCompleted => write!(f, "Preflight checks have not been completed"),
            Self::PreflightFailed => write!(f, "Preflight checks failed"),
            Self::AdapterNotFound => write!(f, "Target adapter not found in registry"),
            Self::AdapterFileNotFound => write!(f, "Target adapter .aos file not found on disk"),
            Self::AdapterFileCorrupted => {
                write!(f, "Target adapter file is corrupted or unreadable")
            }
            Self::InvalidManifest => write!(f, "Target adapter has invalid or missing manifest"),
            Self::MissingHash => write!(f, "Target adapter missing required file hash"),
            Self::MissingContentHash => write!(f, "Target adapter missing required content hash"),
            Self::MissingTrainingEvidence => write!(f, "Training evidence snapshot missing"),
            Self::InvalidLifecycleState => {
                write!(f, "Adapter lifecycle state does not permit activation")
            }
            Self::ConflictingAdapters => {
                write!(f, "Conflicting active adapters exist for same repo/branch")
            }
            Self::MaintenanceMode => write!(f, "System is in maintenance mode"),
            Self::TenantIsolationViolation => {
                write!(f, "Activation would violate tenant isolation boundaries")
            }
            Self::DatabaseError => write!(f, "Database error during validation"),
            Self::FileReadinessCheckFailed => write!(f, "File readiness validation failed"),
        }
    }
}

/// Configuration for activation gating behavior
#[derive(Debug, Clone, Default)]
pub struct ActivationGateConfig {
    /// Skip maintenance mode check
    pub skip_maintenance_check: bool,
    /// Skip conflict detection
    pub skip_conflict_check: bool,
    /// Skip file readiness validation
    pub skip_file_readiness: bool,
    /// Tenant ID for isolation checks
    pub tenant_id: Option<String>,
    /// Allow activation from training state (normally must be ready)
    pub allow_from_training: bool,
}

/// Result of activation gating preflight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationGateResult {
    /// Adapter ID being checked
    pub adapter_id: String,
    /// Whether activation is allowed
    pub allowed: bool,
    /// Blocking reasons (if any)
    pub block_reasons: Vec<ActivationBlockReason>,
    /// Warning messages (non-blocking)
    pub warnings: Vec<String>,
    /// Individual check results
    pub checks: Vec<CheckResult>,
    /// Suggested remediation steps
    pub remediation: Vec<String>,
    /// Time taken for checks in milliseconds
    pub check_duration_ms: u64,
}

impl ActivationGateResult {
    /// Get a formatted error message for blocked activation
    pub fn error_message(&self) -> String {
        if self.allowed {
            return String::new();
        }

        let reasons: Vec<String> = self
            .block_reasons
            .iter()
            .map(|r| format!("  - {}", r))
            .collect();

        let failed_checks: Vec<String> = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .map(|c| format!("  - {}: {}", c.name, c.message))
            .collect();

        let mut msg = format!(
            "Activation blocked for adapter '{}'.\n\nReasons:\n{}\n\nFailed checks:\n{}",
            self.adapter_id,
            reasons.join("\n"),
            failed_checks.join("\n")
        );

        if !self.remediation.is_empty() {
            msg.push_str(&format!(
                "\n\nRemediation:\n  {}",
                self.remediation.join("\n  ")
            ));
        }

        msg
    }
}

/// Check if an adapter is ready for activation (transition to 'active' state)
///
/// This function performs comprehensive preflight checks before allowing
/// an adapter to be activated. The adapter must:
/// - Exist in the registry
/// - Have a valid .aos file on disk
/// - Be in 'ready' lifecycle state (or 'training' if allow_from_training=true)
/// - Pass file integrity checks
/// - Have content hash recorded for reproducibility
/// - Have training snapshot evidence recorded
/// - Not conflict with other active adapters
///
/// # Arguments
/// * `adapter_id` - The adapter ID to check
/// * `db` - Database connection
/// * `config` - Optional configuration for gating behavior
///
/// # Returns
/// * `ActivationGateResult` - Detailed results of all checks
pub async fn check_activation_readiness(
    adapter_id: &str,
    db: &adapteros_db::Db,
    config: Option<ActivationGateConfig>,
) -> Result<ActivationGateResult> {
    let config = config.unwrap_or_default();
    let start = Instant::now();
    let mut checks = Vec::new();
    let mut block_reasons = Vec::new();
    let mut warnings = Vec::new();
    let mut remediation = Vec::new();
    let mut lifecycle_state = None;
    let mut has_aos_path = false;
    let mut aos_file_exists = false;
    let mut has_aos_hash = false;
    let mut has_content_hash = false;
    let mut file_readiness_failed = false;
    let mut has_training_evidence = false;
    let mut conflicting_adapter_ids: Vec<String> = Vec::new();

    // Check 1: System maintenance mode
    if !config.skip_maintenance_check {
        let maintenance_check = check_maintenance_mode();
        if maintenance_check.status == CheckStatus::Fail {
            block_reasons.push(ActivationBlockReason::MaintenanceMode);
            if let Some(ref fix) = maintenance_check.fix_command {
                remediation.push(fix.clone());
            }
        }
        checks.push(maintenance_check);
    }

    // Check 2: Adapter exists
    #[allow(deprecated)]
    let adapter = match config.tenant_id.as_deref() {
        Some(tenant_id) => db.get_adapter_for_tenant(tenant_id, adapter_id).await?,
        None => db.get_adapter(adapter_id).await?,
    };
    let adapter = match adapter {
        Some(a) => {
            checks.push(CheckResult::pass(
                "Adapter Exists",
                &format!("Adapter '{}' found in registry", adapter_id),
            ));
            a
        }
        None => {
            checks.push(CheckResult::fail(
                "Adapter Exists",
                &format!("Adapter '{}' not found in registry", adapter_id),
                Some(format!(
                    "aosctl adapter register <path> --id {}",
                    adapter_id
                )),
            ));
            block_reasons.push(ActivationBlockReason::AdapterNotFound);
            remediation.push(format!(
                "Register the adapter first: aosctl adapter register <path> --id {}",
                adapter_id
            ));

            return Ok(ActivationGateResult {
                adapter_id: adapter_id.to_string(),
                allowed: false,
                block_reasons,
                warnings,
                checks,
                remediation,
                check_duration_ms: start.elapsed().as_millis() as u64,
            });
        }
    };

    // Check 3: Tenant isolation (if configured)
    if let Some(ref tenant_id) = config.tenant_id {
        if adapter.tenant_id != *tenant_id {
            checks.push(CheckResult::fail(
                "Tenant Isolation",
                &format!(
                    "Adapter tenant '{}' does not match requested tenant '{}'",
                    adapter.tenant_id, tenant_id
                ),
                None,
            ));
            block_reasons.push(ActivationBlockReason::TenantIsolationViolation);
        } else {
            checks.push(CheckResult::pass(
                "Tenant Isolation",
                "Adapter tenant matches requested tenant",
            ));
        }
    }

    // Check 4: Lifecycle state must allow activation
    match LifecycleState::from_str(&adapter.lifecycle_state) {
        Ok(state) => {
            lifecycle_state = Some(state);
            if state.is_terminal() {
                let recovery_hint = if state == LifecycleState::Retired {
                    "Retired adapters cannot be reactivated. Create a new adapter version instead."
                } else {
                    "Failed adapters cannot be reactivated. Investigate the failure cause and retrain."
                };
                checks.push(CheckResult::fail(
                    "Lifecycle State",
                    &format!(
                        "Adapter in terminal state '{}' - cannot be activated",
                        adapter.lifecycle_state
                    ),
                    Some(recovery_hint.to_string()),
                ));
                block_reasons.push(ActivationBlockReason::InvalidLifecycleState);
            } else {
                let mut lifecycle_ok =
                    LifecycleTransition::new(state, LifecycleState::Active).is_valid();
                if config.allow_from_training && state == LifecycleState::Training {
                    lifecycle_ok = true;
                }

                if lifecycle_ok {
                    checks.push(CheckResult::pass(
                        "Lifecycle State",
                        &format!(
                            "Adapter in '{}' state - valid for activation",
                            adapter.lifecycle_state
                        ),
                    ));
                } else {
                    let allowed_states = if config.allow_from_training {
                        "ready/active/training"
                    } else {
                        "ready/active"
                    };
                    checks.push(CheckResult::fail(
                        "Lifecycle State",
                        &format!(
                            "Adapter in '{}' state - must be {} before activation",
                            adapter.lifecycle_state, allowed_states
                        ),
                        Some(format!(
                            "Complete preflight and transition to 'ready' state first:\n  aosctl adapter update-lifecycle {} ready",
                            adapter_id
                        )),
                    ));
                    block_reasons.push(ActivationBlockReason::InvalidLifecycleState);
                    remediation.push(format!(
                        "Transition adapter to 'ready' state: aosctl adapter update-lifecycle {} ready",
                        adapter_id
                    ));
                }
            }
        }
        Err(_) => {
            checks.push(CheckResult::fail(
                "Lifecycle State",
                &format!(
                    "Adapter lifecycle state '{}' is not recognized",
                    adapter.lifecycle_state
                ),
                None,
            ));
            block_reasons.push(ActivationBlockReason::InvalidLifecycleState);
        }
    }

    // Check 5: .aos file path is set
    let aos_path = if adapter
        .aos_file_path
        .as_ref()
        .map(|p| !p.is_empty())
        .unwrap_or(false)
    {
        has_aos_path = true;
        checks.push(CheckResult::pass(
            "AOS File Path",
            "Adapter has .aos file path configured",
        ));
        adapter.aos_file_path.as_ref().map(PathBuf::from)
    } else {
        checks.push(CheckResult::fail(
            "AOS File Path",
            "Adapter missing .aos file path - preflight incomplete",
            Some("Register adapter with --aos-file-path or complete training".to_string()),
        ));
        block_reasons.push(ActivationBlockReason::PreflightNotCompleted);
        remediation.push("Ensure adapter has .aos file path set during registration".to_string());
        None
    };

    // Check 6: .aos file exists on disk
    if let Some(ref path) = aos_path {
        if path.exists() {
            aos_file_exists = true;
            checks.push(CheckResult::pass(
                "AOS File Exists",
                &format!("Adapter file found at {}", path.display()),
            ));

            // Check 7: File readiness (if not skipped)
            if !config.skip_file_readiness {
                let integrity_check =
                    check_adapter_file_integrity(path, adapter.aos_file_hash.as_deref());
                if integrity_check.status == CheckStatus::Fail {
                    file_readiness_failed = true;
                    block_reasons.push(ActivationBlockReason::FileReadinessCheckFailed);
                    if let Some(ref fix) = integrity_check.fix_command {
                        remediation.push(fix.clone());
                    }
                } else if integrity_check.status == CheckStatus::Warning {
                    warnings.push(integrity_check.message.clone());
                }
                checks.push(integrity_check);
            }
        } else {
            checks.push(CheckResult::fail(
                "AOS File Exists",
                &format!("Adapter file not found at {}", path.display()),
                Some(format!("Ensure .aos file exists at {}", path.display())),
            ));
            block_reasons.push(ActivationBlockReason::AdapterFileNotFound);
            remediation.push(format!("Ensure adapter file exists at: {}", path.display()));
        }
    }

    // Check 8: .aos file hash is set (required for activation integrity)
    if adapter
        .aos_file_hash
        .as_ref()
        .map(|h| !h.is_empty())
        .unwrap_or(false)
    {
        has_aos_hash = true;
        checks.push(CheckResult::pass(
            "AOS File Hash",
            "Adapter has file hash for integrity verification",
        ));
    } else {
        checks.push(CheckResult::fail(
            "AOS File Hash",
            "Adapter missing file hash - preflight incomplete",
            Some("Re-register adapter to compute file hash".to_string()),
        ));
        block_reasons.push(ActivationBlockReason::MissingHash);
        remediation.push("Re-register the adapter to compute file hash".to_string());
    }

    // Check 9: Content hash is set (required for reproducibility)
    if adapter
        .content_hash_b3
        .as_ref()
        .map(|h| !h.trim().is_empty())
        .unwrap_or(false)
    {
        has_content_hash = true;
        checks.push(CheckResult::pass(
            "Content Hash",
            "Adapter has content hash for reproducibility verification",
        ));
    } else {
        checks.push(CheckResult::fail(
            "Content Hash",
            "Adapter missing content hash - preflight incomplete",
            Some("Re-register adapter to compute content hash".to_string()),
        ));
        block_reasons.push(ActivationBlockReason::MissingContentHash);
        remediation.push("Re-register the adapter to compute content hash".to_string());
    }

    // Check 10: Training evidence snapshot exists
    match db.get_adapter_training_snapshot(adapter_id).await? {
        Some(_) => {
            has_training_evidence = true;
            checks.push(CheckResult::pass(
                "Training Evidence",
                "Training snapshot evidence found",
            ));
        }
        None => {
            checks.push(CheckResult::fail(
                "Training Evidence",
                "Training snapshot evidence missing",
                Some("Re-run training to record snapshot evidence".to_string()),
            ));
            block_reasons.push(ActivationBlockReason::MissingTrainingEvidence);
            remediation.push("Re-run training to record snapshot evidence".to_string());
        }
    }

    // Check 11: Conflicting adapters (if not skipped)
    if !config.skip_conflict_check {
        if let Some(ref repo_id) = adapter.repo_id {
            let adapter_branch = extract_branch_from_metadata(&adapter.metadata_json);

            let active_adapters = db
                .list_active_adapters_for_repo(repo_id)
                .await
                .unwrap_or_default();

            let conflicting: Vec<_> = active_adapters
                .iter()
                .filter(|(other_id, other_branch)| {
                    if other_id == adapter_id {
                        return false;
                    }
                    match (&adapter_branch, other_branch) {
                        (Some(req), Some(other)) => req == other,
                        (Some(_), None) => true,
                        (None, _) => true,
                    }
                })
                .collect();
            conflicting_adapter_ids = conflicting.iter().map(|(id, _)| id.to_string()).collect();

            if conflicting_adapter_ids.is_empty() {
                checks.push(CheckResult::pass(
                    "No Conflicts",
                    "No conflicting active adapters for this repo/branch",
                ));
            } else {
                checks.push(CheckResult::fail(
                    "No Conflicts",
                    &format!("Conflicting active adapters: {:?}", conflicting_adapter_ids),
                    Some(format!(
                        "Deactivate conflicting adapter first:\n  aosctl adapter update-lifecycle {} deprecated",
                        conflicting_adapter_ids.first().map(|id| id.as_str()).unwrap_or("")
                    )),
                ));
                block_reasons.push(ActivationBlockReason::ConflictingAdapters);
                remediation.push(format!(
                    "Deactivate conflicting adapter: aosctl adapter update-lifecycle {} deprecated",
                    conflicting_adapter_ids
                        .first()
                        .map(|id| id.as_str())
                        .unwrap_or("")
                ));
            }
        } else {
            checks.push(CheckResult::pass(
                "No Conflicts",
                "Adapter not linked to repository (no conflict possible)",
            ));
        }
    }

    let has_artifact = has_aos_path && aos_file_exists && has_aos_hash && has_content_hash;
    let preflight_status = if !has_artifact {
        PreflightStatus::Pending
    } else if !config.skip_file_readiness && file_readiness_failed {
        PreflightStatus::Failed
    } else {
        PreflightStatus::Passed
    };

    if let Some(state) = lifecycle_state {
        let from_state = if config.allow_from_training && state == LifecycleState::Training {
            LifecycleState::Ready
        } else {
            state
        };
        let ctx = ValidationContext::new()
            .with_tier(adapter.tier.clone())
            .with_preflight_status(preflight_status)
            .with_artifact(has_artifact)
            .with_training_evidence(has_training_evidence)
            .with_conflicting_adapters(conflicting_adapter_ids.clone());
        if let Err(violations) =
            validate_transition_with_context(from_state, LifecycleState::Active, &ctx)
        {
            let mut push_reason = |reason: ActivationBlockReason| {
                if !block_reasons.contains(&reason) {
                    block_reasons.push(reason);
                }
            };

            for violation in violations {
                match violation.rule.as_str() {
                    "state_transition" => {
                        push_reason(ActivationBlockReason::InvalidLifecycleState);
                    }
                    "preflight_required" => match preflight_status {
                        PreflightStatus::Failed => {
                            push_reason(ActivationBlockReason::PreflightFailed);
                        }
                        _ => push_reason(ActivationBlockReason::PreflightNotCompleted),
                    },
                    "artifact_required" => {
                        push_reason(ActivationBlockReason::PreflightNotCompleted);
                    }
                    "training_evidence_required" => {
                        push_reason(ActivationBlockReason::MissingTrainingEvidence);
                    }
                    "single_active_per_repo" => {
                        push_reason(ActivationBlockReason::ConflictingAdapters);
                    }
                    _ => {}
                }
            }
        }
    }

    let allowed = block_reasons.is_empty();

    Ok(ActivationGateResult {
        adapter_id: adapter_id.to_string(),
        allowed,
        block_reasons,
        warnings,
        checks,
        remediation,
        check_duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Gate adapter activation behind preflight checks
///
/// This is the main entry point for enforcing preflight before activation.
/// Returns Ok(()) if all checks pass, or an error with detailed information
/// about what failed and how to fix it.
///
/// # Arguments
/// * `adapter_id` - The adapter ID to activate
/// * `db` - Database connection
///
/// # Returns
/// * `Ok(())` - Activation is allowed
/// * `Err(e)` - Activation is blocked with detailed error message
///
/// # Example
/// ```no_run
/// # use adapteros_db::Db;
/// # async fn example(db: &Db) -> anyhow::Result<()> {
/// use adapteros_cli::commands::preflight::gate_activation;
///
/// // This will fail if preflight checks haven't passed
/// gate_activation("my-adapter", db).await?;
///
/// // Only reaches here if all checks passed - safe to activate
/// // db.update_adapter_lifecycle_state("my-adapter", LifecycleState::Active).await?;
/// # Ok(())
/// # }
/// ```
pub async fn gate_activation(adapter_id: &str, db: &adapteros_db::Db) -> Result<()> {
    gate_activation_with_config(adapter_id, db, &ActivationGateConfig::default()).await
}

/// Gate activation with custom configuration
///
/// Allows fine-grained control over which checks are performed.
pub async fn gate_activation_with_config(
    adapter_id: &str,
    db: &adapteros_db::Db,
    config: &ActivationGateConfig,
) -> Result<()> {
    let result = check_activation_readiness(adapter_id, db, Some(config.clone())).await?;

    if result.allowed {
        if !result.warnings.is_empty() {
            tracing::warn!(
                adapter_id = adapter_id,
                warnings = ?result.warnings,
                "Activation allowed with warnings"
            );
        }
        tracing::info!(
            adapter_id = adapter_id,
            check_duration_ms = result.check_duration_ms,
            "Preflight checks passed - activation allowed"
        );
        Ok(())
    } else {
        tracing::error!(
            adapter_id = adapter_id,
            block_reasons = ?result.block_reasons,
            "Activation blocked by preflight checks"
        );
        Err(anyhow::anyhow!("{}", result.error_message()))
    }
}

/// Display activation preflight results to the user
pub fn display_activation_preflight_results(result: &ActivationGateResult, output: &OutputWriter) {
    output.blank();
    output.info(format!(
        "Activation Preflight Results for: {}",
        result.adapter_id
    ));
    output.info("─".repeat(50));

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
            output.success("ACTIVATION ALLOWED: All preflight checks passed");
        } else {
            output.warning(format!(
                "ACTIVATION ALLOWED with {} warning(s):",
                result.warnings.len()
            ));
            for warning in &result.warnings {
                output.warning(format!("  - {}", warning));
            }
        }
    } else {
        output.error("ACTIVATION BLOCKED:");
        for reason in &result.block_reasons {
            output.error(format!("  - {}", reason));
        }

        if !result.remediation.is_empty() {
            output.blank();
            output.info("Remediation steps:");
            for (i, fix) in result.remediation.iter().enumerate() {
                output.info(format!("  {}. {}", i + 1, fix));
            }
        }
    }
}

// =============================================================================
// Deterministic Adapter Swap Gating
// =============================================================================

/// Reason why a deterministic adapter swap is blocked
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeterminismBlockReason {
    /// Adapter not found in registry
    AdapterNotFound,
    /// Adapter missing content_hash_b3 (required for reproducibility)
    MissingContentHash,
    /// Lifecycle state does not support deterministic execution
    InvalidLifecycleState,
    /// No seed override configured (required for strict determinism)
    NoSeedOverride,
    /// Database error during validation
    DatabaseError,
}

impl std::fmt::Display for DeterminismBlockReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdapterNotFound => write!(f, "Adapter not found in registry"),
            Self::MissingContentHash => {
                write!(f, "Adapter missing content_hash_b3 for reproducibility")
            }
            Self::InvalidLifecycleState => {
                write!(
                    f,
                    "Lifecycle state does not support deterministic execution"
                )
            }
            Self::NoSeedOverride => {
                write!(
                    f,
                    "No seed override configured (ADAPTEROS_SEED_OVERRIDE or AOS_SEED_OVERRIDE)"
                )
            }
            Self::DatabaseError => write!(f, "Database error during validation"),
        }
    }
}

/// Configuration for deterministic swap gating behavior
#[derive(Debug, Clone, Default)]
pub struct DeterministicSwapGateConfig {
    /// Require seed override environment variable for strict determinism
    pub require_seed_override: bool,
}

impl DeterministicSwapGateConfig {
    /// Create a strict configuration that requires seed override
    pub fn strict() -> Self {
        Self {
            require_seed_override: true,
        }
    }
}

/// Result of deterministic swap readiness check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicSwapReadiness {
    /// Whether the adapter is ready for deterministic swap
    pub ready: bool,
    /// Adapter ID that was checked
    pub adapter_id: String,
    /// Blocking issues that prevent deterministic execution
    pub blocking_issues: Vec<DeterminismBlockReason>,
    /// Time taken for checks in milliseconds
    #[serde(default)]
    pub check_duration_ms: u64,
}

impl DeterministicSwapReadiness {
    /// Get a formatted error message for blocked deterministic swap
    pub fn error_message(&self) -> String {
        if self.ready {
            return String::new();
        }

        let reasons: Vec<String> = self
            .blocking_issues
            .iter()
            .map(|r| format!("  - {}", r))
            .collect();

        format!(
            "Deterministic swap blocked for adapter '{}'.\n\nBlocking issues:\n{}",
            self.adapter_id,
            reasons.join("\n")
        )
    }
}

/// Check if a lifecycle state is compatible with deterministic execution
///
/// Deterministic execution requires stable, verified adapter state.
/// Only `ready` and `active` states are considered stable enough.
fn is_determinism_compatible(lifecycle_state: &str) -> bool {
    LifecycleState::from_str(lifecycle_state)
        .map(|state| state.is_determinism_compatible())
        .unwrap_or(false)
}

/// Gates adapter swap operations that require determinism.
///
/// Validates that the adapter has all required fields for reproducible execution:
/// 1. Adapter exists in registry
/// 2. Adapter has content_hash_b3 set (for integrity verification)
/// 3. Lifecycle state allows deterministic execution (ready or active)
/// 4. Seed override configured (for strict mode only)
///
/// # Arguments
/// * `db` - Database connection
/// * `adapter_id` - The adapter ID to validate
/// * `config` - Configuration for gating behavior
///
/// # Returns
/// * `DeterministicSwapReadiness` - Result indicating if adapter is ready for deterministic swap
///
/// # Example
/// ```no_run
/// # use adapteros_db::Db;
/// # async fn example(db: &Db) -> anyhow::Result<()> {
/// use adapteros_cli::commands::preflight::{gate_deterministic_adapter_swap, DeterministicSwapGateConfig};
///
/// let config = DeterministicSwapGateConfig::default();
/// let result = gate_deterministic_adapter_swap(db, "my-adapter", &config).await?;
/// if result.ready {
///     // Proceed with deterministic swap
/// }
/// # Ok(())
/// # }
/// ```
pub async fn gate_deterministic_adapter_swap(
    db: &adapteros_db::Db,
    adapter_id: &str,
    config: &DeterministicSwapGateConfig,
) -> Result<DeterministicSwapReadiness> {
    let start = Instant::now();
    let mut blocking_issues = Vec::new();

    // Check 1: Adapter exists and retrieve it
    #[allow(deprecated)]
    let adapter = match db.get_adapter(adapter_id).await? {
        Some(a) => a,
        None => {
            blocking_issues.push(DeterminismBlockReason::AdapterNotFound);
            return Ok(DeterministicSwapReadiness {
                ready: false,
                adapter_id: adapter_id.to_string(),
                blocking_issues,
                check_duration_ms: start.elapsed().as_millis() as u64,
            });
        }
    };

    // Check 2: Adapter has content_hash_b3 (required for reproducibility)
    if adapter
        .content_hash_b3
        .as_ref()
        .map(|h| h.is_empty())
        .unwrap_or(true)
    {
        blocking_issues.push(DeterminismBlockReason::MissingContentHash);
    }

    // Check 3: Lifecycle state allows deterministic execution
    if !is_determinism_compatible(&adapter.lifecycle_state) {
        blocking_issues.push(DeterminismBlockReason::InvalidLifecycleState);
    }

    // Check 4: Seed override configured (for strict mode)
    if config.require_seed_override {
        let has_seed_override = std::env::var("ADAPTEROS_SEED_OVERRIDE").is_ok()
            || std::env::var("AOS_SEED_OVERRIDE").is_ok();
        if !has_seed_override {
            blocking_issues.push(DeterminismBlockReason::NoSeedOverride);
        }
    }

    let ready = blocking_issues.is_empty();
    let check_duration_ms = start.elapsed().as_millis() as u64;

    if ready {
        tracing::debug!(
            adapter_id = adapter_id,
            check_duration_ms = check_duration_ms,
            "Deterministic swap readiness check passed"
        );
    } else {
        tracing::warn!(
            adapter_id = adapter_id,
            blocking_issues = ?blocking_issues,
            "Deterministic swap blocked"
        );
    }

    Ok(DeterministicSwapReadiness {
        ready,
        adapter_id: adapter_id.to_string(),
        blocking_issues,
        check_duration_ms,
    })
}

/// Require deterministic swap readiness, returning an error if not ready
///
/// This is a convenience wrapper that returns an error if the adapter
/// is not ready for deterministic execution.
pub async fn require_deterministic_swap_ready(
    db: &adapteros_db::Db,
    adapter_id: &str,
    config: &DeterministicSwapGateConfig,
) -> Result<()> {
    let result = gate_deterministic_adapter_swap(db, adapter_id, config).await?;

    if result.ready {
        Ok(())
    } else {
        Err(anyhow::anyhow!("{}", result.error_message()))
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
        assert!(!config.allow_training_state);
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
        let result = check_adapter_file_integrity(Path::new("/nonexistent/path/adapter.aos"), None);
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

    // =========================================================================
    // Activation Gating Tests
    // =========================================================================

    #[test]
    fn test_activation_block_reason_display() {
        assert_eq!(
            ActivationBlockReason::PreflightNotCompleted.to_string(),
            "Preflight checks have not been completed"
        );
        assert_eq!(
            ActivationBlockReason::PreflightFailed.to_string(),
            "Preflight checks failed"
        );
        assert_eq!(
            ActivationBlockReason::AdapterNotFound.to_string(),
            "Target adapter not found in registry"
        );
        assert_eq!(
            ActivationBlockReason::AdapterFileNotFound.to_string(),
            "Target adapter .aos file not found on disk"
        );
        assert_eq!(
            ActivationBlockReason::InvalidLifecycleState.to_string(),
            "Adapter lifecycle state does not permit activation"
        );
        assert_eq!(
            ActivationBlockReason::ConflictingAdapters.to_string(),
            "Conflicting active adapters exist for same repo/branch"
        );
        assert_eq!(
            ActivationBlockReason::MaintenanceMode.to_string(),
            "System is in maintenance mode"
        );
        assert_eq!(
            ActivationBlockReason::MissingHash.to_string(),
            "Target adapter missing required file hash"
        );
        assert_eq!(
            ActivationBlockReason::MissingContentHash.to_string(),
            "Target adapter missing required content hash"
        );
        assert_eq!(
            ActivationBlockReason::MissingTrainingEvidence.to_string(),
            "Training evidence snapshot missing"
        );
        assert_eq!(
            ActivationBlockReason::FileReadinessCheckFailed.to_string(),
            "File readiness validation failed"
        );
    }

    #[test]
    fn test_activation_gate_config_default() {
        let config = ActivationGateConfig::default();
        assert!(!config.skip_maintenance_check);
        assert!(!config.skip_conflict_check);
        assert!(!config.skip_file_readiness);
        assert!(config.tenant_id.is_none());
        assert!(!config.allow_from_training);
    }

    #[test]
    fn test_activation_gate_result_allowed() {
        let result = ActivationGateResult {
            adapter_id: "test-adapter".to_string(),
            allowed: true,
            block_reasons: vec![],
            warnings: vec![],
            checks: vec![
                CheckResult::pass("Adapter Exists", "found"),
                CheckResult::pass("Lifecycle State", "ready"),
            ],
            remediation: vec![],
            check_duration_ms: 10,
        };

        assert!(result.allowed);
        assert!(result.block_reasons.is_empty());
        assert!(result.error_message().is_empty());
    }

    #[test]
    fn test_activation_gate_result_blocked() {
        let result = ActivationGateResult {
            adapter_id: "test-adapter".to_string(),
            allowed: false,
            block_reasons: vec![
                ActivationBlockReason::InvalidLifecycleState,
                ActivationBlockReason::MissingContentHash,
            ],
            warnings: vec![],
            checks: vec![
                CheckResult::pass("Adapter Exists", "found"),
                CheckResult::fail("Lifecycle State", "draft state", None),
                CheckResult::fail("Content Hash", "missing content hash", None),
            ],
            remediation: vec![
                "Transition to ready state".to_string(),
                "Re-register adapter".to_string(),
            ],
            check_duration_ms: 15,
        };

        assert!(!result.allowed);
        assert_eq!(result.block_reasons.len(), 2);

        let error_msg = result.error_message();
        assert!(error_msg.contains("Activation blocked"));
        assert!(error_msg.contains("test-adapter"));
        assert!(error_msg.contains("Adapter lifecycle state does not permit activation"));
        assert!(error_msg.contains("missing required content hash"));
        assert!(error_msg.contains("Remediation"));
    }

    #[test]
    fn test_activation_gate_result_with_warnings() {
        let result = ActivationGateResult {
            adapter_id: "test-adapter".to_string(),
            allowed: true,
            block_reasons: vec![],
            warnings: vec!["File is small".to_string()],
            checks: vec![CheckResult::pass("Adapter Exists", "found")],
            remediation: vec![],
            check_duration_ms: 5,
        };

        assert!(result.allowed);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.error_message().is_empty()); // No error when allowed
    }

    #[test]
    fn test_activation_gate_config_with_training() {
        let mut config = ActivationGateConfig::default();
        config.allow_from_training = true;

        assert!(config.allow_from_training);
    }

    #[test]
    fn test_activation_block_reason_variants() {
        // Ensure all variants are properly defined
        let reasons = vec![
            ActivationBlockReason::PreflightNotCompleted,
            ActivationBlockReason::PreflightFailed,
            ActivationBlockReason::AdapterNotFound,
            ActivationBlockReason::AdapterFileNotFound,
            ActivationBlockReason::AdapterFileCorrupted,
            ActivationBlockReason::InvalidManifest,
            ActivationBlockReason::MissingHash,
            ActivationBlockReason::MissingContentHash,
            ActivationBlockReason::MissingTrainingEvidence,
            ActivationBlockReason::InvalidLifecycleState,
            ActivationBlockReason::ConflictingAdapters,
            ActivationBlockReason::MaintenanceMode,
            ActivationBlockReason::TenantIsolationViolation,
            ActivationBlockReason::DatabaseError,
            ActivationBlockReason::FileReadinessCheckFailed,
        ];

        for reason in reasons {
            // Verify each reason has a non-empty display string
            let display = reason.to_string();
            assert!(!display.is_empty(), "Reason {:?} has empty display", reason);
        }
    }

    // =========================================================================
    // Deterministic Swap Gating Tests
    // =========================================================================

    #[test]
    fn test_determinism_block_reason_display() {
        assert_eq!(
            DeterminismBlockReason::AdapterNotFound.to_string(),
            "Adapter not found in registry"
        );
        assert_eq!(
            DeterminismBlockReason::MissingContentHash.to_string(),
            "Adapter missing content_hash_b3 for reproducibility"
        );
        assert_eq!(
            DeterminismBlockReason::InvalidLifecycleState.to_string(),
            "Lifecycle state does not support deterministic execution"
        );
        assert_eq!(
            DeterminismBlockReason::NoSeedOverride.to_string(),
            "No seed override configured (ADAPTEROS_SEED_OVERRIDE or AOS_SEED_OVERRIDE)"
        );
        assert_eq!(
            DeterminismBlockReason::DatabaseError.to_string(),
            "Database error during validation"
        );
    }

    #[test]
    fn test_deterministic_swap_gate_config_default() {
        let config = DeterministicSwapGateConfig::default();
        assert!(!config.require_seed_override);
    }

    #[test]
    fn test_deterministic_swap_gate_config_strict() {
        let config = DeterministicSwapGateConfig::strict();
        assert!(config.require_seed_override);
    }

    #[test]
    fn test_deterministic_swap_readiness_ready() {
        let result = DeterministicSwapReadiness {
            ready: true,
            adapter_id: "test-adapter".to_string(),
            blocking_issues: vec![],
            check_duration_ms: 5,
        };

        assert!(result.ready);
        assert!(result.blocking_issues.is_empty());
        assert!(result.error_message().is_empty());
    }

    #[test]
    fn test_deterministic_swap_readiness_blocked() {
        let result = DeterministicSwapReadiness {
            ready: false,
            adapter_id: "test-adapter".to_string(),
            blocking_issues: vec![
                DeterminismBlockReason::MissingContentHash,
                DeterminismBlockReason::InvalidLifecycleState,
            ],
            check_duration_ms: 10,
        };

        assert!(!result.ready);
        assert_eq!(result.blocking_issues.len(), 2);

        let error_msg = result.error_message();
        assert!(error_msg.contains("Deterministic swap blocked"));
        assert!(error_msg.contains("test-adapter"));
        assert!(error_msg.contains("content_hash_b3"));
        assert!(error_msg.contains("Lifecycle state"));
    }

    #[test]
    fn test_is_determinism_compatible() {
        // Compatible states
        assert!(is_determinism_compatible("ready"));
        assert!(is_determinism_compatible("Ready"));
        assert!(is_determinism_compatible("READY"));
        assert!(is_determinism_compatible("active"));
        assert!(is_determinism_compatible("Active"));
        assert!(is_determinism_compatible("ACTIVE"));

        // Incompatible states
        assert!(!is_determinism_compatible("draft"));
        assert!(!is_determinism_compatible("training"));
        assert!(!is_determinism_compatible("deprecated"));
        assert!(!is_determinism_compatible("retired"));
        assert!(!is_determinism_compatible("failed"));
    }

    #[test]
    fn test_determinism_block_reason_variants() {
        // Ensure all variants are properly defined
        let reasons = vec![
            DeterminismBlockReason::AdapterNotFound,
            DeterminismBlockReason::MissingContentHash,
            DeterminismBlockReason::InvalidLifecycleState,
            DeterminismBlockReason::NoSeedOverride,
            DeterminismBlockReason::DatabaseError,
        ];

        for reason in reasons {
            // Verify each reason has a non-empty display string
            let display = reason.to_string();
            assert!(!display.is_empty(), "Reason {:?} has empty display", reason);
        }
    }
}
