//! System diagnostics command

use crate::commands::infer::uds_infer_url_string;
use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use anyhow::Context;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::System;
use tracing::{error, info, warn};

/// Diagnostic command variants
#[derive(Debug, Subcommand, Clone)]
pub enum DiagCommand {
    /// Run system diagnostics
    #[command(after_help = "\
Examples:
  aosctl diag run --full
  aosctl diag run --system
  aosctl diag run --tenant dev
  aosctl diag run --full --bundle ./diag_bundle.zip
")]
    Run {
        /// Diagnostic profile: system, tenant, or full
        #[arg(long, default_value = "full")]
        profile: Option<String>,

        /// Tenant ID for tenant-specific checks
        #[arg(long)]
        tenant: Option<String>,

        /// Output JSON format
        #[arg(long)]
        json: bool,

        /// Create diagnostic bundle
        #[arg(long)]
        bundle: Option<PathBuf>,

        /// Filter telemetry bundles to a specific CPID (alias: --trace-id)
        #[arg(long, alias = "trace-id", requires = "bundle")]
        cpid: Option<String>,

        /// Include full database file in bundle (var/aos-cp.sqlite3)
        #[arg(long, requires = "bundle")]
        full_db: bool,

        /// System checks only
        #[arg(long, conflicts_with_all = ["tenant_only", "profile"])]
        system: bool,

        /// Tenant checks only
        #[arg(long, conflicts_with_all = ["system", "profile"])]
        tenant_only: bool,

        /// Full diagnostics (default)
        #[arg(long, conflicts_with_all = ["system", "tenant_only", "profile"])]
        full: bool,
    },

    /// Export a signed diagnostic bundle via API
    #[command(after_help = "\
Examples:
  aosctl diag export --trace-id trace-abc123 -o bundle.tar.zst
  aosctl diag export --trace-id trace-abc123 -o bundle.zip --format zip
  aosctl diag export --trace-id trace-abc123 -o bundle.tar.zst --include-evidence --evidence-token <token>
")]
    Export {
        /// Trace ID to export
        #[arg(long)]
        trace_id: String,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,

        /// Bundle format: tar.zst or zip
        #[arg(long, default_value = "tar.zst")]
        format: String,

        /// Include evidence payload (requires token)
        #[arg(long)]
        include_evidence: bool,

        /// Evidence authorization token
        #[arg(long)]
        evidence_token: Option<String>,

        /// API base URL
        #[arg(long, env = "AOS_API_URL", default_value = "http://127.0.0.1:8080")]
        base_url: String,
    },

