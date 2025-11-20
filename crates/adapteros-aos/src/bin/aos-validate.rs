//! AOS file validation tool
//!
//! Validates .aos archive files for production deployment readiness.
//! Replaces the Python validate_aos.py script with comprehensive Rust validation.
//!
//! ## Validation Checks
//!
//! - File structure integrity (header, manifest, weights)
//! - Manifest schema validation (required fields, types)
//! - Semantic naming convention (tenant/domain/purpose/revision)
//! - BLAKE3 hash verification (if present in manifest)
//! - Rank validation (1-256 range)
//! - Alpha parameter validation
//! - Target modules validation
//! - Format version compatibility (2.0)
//! - File size limits
//! - Tensor metadata consistency
//! - Safetensors format validation
//!
//! ## Exit Codes
//!
//! - 0: All validations passed
//! - 1: One or more validations failed

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{naming::AdapterName, AosError, B3Hash, Result};
use clap::Parser;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aos-validate")]
#[command(about = "Validate AOS archive files for production deployment", long_about = None)]
struct Cli {
    /// Path to .aos file
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Output as JSON (for CI/CD integration)
    #[arg(long)]
    json: bool,

    /// Verbose output (show all checks including passed)
    #[arg(short, long)]
    verbose: bool,

    /// Skip tensor data validation (faster, but less thorough)
    #[arg(long)]
    skip_tensors: bool,

    /// Skip BLAKE3 hash verification
    #[arg(long)]
    skip_hash: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.file.exists() {
        return Err(AosError::NotFound(format!(
            "File not found: {}",
            cli.file.display()
        )));
    }

    let result = validate_file(&cli.file, &cli)?;

    if cli.json {
        print_json(&result)?;
    } else {
        print_human_readable(&result, &cli)?;
    }

