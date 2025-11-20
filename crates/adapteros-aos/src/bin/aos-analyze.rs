//! AOS v2 File Format Analyzer
//!
//! Comprehensive analysis tool for .aos adapter files.
//! Supports both v2.0 format (JSON weights) and production safetensors format.
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use adapteros_core::{AosError, Result};
use clap::Parser;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aos-analyze")]
#[command(about = "Analyze .aos file format and structure", long_about = None)]
struct Cli {
    /// Path to .aos file
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Output analysis as JSON
    #[arg(long)]
    json: bool,
}

/// AOS v2 file header
#[derive(Debug, Clone, Serialize)]
struct AosHeader {
    manifest_offset: u32,
    manifest_len: u32,
}

/// Detected weights format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum WeightsFormat {
    Json,
    Safetensors,
    Unknown,
}

impl std::fmt::Display for WeightsFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WeightsFormat::Json => write!(f, "JSON"),
            WeightsFormat::Safetensors => write!(f, "SafeTensors"),
            WeightsFormat::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Tensor metadata information
#[derive(Debug, Clone, Serialize)]
struct TensorInfo {
    name: String,
    dtype: String,
    shape: Vec<usize>,
    num_params: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_offsets: Option<[usize; 2]>,
}

/// Weights analysis results
#[derive(Debug, Clone, Serialize)]
struct WeightsAnalysis {
    format: WeightsFormat,
    tensors: Vec<TensorInfo>,
    total_params: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    safetensors_header_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    safetensors_data_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Complete analysis report
#[derive(Debug, Clone, Serialize)]
struct AnalysisReport {
    file_path: String,
    file_size: usize,
    header: AosHeader,
    weights: Option<WeightsAnalysis>,
    manifest: serde_json::Value,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.file.exists() {
        return Err(AosError::NotFound(format!(
            "File not found: {}",
            cli.file.display()
        )));
    }

    let report = analyze_aos_file(&cli.file)?;

    if cli.json {
        print_json(&report)?;
    } else {
        print_human_readable(&report)?;
    }

    // Exit with error if validation failed
    if !report.errors.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

/// Read and analyze the entire .aos file
fn analyze_aos_file(path: &PathBuf) -> Result<AnalysisReport> {
    // Read entire file
    let mut file =
        File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    let file_size = data.len();

    // Parse header
    let header = read_header(&data)?;

    // Detect weights format
    let weights_format = detect_weights_format(&data, 8);

    // Analyze weights based on format
    let weights_analysis = analyze_weights(&data, &header, weights_format)?;

    // Parse manifest
    let manifest = parse_manifest(&data, &header)?;

    // Validation
    let (errors, warnings) = validate(&data, &header, &weights_analysis, &manifest);

    Ok(AnalysisReport {
        file_path: path.display().to_string(),
        file_size,
        header,
        weights: weights_analysis,
        manifest,
        errors,
        warnings,
    })
}

/// Read the 8-byte header
fn read_header(data: &[u8]) -> Result<AosHeader> {
    if data.len() < 8 {
        return Err(AosError::Validation(format!(
            "File too small: {} bytes (expected at least 8)",
            data.len()
        )));
    }

    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

    Ok(AosHeader {
        manifest_offset,
        manifest_len,
    })
}

/// Detect the format of the weights section
fn detect_weights_format(data: &[u8], offset: usize) -> WeightsFormat {
    if data.len() <= offset {
        return WeightsFormat::Unknown;
    }

    let sample_len = std::cmp::min(100, data.len() - offset);
    let sample = &data[offset..offset + sample_len];

    // Check for JSON (starts with '{' or '[')
    if let Ok(sample_str) = std::str::from_utf8(sample) {
        let trimmed = sample_str.trim();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            // Look for LoRA-specific JSON patterns
            if trimmed.contains("lora_a_q15") || trimmed.contains("lora_b_q15") {
                return WeightsFormat::Json;
            }
            // Try to parse as JSON
            if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                return WeightsFormat::Json;
            }
        }
    }