    /// Verify a diagnostic bundle offline
    #[command(after_help = "\
Examples:
  aosctl diag verify bundle.tar.zst
  aosctl diag verify bundle.tar.zst --verbose
")]
    Verify {
        /// Bundle path to verify
        bundle: PathBuf,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
}

/// Handle diagnostic commands
pub async fn handle_diag_command(cmd: DiagCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        DiagCommand::Run {
            profile,
            tenant,
            json,
            bundle,
            cpid,
            full_db,
            system,
            tenant_only,
            full,
        } => {
            let diag_profile = if system {
                DiagProfile::System
            } else if tenant_only {
                DiagProfile::Tenant
            } else if full {
                DiagProfile::Full
            } else if let Some(p) = profile {
                match p.as_str() {
                    "system" => DiagProfile::System,
                    "tenant" => DiagProfile::Tenant,
                    "full" => DiagProfile::Full,
                    _ => {
                        return Err(AosError::validation(format!(
                            "Invalid profile: {}. Use: system, tenant, or full",
                            p
                        )))
                    }
                }
            } else {
                DiagProfile::Full
            };

            run(diag_profile, tenant, json, bundle, cpid, full_db).await
        }
        DiagCommand::Export {
            trace_id,
            output: output_path,
            format,
            include_evidence,
            evidence_token,
            base_url,
        } => {
            use super::diag_bundle::{export_signed_bundle, ExportFormat};

            let fmt: ExportFormat = format
                .parse()
                .map_err(|e: String| AosError::validation(e))?;
            let response = export_signed_bundle(
                &trace_id,
                &output_path,
                fmt,
                include_evidence,
                evidence_token.as_deref(),
                &base_url,
            )
            .await
            .map_err(|e| AosError::internal(e.to_string()))?;

            if output.mode().is_json() {
                output.print_json(&response)?;
            }
            Ok(())
        }
        DiagCommand::Verify { bundle, verbose } => {
            use super::diag_bundle::{print_verification_summary, verify_bundle};

            let response =
                verify_bundle(&bundle, verbose).map_err(|e| AosError::internal(e.to_string()))?;

            if output.mode().is_json() {
                output.print_json(&response)?;
            } else {
                print_verification_summary(&response);
            }

            if !response.valid {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagProfile {
    System,
    Tenant,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
}

impl CheckStatus {
    fn symbol(&self) -> &str {
        match self {
            CheckStatus::Pass => "✅",
            CheckStatus::Warning => "⚠",
            CheckStatus::Fail => "✗",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DiagResult {
    check_name: String,
    status: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

struct DiagnosticRunner {
    #[allow(dead_code)]
    profile: DiagProfile,
    tenant_id: Option<String>,
    results: Vec<DiagResult>,
    has_warnings: bool,
    has_failures: bool,
}

impl DiagnosticRunner {
    fn new(profile: DiagProfile, tenant_id: Option<String>) -> Self {
        Self {
            profile,
            tenant_id,
            results: Vec::new(),
            has_warnings: false,
            has_failures: false,
        }
    }

    fn check(
        &mut self,
        name: &str,
        status: CheckStatus,
        message: String,
        details: Option<serde_json::Value>,
    ) {
        match status {
            CheckStatus::Warning => self.has_warnings = true,
            CheckStatus::Fail => self.has_failures = true,
            CheckStatus::Pass => {}
        }

        self.results.push(DiagResult {
            check_name: name.to_string(),
            status: match status {
                CheckStatus::Pass => "pass".to_string(),
                CheckStatus::Warning => "warning".to_string(),
                CheckStatus::Fail => "fail".to_string(),
            },
            message,
            details,
        });

        if !self.is_json_mode() {
            let message = self
                .results
                .last()
                .map(|r| r.message.as_str())
                .unwrap_or("No results");
            match status {
                CheckStatus::Pass => info!("{} {} - {}", status.symbol(), name, message),
                CheckStatus::Warning => warn!("{} {} - {}", status.symbol(), name, message),
                CheckStatus::Fail => error!("{} {} - {}", status.symbol(), name, message),
            }
        }
    }

    fn is_json_mode(&self) -> bool {
        false // Will be set from command args
    }

    async fn run_system_checks(&mut self) -> Result<()> {
        if !self.is_json_mode() {
            info!("🔧 System Checks");
            info!("════════════════════════════════════════════════════════════════");
        }

        // Metal device check
        self.check_metal_device();

        // Memory check
        self.check_memory();

        // Disk space check
        self.check_disk_space();

        // Permissions check
        self.check_permissions();

        // Database check
        if let Err(e) = self.check_database().await {
            tracing::debug!(error = %e, "Database check failed (non-fatal)");
        }

        // Kernel check
        self.check_kernels();

        Ok(())
    }

    fn check_metal_device(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            let output = Command::new("system_profiler")
                .arg("SPDisplaysDataType")
                .output();

            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.contains("Metal") || stdout.contains("Apple") {
                        self.check(
                            "Metal Device",
                            CheckStatus::Pass,
                            "Metal GPU detected".to_string(),
                            None,
                        );
                    } else {
                        self.check(
                            "Metal Device",
                            CheckStatus::Fail,
                            "No Metal-compatible GPU found".to_string(),
                            Some(serde_json::json!({"error_code": "E3004"})),
                        );
                    }
                }
                _ => {
                    self.check(
                        "Metal Device",
                        CheckStatus::Warning,
                        "Could not detect Metal device (system_profiler failed)".to_string(),
                        None,
                    );
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.check(
                "Metal Device",
                CheckStatus::Warning,
                "Metal device check skipped (not macOS)".to_string(),
                None,
            );
        }
    }

    fn check_memory(&mut self) {
        let mut sys = System::new_all();
        sys.refresh_all();

        let total_mem_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let available_mem_gb = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let used_percent = ((sys.total_memory() - sys.available_memory()) as f64
            / sys.total_memory() as f64)
            * 100.0;

        let min_required_gb = 8.0;
        let min_available_gb = 2.0;

        let status = if total_mem_gb < min_required_gb {
            CheckStatus::Fail
        } else if available_mem_gb < min_available_gb {
            CheckStatus::Warning
        } else {
            CheckStatus::Pass
        };

        self.check(
            "Memory",
            status,
            format!(
                "Total: {:.1} GB, Available: {:.1} GB ({:.1}% used)",
                total_mem_gb, available_mem_gb, used_percent
            ),
            Some(serde_json::json!({
                "total_gb": format!("{:.1}", total_mem_gb),
                "available_gb": format!("{:.1}", available_mem_gb),
                "used_percent": format!("{:.1}", used_percent),
                "error_code": if status == CheckStatus::Fail { Some("E9001") } else { None },
            })),
        );
    }

    fn check_disk_space(&mut self) {
        use std::process::Command;

        let paths_to_check = vec!["./var", "."];

        for path in paths_to_check {
            let output = Command::new("df").arg("-h").arg(path).output();

            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = stdout.lines().collect();

                    if lines.len() >= 2 {
                        let parts: Vec<&str> = lines[1].split_whitespace().collect();
                        if parts.len() >= 5 {
                            let available = parts[3];
                            let used_pct = parts[4].trim_end_matches('%');

                            let status = if let Ok(pct) = used_pct.parse::<u32>() {
                                if pct > 95 {
                                    CheckStatus::Fail
                                } else if pct > 85 {
                                    CheckStatus::Warning
                                } else {
                                    CheckStatus::Pass
                                }
                            } else {
                                CheckStatus::Warning
                            };

                            self.check(
                                &format!("Disk Space ({})", path),
                                status,
                                format!("Available: {}, Used: {}%", available, used_pct),
                                Some(serde_json::json!({
                                    "path": path,
                                    "available": available,
                                    "used_percent": used_pct,
                                    "error_code": if status == CheckStatus::Fail { Some("E9004") } else { None },
                                })),
                            );
                            continue;
                        }
                    }
                }
                _ => {}
            }

            self.check(
                &format!("Disk Space ({})", path),
                CheckStatus::Warning,
                "Could not check disk space".to_string(),
                None,
            );
        }
    }

    fn check_permissions(&mut self) {
        let paths_to_check = vec![("./var", true), ("./var/cas", true), ("./configs", false)];

        for (path, should_be_writable) in paths_to_check {
            let path_obj = Path::new(path);

            if !path_obj.exists() {
                self.check(
                    &format!("Path Exists ({})", path),
                    CheckStatus::Warning,
                    format!("Path does not exist: {}", path),
                    None,
                );
                continue;
            }

            let metadata = std::fs::metadata(path_obj);
            match metadata {
                Ok(meta) => {
                    let is_readable = true; // If we got metadata, we can read
                    let is_writable = !meta.permissions().readonly();

                    let status = if should_be_writable && !is_writable {
                        CheckStatus::Warning
                    } else if !is_readable {
                        CheckStatus::Fail
                    } else {
                        CheckStatus::Pass
                    };

                    self.check(
                        &format!("Permissions ({})", path),
                        status,
                        format!(
                            "readable: {}, writable: {}",
                            is_readable, is_writable
                        ),
                        Some(serde_json::json!({
                            "path": path,
                            "readable": is_readable,
                            "writable": is_writable,
                            "error_code": if status == CheckStatus::Fail { Some("E9002") } else { None },
                        })),
                    );
                }
                Err(e) => {
                    self.check(
                        &format!("Permissions ({})", path),
                        CheckStatus::Fail,
                        format!("Cannot check permissions: {}", e),
                        Some(serde_json::json!({"error_code": "E9002"})),
                    );
                }
            }
        }
    }

    async fn check_database(&mut self) -> Result<()> {
        let db_path = Path::new("./var/aos-cp.sqlite3");

        if !db_path.exists() {
            self.check(
                "Database",
                CheckStatus::Warning,
                "Control plane database not found (run: aosctl init-cp)".to_string(),
                Some(serde_json::json!({"error_code": "E8003"})),
            );
            return Ok(());
        }

        // Try to connect
        let db_path_str = db_path
            .to_str()
            .ok_or_else(|| AosError::Validation("Invalid database path".to_string()))?;
        match adapteros_db::Db::connect(db_path_str).await {
            Ok(_db) => {
                self.check(
                    "Database",
                    CheckStatus::Pass,
                    format!("Database connected: {}", db_path.display()),
                    None,
                );
                Ok(())
            }
            Err(e) => {
                self.check(
                    "Database",
                    CheckStatus::Fail,
                    format!("Database connection failed: {}", e),
                    Some(serde_json::json!({"error_code": "E8003"})),
                );
                Ok(())
            }
        }
    }

    fn check_kernels(&mut self) {
        let metallib_path = Path::new("./metal/aos_kernels.metallib");
        let sig_path = Path::new("./metal/aos_kernels.metallib.sig");

        if !metallib_path.exists() {
            self.check(
                "Kernel Library",
                CheckStatus::Fail,
                "Kernel metallib not found (run: cd metal && ./build.sh)".to_string(),
                Some(serde_json::json!({"error_code": "E3001"})),
            );
            return;
        }

        self.check(
            "Kernel Library",
            CheckStatus::Pass,
            format!("Found: {}", metallib_path.display()),
            None,
        );

        if !sig_path.exists() {
            self.check(
                "Kernel Signature",
                CheckStatus::Warning,
                "Kernel signature not found".to_string(),
                None,
            );
        } else {
            self.check(
                "Kernel Signature",
                CheckStatus::Pass,
                "Kernel signature present".to_string(),
                None,
            );
        }
    }

    async fn run_tenant_checks(&mut self) -> Result<()> {
        if !self.is_json_mode() {
            info!("👤 Tenant Checks");
            info!("════════════════════════════════════════════════════════════════");
        }

        let tenant_id = match &self.tenant_id {
            Some(id) => id.clone(),
            None => {
                self.check(
                    "Tenant",
                    CheckStatus::Warning,
                    "No tenant specified (use --tenant <id>)".to_string(),
                    None,
                );
                return Ok(());
            }
        };

        // Check tenant registry
        if let Err(e) = self.check_tenant_registry(&tenant_id).await {
            tracing::debug!(error = %e, tenant = %tenant_id, "Tenant registry check failed (non-fatal)");
        }

        // Check telemetry
        self.check_telemetry(&tenant_id);

        Ok(())
    }

    async fn check_tenant_registry(&mut self, tenant_id: &str) -> Result<()> {
        let db_path = Path::new("./var/aos-cp.sqlite3");

        if !db_path.exists() {
            self.check(
                "Tenant Registry",
                CheckStatus::Warning,
                "Database not initialized".to_string(),
                None,
            );
            return Ok(());
        }

        let db_path_str = db_path
            .to_str()
            .ok_or_else(|| AosError::Validation("Invalid database path".to_string()))?;
        match adapteros_db::Db::connect(db_path_str).await {
            Ok(db) => {
                // Check if tenant exists
                let query = "SELECT uid, gid FROM tenants WHERE id = ?";
                match sqlx::query(query)
                    .bind(tenant_id)
                    .fetch_optional(db.pool())
                    .await
                {
                    Ok(Some(row)) => {
                        let uid: i64 = row.try_get("uid").unwrap_or(0);
                        let gid: i64 = row.try_get("gid").unwrap_or(0);
                        self.check(
                            "Tenant Registry",
                            CheckStatus::Pass,
                            format!(
                                "Tenant {} registered (UID: {}, GID: {})",
                                tenant_id, uid, gid
                            ),
                            None,
                        );
                        Ok(())
                    }
                    Ok(None) => {
                        self.check(
                            "Tenant Registry",
                            CheckStatus::Fail,
                            format!("Tenant {} not found (run: aosctl init-tenant)", tenant_id),
                            None,
                        );
                        Ok(())
                    }
                    Err(e) => {
                        self.check(
                            "Tenant Registry",
                            CheckStatus::Fail,
                            format!("Query failed: {}", e),
                            None,
                        );
                        Ok(())
                    }
                }
            }
            Err(e) => {
                self.check(
                    "Tenant Registry",
                    CheckStatus::Fail,
                    format!("Database connection failed: {}", e),
                    None,
                );
                Ok(())
            }
        }
    }

    fn check_telemetry(&mut self, tenant_id: &str) {
        let telemetry_dir = PathBuf::from(format!("./var/telemetry/{}", tenant_id));

        if !telemetry_dir.exists() {
            self.check(
                "Telemetry Directory",
                CheckStatus::Warning,
                format!("Telemetry directory not found: {}", telemetry_dir.display()),
                None,
            );
            return;
        }

        // Check if writable
        match std::fs::metadata(&telemetry_dir) {
            Ok(meta) => {
                let is_writable = !meta.permissions().readonly();
                let status = if is_writable {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Warning
                };

                self.check(
                    "Telemetry Directory",
                    status,
                    format!("Telemetry dir: {} (writable: {})", telemetry_dir.display(), is_writable),
                    Some(serde_json::json!({"error_code": if !is_writable { Some("E4002") } else { None }})),
                );
            }
            Err(e) => {
                self.check(
                    "Telemetry Directory",
                    CheckStatus::Fail,
                    format!("Cannot check telemetry dir: {}", e),
                    Some(serde_json::json!({"error_code": "E4002"})),
                );
            }
        }
    }

    async fn run_service_checks(&mut self) -> Result<()> {
        if !self.is_json_mode() {
            info!("⚙️  Service Checks");
            info!("════════════════════════════════════════════════════════════════");
        }

        // Check aos-secd
        self.check_secd_service();

        // Check worker socket
        self.check_worker_socket();

        Ok(())
    }

    fn check_secd_service(&mut self) {
        use std::process::Command;

        let output = Command::new("pgrep").arg("-f").arg("aos-secd").output();

        match output {
            Ok(output) if output.status.success() && !output.stdout.is_empty() => {
                let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                self.check(
                    "aos-secd Service",
                    CheckStatus::Pass,
                    format!("Running (PID: {})", pid),
                    None,
                );

                // Check heartbeat file age
                self.check_heartbeat();
            }
            _ => {
                self.check(
                    "aos-secd Service",
                    CheckStatus::Warning,
                    "Service not running (optional for dev)".to_string(),
                    Some(serde_json::json!({"error_code": "E9003"})),
                );
            }
        }
    }

    fn check_heartbeat(&mut self) {
        let heartbeat_path = Path::new("./var/aos-secd.heartbeat");

        if !heartbeat_path.exists() {
            self.check(
                "Service Heartbeat",
                CheckStatus::Warning,
                "Heartbeat file not found".to_string(),
                None,
            );
            return;
        }

        if let Ok(meta) = std::fs::metadata(heartbeat_path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    let secs = elapsed.as_secs();
                    let status = if secs < 30 {
                        CheckStatus::Pass
                    } else if secs < 120 {
                        CheckStatus::Warning
                    } else {
                        CheckStatus::Fail
                    };

                    self.check(
                        "Service Heartbeat",
                        status,
                        format!("Last heartbeat: {} seconds ago", secs),
                        None,
                    );
                    return;
                }
            }
        }

        self.check(
            "Service Heartbeat",
            CheckStatus::Warning,
            "Could not check heartbeat age".to_string(),
            None,
        );
    }

    fn check_worker_socket(&mut self) {
        let socket_path = Path::new("/var/run/aos/aos.sock");

        if !socket_path.exists() {
            self.check(
                "Worker Socket",
                CheckStatus::Warning,
                format!("Socket not found: {} (not serving)", socket_path.display()),
                None,
            );
            return;
        }

        self.check(
            "Worker Socket",
            CheckStatus::Pass,
            format!("Socket exists: {}", socket_path.display()),
            None,
        );
    }

    fn exit_code(&self) -> i32 {
        if self.has_failures {
            20
        } else if self.has_warnings {
            10
        } else {
            0
        }
    }
}

/// Run diagnostics
pub async fn run(
    profile: DiagProfile,
    tenant_id: Option<String>,
    json: bool,
    bundle_path: Option<PathBuf>,
    bundle_cpid: Option<String>,
    full_db: bool,
) -> Result<()> {
    let mut runner = DiagnosticRunner::new(profile, tenant_id.clone());

    if !json {
        info!("adapterOS Diagnostics");
        info!("════════════════════════════════════════════════════════════════");
        info!("Profile: {:?}", profile);
        if let Some(ref tenant) = tenant_id {
            info!("Tenant: {}", tenant);
        }
    }

    // Run checks based on profile
    match profile {
        DiagProfile::System => {
            runner.run_system_checks().await?;
        }
        DiagProfile::Tenant => {
            runner.run_tenant_checks().await?;
        }
        DiagProfile::Full => {
            runner.run_system_checks().await?;
            runner.run_tenant_checks().await?;
            runner.run_service_checks().await?;
        }
    }

    // Output results
    if json {
        let output = serde_json::json!({
            "profile": format!("{:?}", profile),
            "tenant": tenant_id,
            "has_warnings": runner.has_warnings,
            "has_failures": runner.has_failures,
            "exit_code": runner.exit_code(),
            "checks": runner.results,
        });
        info!(
            "Diagnostic results: {}",
            serde_json::to_string_pretty(&output)?
        );
    } else {
        info!("════════════════════════════════════════════════════════════════");
        info!("Summary:");
        info!("  Total checks: {}", runner.results.len());
        info!(
            "  Passed: {}",
            runner.results.iter().filter(|r| r.status == "pass").count()
        );
        info!(
            "  Warnings: {}",
            runner
                .results
                .iter()
                .filter(|r| r.status == "warning")
                .count()
        );
        info!(
            "  Failures: {}",
            runner.results.iter().filter(|r| r.status == "fail").count()
        );
        info!("Exit code: {}", runner.exit_code());

        if runner.has_failures || runner.has_warnings {
            warn!("For help with specific errors, run: aosctl explain <CODE>");
        }
    }

    // Create bundle if requested
    if let Some(bundle_path) = bundle_path {
        create_diag_bundle(
            &bundle_path,
            &runner.results,
            bundle_cpid.as_deref(),
            full_db,
        )
        .await?;
        info!("📦 Diagnostic bundle created: {}", bundle_path.display());
    }

    std::process::exit(runner.exit_code());
}

async fn add_log_files(zip: &mut zip::ZipWriter<std::fs::File>, logs_dir: &str) -> Result<()> {
    use std::fs;
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let logs_path = Path::new(logs_dir);

    // Collect log files recursively
    let mut log_files = Vec::new();
    collect_log_files(logs_path, &mut log_files)?;

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

    // Add up to 10 most recent log files, limiting total size to 1MB
    let mut total_size = 0;
    const MAX_TOTAL_SIZE: usize = 1024 * 1024; // 1MB
    const MAX_FILES: usize = 10;

    for (i, log_file) in log_files.iter().enumerate() {
        if i >= MAX_FILES || total_size >= MAX_TOTAL_SIZE {
            break;
        }

        let metadata = match fs::metadata(log_file) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_size = metadata.len() as usize;
        if total_size + file_size > MAX_TOTAL_SIZE {
            // Truncate the file to fit within the limit
            let remaining_space = MAX_TOTAL_SIZE - total_size;
            if remaining_space > 0 {
                add_truncated_log_file(zip, log_file, remaining_space)?;
            }
            break;
        }

        // Add the full log file
        let relative_path = log_file
            .strip_prefix(logs_path)
            .unwrap_or(log_file)
            .to_string_lossy()
            .replace('\\', "/"); // Normalize path separators

        zip.start_file(
            format!("logs/{}", relative_path),
            SimpleFileOptions::default(),
        )?;

        let mut file = fs::File::open(log_file)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        zip.write_all(&buffer)?;

        total_size += file_size;
    }

    Ok(())
}

fn collect_log_files(dir: &Path, log_files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Recursively collect from subdirectories
            collect_log_files(&path, log_files)?;
        } else if path.is_file() {
            // Check if it's a log file by extension or name
            if is_log_file(&path) {
                log_files.push(path);
            }
        }
    }

    Ok(())
}

fn is_log_file(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Check by extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "log" | "txt" | "out" | "err" => return true,
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
        || lower_name.starts_with("mplora-")
}

fn add_truncated_log_file(
    zip: &mut zip::ZipWriter<std::fs::File>,
    log_file: &Path,
    max_size: usize,
) -> Result<()> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
    use zip::write::SimpleFileOptions;

