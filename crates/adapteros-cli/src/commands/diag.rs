//! System diagnostics command

use adapteros_core::{AosError, Result};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::path::{Path, PathBuf};
use sysinfo::System;
use tracing::{error, info, warn};

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
        self.check_database().await?;

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
            .ok_or_else(|| AosError::Other("Invalid database path".to_string()))?;
        match adapteros_db::Database::connect_env().await {
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
        self.check_tenant_registry(&tenant_id).await?;

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
            .ok_or_else(|| AosError::Other("Invalid database path".to_string()))?;
        match adapteros_db::Database::connect_env().await {
            Ok(db) => {
                // Check if tenant exists - use appropriate query syntax
                let result = if db.as_sqlite().is_some() {
                    sqlx::query("SELECT uid, gid FROM tenants WHERE id = ?")
                        .bind(tenant_id)
                        .fetch_optional(db.pool())
                        .await
                } else {
                    sqlx::query("SELECT uid, gid FROM tenants WHERE id = $1")
                        .bind(tenant_id)
                        .fetch_optional(db.postgres_pool())
                        .await
                };
                match result {
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
        // Prefer per-tenant socket when tenant_id is provided
        let socket_path_buf = if let Some(ref tenant) = self.tenant_id {
            std::path::PathBuf::from(format!("/var/run/aos/{}/aos.sock", tenant))
        } else {
            std::path::PathBuf::from("/var/run/aos/aos.sock")
        };
        let socket_path = socket_path_buf.as_path();

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
) -> Result<()> {
    let mut runner = DiagnosticRunner::new(profile, tenant_id.clone());

    if !json {
        info!("AdapterOS Diagnostics");
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
        create_diag_bundle(&bundle_path, &runner.results).await?;
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
    collect_log_files(logs_path, &mut log_files, logs_path)?;

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

fn collect_log_files(dir: &Path, log_files: &mut Vec<PathBuf>, base_path: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Recursively collect from subdirectories
            collect_log_files(&path, log_files, base_path)?;
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

async fn create_diag_bundle(bundle_path: &Path, results: &[DiagResult]) -> Result<()> {
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

    // Add config files if they exist
    if Path::new("./configs/cp.toml").exists() {
        let config = std::fs::read_to_string("./configs/cp.toml")?;
        zip.start_file("configs/cp.toml", SimpleFileOptions::default())?;
        zip.write_all(config.as_bytes())?;
    }

    // Add recent logs if they exist
    if Path::new("./var/logs").exists() {
        add_log_files(&mut zip, "./var/logs").await?;
    }

    zip.finish()?;
    Ok(())
}