    // Check for safetensors (starts with u64 header size)
    if sample.len() >= 8 {
        if let Ok(header_size_bytes) = sample[..8].try_into() {
            let header_size = u64::from_le_bytes(header_size_bytes);
            // Safetensors header is typically reasonable (< 1MB)
            if header_size > 0 && header_size < 1_000_000 {
                return WeightsFormat::Safetensors;
            }
        }
    }

    WeightsFormat::Unknown
}

/// Analyze weights section based on detected format
fn analyze_weights(
    data: &[u8],
    header: &AosHeader,
    format: WeightsFormat,
) -> Result<Option<WeightsAnalysis>> {
    match format {
        WeightsFormat::Json => analyze_json_weights(data, 8, header.manifest_offset as usize),
        WeightsFormat::Safetensors => {
            analyze_safetensors_weights(data, 8, header.manifest_offset as usize)
        }
        WeightsFormat::Unknown => Ok(None),
    }
}

/// Analyze JSON weights format
fn analyze_json_weights(
    data: &[u8],
    offset: usize,
    manifest_offset: usize,
) -> Result<Option<WeightsAnalysis>> {
    let weights_bytes = &data[offset..manifest_offset];

    let weights: serde_json::Value =
        serde_json::from_slice(weights_bytes).map_err(AosError::Serialization)?;

    let mut tensors = Vec::new();
    let mut total_params = 0;

    if let Some(obj) = weights.as_object() {
        for (key, value) in obj {
            if let Some(arr) = value.as_array() {
                // Calculate shape and params
                let (shape, num_params) = if !arr.is_empty() {
                    if let Some(first_row) = arr[0].as_array() {
                        // 2D array
                        let rows = arr.len();
                        let cols = first_row.len();
                        (vec![rows, cols], rows * cols)
                    } else {
                        // 1D array
                        let len = arr.len();
                        (vec![len], len)
                    }
                } else {
                    (vec![0], 0)
                };

                total_params += num_params;

                tensors.push(TensorInfo {
                    name: key.clone(),
                    dtype: if key.to_lowercase().contains("q15") {
                        "Q15".to_string()
                    } else {
                        "unknown".to_string()
                    },
                    shape,
                    num_params,
                    size_bytes: None,
                    data_offsets: None,
                });
            }
        }
    }

    Ok(Some(WeightsAnalysis {
        format: WeightsFormat::Json,
        tensors,
        total_params,
        safetensors_header_size: None,
        safetensors_data_start: None,
        metadata: None,
    }))
}

/// Analyze safetensors weights format
fn analyze_safetensors_weights(
    data: &[u8],
    offset: usize,
    _manifest_offset: usize,
) -> Result<Option<WeightsAnalysis>> {
    if data.len() < offset + 8 {
        return Ok(None);
    }

    // Read header size (u64 LE)
    let header_size_bytes: [u8; 8] = data[offset..offset + 8]
        .try_into()
        .map_err(|_| AosError::Validation("Failed to read safetensors header size".to_string()))?;
    let header_size = u64::from_le_bytes(header_size_bytes) as usize;

    // Read JSON metadata
    let metadata_start = offset + 8;
    let metadata_end = metadata_start + header_size;

    if data.len() < metadata_end {
        return Err(AosError::Validation(format!(
            "Not enough data for safetensors metadata (need {}, have {})",
            metadata_end,
            data.len()
        )));
    }

    let metadata_bytes = &data[metadata_start..metadata_end];
    let metadata: serde_json::Value =
        serde_json::from_slice(metadata_bytes).map_err(AosError::Serialization)?;

    // Data starts after metadata
    let data_start = metadata_end;

    let mut tensors = Vec::new();
    let mut total_params = 0;
    let mut st_metadata = None;

    if let Some(obj) = metadata.as_object() {
        for (name, info) in obj {
            if name == "__metadata__" {
                st_metadata = Some(
                    info.as_object()
                        .unwrap_or(&serde_json::Map::new())
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );
                continue;
            }

            if let Some(tensor_obj) = info.as_object() {
                let dtype = tensor_obj
                    .get("dtype")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let shape: Vec<usize> = tensor_obj
                    .get("shape")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_u64().map(|n| n as usize))
                            .collect()
                    })
                    .unwrap_or_default();

                let num_elements: usize = shape.iter().product();
                total_params += num_elements;

                let data_offsets = tensor_obj
                    .get("data_offsets")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| {
                        if arr.len() == 2 {
                            Some([
                                arr[0].as_u64().unwrap_or(0) as usize,
                                arr[1].as_u64().unwrap_or(0) as usize,
                            ])
                        } else {
                            None
                        }
                    });

                let size_bytes = data_offsets.map(|[start, end]| end - start);

                tensors.push(TensorInfo {
                    name: name.clone(),
                    dtype,
                    shape,
                    num_params: num_elements,
                    size_bytes,
                    data_offsets,
                });
            }
        }
    }

    // Sort tensors by name for consistent output
    tensors.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Some(WeightsAnalysis {
        format: WeightsFormat::Safetensors,
        tensors,
        total_params,
        safetensors_header_size: Some(header_size),
        safetensors_data_start: Some(data_start),
        metadata: st_metadata,
    }))
}

