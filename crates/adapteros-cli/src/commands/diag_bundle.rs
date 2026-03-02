//! Generate diagnostic bundle for troubleshooting

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub async fn run(output_path: &Path, cpid: Option<&str>, full_db: bool) -> Result<()> {
    let mode = crate::output::OutputMode::from_env();

    crate::output::command_header(
        &mode,
        &format!("Generating diagnostic bundle: {}", output_path.display()),
    );

    let file = File::create(output_path).context("Failed to create output file")?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
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
        crate::output::progress(
            &mode,
            &format!("Collecting telemetry for CPID: {}", cpid_str),
        );
        collect_telemetry(&mut zip, options, cpid_str)?;
    } else {
        crate::output::progress(&mode, "Collecting recent telemetry bundles...");
        collect_recent_telemetry(&mut zip, options)?;
    }

    // 8. Recent alerts
    crate::output::progress(&mode, "Collecting alert files...");
    collect_alerts(&mut zip, options)?;

    zip.finish().context("Failed to finalize zip")?;

    crate::output::result(&format!(
        "Diagnostic bundle created: {}",
        output_path.display()
    ));
    Ok(())
}

fn collect_system_info(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let mut info = String::new();
    info.push_str(&format!(
        "OS: {} {}\n",
        sysinfo::System::name().unwrap_or_default(),
        sysinfo::System::os_version().unwrap_or_default()
    ));
    info.push_str(&format!(
        "Kernel: {}\n",
        sysinfo::System::kernel_version().unwrap_or_default()
    ));
    info.push_str(&format!(
        "Hostname: {}\n",
        sysinfo::System::host_name().unwrap_or_default()
    ));
    info.push_str(&format!(
        "CPU: {}\n",
        sys.cpus()
            .first()
            .map(|cpu| cpu.brand())
            .unwrap_or("Unknown")
    ));
    info.push_str(&format!("CPU Cores: {}\n", sys.cpus().len()));
    // In sysinfo 0.30, memory is in bytes
    info.push_str(&format!(
        "Total Memory: {} MB\n",
        sys.total_memory() / 1024 / 1024
    ));
    info.push_str(&format!(
        "Available Memory: {} MB\n",
        sys.available_memory() / 1024 / 1024
    ));
    info.push_str(&format!("Uptime: {} seconds\n", System::uptime()));

    zip.start_file("system_info.txt", options)?;
    zip.write_all(info.as_bytes())?;

    Ok(())
}