    let file = File::open(log_file)?;
    let mut reader = BufReader::new(file);

    // Seek to the end and read backwards to get the most recent lines
    let file_size = reader.get_ref().metadata()?.len() as usize;
    if file_size <= max_size {
        // File is small enough, add it entirely
        reader.seek(SeekFrom::Start(0))?;
        let mut content = Vec::new();
        reader.read_to_end(&mut content)?;

        let relative_path = log_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.log");

        zip.start_file(
            format!("logs/{}", relative_path),
            SimpleFileOptions::default(),
        )?;
        zip.write_all(&content)?;
        return Ok(());
    }

    // File is too large, truncate from the end
    let start_pos = file_size.saturating_sub(max_size);
    reader.seek(SeekFrom::Start(start_pos as u64))?;

    // Skip the first line as it might be partial
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let mut content = Vec::new();
    reader.read_to_end(&mut content)?;

    // Add truncation notice
    let truncated_content = format!(
        "[TRUNCATED - showing last {} bytes of {}]\n{}",
        content.len(),
        file_size,
        String::from_utf8_lossy(&content)
    );

    let relative_path = log_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.log");

    zip.start_file(
        format!("logs/{}", relative_path),
        zip::write::SimpleFileOptions::default(),
    )?;
    zip.write_all(truncated_content.as_bytes())?;

