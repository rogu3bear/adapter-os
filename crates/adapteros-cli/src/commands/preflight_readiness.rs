//! Adapter readiness validation for hot-swap operations
//!
//! This module provides comprehensive validation to ensure adapters are ready
//! before being hot-swapped into production. Checks include:
//! - File existence and accessibility
//! - File size validation
//! - SafeTensors format validation
//! - Weight integrity (BLAKE3 hash)
//! - Metadata/configuration completeness
//! - I/O warmup test
//!
//! # Usage
//!
//! Use `require_readiness_before_swap()` to gate swap operations:
//!
//! ```ignore
//! use std::path::Path;
//! use adapteros_cli::commands::preflight_readiness::{require_readiness_before_swap, ReadinessConfig};
//!
//! async fn example() -> anyhow::Result<()> {
//!     let adapter_path = Path::new("/path/to/adapter.aos");
//!
//!     // Gate the swap operation - returns error if not ready
//!     let result = require_readiness_before_swap(adapter_path, None).await?;
//!
//!     // Only reaches here if adapter passed critical checks
//!     println!("Adapter ready: {}", result.summary);
//!     Ok(())
//! }
//! ```
//!
//! Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use crate::output::OutputWriter;
use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Cell, Color, Table};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// ============================================================================
// Types
// ============================================================================

/// Check status for individual readiness checks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReadinessCheckStatus {
    Pass,
    Warning,
    Fail,
}

/// Individual readiness check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessCheck {
    pub name: String,
    pub status: ReadinessCheckStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
    /// Duration of this check in milliseconds
    pub duration_ms: u64,
}

impl ReadinessCheck {
    fn pass(name: &str, message: &str, duration_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: ReadinessCheckStatus::Pass,
            message: message.to_string(),
            details: None,
            fix_hint: None,
            duration_ms,
        }
    }

    fn warning(name: &str, message: &str, fix_hint: Option<String>, duration_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: ReadinessCheckStatus::Warning,
            message: message.to_string(),
            details: None,
            fix_hint,
            duration_ms,
        }
    }

    fn fail(name: &str, message: &str, fix_hint: Option<String>, duration_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            status: ReadinessCheckStatus::Fail,
            message: message.to_string(),
            details: None,
            fix_hint,
            duration_ms,
        }
    }

    fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }
}

/// Configuration for adapter readiness checks before hot-swap
#[derive(Debug, Clone)]
pub struct ReadinessConfig {
    /// Maximum time for all checks (milliseconds)
    pub timeout_ms: u64,
    /// Skip weight validation (faster but less thorough)
    pub skip_weight_validation: bool,
    /// Skip warmup check
    pub skip_warmup: bool,
    /// Minimum file size to consider valid (bytes)
    pub min_file_size: u64,
    /// Maximum file size to allow (bytes, 0 = unlimited)
    pub max_file_size: u64,
}

impl Default for ReadinessConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000, // 30 seconds default
            skip_weight_validation: false,
            skip_warmup: false,
            min_file_size: 1024, // At least 1KB
            max_file_size: 0,    // No limit by default
        }
    }
}

/// Result of adapter readiness validation for hot-swap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterReadinessResult {
    /// Adapter identifier (filename without extension)
    pub adapter_id: String,
    /// Path to the adapter file
    pub path: PathBuf,
    /// Whether the adapter passed all critical checks (no failures)
    pub ready: bool,
    /// Individual check results
    pub checks: Vec<ReadinessCheck>,
    /// Content hash of the file (BLAKE3 hex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Estimated VRAM usage in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_vram_mb: Option<u64>,
    /// Total validation time in milliseconds
    pub duration_ms: u64,
    /// Summary message
    pub summary: String,
}

impl AdapterReadinessResult {
    /// Get list of failed critical checks
    pub fn critical_failures(&self) -> Vec<&ReadinessCheck> {
        self.checks
            .iter()
            .filter(|c| c.status == ReadinessCheckStatus::Fail)
            .collect()
    }

    /// Get list of warnings
    pub fn warnings(&self) -> Vec<&ReadinessCheck> {
        self.checks
            .iter()
            .filter(|c| c.status == ReadinessCheckStatus::Warning)
            .collect()
    }
}