/// Parse the manifest JSON
fn parse_manifest(data: &[u8], header: &AosHeader) -> Result<serde_json::Value> {
    let offset = header.manifest_offset as usize;
    let len = header.manifest_len as usize;

    if data.len() < offset + len {
        return Err(AosError::Validation(format!(
            "Not enough data for manifest (need {}, have {})",
            offset + len,
            data.len()
        )));
    }

    let manifest_bytes = &data[offset..offset + len];
    serde_json::from_slice(manifest_bytes).map_err(AosError::Serialization)
}

/// Validate the file structure
fn validate(
    data: &[u8],
    header: &AosHeader,
    weights: &Option<WeightsAnalysis>,
    manifest: &serde_json::Value,
) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check header consistency
    let expected_size = header.manifest_offset as usize + header.manifest_len as usize;
    if expected_size != data.len() {
        errors.push(format!(
            "File size mismatch: header says {}, actual {}",
            expected_size,
            data.len()
        ));
    }

    // Check manifest offset alignment
    if header.manifest_offset < 8 {
        errors.push(format!(
            "Invalid manifest offset: {} (must be >= 8)",
            header.manifest_offset
        ));
    }

    // Check weights
    if let Some(w) = weights {
        if w.total_params == 0 {
            warnings.push("No parameters found in weights".to_string());
        }
    } else {
        warnings.push("Could not parse weights".to_string());
    }

    // Validate manifest fields
    if manifest.get("version").is_none() {
        warnings.push("Missing 'version' field in manifest".to_string());
    }

    (errors, warnings)
}

/// Print analysis as JSON
fn print_json(report: &AnalysisReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).map_err(AosError::Serialization)?;
    println!("{}", json);
    Ok(())
}