    Ok(())
}

fn collect_config_files(zip: &mut zip::ZipWriter<std::fs::File>) -> Result<()> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let config_files = [
        "configs/cp.toml",
        ".env",
        "Cargo.toml",
        "manifests/qwen7b.yaml",
    ];

    for file_path in config_files {
        if let Ok(content) = fs::read_to_string(file_path) {
            let zip_path = format!("config/{}", file_path.replace('/', "_"));
            zip.start_file(zip_path, SimpleFileOptions::default())?;
            zip.write_all(content.as_bytes())?;
        }
    }

    Ok(())
}

async fn collect_database_state(
    zip: &mut zip::ZipWriter<std::fs::File>,
    full_db: bool,
) -> Result<()> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let db = adapteros_db::Db::connect_env().await.ok();
    let mut info = String::new();

    if let Some(db) = db {
        if let Ok(result) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM manifests")
            .fetch_one(db.pool())
            .await
        {
            info.push_str(&format!("Manifests: {}\n", result));
        }

        if let Ok(result) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters")
            .fetch_one(db.pool())
            .await
        {
            info.push_str(&format!("Adapters: {}\n", result));
        }

        if let Ok(rows) =
            sqlx::query_as::<_, (String, i64)>("SELECT status, COUNT(*) FROM jobs GROUP BY status")
                .fetch_all(db.pool())
                .await
        {
            info.push_str("\nJobs by status:\n");
            for (status, count) in rows {
                info.push_str(&format!("  {}: {}\n", status, count));
            }
        }

        if let Ok(rows) = sqlx::query_as::<_, (String, String, String)>(
            "SELECT id, job_type, status FROM jobs ORDER BY created_at DESC LIMIT 20",
        )
        .fetch_all(db.pool())
        .await
        {
            info.push_str("\nRecent jobs:\n");
            for (id, job_type, status) in rows {
                info.push_str(&format!("  {} | {} | {}\n", id, job_type, status));
            }
        }
    }

    if !info.is_empty() {
        zip.start_file("database_state.txt", SimpleFileOptions::default())?;
        zip.write_all(info.as_bytes())?;
    }

    if full_db {
        if let Ok(db_bytes) = fs::read("var/aos-cp.sqlite3") {
            zip.start_file("database/aos-cp.sqlite3", SimpleFileOptions::default())?;
            zip.write_all(&db_bytes)?;
        }
    }

    Ok(())
}

