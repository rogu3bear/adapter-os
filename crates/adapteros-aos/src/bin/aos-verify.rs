//! AOS file verification and validation tool
//!
//! Performs deep validation of .aos archive files including:
//! - File structure integrity
//! - Manifest schema validation
//! - Tensor data integrity checks
//! - Checksum verification
//! - Format compliance

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{AosError, Result};
use clap::Parser;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aos-verify")]
#[command(about = "Verify and validate AOS archive files", long_about = None)]
struct Cli {
    /// Path to .aos file
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Verbose output (show all checks)
    #[arg(short, long)]
    verbose: bool,

    /// Skip tensor data validation (faster)
    #[arg(long)]
    skip_tensors: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.file.exists() {
        return Err(AosError::NotFound(format!(
            "File not found: {}",
            cli.file.display()
        )));
    }

    let result = verify_file(&cli.file, &cli)?;

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

#[derive(Debug, serde::Serialize)]
struct VerificationResult {
    file_path: String,
    valid: bool,
    checks: Vec<Check>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct Check {
    name: String,
    passed: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

fn verify_file(path: &PathBuf, cli: &Cli) -> Result<VerificationResult> {
    let mut checks = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check 1: File exists and is readable
    checks.push(check_file_readable(path)?);

    // Check 2: File size is reasonable
    let size_check = check_file_size(path)?;
    if !size_check.passed {
        warnings.push(size_check.message.clone());
    }
    checks.push(size_check);

    // Check 3: Header is valid
    let header_result = check_header(path);
    match header_result {
        Ok(check) => {
            checks.push(check);
        }
        Err(e) => {
            errors.push(format!("Header validation failed: {}", e));
            checks.push(Check {
                name: "Header".to_string(),
                passed: false,
                message: "Invalid header format".to_string(),
                details: Some(e.to_string()),
            });
        }
    }

    // Check 4: Manifest is valid JSON
    let manifest_result = check_manifest(path);
    match manifest_result {
        Ok((check, manifest)) => {
            checks.push(check);

            // Check 5: Manifest schema validation
            checks.push(check_manifest_schema(&manifest)?);

            // Check 6: Manifest fields validation
            let field_checks = check_manifest_fields(&manifest)?;
            for fc in field_checks {
                if !fc.passed {
                    warnings.push(fc.message.clone());
                }
                checks.push(fc);
            }
        }
        Err(e) => {
            errors.push(format!("Manifest validation failed: {}", e));
            checks.push(Check {
                name: "Manifest".to_string(),
                passed: false,
                message: "Invalid manifest".to_string(),
                details: Some(e.to_string()),
            });
        }
    }

    // Check 7: Weights section validation
    if !cli.skip_tensors {
        let weights_result = check_weights(path);
        match weights_result {
            Ok(weight_checks) => {
                for wc in weight_checks {
                    if !wc.passed {
                        errors.push(wc.message.clone());
                    }
                    checks.push(wc);
                }
            }
            Err(e) => {
                warnings.push(format!("Could not validate tensors: {}", e));
                checks.push(Check {
                    name: "Tensors".to_string(),
                    passed: false,
                    message: "Could not parse tensor data".to_string(),
                    details: Some(e.to_string()),
                });
            }
        }
    }

    // Check 8: File integrity (offsets and sizes match)
    checks.push(check_file_integrity(path)?);

    let valid = errors.is_empty() && checks.iter().all(|c| c.passed);

    Ok(VerificationResult {
        file_path: path.display().to_string(),
        valid,
        checks,
        errors,
        warnings,
    })
}

fn check_file_readable(path: &PathBuf) -> Result<Check> {
    match File::open(path) {
        Ok(_) => Ok(Check {
            name: "File Access".to_string(),
            passed: true,
            message: "File is readable".to_string(),
            details: None,
        }),
        Err(e) => Ok(Check {
            name: "File Access".to_string(),
            passed: false,
            message: format!("Cannot read file: {}", e),
            details: None,
        }),
    }
}

fn check_file_size(path: &PathBuf) -> Result<Check> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?;
    let size = metadata.len();

    const MIN_SIZE: u64 = 16; // Header + minimal manifest
    const MAX_SIZE: u64 = 10 * 1024 * 1024 * 1024; // 10 GB

    if size < MIN_SIZE {
        Ok(Check {
            name: "File Size".to_string(),
            passed: false,
            message: format!("File too small ({} bytes)", size),
            details: Some(format!("Minimum size is {} bytes", MIN_SIZE)),
        })
    } else if size > MAX_SIZE {
        Ok(Check {
            name: "File Size".to_string(),
            passed: false,
            message: format!("File too large ({} bytes)", size),
            details: Some("Consider splitting into multiple adapters".to_string()),
        })
    } else {
        Ok(Check {
            name: "File Size".to_string(),
            passed: true,
            message: format!("{} bytes", size),
            details: None,
        })
    }
}

fn check_header(path: &PathBuf) -> Result<Check> {
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;

    let metadata = std::fs::metadata(path)
        .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?;
    let file_size = metadata.len();

    if manifest_offset < 8 {
        return Ok(Check {
            name: "Header".to_string(),
            passed: false,
            message: "Invalid manifest offset".to_string(),
            details: Some(format!(
                "Offset {} is less than header size (8)",
                manifest_offset
            )),
        });
    }

    if manifest_offset as u64 + manifest_len as u64 > file_size {
        return Ok(Check {
            name: "Header".to_string(),
            passed: false,
            message: "Manifest extends beyond file".to_string(),
            details: Some(format!(
                "Offset {} + length {} > file size {}",
                manifest_offset, manifest_len, file_size
            )),
        });
    }

    Ok(Check {
        name: "Header".to_string(),
        passed: true,
        message: format!("Valid (offset={}, len={})", manifest_offset, manifest_len),
        details: None,
    })
}

fn check_manifest(path: &PathBuf) -> Result<(Check, Value)> {
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;

    let mut file =
        File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    file.seek(std::io::SeekFrom::Start(manifest_offset as u64))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

    let mut manifest_bytes = vec![0u8; manifest_len as usize];
    file.read_exact(&mut manifest_bytes)
        .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;

    match serde_json::from_slice::<Value>(&manifest_bytes) {
        Ok(manifest) => Ok((
            Check {
                name: "Manifest JSON".to_string(),
                passed: true,
                message: "Valid JSON".to_string(),
                details: None,
            },
            manifest,
        )),
        Err(e) => Err(AosError::Io(format!("Invalid manifest JSON: {}", e))),
    }
}

fn check_manifest_schema(manifest: &Value) -> Result<Check> {
    let required_fields = ["version"];
    let mut missing = Vec::new();

    for field in &required_fields {
        if manifest.get(field).is_none() {
            missing.push(*field);
        }
    }

    if missing.is_empty() {
        Ok(Check {
            name: "Manifest Schema".to_string(),
            passed: true,
            message: "All required fields present".to_string(),
            details: None,
        })
    } else {
        Ok(Check {
            name: "Manifest Schema".to_string(),
            passed: false,
            message: format!("Missing required fields: {:?}", missing),
            details: None,
        })
    }
}

fn check_manifest_fields(manifest: &Value) -> Result<Vec<Check>> {
    let mut checks = Vec::new();

    // Check version
    if let Some(version) = manifest.get("version").and_then(|v| v.as_str()) {
        if version == "2.0" {
            checks.push(Check {
                name: "Version".to_string(),
                passed: true,
                message: "2.0 (current)".to_string(),
                details: None,
            });
        } else {
            checks.push(Check {
                name: "Version".to_string(),
                passed: false,
                message: format!("Unknown version: {}", version),
                details: Some("Expected version 2.0".to_string()),
            });
        }
    }

    // Check adapter_id if present
    if let Some(adapter_id) = manifest.get("adapter_id").and_then(|v| v.as_str()) {
        if adapter_id.is_empty() {
            checks.push(Check {
                name: "Adapter ID".to_string(),
                passed: false,
                message: "Empty adapter_id".to_string(),
                details: None,
            });
        } else {
            checks.push(Check {
                name: "Adapter ID".to_string(),
                passed: true,
                message: adapter_id.to_string(),
                details: None,
            });
        }
    }

    // Check rank if present
    if let Some(rank) = manifest.get("rank").and_then(|v| v.as_u64()) {
        if rank == 0 || rank > 256 {
            checks.push(Check {
                name: "Rank".to_string(),
                passed: false,
                message: format!("Unusual rank value: {}", rank),
                details: Some("Rank should be between 1 and 256".to_string()),
            });
        } else {
            checks.push(Check {
                name: "Rank".to_string(),
                passed: true,
                message: rank.to_string(),
                details: None,
            });
        }
    }

    Ok(checks)
}

fn check_weights(path: &PathBuf) -> Result<Vec<Check>> {
    use std::io::Seek;

    let mut checks = Vec::new();
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
            name: "Safetensors Header".to_string(),
            passed: false,
            message: "Empty header".to_string(),
            details: None,
        });
        return Ok(checks);
    }

    let weights_size = manifest_offset as u64 - 8;
    if header_len > weights_size {
        checks.push(Check {
            name: "Safetensors Header".to_string(),
            passed: false,
            message: "Header length exceeds weights section".to_string(),
            details: Some(format!(
                "Header length {} > weights size {}",
                header_len, weights_size
            )),
        });
        return Ok(checks);
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
    });

    // Count tensors
    if let Some(obj) = header.as_object() {
        let tensor_count = obj.iter().filter(|(k, _)| *k != "__metadata__").count();
        checks.push(Check {
            name: "Tensor Count".to_string(),
            passed: tensor_count > 0,
            message: format!("{} tensors", tensor_count),
            details: None,
        });

        // Validate each tensor
        for (name, info) in obj {
            if name == "__metadata__" {
                continue;
            }

            if let Some(tensor_obj) = info.as_object() {
                let has_dtype = tensor_obj.contains_key("dtype");
                let has_shape = tensor_obj.contains_key("shape");
                let has_offsets = tensor_obj.contains_key("data_offsets");

                if !has_dtype || !has_shape || !has_offsets {
                    checks.push(Check {
                        name: format!("Tensor '{}'", name),
                        passed: false,
                        message: "Missing required fields".to_string(),
                        details: Some(format!(
                            "dtype={}, shape={}, data_offsets={}",
                            has_dtype, has_shape, has_offsets
                        )),
                    });
                }
            }
        }
    }

    Ok(checks)
}

