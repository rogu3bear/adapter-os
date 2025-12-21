//! Offline FP16→int4 converter for Qwen models
//!
//! Packs row-major 2D weight tensors to 4-bit (two nibbles per byte) with
//! per-output-channel (row) scale and zero-point, and writes a manifest.

use crate::output::OutputWriter;
use adapteros_core::AosError;
use anyhow::Result;
use half;
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationManifest {
    pub model_name: String,
    pub quant_method: String,
    pub bits: u8,
    pub per_channel: bool,
    pub tensors: BTreeMap<String, QuantizedTensorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedTensorInfo {
    pub shape: Vec<usize>,
    pub packed_path: String,
    pub scales_path: String,
    pub zero_points_path: String,
}

pub async fn run(
    input: &Path,
    output: &Path,
    model_name: &str,
    group_size: Option<usize>,
    output_json: bool,
    out: &OutputWriter,
) -> Result<()> {
    if !input.exists() {
        return Err(AosError::Io(format!("Input path does not exist: {}", input.display())).into());
    }
    fs::create_dir_all(output)?;

    let mut manifest = QuantizationManifest {
        model_name: model_name.to_string(),
        quant_method: "int4_per_out_channel".to_string(),
        bits: 4,
        per_channel: true,
        tensors: BTreeMap::new(),
    };

    let mut files_processed: usize = 0;

    if input.is_dir() {
        for entry in fs::read_dir(input)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .map(|e| e == "safetensors")
                .unwrap_or(false)
            {
                files_processed += 1;
                quantize_safetensors_file(&path, output, &mut manifest, group_size, out)?;
            }
        }
    } else if input
        .extension()
        .map(|e| e == "safetensors")
        .unwrap_or(false)
    {
        files_processed += 1;
        quantize_safetensors_file(input, output, &mut manifest, group_size, out)?;
    } else {
        return Err(AosError::Io(format!(
            "Input file is not .safetensors: {}",
            input.display()
        ))
        .into());
    }

    let tensors_quantized = manifest.tensors.len();

    // Write manifest
    let manifest_path = output.join("manifest.json");
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    fs::write(&manifest_path, &manifest_bytes)?;

    if output_json {
        out.json(&manifest)?;
    } else {
        out.info(format!(
            "Quantized {} tensors from {} file(s) → {}",
            tensors_quantized,
            files_processed,
            output.display()
        ));
        out.info(format!("Wrote manifest: {}", manifest_path.display()));
    }

    Ok(())
}

fn quantize_safetensors_file(
    path: &Path,
    out_dir: &Path,
    manifest: &mut QuantizationManifest,
    _group_size: Option<usize>,
    _out: &OutputWriter,
) -> Result<()> {
    let data = fs::read(path)?;
    let st = SafeTensors::deserialize(&data)?;

    for name in st.names() {
        let tv = st.tensor(name)?;
        if tv.dtype() != safetensors::Dtype::F32 && tv.dtype() != safetensors::Dtype::F16 {
            continue; // skip non-float tensors
        }
        let shape = tv.shape().to_vec();
        if shape.len() != 2 {
            continue; // pack only 2D (weights)
        }
        let rows = shape[0];
        let cols = shape[1];

        // Read tensor as f32
        let f32_data = match tv.dtype() {
            safetensors::Dtype::F32 => {
                let view: &[f32] = bytemuck::cast_slice(tv.data());
                view.to_vec()
            }
            safetensors::Dtype::F16 => {
                // Convert FP16 → f32
                let halfs: &[u16] = bytemuck::cast_slice(tv.data());
                halfs
                    .iter()
                    .map(|h| half::f16::from_bits(*h).to_f32())
                    .collect::<Vec<f32>>()
            }
            _ => unreachable!(),
        };

        // Pack per-row with a single scale & zero point per row
        let mut packed: Vec<u8> = Vec::with_capacity(rows * cols.div_ceil(2));
        let mut scales: Vec<f32> = Vec::with_capacity(rows);
        let mut zero_points: Vec<i8> = Vec::with_capacity(rows);

        let data_rows = f32_data.chunks_exact(cols);
        for row in data_rows {
            let (scale, zp) = compute_affine_scale_zero_point(row);
            scales.push(scale);
            zero_points.push(zp);
            pack_row_int4(row, scale, zp, &mut packed);
        }

        // Write outputs
        let safe_name = sanitize_tensor_name(name);
        let base = out_dir.join(&safe_name);
        let packed_path = base.with_extension("q4.bin");
        let scales_path = base.with_extension("scales.f32.bin");
        let zps_path = base.with_extension("zps.i8.bin");

        write_all_bytes(&packed_path, &packed)?;
        write_all_bytes(&scales_path, bytemuck::cast_slice(&scales))?;
        write_all_bytes(&zps_path, bytemuck::cast_slice(&zero_points))?;

        manifest.tensors.insert(
            name.to_string(),
            QuantizedTensorInfo {
                shape,
                packed_path: packed_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                scales_path: scales_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                zero_points_path: zps_path.file_name().unwrap().to_string_lossy().into_owned(),
            },
        );
    }

    info!(
        input = %path.display(),
        tensors = manifest.tensors.len(),
        "Quantized safetensors file"
    );
    Ok(())
}

fn write_all_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = fs::File::create(path)?;
    f.write_all(bytes)?;
    Ok(())
}

fn sanitize_tensor_name(name: &str) -> String {
    name.replace('/', "__").replace('.', "_")
}

fn compute_affine_scale_zero_point(row: &[f32]) -> (f32, i8) {
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for &v in row.iter() {
        if v < min_v {
            min_v = v;
        }
        if v > max_v {
            max_v = v;
        }
    }
    let range = (max_v - min_v).max(1e-8);
    let scale = range / 15.0; // 4-bit
    let zp = (-min_v / scale).round().clamp(0.0, 15.0) as i8;
    (scale, zp)
}

fn pack_row_int4(row: &[f32], scale: f32, zp: i8, dst: &mut Vec<u8>) {
    let mut i = 0;
    let n = row.len();
    while i < n {
        let q0 = quantize_to_4bit(row[i], scale, zp);
        let q1 = if i + 1 < n {
            quantize_to_4bit(row[i + 1], scale, zp)
        } else {
            0
        };
        let byte = (q0 & 0x0F) | ((q1 & 0x0F) << 4);
        dst.push(byte);
        i += 2;
    }
}

#[inline]
fn quantize_to_4bit(v: f32, scale: f32, zp: i8) -> u8 {
    let q = (v / scale + (zp as f32)).round();
    q.clamp(0.0, 15.0) as u8
}
