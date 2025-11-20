//! AOS file information and inspection tool
//!
//! Displays detailed information about .aos archive files including:
//! - File version and format
//! - Manifest contents
//! - Tensor list with shapes and sizes
//! - Checksums and verification

use adapteros_aos::aos2_writer::AOS2Writer;
use adapteros_core::{AosError, Result};
use clap::Parser;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Cell, Color, Table};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aos-info")]
#[command(about = "Display information about AOS archive files", long_about = None)]
struct Cli {
    /// Path to .aos file
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Show full manifest JSON
    #[arg(long)]
    full_manifest: bool,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Show tensor data checksums
    #[arg(long)]
    checksums: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.file.exists() {
        return Err(AosError::NotFound(format!(
            "File not found: {}",
            cli.file.display()
        )));
    }

    let info = extract_info(&cli.file)?;

    if cli.json {
        print_json(&info)?;
    } else {
        print_human_readable(&info, &cli)?;
    }

    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct AosInfo {
    file_path: String,
    file_size: u64,
    manifest_offset: u32,
    manifest_len: u32,
    weights_size: u64,
    manifest: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    tensors: Option<Vec<TensorInfo>>,
}

#[derive(Debug, serde::Serialize)]
struct TensorInfo {
    name: String,
    dtype: String,
    shape: Vec<usize>,
    size_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<String>,
}

fn extract_info(path: &PathBuf) -> Result<AosInfo> {
    // Read header
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;

    // Get file size
    let metadata = std::fs::metadata(path)
        .map_err(|e| AosError::Io(format!("Failed to read file metadata: {}", e)))?;
    let file_size = metadata.len();

    // Read manifest
    let mut file =
        File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    file.seek(std::io::SeekFrom::Start(manifest_offset as u64))
        .map_err(|e| AosError::Io(format!("Failed to seek to manifest: {}", e)))?;

    let mut manifest_bytes = vec![0u8; manifest_len as usize];
    file.read_exact(&mut manifest_bytes)
        .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;

    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;

    // Calculate weights size
    let header_size = 8u64;
    let weights_size = manifest_offset as u64 - header_size;

    // Try to parse safetensors if available
    let tensors = extract_tensor_info(path, weights_size)?;

    Ok(AosInfo {
        file_path: path.display().to_string(),
        file_size,
        manifest_offset,
        manifest_len,
        weights_size,
        manifest,
        tensors,
    })
}

fn extract_tensor_info(path: &PathBuf, weights_size: u64) -> Result<Option<Vec<TensorInfo>>> {
    use std::io::Seek;

    let mut file =
        File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    // Seek to weights section (after 8-byte header)
    file.seek(std::io::SeekFrom::Start(8))
        .map_err(|e| AosError::Io(format!("Failed to seek to weights: {}", e)))?;

    // Read first 8 bytes to get header size
    let mut header_len_bytes = [0u8; 8];
    if file.read_exact(&mut header_len_bytes).is_err() {
        return Ok(None);
    }

    let header_len = u64::from_le_bytes(header_len_bytes);
    if header_len == 0 || header_len > weights_size {
        return Ok(None);
    }

    // Read header JSON
    let mut header_bytes = vec![0u8; header_len as usize];
    if file.read_exact(&mut header_bytes).is_err() {
        return Ok(None);
    }

    let header: Value = match serde_json::from_slice(&header_bytes) {
        Ok(h) => h,
        Err(_) => return Ok(None),
    };

    // Extract tensor information
    let mut tensors = Vec::new();

    if let Some(obj) = header.as_object() {
        for (name, info) in obj {
            if name == "__metadata__" {
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

                let data_offsets = tensor_obj.get("data_offsets").and_then(|v| v.as_array());
                let size_bytes = if let Some(offsets) = data_offsets {
                    let start = offsets.get(0).and_then(|v| v.as_u64()).unwrap_or(0);
                    let end = offsets.get(1).and_then(|v| v.as_u64()).unwrap_or(0);
                    (end - start) as usize
                } else {
                    0
                };

                tensors.push(TensorInfo {
                    name: name.clone(),
                    dtype,
                    shape,
                    size_bytes,
                    checksum: None,
                });
            }
        }
    }

    tensors.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Some(tensors))
}