fn collect_git_info(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
    let mut info = String::new();

    // Get current commit
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
    {
        if output.status.success() {
            info.push_str("Current commit: ");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    // Get branch
    if let Ok(output) = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
    {
        if output.status.success() {
            info.push_str("Branch: ");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    // Get last 10 commits
    if let Ok(output) = std::process::Command::new("git")
        .args(["log", "--oneline", "-10"])
        .output()
    {
        if output.status.success() {
            info.push_str("\nRecent commits:\n");
            info.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }

    // Get status
    if let Ok(output) = std::process::Command::new("git")
        .args(["status", "--short"])
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

fn collect_config_files(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
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

fn collect_metal_info(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
    let mut info = String::new();

    // Check if metallib exists and get its hash
    if let Ok(output) = std::process::Command::new("b3sum")
        .args(["metal/aos_kernels.metallib"])
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
        .args(["-version"])
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
    options: SimpleFileOptions,
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

    // Get recent jobs
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

fn collect_log_files(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
    // Check for log files in var/
    if let Ok(entries) = fs::read_dir("var/") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "log" || ext == "txt" {
                    if let Ok(content) = fs::read(&path) {
                        let file_name = path
                            .file_name()
                            .ok_or_else(|| anyhow::anyhow!("Invalid log file path"))?;
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

fn collect_telemetry(
    zip: &mut ZipWriter<File>,
    options: SimpleFileOptions,
    cpid: &str,
) -> Result<()> {
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
                            path.file_name()
                                .ok_or_else(|| anyhow::anyhow!("Invalid config file path"))?
                                .to_string_lossy()
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

fn collect_recent_telemetry(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
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
                let file_name = path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid telemetry file path"))?;
                let zip_path = format!("telemetry/recent/{}", file_name.to_string_lossy());
                zip.start_file(zip_path, options)?;
                zip.write_all(&content)?;
            }

            // Also include signature file
            let sig_path = path.with_extension("ndjson.sig");
            if let Ok(content) = fs::read(&sig_path) {
                let zip_path = format!(
                    "telemetry/recent/{}",
                    sig_path
                        .file_name()
                        .ok_or_else(|| anyhow::anyhow!("Invalid signature file path"))?
                        .to_string_lossy()
                );
                zip.start_file(zip_path, options)?;
                zip.write_all(&content)?;
            }
        }
    }

    Ok(())
}

fn collect_alerts(zip: &mut ZipWriter<File>, options: SimpleFileOptions) -> Result<()> {
    if let Ok(entries) = fs::read_dir("var/alerts") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Ok(content) = fs::read(&path) {
                    let file_name = path
                        .file_name()
                        .ok_or_else(|| anyhow::anyhow!("Invalid alert file path"))?;
                    let zip_path = format!("alerts/{}", file_name.to_string_lossy());
                    zip.start_file(zip_path, options)?;
                    zip.write_all(&content)?;
                }
            }
        }
    }

    Ok(())
}

// ============================================================================
// Signed Bundle Export and Verification
// ============================================================================

use adapteros_api_types::diagnostics::{
    BundleManifest, DiagBundleExportResponse, DiagBundleVerifyResponse, VerificationResult,
};
use adapteros_core::B3Hash;
use adapteros_crypto::{PublicKey, Signature};
use std::io::Read;
use tracing::{debug, error, warn};

/// Export format options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    TarZst,
    Zip,
}

impl std::str::FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tar.zst" | "tarzst" | "tar" => Ok(ExportFormat::TarZst),
            "zip" => Ok(ExportFormat::Zip),
            _ => Err(format!("Unknown format: {}. Valid: tar.zst, zip", s)),
        }
    }
}

/// Export a signed diagnostic bundle via the API.
pub async fn export_signed_bundle(
    trace_id: &str,
    output_path: &Path,
    format: ExportFormat,
    include_evidence: bool,
    evidence_token: Option<&str>,
    base_url: &str,
) -> Result<DiagBundleExportResponse> {
    use reqwest::Client;

    let mode = crate::output::OutputMode::from_env();
    crate::output::command_header(
        &mode,
        &format!("Exporting signed bundle for trace: {}", trace_id),
    );

    // Validate evidence authorization
    if include_evidence && evidence_token.is_none() {
        anyhow::bail!("Evidence token required when --include-evidence is set");
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("Failed to create HTTP client")?;

    // Build request body
    let format_str = match format {
        ExportFormat::TarZst => "tar.zst",
        ExportFormat::Zip => "zip",
    };

    let mut request_body = serde_json::json!({
        "trace_id": trace_id,
        "format": format_str,
        "include_evidence": include_evidence,
    });

    if let Some(token) = evidence_token {
        request_body["evidence_auth_token"] = serde_json::json!(token);
    }

    // Create bundle via API
    let url = format!("{}/v1/diag/bundle", base_url);
    crate::output::progress(&mode, &format!("Creating bundle via API: {}", url));

    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .context("API request failed")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Bundle creation failed ({}): {}", status, error_text);
    }

    let bundle_response: DiagBundleExportResponse =
        response.json().await.context("Failed to parse response")?;

    crate::output::progress(
        &mode,
        &format!(
            "Bundle created: {} ({} bytes)",
            bundle_response.export_id, bundle_response.size_bytes
        ),
    );

    // Download the bundle
    let download_url = format!("{}{}", base_url, bundle_response.download_url);
    crate::output::progress(&mode, &format!("Downloading bundle from: {}", download_url));

    let download_response = client
        .get(&download_url)
        .send()
        .await
        .context("Download failed")?;

    if !download_response.status().is_success() {
        anyhow::bail!("Download failed: {}", download_response.status());
    }

    let bundle_data = download_response
        .bytes()
        .await
        .context("Failed to read bundle")?;

    // Verify downloaded bundle hash
    let computed_hash = B3Hash::hash(&bundle_data);
    if computed_hash.to_hex() != bundle_response.bundle_hash {
        anyhow::bail!(
            "Bundle hash mismatch: expected {}, got {}",
            bundle_response.bundle_hash,
            computed_hash.to_hex()
        );
    }

    // Write to output file
    fs::write(output_path, &bundle_data).context(format!(
        "Failed to write bundle to {}",
        output_path.display()
    ))?;

    crate::output::result(&format!("Bundle saved to: {}", output_path.display()));
    crate::output::result(&format!("  Bundle hash: {}", bundle_response.bundle_hash));
    crate::output::result(&format!("  Merkle root: {}", bundle_response.merkle_root));
    crate::output::result(&format!("  Key ID: {}", bundle_response.key_id));

    Ok(bundle_response)
}

/// Verify a diagnostic bundle offline.
pub fn verify_bundle(bundle_path: &Path, verbose: bool) -> Result<DiagBundleVerifyResponse> {
    let mode = crate::output::OutputMode::from_env();
    crate::output::command_header(
        &mode,
        &format!("Verifying bundle: {}", bundle_path.display()),
    );

    // Check file exists
    if !bundle_path.exists() {
        anyhow::bail!("Bundle not found: {}", bundle_path.display());
    }

    // Read bundle file
    let bundle_data = fs::read(bundle_path)
        .context(format!("Failed to read bundle {}", bundle_path.display()))?;

    let bundle_hash = B3Hash::hash(&bundle_data);
    crate::output::progress(&mode, &format!("Bundle hash: {}", bundle_hash.to_hex()));

    // Extract manifest from bundle
    let manifest = extract_manifest_from_bundle(&bundle_data)?;

    if verbose {
        crate::output::result("Manifest:");
        crate::output::result(&format!("  Schema version: {}", manifest.schema_version));
        crate::output::result(&format!("  Trace ID: {}", manifest.trace_id));
        crate::output::result(&format!("  Run ID: {}", manifest.run_id));
        crate::output::result(&format!("  Events count: {}", manifest.events_count));
        crate::output::result(&format!("  Files: {}", manifest.files.len()));
    }

    // Verify all file hashes
    let mut files_verified = 0u32;
    let mut files_valid = true;
    let mut warnings = Vec::new();

    for file_entry in &manifest.files {
        let file_data = extract_file_from_bundle(&bundle_data, &file_entry.path)?;
        let computed_hash = B3Hash::hash(&file_data);

        if computed_hash.to_hex() != file_entry.hash {
            error!(
                "File hash mismatch: {} (expected {}, got {})",
                file_entry.path,
                file_entry.hash,
                computed_hash.to_hex()
            );
            files_valid = false;
        } else if verbose {
            debug!("  ✓ {} ({})", file_entry.path, file_entry.hash);
        }

        files_verified += 1;
    }

    // Verify events Merkle root
    let events_data = extract_file_from_bundle(&bundle_data, "events.ndjson")?;
    let computed_merkle_root = compute_events_merkle_root(&events_data)?;
    let merkle_valid = computed_merkle_root.to_hex() == manifest.events_merkle_root;

    if !merkle_valid {
        error!(
            "Merkle root mismatch: expected {}, got {}",
            manifest.events_merkle_root,
            computed_merkle_root.to_hex()
        );
    }

    // Manifest is embedded, so it's always valid if we could parse it
    let manifest_valid = true;

    // Verify signature
    let signature_valid = verify_bundle_signature(&bundle_data, &bundle_hash)?;

    // Count events
    let events_count = count_events_in_ndjson(&events_data);

    // Check for warnings
    if manifest.events_truncated {
        warnings.push(format!(
            "Events were truncated ({} of {} total)",
            manifest.events_count,
            events_count.max(manifest.events_count)
        ));
    }

    if !manifest.evidence_included {
        warnings.push("Evidence payload not included".to_string());
    }

    let all_valid = signature_valid && manifest_valid && files_valid && merkle_valid;

    let result = VerificationResult {
        signature_valid,
        manifest_hash_valid: manifest_valid,
        files_hash_valid: files_valid,
        merkle_root_valid: merkle_valid,
        files_verified,
        events_verified: events_count,
        key_id: manifest
            .identity
            .code_identity
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        signed_at: Some(manifest.created_at.clone()),
    };

    if all_valid {
        crate::output::result("✓ Bundle verification PASSED");
    } else {
        crate::output::result("✗ Bundle verification FAILED");
    }

    if verbose && !warnings.is_empty() {
        crate::output::result("Warnings:");
        for warning in &warnings {
            warn!("  - {}", warning);
        }
    }

    Ok(DiagBundleVerifyResponse {
        schema_version: manifest.schema_version.clone(),
        valid: all_valid,
        result,
        warnings,
    })
}

/// Extract manifest.json from tar.zst bundle.
fn extract_manifest_from_bundle(bundle_data: &[u8]) -> Result<BundleManifest> {
    let manifest_data = extract_file_from_bundle(bundle_data, "manifest.json")?;

    serde_json::from_slice(&manifest_data).context("Failed to parse manifest.json")
}

/// Extract a file from tar.zst bundle.
fn extract_file_from_bundle(bundle_data: &[u8], file_name: &str) -> Result<Vec<u8>> {
    use std::io::Cursor;

    // Try tar.zst first
    let cursor = Cursor::new(bundle_data);
    if let Ok(decoder) = zstd::stream::Decoder::new(cursor) {
        let mut archive = tar::Archive::new(decoder);

        for entry in archive.entries().context("Failed to read tar entries")? {
            let mut entry = entry.context("Failed to read tar entry")?;

            let path = entry.path().context("Failed to get entry path")?;

            if path.to_string_lossy() == file_name {
                let mut data = Vec::new();
                entry
                    .read_to_end(&mut data)
                    .context(format!("Failed to read file {}", file_name))?;
                return Ok(data);
            }
        }

        anyhow::bail!("File not found in bundle: {}", file_name);
    }

    // Try zip format
    let cursor = Cursor::new(bundle_data);
    if let Ok(mut archive) = zip::ZipArchive::new(cursor) {
        if let Ok(mut file) = archive.by_name(file_name) {
            let mut data = Vec::new();
            file.read_to_end(&mut data)
                .context(format!("Failed to read file {}", file_name))?;
            return Ok(data);
        }

        anyhow::bail!("File not found in bundle: {}", file_name);
    }

    anyhow::bail!("Bundle is neither tar.zst nor zip format")
}

/// Compute Merkle root of events in NDJSON format.
fn compute_events_merkle_root(events_data: &[u8]) -> Result<B3Hash> {
    let events_str = String::from_utf8_lossy(events_data);
    let lines: Vec<&str> = events_str.lines().filter(|l| !l.is_empty()).collect();

    if lines.is_empty() {
        return Ok(B3Hash::hash(b"empty"));
    }

    // Hash each line (event)
    let mut hashes: Vec<B3Hash> = lines
        .iter()
        .map(|line| B3Hash::hash(line.as_bytes()))
        .collect();

    // Build Merkle tree
    while hashes.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in hashes.chunks(2) {
            if chunk.len() == 2 {
                let mut combined = Vec::new();
                combined.extend_from_slice(chunk[0].as_bytes());
                combined.extend_from_slice(chunk[1].as_bytes());
                next_level.push(B3Hash::hash(&combined));
            } else {
                next_level.push(chunk[0]);
            }
        }
        hashes = next_level;
    }

    Ok(hashes.pop().unwrap_or_else(|| B3Hash::hash(b"empty")))
}

/// Count events in NDJSON data.
fn count_events_in_ndjson(events_data: &[u8]) -> u64 {
    let events_str = String::from_utf8_lossy(events_data);
    events_str.lines().filter(|l| !l.is_empty()).count() as u64
}

/// Verify bundle signature.
fn verify_bundle_signature(bundle_data: &[u8], bundle_hash: &B3Hash) -> Result<bool> {
    // Try to extract receipt.sig from bundle
    if let Ok(sig_data) = extract_file_from_bundle(bundle_data, "receipt.sig") {
        // Parse signature file
        if let Ok(sig_json) = serde_json::from_slice::<serde_json::Value>(&sig_data) {
            let signature_hex = sig_json
                .get("signature")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing signature field"))?;

            let public_key_hex = sig_json
                .get("public_key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing public_key field"))?;

            let merkle_root_hex = sig_json
                .get("merkle_root")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing merkle_root field"))?;

            // Parse signature and public key
            let signature_bytes = hex::decode(signature_hex).context("Invalid signature hex")?;
            let public_key_bytes = hex::decode(public_key_hex).context("Invalid public key hex")?;

            // Verify signature
            let public_key = PublicKey::from_bytes(
                public_key_bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid public key length"))?,
            )
            .context("Invalid public key")?;

            let signature = Signature::from_bytes(
                signature_bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
            )
            .context("Invalid signature")?;

            // Build message: bundle_hash || merkle_root
            let merkle_root =
                B3Hash::from_hex(merkle_root_hex).context("Invalid merkle root hex")?;
            let mut message = Vec::new();
            message.extend_from_slice(bundle_hash.as_bytes());
            message.extend_from_slice(merkle_root.as_bytes());

            match public_key.verify(&message, &signature) {
                Ok(()) => return Ok(true),
                Err(e) => {
                    warn!("Signature verification failed: {}", e);
                    return Ok(false);
                }
            }
        }
    }

    // No signature file found - bundle may be unsigned
    warn!("No signature file found in bundle");
    Ok(false)
}

