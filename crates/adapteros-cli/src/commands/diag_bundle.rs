//! Generate diagnostic bundle for troubleshooting

use anyhow::{Context, Result};
use std::fs::{File, self};
use std::io::Write;
use std::path::Path;
use zip::write::FileOptions;
use zip::ZipWriter;

pub async fn run(output_path: &Path, cpid: Option<&str>, full_db: bool) -> Result<()> {
    let mode = crate::output::OutputMode::from_env();

    crate::output::command_header(&mode, &format!("Generating diagnostic bundle: {}", output_path.display()));

    let file = File::create(output_path)
        .context("Failed to create output file")?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    // 1. System information
    crate::output::progress(&mode, "Collecting system information...");
    collect_system_info(&mut zip, options)?;

    // 2. Git information
    crate::output::progress(&mode, "Collecting git information...");
    collect_git_info(&mut zip, options)?;

    // 3. Configuration files
    crate::output::progress(&mode, "Collecting configuration files...");
    collect_config_files(&mut zip, options)?;

    // 4. Metal kernel information
    crate::output::progress(&mode, "Collecting Metal kernel hashes...");
    collect_metal_info(&mut zip, options)?;

    // 5. Database state
    crate::output::progress(&mode, "Collecting database state...");
    collect_database_state(&mut zip, options, full_db).await?;

    // 6. Log files
    crate::output::progress(&mode, "Collecting log files...");
    collect_log_files(&mut zip, options)?;

    // 7. Telemetry bundles
    if let Some(cpid_str) = cpid {
        crate::output::progress(&mode, &format!("Collecting telemetry for CPID: {}", cpid_str));
        collect_telemetry(&mut zip, options, cpid_str)?;
    } else {
        crate::output::progress(&mode, "Collecting recent telemetry bundles...");
        collect_recent_telemetry(&mut zip, options)?;
    }

    // 8. Recent alerts
    crate::output::progress(&mode, "Collecting alert files...");
    collect_alerts(&mut zip, options)?;

    zip.finish().context("Failed to finalize zip")?;

    crate::output::result(&format!("✅ Diagnostic bundle created: {}", output_path.display()));
    Ok(())
}

fn collect_system_info(zip: &mut ZipWriter<File>, options: FileOptions) -> Result<()> {
    use sysinfo::{System, SystemExt, CpuExt};

    let mut sys = System::new_all();
    sys.refresh_all();

    let mut info = String::new();
    info.push_str(&format!("OS: {} {}\n", sys.name().unwrap_or_default(), sys.os_version().unwrap_or_default()));
    info.push_str(&format!("Kernel: {}\n", sys.kernel_version().unwrap_or_default()));
    info.push_str(&format!("Hostname: {}\n", sys.host_name().unwrap_or_default()));
    info.push_str(&format!("CPU: {}\n", sys.cpus().first().map(|cpu| cpu.brand()).unwrap_or("Unknown")));
    info.push_str(&format!("CPU Cores: {}\n", sys.cpus().len()));
    info.push_str(&format!("Total Memory: {} MB\n", sys.total_memory() / 1024 / 1024));
    info.push_str(&format!("Available Memory: {} MB\n", sys.available_memory() / 1024 / 1024));
    info.push_str(&format!("Uptime: {} seconds\n", sys.uptime()));

    zip.start_file("system_info.txt", options)?;
    zip.write_all(info.as_bytes())?;

    Ok(())
}