/// Print human-readable analysis
fn print_human_readable(report: &AnalysisReport) -> Result<()> {
    println!();
    println!("{}", "=".repeat(80));
    println!("Analyzing: {}", report.file_path);
    println!("{}", "=".repeat(80));
    println!();

    // File size
    println!(
        "File size: {} ({} bytes)",
        format_size(report.file_size as u64),
        report.file_size
    );
    println!();

    // Header analysis
    println!("{}", "=".repeat(80));
    println!("HEADER ANALYSIS (First 8 bytes)");
    println!("{}", "=".repeat(80));
    println!(
        "Manifest offset: {:8} bytes (0x{:08x})",
        report.header.manifest_offset, report.header.manifest_offset
    );
    println!(
        "Manifest length: {:8} bytes (0x{:08x})",
        report.header.manifest_len, report.header.manifest_len
    );
    println!("Weights start:   {:8} bytes (0x{:08x})", 8, 8);
    let weights_size = report.header.manifest_offset - 8;
    println!("Weights size:    {:8} bytes", weights_size);
    println!();

    // Weights analysis
    if let Some(weights) = &report.weights {
        println!("{}", "=".repeat(80));
        println!("WEIGHTS ANALYSIS");
        println!("{}", "=".repeat(80));
        println!();
        println!(
            "Format: {} ({})",
            weights.format,
            match weights.format {
                WeightsFormat::Json => "test/development format",
                WeightsFormat::Safetensors => "production format",
                WeightsFormat::Unknown => "unknown format",
            }
        );

        if weights.format == WeightsFormat::Safetensors {
            if let Some(header_size) = weights.safetensors_header_size {
                println!("Safetensors header size: {} bytes", header_size);
            }
            if let Some(data_start) = weights.safetensors_data_start {
                println!(
                    "Safetensors data start:  {} bytes (0x{:08x})",
                    data_start, data_start
                );
                let data_size = report.header.manifest_offset as usize - data_start;
                println!("Safetensors data size:   {} bytes", data_size);
            }
        }

        println!();
        println!("Tensor count: {}", weights.tensors.len());
        println!();
        println!("Tensors:");

        for tensor in &weights.tensors {
            let shape_str = tensor
                .shape
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join("x");
            let size_str = if let Some(size) = tensor.size_bytes {
                format!(" {:>12}", format_size(size as u64))
            } else {
                String::new()
            };
            println!(
                "  {:40} {:10} [{:20}]{} ({} params)",
                tensor.name, tensor.dtype, shape_str, size_str, tensor.num_params
            );
        }

        println!();
        println!("Total parameters: {}", weights.total_params);

        // Check for metadata
        if let Some(metadata) = &weights.metadata {
            println!();
            println!("Safetensors metadata:");
            for (key, value) in metadata {
                println!("  {}: {}", key, value);
            }
        }
    } else {
        println!();
        println!("WARNING: Unknown weights format");
    }

    println!();

    // Manifest analysis
    println!("{}", "=".repeat(80));
    println!("MANIFEST ANALYSIS");
    println!("{}", "=".repeat(80));
    println!();

    if let Some(version) = report.manifest.get("version") {
        println!(
            "Manifest version: {}",
            version.as_str().unwrap_or("unknown")
        );
    }
    if let Some(adapter_id) = report.manifest.get("adapter_id") {
        println!("Adapter ID: {}", adapter_id.as_str().unwrap_or("N/A"));
    }

    if let Some(base_model) = report.manifest.get("base_model") {
        if let Some(obj) = base_model.as_object() {
            println!();
            println!("Base model:");
            if let Some(name) = obj.get("name") {
                println!("  Name: {}", name.as_str().unwrap_or("unknown"));
            }
            if let Some(hash) = obj.get("hash") {
                println!("  Hash: {}", hash.as_str().unwrap_or("unknown"));
            }
            if let Some(revision) = obj.get("revision") {
                println!("  Revision: {}", revision.as_str().unwrap_or("unknown"));
            }
        } else if let Some(s) = base_model.as_str() {
            println!();
            println!("Base model: {}", s);
        }
    }

    if let Some(lora_config) = report.manifest.get("lora_config") {
        if let Some(obj) = lora_config.as_object() {
            println!();
            println!("LoRA config:");
            if let Some(rank) = obj.get("rank") {
                println!("  Rank: {}", rank);
            }
            if let Some(alpha) = obj.get("alpha") {
                println!("  Alpha: {}", alpha);
            }
            if let Some(dropout) = obj.get("dropout") {
                println!("  Dropout: {}", dropout);
            }
            if let Some(target_modules) = obj.get("target_modules") {
                if let Some(arr) = target_modules.as_array() {
                    let modules: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    println!("  Target modules: {}", modules.join(", "));
                }
            }
        }
    }

    if let Some(training_config) = report.manifest.get("training_config") {
        if let Some(obj) = training_config.as_object() {
            println!();
            println!("Training config:");
            for (key, value) in obj {
                println!("  {}: {}", key, value);
            }
        }
    }

    if let Some(created_at) = report.manifest.get("created_at") {
        println!();
        println!("Created: {}", created_at.as_str().unwrap_or("unknown"));
    }

    if let Some(hash) = report
        .manifest
        .get("hash")
        .or_else(|| report.manifest.get("weights_hash"))
    {
        println!("Hash: {}", hash.as_str().unwrap_or("unknown"));
    }

    println!();
    println!("Full manifest JSON:");
    println!(
        "{}",
        serde_json::to_string_pretty(&report.manifest).unwrap_or_default()
    );

    println!();

    // Hex dump
    println!("{}", "=".repeat(80));
    println!("HEX DUMP (First 512 bytes)");
    println!("{}", "=".repeat(80));

    // Read file again for hex dump
    let mut file = File::open(&report.file_path)
        .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    println!("{}", hex_dump(&data, 0, 512));

    println!();

    // Structure summary
    println!("{}", "=".repeat(80));
    println!("STRUCTURE SUMMARY");
    println!("{}", "=".repeat(80));
    println!();

    let weights_format_str = if let Some(w) = &report.weights {
        format!("{}", w.format)
    } else {
        "unknown".to_string()
    };

    println!("Byte Range               Section                Size");
    println!("----------               -------                ----");
    println!(
        "0x{:08x} - 0x{:08x}   Header                 {:>12}",
        0,
        8,
        format_size(8)
    );
    println!(
        "0x{:08x} - 0x{:08x}   Weights ({:13}) {:>12}",
        8,
        report.header.manifest_offset,
        weights_format_str,
        format_size((report.header.manifest_offset - 8) as u64)
    );
    println!(
        "0x{:08x} - 0x{:08x}   Manifest (JSON)        {:>12}",
        report.header.manifest_offset,
        report.file_size,
        format_size(report.header.manifest_len as u64)
    );
    println!(
        "                         TOTAL                  {:>12}",
        format_size(report.file_size as u64)
    );

    println!();

    // Validation
    println!("{}", "=".repeat(80));
    println!("VALIDATION");
    println!("{}", "=".repeat(80));
    println!();

    if !report.errors.is_empty() {
        println!("ERRORS:");
        for err in &report.errors {
            println!("  x {}", err);
        }
        println!();
    }

    if !report.warnings.is_empty() {
        println!("WARNINGS:");
        for warn in &report.warnings {
            println!("  ! {}", warn);
        }
        println!();
    }

    if report.errors.is_empty() && report.warnings.is_empty() {
        println!("✓ File structure is valid");
        println!();
    }

    println!("{}", "=".repeat(80));
    println!();

    Ok(())
}