fn collect_telemetry(zip: &mut zip::ZipWriter<std::fs::File>, cpid: &str) -> Result<()> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let telemetry_dir = format!("var/telemetry/{}", cpid);
    if let Ok(entries) = fs::read_dir(&telemetry_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext == "ndjson" || ext == "sig" || ext == "json" {
                    if let Ok(content) = fs::read(&path) {
                        let Some(file_name) = path.file_name() else {
                            continue;
                        };
                        let zip_path =
                            format!("telemetry/{}/{}", cpid, file_name.to_string_lossy());
                        zip.start_file(zip_path, SimpleFileOptions::default())?;
                        zip.write_all(&content)?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn collect_recent_telemetry(zip: &mut zip::ZipWriter<std::fs::File>) -> Result<()> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    if let Ok(entries) = fs::read_dir("var/telemetry") {
        let mut bundles: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("ndjson"))
            .collect();

        bundles.sort_by_key(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        bundles.reverse();

        for entry in bundles.iter().take(3) {
            let path = entry.path();
            if let Ok(content) = fs::read(&path) {
                let Some(file_name) = path.file_name() else {
                    continue;
                };
                let zip_path = format!("telemetry/recent/{}", file_name.to_string_lossy());
                zip.start_file(zip_path, SimpleFileOptions::default())?;
                zip.write_all(&content)?;
            }

            let sig_path = path.with_extension("ndjson.sig");
            if let Ok(content) = fs::read(&sig_path) {
                let Some(file_name) = sig_path.file_name() else {
                    continue;
                };
                let zip_path = format!("telemetry/recent/{}", file_name.to_string_lossy());
                zip.start_file(zip_path, SimpleFileOptions::default())?;
                zip.write_all(&content)?;
            }
        }
    }

    Ok(())
}

fn collect_alerts(zip: &mut zip::ZipWriter<std::fs::File>) -> Result<()> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    if let Ok(entries) = fs::read_dir("var/alerts") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Ok(content) = fs::read(&path) {
                    let Some(file_name) = path.file_name() else {
                        continue;
                    };
                    let zip_path = format!("alerts/{}", file_name.to_string_lossy());
                    zip.start_file(zip_path, SimpleFileOptions::default())?;
                    zip.write_all(&content)?;
                }
            }
        }
    }

    Ok(())
}

