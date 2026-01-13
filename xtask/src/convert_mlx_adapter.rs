//! Convert mlx-lm LoRA adapter to AdapterOS LoRAWeights JSON format

use anyhow::{Context, Result};
use clap::Parser;
use safetensors::SafeTensors;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Parser, Clone)]
pub struct ConvertMlxAdapterArgs {
    /// Input mlx-lm adapter directory (contains adapters.safetensors)
    #[arg(long)]
    pub input_dir: PathBuf,

    /// Output path for lora_weights.json (defaults to input_dir/lora_weights.json)
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Parse mlx-lm tensor key to extract module name and lora type
///
/// mlx-lm format: model.layers.{i}.self_attn.{q,k,v,o}_proj.lora_{a,b}
///                model.layers.{i}.mlp.{gate,up,down}_proj.lora_{a,b}
fn parse_tensor_key(key: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = key.split('.').collect();

    // Find lora_a or lora_b
    let lora_type = parts.iter().find_map(|&p| {
        let lower = p.to_lowercase();
        if lower.contains("lora_a") {
            Some("lora_a".to_string())
        } else if lower.contains("lora_b") {
            Some("lora_b".to_string())
        } else {
            None
        }
    })?;

    // Find projection name (q_proj, v_proj, gate_proj, etc.)
    let proj_name = parts.iter().find(|&&p| p.ends_with("_proj"))?;

    Some((proj_name.to_string(), lora_type))
}

/// Convert f32 bytes (little-endian) to Vec<Vec<f32>>
fn bytes_to_matrix(data: &[u8], rows: usize, cols: usize) -> Vec<Vec<f32>> {
    let mut matrix = Vec::with_capacity(rows);
    let floats: Vec<f32> = data
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    for row in floats.chunks(cols) {
        matrix.push(row.to_vec());
    }

    matrix
}

pub fn run(args: ConvertMlxAdapterArgs) -> Result<()> {
    let adapter_path = args.input_dir.join("adapters.safetensors");

    if !adapter_path.exists() {
        anyhow::bail!("Adapter file not found: {}", adapter_path.display());
    }

    println!("📦 Converting mlx-lm adapter to LoRAWeights format");
    println!("   Input: {}", adapter_path.display());

    // Read safetensors file
    let data = std::fs::read(&adapter_path)
        .with_context(|| format!("Failed to read {}", adapter_path.display()))?;

    let tensors = SafeTensors::deserialize(&data).context("Failed to parse safetensors")?;

    println!("   Loaded {} tensors", tensors.names().len());

    // Group tensors by module name
    let mut modules: HashMap<String, HashMap<String, Vec<Vec<f32>>>> = HashMap::new();

    for (name, tensor) in tensors.tensors() {
        let name_str = name.to_string();
        let Some((module_name, lora_type)) = parse_tensor_key(&name_str) else {
            println!("   Skipping unknown tensor: {}", name_str);
            continue;
        };

        let shape = tensor.shape();
        if shape.len() != 2 {
            println!(
                "   Skipping non-2D tensor: {} (shape: {:?})",
                name_str, shape
            );
            continue;
        }

        let rows = shape[0];
        let cols = shape[1];
        let matrix = bytes_to_matrix(tensor.data(), rows, cols);

        modules
            .entry(module_name)
            .or_default()
            .insert(lora_type, matrix);
    }

    // Build LoRAWeights structure
    let mut lora_modules: HashMap<String, serde_json::Value> = HashMap::new();

    for (module_name, matrices) in &modules {
        let lora_a = matrices.get("lora_a");
        let lora_b = matrices.get("lora_b");

        if let (Some(a), Some(b)) = (lora_a, lora_b) {
            lora_modules.insert(
                module_name.clone(),
                json!({
                    "lora_a": a,
                    "lora_b": b
                }),
            );

            println!(
                "   ✓ {}: lora_a={}x{}, lora_b={}x{}",
                module_name,
                a.len(),
                a.first().map(|r| r.len()).unwrap_or(0),
                b.len(),
                b.first().map(|r| r.len()).unwrap_or(0)
            );
        } else {
            println!(
                "   ⚠ {}: incomplete (missing lora_a or lora_b)",
                module_name
            );
        }
    }

    // Create final JSON structure matching LoRAWeights
    let lora_weights = json!({
        "modules": lora_modules,
        "lora_a": Vec::<Vec<f32>>::new(),
        "lora_b": Vec::<Vec<f32>>::new()
    });

    // Write output
    let output_path = args
        .output
        .unwrap_or_else(|| args.input_dir.join("lora_weights.json"));
    let json_str = serde_json::to_string_pretty(&lora_weights)?;
    std::fs::write(&output_path, &json_str)
        .with_context(|| format!("Failed to write {}", output_path.display()))?;

    let size_mb = json_str.len() as f64 / (1024.0 * 1024.0);

    println!("\n✅ Created: {}", output_path.display());
    println!("   Size: {:.2} MB", size_mb);
    println!("   Modules: {}", lora_modules.len());

    Ok(())
}