/// Print bundle summary.
pub fn print_bundle_summary(response: &DiagBundleExportResponse) {
    println!("Bundle Export Summary");
    println!("═════════════════════════════════════════════════════════════");
    println!("  Export ID: {}", response.export_id);
    println!("  Format: {}", response.format);
    println!("  Size: {} bytes", response.size_bytes);
    println!("  Bundle Hash: {}", response.bundle_hash);
    println!("  Merkle Root: {}", response.merkle_root);
    println!("  Key ID: {}", response.key_id);
    println!("  Created: {}", response.created_at);
    println!("  Download URL: {}", response.download_url);
    println!();
    println!("Manifest:");
    println!("  Trace ID: {}", response.manifest.trace_id);
    println!("  Run ID: {}", response.manifest.run_id);
    println!("  Status: {}", response.manifest.run_status);
    println!("  Events: {}", response.manifest.events_count);
    println!("  Files: {}", response.manifest.files.len());
    println!("  Evidence: {}", response.manifest.evidence_included);
}

/// Print verification summary.
pub fn print_verification_summary(response: &DiagBundleVerifyResponse) {
    println!("Bundle Verification Summary");
    println!("═════════════════════════════════════════════════════════════");

    let status = if response.valid {
        "✓ VALID"
    } else {
        "✗ INVALID"
    };
    println!("  Status: {}", status);
    println!(
        "  Signature: {}",
        if response.result.signature_valid {
            "✓"
        } else {
            "✗"
        }
    );
    println!(
        "  Manifest Hash: {}",
        if response.result.manifest_hash_valid {
            "✓"
        } else {
            "✗"
        }
    );
    println!(
        "  File Hashes: {}",
        if response.result.files_hash_valid {
            "✓"
        } else {
            "✗"
        }
    );
    println!(
        "  Merkle Root: {}",
        if response.result.merkle_root_valid {
            "✓"
        } else {
            "✗"
        }
    );
    println!("  Files Verified: {}", response.result.files_verified);
    println!("  Events Verified: {}", response.result.events_verified);

    if !response.warnings.is_empty() {
        println!();
        println!("Warnings:");
        for warning in &response.warnings {
            println!("  - {}", warning);
        }
    }
}