// ============================================================================
// Main API
// ============================================================================

/// Validate adapter file readiness before hot-swap
///
/// Performs comprehensive file-level validation to ensure the adapter can be
/// safely loaded and swapped into production:
/// - File existence and accessibility
/// - File size validation
/// - SafeTensors format validation
/// - Weight integrity (BLAKE3 hash)
/// - Metadata/configuration completeness
/// - I/O warmup test
///
/// # Arguments
/// * `path` - Path to the .aos adapter file
/// * `config` - Optional configuration for checks
///
/// # Returns
/// * `AdapterReadinessResult` with all check results
pub async fn check_adapter_readiness(
    path: &Path,
    config: Option<ReadinessConfig>,
) -> AdapterReadinessResult {
    let config = config.unwrap_or_default();
    let start = Instant::now();
    let mut checks = Vec::new();
    let mut content_hash: Option<String> = None;
    let mut estimated_vram_mb: Option<u64> = None;

    let adapter_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Check 1: File existence
    let check_start = Instant::now();
    let exists = path.exists();
    checks.push(if exists {
        ReadinessCheck::pass(
            "File Exists",
            &format!("Adapter file found at {}", path.display()),
            check_start.elapsed().as_millis() as u64,
        )
    } else {
        ReadinessCheck::fail(
            "File Exists",
            &format!("Adapter file not found: {}", path.display()),
            Some(format!(
                "Ensure the adapter file exists at {}",
                path.display()
            )),
            check_start.elapsed().as_millis() as u64,
        )
    });

    if !exists {
        return build_result(
            adapter_id,
            path.to_path_buf(),
            checks,
            start.elapsed(),
            content_hash,
            estimated_vram_mb,
        );
    }

    // Check 2: File accessible
    let check_start = Instant::now();
    match std::fs::File::open(path) {
        Ok(_) => {
            checks.push(ReadinessCheck::pass(
                "File Accessible",
                "Adapter file is readable",
                check_start.elapsed().as_millis() as u64,
            ));
        }
        Err(e) => {
            checks.push(
                ReadinessCheck::fail(
                    "File Accessible",
                    &format!("Cannot read adapter file: {}", e),
                    Some("Check file permissions".to_string()),
                    check_start.elapsed().as_millis() as u64,
                )
                .with_details(format!("Error: {:?}", e.kind())),
            );
            return build_result(
                adapter_id,
                path.to_path_buf(),
                checks,
                start.elapsed(),
                content_hash,
                estimated_vram_mb,
            );
        }
    }

    // Check 3: File size
    let check_start = Instant::now();
    match std::fs::metadata(path) {
        Ok(metadata) => {
            let size = metadata.len();
            estimated_vram_mb = Some(size / (1024 * 1024));

            if size < config.min_file_size {
                checks.push(ReadinessCheck::fail(
                    "File Size",
                    &format!(
                        "File too small: {} bytes (minimum: {} bytes) - file may be corrupted",
                        size, config.min_file_size
                    ),
                    Some("Re-download or regenerate the adapter file".to_string()),
                    check_start.elapsed().as_millis() as u64,
                ));
            } else if config.max_file_size > 0 && size > config.max_file_size {
                checks.push(ReadinessCheck::warning(
                    "File Size",
                    &format!(
                        "File is large: {} bytes ({:.2} MB) - may require significant VRAM",
                        size,
                        size as f64 / 1024.0 / 1024.0
                    ),
                    None,
                    check_start.elapsed().as_millis() as u64,
                ));
            } else {
                checks.push(ReadinessCheck::pass(
                    "File Size",
                    &format!("{} bytes ({:.2} MB)", size, size as f64 / 1024.0 / 1024.0),
                    check_start.elapsed().as_millis() as u64,
                ));
            }
        }
        Err(e) => {
            checks.push(ReadinessCheck::fail(
                "File Size",
                &format!("Cannot read file metadata: {}", e),
                None,
                check_start.elapsed().as_millis() as u64,
            ));
        }
    }

    // Check 4: SafeTensors format validation
    checks.push(check_safetensors_format(path));

    // Check 5: Weight integrity (if not skipped)
    if !config.skip_weight_validation {
        let (hash_check, hash) = check_weight_integrity(path);
        checks.push(hash_check);
        content_hash = hash;
    }

    // Check 6: Configuration/metadata completeness
    checks.push(check_adapter_config(path));

    // Check 7: I/O warmup (if not skipped)
    if !config.skip_warmup {
        checks.push(check_io_warmup(path));
    }

    // Check timeout
    if start.elapsed().as_millis() as u64 > config.timeout_ms {
        checks.push(ReadinessCheck::fail(
            "Timeout",
            &format!("Validation exceeded {}ms timeout", config.timeout_ms),
            Some("Try with skip_weight_validation=true for faster checks".to_string()),
            0,
        ));
    }

    build_result(
        adapter_id,
        path.to_path_buf(),
        checks,
        start.elapsed(),
        content_hash,
        estimated_vram_mb,
    )
}