/// Format byte size in human-readable format
fn format_size(num_bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if num_bytes >= GB {
        format!("{:.2} GB", num_bytes as f64 / GB as f64)
    } else if num_bytes >= MB {
        format!("{:.2} MB", num_bytes as f64 / MB as f64)
    } else if num_bytes >= KB {
        format!("{:.2} KB", num_bytes as f64 / KB as f64)
    } else {
        format!("{} B", num_bytes)
    }
}

/// Generate a hex dump of binary data
fn hex_dump(data: &[u8], offset: usize, length: usize) -> String {
    const WIDTH: usize = 16;
    let mut lines = Vec::new();
    let end = std::cmp::min(offset + length, data.len());

    for i in (offset..end).step_by(WIDTH) {
        // Offset
        let mut line = format!("{:08x}  ", i);

        // Hex bytes
        let mut hex_part = String::new();
        let mut ascii_part = String::new();

        for j in 0..WIDTH {
            if i + j < end {
                let byte = data[i + j];
                hex_part.push_str(&format!("{:02x} ", byte));
                ascii_part.push(if (32..127).contains(&byte) {
                    byte as char
                } else {
                    '.'
                });
            } else {
                hex_part.push_str("   ");
                ascii_part.push(' ');
            }
        }

        // Add separator in middle
        let hex_with_separator = format!("{} {}", &hex_part[..24], &hex_part[24..]);

        line.push_str(&hex_with_separator);
        line.push_str(" |");
        line.push_str(&ascii_part);
        line.push('|');

        lines.push(line);
    }

    lines.join("\n")
}