fn print_json(info: &AosInfo) -> Result<()> {
    let json = serde_json::to_string_pretty(info)?;
    println!("{}", json);
    Ok(())
}

fn print_human_readable(info: &AosInfo, cli: &Cli) -> Result<()> {
    // File information header
    println!(
        "\n{}",
        colorize("AOS Archive Information", Color::Cyan, true)
    );
    println!("{}", "=".repeat(60));

    // File details
    let mut file_table = Table::new();
    file_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Property").fg(Color::Green),
            Cell::new("Value").fg(Color::Yellow),
        ]);

    file_table.add_row(vec!["File", &info.file_path]);
    file_table.add_row(vec!["Total Size", &format_bytes(info.file_size)]);
    file_table.add_row(vec![
        "Format Version",
        info.manifest
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
    ]);

    println!("\n{}", file_table);

    // Archive structure
    println!("\n{}", colorize("Archive Structure", Color::Cyan, true));
    println!("{}", "-".repeat(60));

    let mut structure_table = Table::new();
    structure_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Section").fg(Color::Green),
            Cell::new("Offset").fg(Color::Yellow),
            Cell::new("Size").fg(Color::Yellow),
        ]);

    structure_table.add_row(vec!["Header", "0", "8 bytes"]);
    structure_table.add_row(vec![
        "Weights (safetensors)",
        "8",
        &format_bytes(info.weights_size),
    ]);
    structure_table.add_row(vec![
        "Manifest (JSON)",
        &info.manifest_offset.to_string(),
        &format_bytes(info.manifest_len as u64),
    ]);

    println!("\n{}", structure_table);

    // Manifest information
    if cli.full_manifest {
        println!("\n{}", colorize("Full Manifest", Color::Cyan, true));
        println!("{}", "-".repeat(60));
        println!(
            "\n{}",
            serde_json::to_string_pretty(&info.manifest).unwrap_or_default()
        );
    } else {
        println!("\n{}", colorize("Manifest Summary", Color::Cyan, true));
        println!("{}", "-".repeat(60));

        let mut manifest_table = Table::new();
        manifest_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Field").fg(Color::Green),
                Cell::new("Value").fg(Color::Yellow),
            ]);

        // Display key manifest fields
        if let Some(version) = info.manifest.get("version") {
            manifest_table.add_row(vec!["version", &version.to_string()]);
        }
        if let Some(adapter_id) = info.manifest.get("adapter_id") {
            manifest_table.add_row(vec![
                "adapter_id",
                &adapter_id.to_string().trim_matches('"'),
            ]);
        }
        if let Some(rank) = info.manifest.get("rank") {
            manifest_table.add_row(vec!["rank", &rank.to_string()]);
        }
        if let Some(alpha) = info.manifest.get("alpha") {
            manifest_table.add_row(vec!["alpha", &alpha.to_string()]);
        }
        if let Some(model) = info.manifest.get("base_model") {
            manifest_table.add_row(vec!["base_model", &model.to_string().trim_matches('"')]);
        }

        println!("\n{}", manifest_table);
        println!(
            "\n  {}",
            colorize(
                "Use --full-manifest to see complete JSON",
                Color::DarkGrey,
                false
            )
        );
    }

    // Tensor information
    if let Some(tensors) = &info.tensors {
        println!("\n{}", colorize("Tensors", Color::Cyan, true));
        println!("{}", "-".repeat(60));

        let mut tensor_table = Table::new();
        tensor_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Name").fg(Color::Green),
                Cell::new("Type").fg(Color::Yellow),
                Cell::new("Shape").fg(Color::Yellow),
                Cell::new("Size").fg(Color::Yellow),
            ]);

        for tensor in tensors {
            let shape_str = format!("{:?}", tensor.shape);
            tensor_table.add_row(vec![
                &tensor.name,
                &tensor.dtype,
                &shape_str,
                &format_bytes(tensor.size_bytes as u64),
            ]);
        }

        println!("\n{}", tensor_table);
        println!("\n  Total tensors: {}", tensors.len());
    } else {
        println!(
            "\n  {}",
            colorize("Could not parse tensor information", Color::Red, false)
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

fn format_bytes(bytes: u64) -> String {
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