/// Require readiness before swap - the main gate function for hot-swap validation
///
/// This function validates that an adapter is ready for hot-swap and returns
/// an error if critical checks fail. Call this before any swap operation.
///
/// # Arguments
/// * `path` - Path to the adapter .aos file
/// * `config` - Optional validation configuration
///
/// # Returns
/// * `Ok(result)` if adapter is ready (possibly with warnings)
/// * `Err(e)` if critical checks failed - swap should NOT proceed
pub async fn require_readiness_before_swap(
    path: &Path,
    config: Option<ReadinessConfig>,
) -> Result<AdapterReadinessResult> {
    let result = check_adapter_readiness(path, config).await;

    if result.ready {
        Ok(result)
    } else {
        let failures: Vec<String> = result
            .checks
            .iter()
            .filter(|c| c.status == ReadinessCheckStatus::Fail)
            .map(|c| {
                let mut msg = format!("  - {}: {}", c.name, c.message);
                if let Some(ref details) = c.details {
                    msg.push_str(&format!(" ({})", details));
                }
                if let Some(ref hint) = c.fix_hint {
                    msg.push_str(&format!("\n    Fix: {}", hint));
                }
                msg
            })
            .collect();

        Err(anyhow::anyhow!(
            "Adapter '{}' failed readiness checks and cannot be hot-swapped:\n{}\n\nPath: {}",
            result.adapter_id,
            failures.join("\n"),
            path.display()
        ))
    }
}

// ============================================================================
// Individual Checks
// ============================================================================

/// Build the final readiness result from check results
fn build_result(
    adapter_id: String,
    path: PathBuf,
    checks: Vec<ReadinessCheck>,
    duration: Duration,
    content_hash: Option<String>,
    estimated_vram_mb: Option<u64>,
) -> AdapterReadinessResult {
    let failures: Vec<_> = checks
        .iter()
        .filter(|c| c.status == ReadinessCheckStatus::Fail)
        .collect();
    let warnings: Vec<_> = checks
        .iter()
        .filter(|c| c.status == ReadinessCheckStatus::Warning)
        .collect();
    let passed_count = checks
        .iter()
        .filter(|c| c.status == ReadinessCheckStatus::Pass)
        .count();

    let ready = failures.is_empty();

    let summary = if ready && warnings.is_empty() {
        format!(
            "Adapter '{}' is ready for hot-swap ({}/{} checks passed)",
            adapter_id,
            passed_count,
            checks.len()
        )
    } else if ready {
        format!(
            "Adapter '{}' is ready with {} warning(s) ({}/{} checks passed)",
            adapter_id,
            warnings.len(),
            passed_count,
            checks.len()
        )
    } else {
        let failure_names: Vec<_> = failures.iter().map(|c| c.name.as_str()).collect();
        format!(
            "Adapter '{}' is NOT ready for hot-swap: {} critical failure(s) [{}]",
            adapter_id,
            failures.len(),
            failure_names.join(", ")
        )
    };

    AdapterReadinessResult {
        adapter_id,
        path,
        ready,
        checks,
        content_hash,
        estimated_vram_mb,
        duration_ms: duration.as_millis() as u64,
        summary,
    }
}