async fn create_diag_bundle(
    bundle_path: &Path,
    results: &[DiagResult],
    cpid: Option<&str>,
    full_db: bool,
) -> Result<()> {
    use std::fs::File;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let file = File::create(bundle_path).context("Failed to create bundle file")?;
    let mut zip = ZipWriter::new(file);

    // Add diagnostic results
    zip.start_file("diagnostics.json", SimpleFileOptions::default())?;
    zip.write_all(serde_json::to_string_pretty(results)?.as_bytes())?;

    // Add system info
    let mut sys = System::new_all();
    sys.refresh_all();

    let sysinfo = serde_json::json!({
        "os": System::name(),
        "kernel": System::kernel_version(),
        "os_version": System::os_version(),
        "hostname": System::host_name(),
        "total_memory": sys.total_memory(),
        "available_memory": sys.available_memory(),
        "cpu_count": sys.cpus().len(),
    });

    zip.start_file("system_info.json", SimpleFileOptions::default())?;
    zip.write_all(serde_json::to_string_pretty(&sysinfo)?.as_bytes())?;

    collect_config_files(&mut zip)?;
    collect_database_state(&mut zip, full_db).await?;

    // Add recent logs if they exist
    if Path::new("./var/logs").exists() {
        add_log_files(&mut zip, "./var/logs").await?;
    }

    if let Some(cpid) = cpid {
        collect_telemetry(&mut zip, cpid)?;
    } else {
        collect_recent_telemetry(&mut zip)?;
    }

    collect_alerts(&mut zip)?;

    zip.finish()?;
    Ok(())
}

fn truncate_snippet(s: &str, max_len: usize) -> String {
    let mut cleaned = s.replace(['\n', '\r'], " ");
    if cleaned.len() <= max_len {
        return cleaned;
    }

    cleaned.truncate(max_len);
    cleaned.push('…');
    cleaned
}