fn collect_git_info(zip: &mut ZipWriter<File>, options: FileOptions) -> Result<()> {
    let mut info = String::new();

    // Get current commit
    if let Ok(output) = std::process::Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
    {
        if output.status.success() {
            info.push_str("Current commit: ");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    // Get branch
    if let Ok(output) = std::process::Command::new("git")
        .args(&["branch", "--show-current"])
        .output()
    {
        if output.status.success() {
            info.push_str("Branch: ");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    // Get last 10 commits
    if let Ok(output) = std::process::Command::new("git")
        .args(&["log", "--oneline", "-10"])
        .output()
    {
        if output.status.success() {
            info.push_str("\nRecent commits:\n");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    // Get status
    if let Ok(output) = std::process::Command::new("git")
        .args(&["status", "--short"])
        .output()
    {
        if output.status.success() {
            info.push_str("\nStatus:\n");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    if !info.is_empty() {
        zip.start_file("git_info.txt", options)?;
        zip.write_all(info.as_bytes())?;
    }

    Ok(())
}

fn collect_config_files(zip: &mut ZipWriter<File>, options: FileOptions) -> Result<()> {
    let config_files = vec![
        "configs/cp.toml",
        ".env",
        "Cargo.toml",
        "manifests/qwen7b.yaml",
    ];

    for file_path in config_files {
        if let Ok(content) = fs::read_to_string(file_path) {
            let zip_path = format!("config/{}", file_path.replace("/", "_"));
            zip.start_file(zip_path, options)?;
            zip.write_all(content.as_bytes())?;
        }
    }

    Ok(())
}

fn collect_metal_info(zip: &mut ZipWriter<File>, options: FileOptions) -> Result<()> {
    let mut info = String::new();

    // Check if metallib exists and get its hash
    if let Ok(output) = std::process::Command::new("b3sum")
        .args(&["metal/aos_kernels.metallib"])
        .output()
    {
        if output.status.success() {
            info.push_str("metallib hash: ");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    // Get toolchain info
    if let Ok(content) = fs::read_to_string("metal/toolchain.toml") {
        info.push_str("\nToolchain config:\n");
        info.push_str(&content);
    }

    // Get Xcode version
    if let Ok(output) = std::process::Command::new("xcodebuild")
        .args(&["-version"])
        .output()
    {
        if output.status.success() {
            info.push_str("\nXcode version:\n");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    if !info.is_empty() {
        zip.start_file("metal_info.txt", options)?;
        zip.write_all(info.as_bytes())?;
    }

    Ok(())
}

async fn collect_database_state(
    zip: &mut ZipWriter<File>,
    options: FileOptions,
    full_db: bool,
) -> Result<()> {
    // Connect to database
    let db = adapteros_db::Db::connect_env().await?;

    let mut info = String::new();

    // Get manifest count
    if let Ok(result) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM manifests")
        .fetch_one(db.pool())
        .await
    {
        info.push_str(&format!("Manifests: {}\n", result));
    }

    // Get adapter count
    if let Ok(result) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM adapters")
        .fetch_one(db.pool())
        .await
    {
        info.push_str(&format!("Adapters: {}\n", result));
    }

    // Get job counts by status
    if let Ok(rows) = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) FROM jobs GROUP BY status"
    )
    .fetch_all(db.pool())
    .await
    {
        info.push_str("\nJobs by status:\n");
        for (status, count) in rows {
            info.push_str(&format!("  {}: {}\n", status, count));
        }
    }

    // Get recent jobs
    if let Ok(rows) = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, job_type, status FROM jobs ORDER BY created_at DESC LIMIT 20"
    )
    .fetch_all(db.pool())
    .await
    {
        info.push_str("\nRecent jobs:\n");
        for (id, job_type, status) in rows {
            info.push_str(&format!("  {} | {} | {}\n", id, job_type, status));
        }
    }

    zip.start_file("database_state.txt", options)?;
    zip.write_all(info.as_bytes())?;

    // If full DB requested, include raw database
    if full_db {
        if let Ok(db_bytes) = fs::read("var/aos-cp.sqlite3") {
            zip.start_file("database/aos-cp.sqlite3", options)?;
            zip.write_all(&db_bytes)?;
        }
    }

    Ok(())
}

fn collect_log_files(zip: &mut ZipWriter<File>, options: FileOptions) -> Result<()> {
    // Check for log files in var/
    if let Ok(entries) = fs::read_dir("var/") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "log" || ext == "txt" {
                    if let Ok(content) = fs::read(&path) {
                        let file_name = path.file_name().ok_or_else(|| anyhow::anyhow!("Invalid log file path"))?;
                        let zip_path = format!("logs/{}", file_name.to_string_lossy());
                        zip.start_file(zip_path, options)?;
                        zip.write_all(&content)?;
                    }
                }
            }
        }
    }

    // Check for build.log
    if let Ok(content) = fs::read("build.log") {
        zip.start_file("logs/build.log", options)?;
        zip.write_all(&content)?;
    }

    Ok(())
}

fn collect_telemetry(zip: &mut ZipWriter<File>, options: FileOptions, cpid: &str) -> Result<()> {
    let telemetry_dir = format!("var/telemetry/{}", cpid);
    if let Ok(entries) = fs::read_dir(&telemetry_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "ndjson" || ext == "sig" || ext == "json" {
                    if let Ok(content) = fs::read(&path) {
                        let zip_path = format!(
                            "telemetry/{}/{}",
                            cpid,
                            path.file_name().ok_or_else(|| anyhow::anyhow!("Invalid config file path"))?.to_string_lossy()
                        );
                        zip.start_file(zip_path, options)?;
                        zip.write_all(&content)?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn collect_recent_telemetry(zip: &mut ZipWriter<File>, options: FileOptions) -> Result<()> {
    // Collect last 3 telemetry bundles
    if let Ok(entries) = fs::read_dir("var/telemetry") {
        let mut bundles: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("ndjson"))
            .collect();

        // Sort by modification time
        bundles.sort_by_key(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        bundles.reverse(); // Most recent first

        for entry in bundles.iter().take(3) {
            let path = entry.path();
            if let Ok(content) = fs::read(&path) {
                let file_name = path.file_name().ok_or_else(|| anyhow::anyhow!("Invalid telemetry file path"))?;
                let zip_path = format!("telemetry/recent/{}", file_name.to_string_lossy());
                zip.start_file(zip_path, options)?;
                zip.write_all(&content)?;
            }

            // Also include signature file
            let sig_path = path.with_extension("ndjson.sig");
            if let Ok(content) = fs::read(&sig_path) {
                let zip_path = format!(
                    "telemetry/recent/{}",
                    sig_path.file_name().ok_or_else(|| anyhow::anyhow!("Invalid signature file path"))?.to_string_lossy()
                );
                zip.start_file(zip_path, options)?;
                zip.write_all(&content)?;
            }
        }
    }

    Ok(())
}

fn collect_alerts(zip: &mut ZipWriter<File>, options: FileOptions) -> Result<()> {
    if let Ok(entries) = fs::read_dir("var/alerts") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Ok(content) = fs::read(&path) {
                    let file_name = path.file_name().ok_or_else(|| anyhow::anyhow!("Invalid alert file path"))?;
                    let zip_path = format!("alerts/{}", file_name.to_string_lossy());
                    zip.start_file(zip_path, options)?;
                    zip.write_all(&content)?;
                }
            }
        }
    }

    Ok(())
}