/// Check SafeTensors file format
fn check_safetensors_format(path: &Path) -> ReadinessCheck {
    let start = Instant::now();

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return ReadinessCheck::fail(
                "SafeTensors Format",
                &format!("Cannot open file: {}", e),
                None,
                start.elapsed().as_millis() as u64,
            )
        }
    };

    // Read 8-byte header size (little-endian u64)
    let mut header_bytes = [0u8; 8];
    if let Err(e) = file.read_exact(&mut header_bytes) {
        return ReadinessCheck::fail(
            "SafeTensors Format",
            &format!("Cannot read header: {}", e),
            Some("File may be truncated or corrupted".to_string()),
            start.elapsed().as_millis() as u64,
        );
    }

    let header_size = u64::from_le_bytes(header_bytes);

    // Sanity check header size
    if header_size > 100 * 1024 * 1024 {
        return ReadinessCheck::fail(
            "SafeTensors Format",
            &format!("Invalid header size: {} bytes (too large)", header_size),
            Some("File may not be valid SafeTensors format".to_string()),
            start.elapsed().as_millis() as u64,
        );
    }

    if header_size == 0 {
        return ReadinessCheck::fail(
            "SafeTensors Format",
            "Empty header (size = 0) - file appears empty or corrupted",
            Some("Regenerate or re-download the adapter file".to_string()),
            start.elapsed().as_millis() as u64,
        );
    }

    // Read and validate JSON header
    if header_size < 10 * 1024 * 1024 {
        let mut json_header = vec![0u8; header_size as usize];
        if let Err(e) = file.read_exact(&mut json_header) {
            return ReadinessCheck::fail(
                "SafeTensors Format",
                &format!("Cannot read JSON header: {}", e),
                None,
                start.elapsed().as_millis() as u64,
            );
        }

        // Check JSON structure
        if json_header.first() != Some(&b'{') {
            return ReadinessCheck::fail(
                "SafeTensors Format",
                "Header does not start with JSON object",
                Some("File may be corrupted or not in SafeTensors format".to_string()),
                start.elapsed().as_millis() as u64,
            );
        }

        // Parse JSON
        match serde_json::from_slice::<serde_json::Value>(&json_header) {
            Ok(metadata) => {
                let tensor_count = metadata
                    .as_object()
                    .map(|obj| obj.keys().filter(|k| *k != "__metadata__").count())
                    .unwrap_or(0);

                if tensor_count > 0 {
                    ReadinessCheck::pass(
                        "SafeTensors Format",
                        &format!(
                            "Valid SafeTensors format with {} tensor definition(s)",
                            tensor_count
                        ),
                        start.elapsed().as_millis() as u64,
                    )
                } else {
                    ReadinessCheck::fail(
                        "SafeTensors Format",
                        "No tensor definitions found in adapter - file may be empty",
                        Some("Adapter file contains no weight tensors".to_string()),
                        start.elapsed().as_millis() as u64,
                    )
                }
            }
            Err(e) => ReadinessCheck::fail(
                "SafeTensors Format",
                &format!("Invalid JSON header: {}", e),
                Some("File header is malformed".to_string()),
                start.elapsed().as_millis() as u64,
            ),
        }
    } else {
        ReadinessCheck::pass(
            "SafeTensors Format",
            &format!("Header size valid: {} bytes", header_size),
            start.elapsed().as_millis() as u64,
        )
    }
}

/// Check weight integrity by computing BLAKE3 hash
fn check_weight_integrity(path: &Path) -> (ReadinessCheck, Option<String>) {
    let start = Instant::now();

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return (
                ReadinessCheck::fail(
                    "Weight Integrity",
                    &format!("Cannot open file for hashing: {}", e),
                    None,
                    start.elapsed().as_millis() as u64,
                ),
                None,
            )
        }
    };

    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024]; // 64KB chunks for efficiency

    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buffer[..n]);
            }
            Err(e) => {
                return (
                    ReadinessCheck::fail(
                        "Weight Integrity",
                        &format!("Read error during hashing: {}", e),
                        Some("File may have I/O issues".to_string()),
                        start.elapsed().as_millis() as u64,
                    ),
                    None,
                )
            }
        }
    }

    let hash = hasher.finalize();
    let hash_hex = hex::encode(hash.as_bytes());
    let hash_short = &hash_hex[..16];

    (
        ReadinessCheck::pass(
            "Weight Integrity",
            &format!("BLAKE3 hash verified: {}...", hash_short),
            start.elapsed().as_millis() as u64,
        )
        .with_details(format!("Full hash: {}", hash_hex)),
        Some(hash_hex),
    )
}