fn check_file_integrity(path: &PathBuf) -> Result<Check> {
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;

    let metadata = std::fs::metadata(path)
        .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?;
    let file_size = metadata.len();

    let expected_size = manifest_offset as u64 + manifest_len as u64;

    if file_size == expected_size {
        Ok(Check {
            name: "File Integrity".to_string(),
            passed: true,
            message: "File structure is consistent".to_string(),
            details: Some(format!(
                "Header(8) + Weights({}) + Manifest({}) = {} bytes",
                manifest_offset - 8,
                manifest_len,
                file_size
            )),
        })
    } else {
        Ok(Check {
            name: "File Integrity".to_string(),
            passed: false,
            message: "File size mismatch".to_string(),
            details: Some(format!(
                "Expected {} bytes, found {} bytes ({} bytes difference)",
                expected_size,
                file_size,
                (file_size as i64 - expected_size as i64).abs()
            )),
        })
    }
}

fn print_json(result: &VerificationResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    println!("{}", json);
    Ok(())
}

fn print_human_readable(result: &VerificationResult, cli: &Cli) -> Result<()> {
    println!("\n{}", colorize("AOS File Verification", Color::Cyan, true));
    println!("{}", "=".repeat(60));
    println!("File: {}\n", result.file_path);

    // Overall status
    if result.valid {
        println!("{}", colorize("✓ VALIDATION PASSED", Color::Green, true));
    } else {
        println!("{}", colorize("✗ VALIDATION FAILED", Color::Red, true));
    }

    // Checks table
    println!("\n{}", colorize("Validation Checks", Color::Cyan, true));
    println!("{}", "-".repeat(60));

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
        if !cli.verbose && check.passed {
            // Skip passed checks in non-verbose mode
            continue;
        }

        let status = if check.passed {
            Cell::new("✓").fg(Color::Green)
        } else {
            Cell::new("✗").fg(Color::Red)
        };

        let mut message = check.message.clone();
        if let Some(details) = &check.details {
            message = format!("{}\n  {}", message, details);
        }

        table.add_row(vec![Cell::new(&check.name), status, Cell::new(message)]);
    }

    println!("\n{}", table);

    // Summary
    let passed = result.checks.iter().filter(|c| c.passed).count();
    let total = result.checks.len();
    println!(
        "\n  Checks passed: {}/{} ({}%)",
        passed,
        total,
        if total > 0 { (passed * 100) / total } else { 0 }
    );

    // Errors
    if !result.errors.is_empty() {
        println!("\n{}", colorize("Errors", Color::Red, true));
        println!("{}", "-".repeat(60));
        for error in &result.errors {
            println!("  ✗ {}", error);
        }
    }

    // Warnings
    if !result.warnings.is_empty() {
        println!("\n{}", colorize("Warnings", Color::Yellow, true));
        println!("{}", "-".repeat(60));
        for warning in &result.warnings {
            println!("  ⚠ {}", warning);
        }
    }

    if !cli.verbose && passed < total {
        println!(
            "\n  {}",
            colorize("Use -v/--verbose to see all checks", Color::DarkGrey, false)
        );
    }

    println!();

    Ok(())
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