    // Exit with error code if validation failed
    if !result.valid {
        std::process::exit(1);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct ValidationResult {
    file_path: String,
    valid: bool,
    checks: Vec<Check>,
    errors: Vec<String>,
    warnings: Vec<String>,
    summary: ValidationSummary,
}

#[derive(Debug, Serialize)]
struct ValidationSummary {
    total_checks: usize,
    passed: usize,
    failed: usize,
    warnings: usize,
    file_size_bytes: u64,
    manifest_valid: bool,
    weights_valid: bool,
}

#[derive(Debug, Serialize)]
struct Check {
    name: String,
    passed: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    severity: CheckSeverity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckSeverity {
    Critical,
    Warning,
    Info,
}

fn validate_file(path: &PathBuf, cli: &Cli) -> Result<ValidationResult> {
    let mut checks = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Phase 1: File-level validation
    let file_size = check_file_readable(path, &mut checks)?;
    check_file_size(path, file_size, &mut checks, &mut warnings)?;

    // Phase 2: Header validation
    let (manifest_offset, manifest_len) = match check_header(path, &mut checks, &mut errors) {
        Ok(header) => header,
        Err(_e) => {
            // Critical failure - cannot continue
            return Ok(create_validation_result(
                path, checks, errors, warnings, file_size,
            ));
        }
    };

    // Phase 3: Manifest validation
    let _manifest_valid = match check_manifest(path, manifest_offset, manifest_len, &mut checks) {
        Ok(manifest) => {
            // Validate manifest schema
            check_manifest_schema(&manifest, &mut checks, &mut errors)?;

            // Validate manifest fields
            check_manifest_fields(&manifest, &mut checks, &mut warnings)?;

            // Validate semantic naming if adapter_id present
            if let Some(adapter_id) = manifest.get("adapter_id").and_then(|v| v.as_str()) {
                check_semantic_naming(adapter_id, &mut checks, &mut warnings)?;
            }

            // Validate rank and alpha
            check_rank_alpha(&manifest, &mut checks, &mut errors)?;

            // Validate target modules
            check_target_modules(&manifest, &mut checks, &mut warnings)?;

            // BLAKE3 hash verification
            if !cli.skip_hash {
                if let Some(hash_str) = manifest.get("weights_hash").and_then(|v| v.as_str()) {
                    check_blake3_hash(path, hash_str, &mut checks, &mut errors)?;
                }
            }

            true
        }
        Err(e) => {
            errors.push(format!("Manifest validation failed: {}", e));
            checks.push(Check {
                name: "Manifest Parsing".to_string(),
                passed: false,
                message: "Invalid manifest JSON".to_string(),
                details: Some(e.to_string()),
                severity: CheckSeverity::Critical,
            });
            false
        }
    };

    // Phase 4: Weights validation
    let _weights_valid = if !cli.skip_tensors {
        match check_weights(path, &mut checks) {
            Ok(()) => true,
            Err(e) => {
                warnings.push(format!("Tensor validation incomplete: {}", e));
                false
            }
        }
    } else {
        checks.push(Check {
            name: "Tensor Validation".to_string(),
            passed: true,
            message: "Skipped (--skip-tensors)".to_string(),
            details: None,
            severity: CheckSeverity::Info,
        });
        true
    };

    // Phase 5: File integrity check
    check_file_integrity(
        path,
        manifest_offset,
        manifest_len,
        &mut checks,
        &mut errors,
    )?;

    Ok(create_validation_result(
        path, checks, errors, warnings, file_size,
    ))
}

fn create_validation_result(
    path: &PathBuf,
    checks: Vec<Check>,
    errors: Vec<String>,
    warnings: Vec<String>,
    file_size: u64,
) -> ValidationResult {
    let valid = errors.is_empty() && checks.iter().all(|c| c.passed);
    let passed = checks.iter().filter(|c| c.passed).count();
    let failed = checks.len() - passed;

    ValidationResult {
        file_path: path.display().to_string(),
        valid,
        summary: ValidationSummary {
            total_checks: checks.len(),
            passed,
            failed,
            warnings: warnings.len(),
            file_size_bytes: file_size,
            manifest_valid: !checks
                .iter()
                .any(|c| c.name.contains("Manifest") && !c.passed),
            weights_valid: !checks
                .iter()
                .any(|c| c.name.contains("Tensor") && !c.passed),
        },
        checks,
        errors,
        warnings,
    }
}

fn check_file_readable(path: &PathBuf, checks: &mut Vec<Check>) -> Result<u64> {
    match File::open(path) {
        Ok(file) => {
            let file_size = file
                .metadata()
                .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?
                .len();
            checks.push(Check {
                name: "File Access".to_string(),
                passed: true,
                message: "File is readable".to_string(),
                details: None,
                severity: CheckSeverity::Info,
            });
            Ok(file_size)
        }
        Err(e) => {
            checks.push(Check {
                name: "File Access".to_string(),
                passed: false,
                message: format!("Cannot read file: {}", e),
                details: None,
                severity: CheckSeverity::Critical,
            });
            Err(AosError::Io(format!("Cannot read file: {}", e)))
        }
    }
}

fn check_file_size(
    _path: &PathBuf,
    size: u64,
    checks: &mut Vec<Check>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    const MIN_SIZE: u64 = 16; // Header + minimal manifest
    const MAX_SIZE: u64 = 10 * 1024 * 1024 * 1024; // 10 GB
    const WARN_SIZE: u64 = 1 * 1024 * 1024 * 1024; // 1 GB

    if size < MIN_SIZE {
        checks.push(Check {
            name: "File Size".to_string(),
            passed: false,
            message: format!("File too small ({} bytes)", size),
            details: Some(format!("Minimum size is {} bytes", MIN_SIZE)),
            severity: CheckSeverity::Critical,
        });
    } else if size > MAX_SIZE {
        checks.push(Check {
            name: "File Size".to_string(),
            passed: false,
            message: format!("File too large ({} bytes)", size),
            details: Some("Consider splitting into multiple adapters".to_string()),
            severity: CheckSeverity::Critical,
        });
    } else {
        let message = format_file_size(size);
        let severity = if size > WARN_SIZE {
            warnings.push(format!("Large file size: {}", message));
            CheckSeverity::Warning
        } else {
            CheckSeverity::Info
        };

        checks.push(Check {
            name: "File Size".to_string(),
            passed: true,
            message,
            details: None,
            severity,
        });
    }

    Ok(())
}

fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

fn check_header(
    path: &PathBuf,
    checks: &mut Vec<Check>,
    errors: &mut Vec<String>,
) -> Result<(u32, u32)> {
    let header_result = AOS2Writer::read_header(path);

    match header_result {
        Ok((manifest_offset, manifest_len)) => {
            let metadata = std::fs::metadata(path)
                .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?;
            let file_size = metadata.len();

            if manifest_offset < 8 {
                errors.push("Invalid manifest offset".to_string());
                checks.push(Check {
                    name: "Header Format".to_string(),
                    passed: false,
                    message: "Invalid manifest offset".to_string(),
                    details: Some(format!(
                        "Offset {} is less than header size (8)",
                        manifest_offset
                    )),
                    severity: CheckSeverity::Critical,
                });
                return Err(AosError::Validation("Invalid header offset".to_string()));
            }

            if manifest_offset as u64 + manifest_len as u64 > file_size {
                errors.push("Manifest extends beyond file".to_string());
                checks.push(Check {
                    name: "Header Format".to_string(),
                    passed: false,
                    message: "Manifest extends beyond file".to_string(),
                    details: Some(format!(
                        "Offset {} + length {} > file size {}",
                        manifest_offset, manifest_len, file_size
                    )),
                    severity: CheckSeverity::Critical,
                });
                return Err(AosError::Validation("Invalid header bounds".to_string()));
            }

            checks.push(Check {
                name: "Header Format".to_string(),
                passed: true,
                message: format!("Valid (offset={}, len={})", manifest_offset, manifest_len),
                details: None,
                severity: CheckSeverity::Info,
            });

            Ok((manifest_offset, manifest_len))
        }
        Err(e) => {
            errors.push(format!("Header validation failed: {}", e));
            checks.push(Check {
                name: "Header Format".to_string(),
                passed: false,
                message: "Invalid header format".to_string(),
                details: Some(e.to_string()),
                severity: CheckSeverity::Critical,
            });
            Err(e)
        }
    }
}

fn check_manifest(
    path: &PathBuf,
    manifest_offset: u32,
    manifest_len: u32,
    checks: &mut Vec<Check>,
) -> Result<Value> {
    let mut file =
        File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    file.seek(std::io::SeekFrom::Start(manifest_offset as u64))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

    let mut manifest_bytes = vec![0u8; manifest_len as usize];
    file.read_exact(&mut manifest_bytes)
        .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;

    match serde_json::from_slice::<Value>(&manifest_bytes) {
        Ok(manifest) => {
            checks.push(Check {
                name: "Manifest JSON".to_string(),
                passed: true,
                message: "Valid JSON".to_string(),
                details: None,
                severity: CheckSeverity::Info,
            });
            Ok(manifest)
        }
        Err(e) => Err(AosError::Io(format!("Invalid manifest JSON: {}", e))),
    }
}

fn check_manifest_schema(
    manifest: &Value,
    checks: &mut Vec<Check>,
    errors: &mut Vec<String>,
) -> Result<()> {
    let required_fields = ["version"];
    let mut missing = Vec::new();

    for field in &required_fields {
        if manifest.get(field).is_none() {
            missing.push(*field);
        }
    }

    if missing.is_empty() {
        checks.push(Check {
            name: "Manifest Schema".to_string(),
            passed: true,
            message: "All required fields present".to_string(),
            details: None,
            severity: CheckSeverity::Info,
        });
        Ok(())
    } else {
        let msg = format!("Missing required fields: {:?}", missing);
        errors.push(msg.clone());
        checks.push(Check {
            name: "Manifest Schema".to_string(),
            passed: false,
            message: msg,
            details: None,
            severity: CheckSeverity::Critical,
        });
        Ok(())
    }
}

fn check_manifest_fields(
    manifest: &Value,
    checks: &mut Vec<Check>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    // Check version
    if let Some(version) = manifest.get("version").and_then(|v| v.as_str()) {
        if version == "2.0" {
            checks.push(Check {
                name: "Format Version".to_string(),
                passed: true,
                message: "2.0 (current)".to_string(),
                details: None,
                severity: CheckSeverity::Info,
            });
        } else {
            warnings.push(format!("Unknown version: {}", version));
            checks.push(Check {
                name: "Format Version".to_string(),
                passed: false,
                message: format!("Unknown version: {}", version),
                details: Some("Expected version 2.0".to_string()),
                severity: CheckSeverity::Warning,
            });
        }
    }

    // Check adapter_id if present
    if let Some(adapter_id) = manifest.get("adapter_id").and_then(|v| v.as_str()) {
        if adapter_id.is_empty() {
            warnings.push("Empty adapter_id".to_string());
            checks.push(Check {
                name: "Adapter ID".to_string(),
                passed: false,
                message: "Empty adapter_id".to_string(),
                details: None,
                severity: CheckSeverity::Warning,
            });
        } else {
            checks.push(Check {
                name: "Adapter ID".to_string(),
                passed: true,
                message: adapter_id.to_string(),
                details: None,
                severity: CheckSeverity::Info,
            });
        }
    }

    Ok(())
}

fn check_semantic_naming(
    adapter_id: &str,
    checks: &mut Vec<Check>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    match AdapterName::parse(adapter_id) {
        Ok(name) => {
            checks.push(Check {
                name: "Semantic Naming".to_string(),
                passed: true,
                message: format!(
                    "{}/{}/{}/{}",
                    name.tenant(),
                    name.domain(),
                    name.purpose(),
                    name.revision()
                ),
                details: Some(format!(
                    "Tenant: {}, Domain: {}, Purpose: {}, Revision: {}",
                    name.tenant(),
                    name.domain(),
                    name.purpose(),
                    name.revision_number().unwrap_or(0)
                )),
                severity: CheckSeverity::Info,
            });
        }
        Err(e) => {
            warnings.push(format!("Invalid semantic name: {}", e));
            checks.push(Check {
                name: "Semantic Naming".to_string(),
                passed: false,
                message: "Invalid naming convention".to_string(),
                details: Some(format!(
                    "Expected: {{tenant}}/{{domain}}/{{purpose}}/{{revision}}. Error: {}",
                    e
                )),
                severity: CheckSeverity::Warning,
            });
        }
    }

    Ok(())
}

fn check_rank_alpha(
    manifest: &Value,
    checks: &mut Vec<Check>,
    errors: &mut Vec<String>,
) -> Result<()> {
    // Check rank
    if let Some(rank) = manifest.get("rank").and_then(|v| v.as_u64()) {
        if rank == 0 || rank > 256 {
            errors.push(format!("Invalid rank value: {}", rank));
            checks.push(Check {
                name: "LoRA Rank".to_string(),
                passed: false,
                message: format!("Invalid rank: {}", rank),
                details: Some("Rank must be between 1 and 256".to_string()),
                severity: CheckSeverity::Critical,
            });
        } else {
            checks.push(Check {
                name: "LoRA Rank".to_string(),
                passed: true,
                message: format!("{}", rank),
                details: None,
                severity: CheckSeverity::Info,
            });
        }
    }

    // Check alpha
    if let Some(alpha) = manifest.get("alpha").and_then(|v| v.as_u64()) {
        if alpha == 0 || alpha > 512 {
            checks.push(Check {
                name: "LoRA Alpha".to_string(),
                passed: false,
                message: format!("Unusual alpha value: {}", alpha),
                details: Some("Alpha typically between 1 and 512".to_string()),
                severity: CheckSeverity::Warning,
            });
        } else {
            checks.push(Check {
                name: "LoRA Alpha".to_string(),
                passed: true,
                message: format!("{}", alpha),
                details: None,
                severity: CheckSeverity::Info,
            });
        }
    }

    Ok(())
}

fn check_target_modules(
    manifest: &Value,
    checks: &mut Vec<Check>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    if let Some(targets) = manifest.get("target_modules") {
        if let Some(targets_array) = targets.as_array() {
            if targets_array.is_empty() {
                warnings.push("No target modules specified".to_string());
                checks.push(Check {
                    name: "Target Modules".to_string(),
                    passed: false,
                    message: "No target modules specified".to_string(),
                    details: Some("At least one target module recommended".to_string()),
                    severity: CheckSeverity::Warning,
                });
            } else {
                let module_names: Vec<String> = targets_array
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect();

                checks.push(Check {
                    name: "Target Modules".to_string(),
                    passed: true,
                    message: format!("{} modules", module_names.len()),
                    details: Some(module_names.join(", ")),
                    severity: CheckSeverity::Info,
                });
            }
        }
    }

    Ok(())
}

fn check_blake3_hash(
    path: &PathBuf,
    hash_str: &str,
    checks: &mut Vec<Check>,
    errors: &mut Vec<String>,
) -> Result<()> {
    // Parse expected hash
    let expected_hash = match B3Hash::from_hex(hash_str) {
        Ok(hash) => hash,
        Err(e) => {
            errors.push(format!("Invalid BLAKE3 hash format: {}", e));
            checks.push(Check {
                name: "BLAKE3 Hash".to_string(),
                passed: false,
                message: "Invalid hash format".to_string(),
                details: Some(e.to_string()),
                severity: CheckSeverity::Critical,
            });
            return Ok(());
        }
    };

    // Read weights section
    let (manifest_offset, _) = AOS2Writer::read_header(path)?;
    let mut file =
        File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    file.seek(std::io::SeekFrom::Start(8))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

    let weights_len = manifest_offset - 8;
    let mut weights_data = vec![0u8; weights_len as usize];
    file.read_exact(&mut weights_data)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    // Compute actual hash
    let actual_hash = B3Hash::hash(&weights_data);

    if actual_hash == expected_hash {
        checks.push(Check {
            name: "BLAKE3 Hash".to_string(),
            passed: true,
            message: "Hash verified".to_string(),
            details: Some(format!("Hash: {}", expected_hash.to_short_hex())),
            severity: CheckSeverity::Info,
        });
    } else {
        errors.push("BLAKE3 hash mismatch".to_string());
        checks.push(Check {
            name: "BLAKE3 Hash".to_string(),
            passed: false,
            message: "Hash mismatch".to_string(),
            details: Some(format!(
                "Expected: {}, Got: {}",
                expected_hash.to_short_hex(),
                actual_hash.to_short_hex()
            )),
            severity: CheckSeverity::Critical,
        });
    }

    Ok(())
}

fn check_weights(path: &PathBuf, checks: &mut Vec<Check>) -> Result<()> {
    let (manifest_offset, _) = AOS2Writer::read_header(path)?;

    let mut file =
        File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    // Seek to weights section (after 8-byte header)
    file.seek(std::io::SeekFrom::Start(8))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

    // Read safetensors header length
    let mut header_len_bytes = [0u8; 8];
    file.read_exact(&mut header_len_bytes)
        .map_err(|e| AosError::Io(format!("Failed to read header length: {}", e)))?;

    let header_len = u64::from_le_bytes(header_len_bytes);

    if header_len == 0 {
        checks.push(Check {
            name: "Safetensors Format".to_string(),
            passed: false,
            message: "Empty safetensors header".to_string(),
            details: None,
            severity: CheckSeverity::Critical,
        });
        return Ok(());
    }

    let weights_size = manifest_offset as u64 - 8;
    if header_len > weights_size {
        checks.push(Check {
            name: "Safetensors Format".to_string(),
            passed: false,
            message: "Header length exceeds weights section".to_string(),
            details: Some(format!(
                "Header length {} > weights size {}",
                header_len, weights_size
            )),
            severity: CheckSeverity::Critical,
        });
        return Ok(());
    }

    // Read header JSON
    let mut header_bytes = vec![0u8; header_len as usize];
    file.read_exact(&mut header_bytes)
        .map_err(|e| AosError::Io(format!("Failed to read header: {}", e)))?;

    let header: Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| AosError::Io(format!("Invalid safetensors header: {}", e)))?;

    checks.push(Check {
        name: "Safetensors Format".to_string(),
        passed: true,
        message: "Valid safetensors header".to_string(),
        details: None,
        severity: CheckSeverity::Info,
    });

    // Count and validate tensors
    if let Some(obj) = header.as_object() {
        let tensor_count = obj.iter().filter(|(k, _)| *k != "__metadata__").count();

        if tensor_count == 0 {
            checks.push(Check {
                name: "Tensor Count".to_string(),
                passed: false,
                message: "No tensors found".to_string(),
                details: None,
                severity: CheckSeverity::Critical,
            });
        } else {
            checks.push(Check {
                name: "Tensor Count".to_string(),
                passed: true,
                message: format!("{} tensors", tensor_count),
                details: None,
                severity: CheckSeverity::Info,
            });

            // Validate tensor metadata
            let mut invalid_tensors = Vec::new();
            for (name, info) in obj {
                if name == "__metadata__" {
                    continue;
                }

                if let Some(tensor_obj) = info.as_object() {
                    let has_dtype = tensor_obj.contains_key("dtype");
                    let has_shape = tensor_obj.contains_key("shape");
                    let has_offsets = tensor_obj.contains_key("data_offsets");

                    if !has_dtype || !has_shape || !has_offsets {
                        invalid_tensors.push(name.clone());
                    }
                }
            }

            if !invalid_tensors.is_empty() {
                checks.push(Check {
                    name: "Tensor Metadata".to_string(),
                    passed: false,
                    message: format!("{} invalid tensors", invalid_tensors.len()),
                    details: Some(format!("Invalid tensors: {}", invalid_tensors.join(", "))),
                    severity: CheckSeverity::Critical,
                });
            } else {
                checks.push(Check {
                    name: "Tensor Metadata".to_string(),
                    passed: true,
                    message: "All tensors have valid metadata".to_string(),
                    details: None,
                    severity: CheckSeverity::Info,
                });
            }
        }
    }

    Ok(())
}

fn check_file_integrity(
    path: &PathBuf,
    manifest_offset: u32,
    manifest_len: u32,
    checks: &mut Vec<Check>,
    errors: &mut Vec<String>,
) -> Result<()> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?;
    let file_size = metadata.len();

    let expected_size = manifest_offset as u64 + manifest_len as u64;

    if file_size == expected_size {
        checks.push(Check {
            name: "File Integrity".to_string(),
            passed: true,
            message: "File structure is consistent".to_string(),
            details: Some(format!(
                "Header(8) + Weights({}) + Manifest({}) = {} bytes",
                manifest_offset - 8,
                manifest_len,
                file_size
            )),
            severity: CheckSeverity::Info,
        });
    } else {
        errors.push("File size mismatch".to_string());
        checks.push(Check {
            name: "File Integrity".to_string(),
            passed: false,
            message: "File size mismatch".to_string(),
            details: Some(format!(
                "Expected {} bytes, found {} bytes ({} bytes difference)",
                expected_size,
                file_size,
                (file_size as i64 - expected_size as i64).abs()
            )),
            severity: CheckSeverity::Critical,
        });
    }

    Ok(())
}

fn print_json(result: &ValidationResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    println!("{}", json);
    Ok(())
}

fn print_human_readable(result: &ValidationResult, cli: &Cli) -> Result<()> {
    println!("\n{}", colorize("AOS File Validation", Color::Cyan, true));
    println!("{}", "=".repeat(70));
    println!("File: {}\n", result.file_path);

    // Overall status
    if result.valid {
        println!("{}", colorize("✓ VALIDATION PASSED", Color::Green, true));
    } else {
        println!("{}", colorize("✗ VALIDATION FAILED", Color::Red, true));
    }

    // Summary
    println!("\n{}", colorize("Summary", Color::Cyan, true));
    println!("{}", "-".repeat(70));
    println!("  Total checks: {}", result.summary.total_checks);
    println!(
        "  {} {}",
        colorize(&format!("✓ Passed:"), Color::Green, false),
        result.summary.passed
    );
    if result.summary.failed > 0 {
        println!(
            "  {} {}",
            colorize(&format!("✗ Failed:"), Color::Red, false),
            result.summary.failed
        );
    }
    if result.summary.warnings > 0 {
        println!(
            "  {} {}",
            colorize(&format!("⚠ Warnings:"), Color::Yellow, false),
            result.summary.warnings
        );
    }
    println!(
        "  File size: {}",
        format_file_size(result.summary.file_size_bytes)
    );

    // Checks table
    println!("\n{}", colorize("Validation Checks", Color::Cyan, true));
    println!("{}", "-".repeat(70));

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Check").fg(Color::Green),
            Cell::new("Status").fg(Color::Yellow),
            Cell::new("Result").fg(Color::Yellow),
        ]);

    for check in &result.checks {
        // Skip passed checks in non-verbose mode unless they're important
        if !cli.verbose && check.passed && !is_important_check(&check.name) {
            continue;
        }

        let status = match check.severity {
            CheckSeverity::Critical if !check.passed => Cell::new("✗").fg(Color::Red),
            CheckSeverity::Warning if !check.passed => Cell::new("⚠").fg(Color::Yellow),
            _ if check.passed => Cell::new("✓").fg(Color::Green),
            _ => Cell::new("✗").fg(Color::Red),
        };

        let mut message = check.message.clone();
        if let Some(details) = &check.details {
            message = format!("{}\n  {}", message, details);
        }

        table.add_row(vec![Cell::new(&check.name), status, Cell::new(message)]);
    }

    println!("\n{}", table);

    // Errors
    if !result.errors.is_empty() {
        println!("\n{}", colorize("Errors", Color::Red, true));
        println!("{}", "-".repeat(70));
        for error in &result.errors {
            println!("  ✗ {}", error);
        }
    }

    // Warnings
    if !result.warnings.is_empty() {
        println!("\n{}", colorize("Warnings", Color::Yellow, true));
        println!("{}", "-".repeat(70));
        for warning in &result.warnings {
            println!("  ⚠ {}", warning);
        }
    }

    if !cli.verbose && result.summary.passed > 5 {
        println!(
            "\n  {}",
            colorize("Use -v/--verbose to see all checks", Color::DarkGrey, false)
        );
    }

    println!();

    Ok(())
}

fn is_important_check(name: &str) -> bool {
    matches!(
        name,
        "File Size"
            | "Header Format"
            | "Manifest JSON"
            | "Format Version"
            | "BLAKE3 Hash"
            | "File Integrity"
    )
}

fn colorize(text: &str, color: Color, bold: bool) -> String {
    if bold {
        format!("\x1b[1m\x1b[{}m{}\x1b[0m", color_code(color), text)
    } else {
        format!("\x1b[{}m{}\x1b[0m", color_code(color), text)
    }
}

fn color_code(color: Color) -> u8 {
    match color {
        Color::Cyan => 36,
        Color::Green => 32,
        Color::Yellow => 33,
        Color::Red => 31,
        Color::DarkGrey => 90,
        _ => 37,
    }
}