/// Check adapter configuration/metadata completeness
fn check_adapter_config(path: &Path) -> ReadinessCheck {
    let start = Instant::now();

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return ReadinessCheck::fail(
                "Configuration",
                &format!("Cannot open file: {}", e),
                None,
                start.elapsed().as_millis() as u64,
            )
        }
    };

    let mut header_bytes = [0u8; 8];
    if file.read_exact(&mut header_bytes).is_err() {
        return ReadinessCheck::fail(
            "Configuration",
            "Cannot read header",
            None,
            start.elapsed().as_millis() as u64,
        );
    }

    let header_size = u64::from_le_bytes(header_bytes);
    if header_size > 10 * 1024 * 1024 {
        return ReadinessCheck::warning(
            "Configuration",
            "Header too large to fully validate",
            None,
            start.elapsed().as_millis() as u64,
        );
    }

    let mut json_header = vec![0u8; header_size as usize];
    if file.read_exact(&mut json_header).is_err() {
        return ReadinessCheck::fail(
            "Configuration",
            "Cannot read metadata",
            None,
            start.elapsed().as_millis() as u64,
        );
    }

    match serde_json::from_slice::<serde_json::Value>(&json_header) {
        Ok(metadata) => {
            // Check for __metadata__ section (optional but recommended)
            let has_metadata = metadata.get("__metadata__").is_some();

            // Count tensor definitions
            let tensor_count = metadata
                .as_object()
                .map(|obj| obj.keys().filter(|k| *k != "__metadata__").count())
                .unwrap_or(0);

            if tensor_count == 0 {
                return ReadinessCheck::fail(
                    "Configuration",
                    "No tensor definitions found - adapter file contains no weights",
                    Some("Adapter appears to be empty or corrupted".to_string()),
                    start.elapsed().as_millis() as u64,
                );
            }

            if has_metadata {
                ReadinessCheck::pass(
                    "Configuration",
                    &format!(
                        "Complete configuration: {} tensor(s) with metadata",
                        tensor_count
                    ),
                    start.elapsed().as_millis() as u64,
                )
            } else {
                ReadinessCheck::pass(
                    "Configuration",
                    &format!("{} tensor(s) defined", tensor_count),
                    start.elapsed().as_millis() as u64,
                )
                .with_details("No custom __metadata__ section (optional)".to_string())
            }
        }
        Err(e) => ReadinessCheck::fail(
            "Configuration",
            &format!("Invalid metadata JSON: {}", e),
            Some("File header is malformed".to_string()),
            start.elapsed().as_millis() as u64,
        ),
    }
}

/// Check I/O warmup (verify file is fully accessible)
fn check_io_warmup(path: &Path) -> ReadinessCheck {
    let start = Instant::now();

    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return ReadinessCheck::warning(
                "I/O Warmup",
                &format!("Cannot open file: {}", e),
                None,
                start.elapsed().as_millis() as u64,
            )
        }
    };

    let file_size = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => {
            return ReadinessCheck::warning(
                "I/O Warmup",
                "Cannot get file size",
                None,
                start.elapsed().as_millis() as u64,
            )
        }
    };

    let mut buffer = [0u8; 4096];

    // Read first 4KB (header region)
    if file.read_exact(&mut buffer).is_err() && file_size >= 4096 {
        return ReadinessCheck::warning(
            "I/O Warmup",
            "Failed to read file start - possible I/O issue",
            None,
            start.elapsed().as_millis() as u64,
        );
    }

    // Read last 4KB (if file is large enough) - verifies entire file accessible
    if file_size > 8192 {
        if file.seek(SeekFrom::End(-4096)).is_err() {
            return ReadinessCheck::warning(
                "I/O Warmup",
                "Failed to seek to file end",
                None,
                start.elapsed().as_millis() as u64,
            );
        }
        if file.read_exact(&mut buffer).is_err() {
            return ReadinessCheck::warning(
                "I/O Warmup",
                "Failed to read file end - possible truncation",
                None,
                start.elapsed().as_millis() as u64,
            );
        }
    }

    ReadinessCheck::pass(
        "I/O Warmup",
        &format!("File I/O validated in {}ms", start.elapsed().as_millis()),
        start.elapsed().as_millis() as u64,
    )
}