/// Run determinism check: 3 fixed prompts, N runs, compare outputs
pub async fn run_determinism_check(
    stack_id: Option<String>,
    runs: usize,
    seed: Option<String>,
    _output: &crate::output::OutputWriter,
) -> Result<()> {
    use adapteros_core::B3Hash;
    use reqwest::Client;
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;

    info!("Running determinism check...");
    info!("  Stack ID: {:?}", stack_id);
    info!("  Runs: {}", runs);
    info!("  Seed: {:?}", seed);

    // Fixed test prompts (as specified in PRD G2)
    let test_prompts = [
        "Hello world".to_string(),
        "Explain async in Rust".to_string(),
        "Write a function".to_string(),
    ];

    // Determine seed (convert to u64 for API)
    // Helper to safely convert first 8 bytes of a 32-byte hash to u64
    fn hash_to_u64(hash: &B3Hash) -> u64 {
        let bytes = hash.as_bytes();
        // B3Hash is always 32 bytes, so this slice and conversion is safe
        u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }

    let seed_u64 = if let Some(ref seed_hex) = seed {
        // Parse hex seed and convert to u64
        let base_seed = B3Hash::from_hex(seed_hex)
            .map_err(|e| AosError::Config(format!("Invalid seed hex: {}", e)))?;
        hash_to_u64(&base_seed)
    } else {
        // Default: derive from fixed test seed
        let base_seed = B3Hash::hash(b"determinism-check-default-seed");
        hash_to_u64(&base_seed)
    };

    info!("Using seed: {}", seed_u64);

    // Get worker socket path from environment or use default
    // Check AOS_WORKER_SOCKET env var first, then fall back to tenant-based path
    let socket_path = if let Ok(path) = std::env::var("AOS_WORKER_SOCKET") {
        PathBuf::from(path)
    } else {
        // Default tenant-based path (matches adapter.rs pattern)
        let tenant = std::env::var("AOS_TENANT_ID").unwrap_or_else(|_| "default".to_string());
        PathBuf::from(format!("./var/run/aos/{}/worker.sock", tenant))
    };

    if !socket_path.exists() {
        warn!(
            "Worker socket not found at: {}. Inference requests may fail.",
            socket_path.display()
        );
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AosError::Config(format!("Failed to create HTTP client: {}", e)))?;

    // Determine stack to use
    let actual_stack_id = if let Some(ref sid) = stack_id {
        sid.clone()
    } else {
        // For MVP, use empty stack (base model only)
        // In production, would query for first active stack
        warn!("No stack specified, using base model only");
        String::new()
    };

    info!(
        "Using stack: {}",
        if actual_stack_id.is_empty() {
            "base model"
        } else {
            &actual_stack_id
        }
    );

    // Run inference N times for each prompt
    let mut results: HashMap<String, Vec<String>> = HashMap::new();

    for run in 0..runs {
        info!("Run {}/{}", run + 1, runs);

        for (prompt_idx, prompt) in test_prompts.iter().enumerate() {
            // Build request body
            let mut request_body = json!({
                "prompt": prompt,
                "max_tokens": 100,
                "temperature": 0.0, // Deterministic temperature
                "seed": seed_u64,
            });

            if !actual_stack_id.is_empty() {
                request_body["adapter_stack"] = json!([actual_stack_id.clone()]);
            }

            // Unix socket URL - use http+unix:// format (matches infer.rs pattern)
            // Note: reqwest doesn't natively support Unix sockets, but this format is used
            // throughout the codebase. In production, consider using UdsClient from adapteros-client.
            let socket_resolved = socket_path
                .canonicalize()
                .unwrap_or_else(|_| socket_path.clone());
            let url = uds_infer_url_string(&socket_resolved);
            let url = reqwest::Url::parse(&url).map_err(|e| {
                AosError::Config(format!(
                    "Invalid socket URL: {} (path: {})",
                    e,
                    socket_resolved.display()
                ))
            })?;

            let response = client
                .post(url)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&request_body).unwrap())
                .send()
                .await
                .map_err(|e| AosError::Config(format!("Inference request failed: {}", e)))?;

            let status = response.status();

            if !status.is_success() {
                error!("Inference failed for prompt {}: {}", prompt_idx, status);
                return Err(AosError::Config(format!("Inference failed: {}", status)));
            }

            // Read body as text so we can provide a useful error message (status + snippet)
            // when contract fields are missing.
            let response_body = response
                .text()
                .await
                .map_err(|e| AosError::Config(format!("Failed to read response body: {}", e)))?;

            let json_response: serde_json::Value =
                serde_json::from_str(&response_body).map_err(|e| {
                    AosError::Config(format!(
                        "Failed to parse response JSON (status: {}): {}. Response snippet: {}",
                        status,
                        e,
                        truncate_snippet(&response_body, 512)
                    ))
                })?;

            let output_text = json_response
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    let got = json_response
                        .get("text")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "<missing>".to_string());
                    AosError::Config(format!(
                        "Inference response missing expected string field 'text' (status: {}). got: {}. Response snippet: {}",
                        status,
                        got,
                        truncate_snippet(&response_body, 512)
                    ))
                })?
                .to_string();

            results
                .entry(format!("prompt_{}", prompt_idx))
                .or_default()
                .push(output_text);
        }
    }

    // Compare outputs
    let mut all_deterministic = true;
    let mut diffs = Vec::new();

    for (prompt_key, outputs) in &results {
        if outputs.len() < runs {
            warn!("Incomplete results for {}", prompt_key);
            all_deterministic = false;
            continue;
        }

        // Check if all outputs are identical
        let first_output = &outputs[0];
        for (run_idx, output) in outputs.iter().enumerate().skip(1) {
            if output != first_output {
                all_deterministic = false;
                diffs.push(format!(
                    "{}: Run 0 vs Run {} differ\n  Run 0: {}\n  Run {}: {}",
                    prompt_key, run_idx, first_output, run_idx, output
                ));
            }
        }
    }

    // Print results
    if all_deterministic {
        info!("✅ Deterministic: YES");
        info!(
            "All {} runs produced identical outputs for all {} prompts",
            runs,
            test_prompts.len()
        );
    } else {
        error!("❌ Deterministic: NO");
        error!("Found {} divergence(s):", diffs.len());
        for diff in &diffs {
            error!("{}", diff);
        }
    }

    // Persist results to database (PRD G2 - Fix shortcut)
    let result_str = if all_deterministic { "pass" } else { "fail" };
    let divergence_count = diffs.len();
    let seed_str = if let Some(ref seed_hex) = seed {
        seed_hex.clone()
    } else {
        "default".to_string()
    };

    // Open database and persist results
    let db_path =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "./var/aos-cp.sqlite3".to_string());

    match Db::connect(&db_path).await {
        Ok(db) => {
            match sqlx::query(
                "INSERT INTO determinism_checks (last_run, result, runs, divergences, stack_id, seed)
                 VALUES (datetime('now'), ?, ?, ?, ?, ?)"
            )
            .bind(result_str)
            .bind(runs as i64)
            .bind(divergence_count as i64)
            .bind(&actual_stack_id)
            .bind(&seed_str)
            .execute(db.pool())
            .await
            {
                Ok(_) => {
                    info!("Determinism check results persisted to database");
                }
                Err(e) => {
                    warn!("Failed to persist determinism check results to database: {}", e);
                    // Continue execution - persistence failure shouldn't block the check
                }
            }
        }
        Err(e) => {
            warn!(
                "Failed to open database for determinism check persistence: {}",
                e
            );
            // Continue execution - database access failure shouldn't block the check
        }
    }

    Ok(())
}