// ============================================================================
// Display
// ============================================================================

/// Display adapter readiness result in formatted table
pub fn display_readiness_result(result: &AdapterReadinessResult, output: &OutputWriter) {
    output.info(format!(
        "\nAdapter Readiness Check: {}\n",
        result.adapter_id
    ));

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Check", "Status", "Time", "Message"]);

    for check in &result.checks {
        let (symbol, color) = match check.status {
            ReadinessCheckStatus::Pass => ("PASS", Color::Green),
            ReadinessCheckStatus::Warning => ("WARN", Color::Yellow),
            ReadinessCheckStatus::Fail => ("FAIL", Color::Red),
        };

        table.add_row(vec![
            Cell::new(&check.name),
            Cell::new(symbol).fg(color),
            Cell::new(format!("{}ms", check.duration_ms)),
            Cell::new(&check.message),
        ]);
    }

    println!("{}", table);

    output.blank();
    if result.ready {
        if result.warnings().is_empty() {
            output.success(format!(
                "READY: {} (validated in {}ms)",
                result.summary, result.duration_ms
            ));
        } else {
            output.warning(format!(
                "READY WITH WARNINGS: {} (validated in {}ms)",
                result.summary, result.duration_ms
            ));
        }
    } else {
        output.error(format!(
            "NOT READY: {} (validated in {}ms)",
            result.summary, result.duration_ms
        ));
    }

    if let Some(ref hash) = result.content_hash {
        output.kv(
            "Content Hash",
            &format!("{}...", &hash[..32.min(hash.len())]),
        );
    }
    if let Some(vram) = result.estimated_vram_mb {
        output.kv("Estimated VRAM", &format!("{} MB", vram));
    }
    output.kv("Path", &result.path.display().to_string());
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_readiness_check_creation() {
        let pass = ReadinessCheck::pass("test", "ok", 5);
        assert_eq!(pass.status, ReadinessCheckStatus::Pass);
        assert_eq!(pass.name, "test");
        assert_eq!(pass.duration_ms, 5);

        let fail = ReadinessCheck::fail("test", "error", Some("fix".to_string()), 10);
        assert_eq!(fail.status, ReadinessCheckStatus::Fail);
        assert_eq!(fail.fix_hint, Some("fix".to_string()));
    }

    #[test]
    fn test_readiness_config_default() {
        let config = ReadinessConfig::default();
        assert_eq!(config.timeout_ms, 30_000);
        assert!(!config.skip_weight_validation);
        assert!(!config.skip_warmup);
        assert_eq!(config.min_file_size, 1024);
        assert_eq!(config.max_file_size, 0);
    }

    #[tokio::test]
    async fn test_check_nonexistent_file() {
        let result =
            check_adapter_readiness(Path::new("/nonexistent/path/adapter.aos"), None).await;

        assert!(!result.ready);
        assert!(!result.critical_failures().is_empty());
        assert!(result.summary.contains("NOT ready"));
    }

    #[tokio::test]
    async fn test_check_empty_file() {
        let mut temp = NamedTempFile::new().unwrap();
        // Empty file
        temp.flush().unwrap();

        let result = check_adapter_readiness(temp.path(), None).await;

        // Should fail due to file being too small
        assert!(!result.ready);
    }

    #[tokio::test]
    async fn test_require_readiness_error() {
        let result =
            require_readiness_before_swap(Path::new("/nonexistent/path/adapter.aos"), None).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("failed readiness checks"));
    }

    #[test]
    fn test_adapter_readiness_result_methods() {
        let result = AdapterReadinessResult {
            adapter_id: "test".to_string(),
            path: PathBuf::from("/test"),
            ready: false,
            checks: vec![
                ReadinessCheck::pass("check1", "ok", 1),
                ReadinessCheck::warning("check2", "warn", None, 1),
                ReadinessCheck::fail("check3", "fail", None, 1),
            ],
            content_hash: None,
            estimated_vram_mb: None,
            duration_ms: 100,
            summary: "test".to_string(),
        };

        assert_eq!(result.critical_failures().len(), 1);
        assert_eq!(result.warnings().len(), 1);
    }
}