use adapteros_db::Db;

/// Run quarantine check: list quarantined adapters and verify none in active stacks
pub async fn run_quarantine_check(
    verbose: bool,
    _output: &crate::output::OutputWriter,
) -> Result<()> {
    use serde_json::Value;
    use sqlx::Row;

    info!("Checking quarantine status...");

    // Open database
    let db_path = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://./var/aos-cp.sqlite3".to_string());
    let db = Db::connect(&db_path)
        .await
        .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

    // Query active quarantines
    let quarantines = sqlx::query(
        "SELECT id, reason, created_at, violation_type, cpid, metadata 
         FROM active_quarantine 
         ORDER BY created_at DESC",
    )
    .fetch_all(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to query quarantines: {}", e)))?;

    let quarantine_count = quarantines.len();
    info!("Quarantined adapters present: {}", quarantine_count);

    if quarantine_count == 0 {
        info!("✅ No quarantined adapters found");
        return Ok(());
    }

    // Extract adapter IDs from quarantine records
    // Try to extract from metadata JSON first, then fall back to parsing reason
    let mut quarantined_adapter_ids = Vec::new();
    let mut quarantine_info = Vec::new();

    for row in &quarantines {
        let id: String = row.try_get("id").unwrap_or_default();
        let reason: String = row.try_get("reason").unwrap_or_default();
        let created_at: String = row.try_get("created_at").unwrap_or_default();
        let metadata: Option<String> = row.try_get("metadata").ok();

        // Try to extract adapter ID from metadata JSON
        let mut adapter_id: Option<String> = None;
        if let Some(ref meta_str) = metadata {
            if let Ok(meta_json) = serde_json::from_str::<Value>(meta_str) {
                // Look for adapter_id field in metadata
                if let Some(Value::String(adapter_id_str)) = meta_json.get("adapter_id") {
                    adapter_id = Some(adapter_id_str.clone());
                } else if let Some(Value::String(adapter_id_str)) = meta_json.get("adapter") {
                    adapter_id = Some(adapter_id_str.clone());
                }
            }
        }

        // Fall back to extracting from reason if metadata doesn't have it
        // Look for patterns like "adapter: <id>" or "Adapter <id>"
        if adapter_id.is_none() {
            // Try to find adapter ID pattern in reason
            for part in reason.split_whitespace() {
                if part.starts_with("adapter:") || part.starts_with("Adapter:") {
                    if let Some(id_part) = part.split(':').nth(1) {
                        adapter_id = Some(id_part.trim().to_string());
                        break;
                    }
                }
            }
            // If still not found, check if reason itself looks like an adapter ID
            if adapter_id.is_none() && !reason.contains(' ') && reason.len() > 5 {
                adapter_id = Some(reason.clone());
            }
        }

        if let Some(adapter_id_str) = adapter_id {
            quarantined_adapter_ids.push(adapter_id_str.clone());
            quarantine_info.push((
                adapter_id_str,
                id.clone(),
                reason.clone(),
                created_at.clone(),
            ));
        } else {
            // If we can't extract adapter ID, still record the quarantine
            quarantine_info.push((id.clone(), id.clone(), reason.clone(), created_at.clone()));
        }
    }

    // List quarantined adapters
    if verbose {
        info!("Quarantined adapters:");
        for (adapter_id, q_id, reason, created_at) in &quarantine_info {
            info!(
                "  - {} (quarantine ID: {}): {} (created: {})",
                adapter_id, q_id, reason, created_at
            );
        }
    }

    // Query active stacks
    let stacks = sqlx::query(
        "SELECT id, name, adapter_ids_json 
         FROM adapter_stacks 
         WHERE active = 1",
    )
    .fetch_all(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to query stacks: {}", e)))?;

    // Check if any quarantined adapter IDs appear in active stacks
    let mut found_in_stacks = Vec::new();

    for stack_row in &stacks {
        let stack_id: String = stack_row.try_get("id").unwrap_or_default();
        let stack_name: String = stack_row.try_get("name").unwrap_or_default();
        let adapter_ids_json: Option<String> = stack_row.try_get("adapter_ids_json").ok();

        if let Some(ref json_str) = adapter_ids_json {
            if let Ok(adapter_ids) = serde_json::from_str::<Vec<String>>(json_str) {
                // Check if any quarantined adapter appears in this stack
                for adapter_id in &adapter_ids {
                    if quarantined_adapter_ids.contains(adapter_id) {
                        found_in_stacks.push((
                            stack_id.clone(),
                            stack_name.clone(),
                            adapter_id.clone(),
                        ));
                    }
                }
            }
        }
    }

    if found_in_stacks.is_empty() {
        info!("✅ Confirmed: No quarantined adapters are wired into active stacks");
    } else {
        error!("❌ WARNING: Found quarantined adapters in active stacks:");
        for (stack_id, stack_name, adapter_id) in &found_in_stacks {
            error!(
                "  - Stack '{}' ({}) contains quarantined adapter '{}'",
                stack_name, stack_id, adapter_id
            );
        }
    }

    Ok(())
}
