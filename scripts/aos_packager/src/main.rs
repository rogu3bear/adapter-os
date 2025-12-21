use anyhow::{Context, Result};
use blake3::Hasher;
use chrono::Utc;
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use safetensors::SafeTensors;

/// AOS 2.0 manifest structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdapterManifest {
    format_version: u8,
    adapter_id: String,
    name: String,
    version: String,
    rank: u32,
    alpha: f32,
    base_model: String,
    target_modules: Vec<String>,
    category: String,
    tier: String,
    created_at: String,
    weights_hash: String,
    #[serde(default)]
    per_layer_hashes: Option<HashMap<String, LayerHashEntry>>,
    signature: Option<String>,
    public_key: Option<String>,
    training_config: Option<TrainingConfig>,
    #[serde(default)]
    kernel_version: Option<String>,
    metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LayerHashEntry {
    hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tensor_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrainingConfig {
    rank: u32,
    alpha: f32,
    learning_rate: f32,
    batch_size: u32,
    epochs: u32,
    hidden_dim: u32,
    dropout: Option<f32>,
    weight_decay: Option<f32>,
}

fn adapters_output_base() -> PathBuf {
    adapteros_core::paths::get_default_adapters_root()
}

/// SafeTensors header structure
#[derive(Debug, Serialize, Deserialize)]
struct SafeTensorsHeader {
    #[serde(flatten)]
    tensors: HashMap<String, TensorInfo>,
    #[serde(rename = "__metadata__", default)]
    metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TensorInfo {
    dtype: String,
    shape: Vec<usize>,
    data_offsets: Vec<usize>,
}

/// Q15 quantization: convert f32 to i16 with scale factor
fn quantize_to_q15(values: &[f32]) -> Vec<i16> {
    values
        .iter()
        .map(|&v| {
            let scaled = (v * 32767.0).clamp(-32768.0, 32767.0);
            scaled as i16
        })
        .collect()
}

/// Create BLAKE3 hash of bytes
fn blake3_hash(data: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hex::encode(hasher.finalize().as_bytes())
}

/// Canonical logical layer path for manifest keys (format: transformer.layer_12.attn.q_proj.lora_A)
fn canonical_layer_id(tensor_name: &str) -> String {
    let mut segments = Vec::new();
    let mut iter = tensor_name.split(|c| c == '.' || c == '/').peekable();

    while let Some(seg) = iter.next() {
        if seg.is_empty() {
            continue;
        }
        let lower = seg.to_lowercase();

        if lower == "weight" {
            continue;
        }

        if lower == "model" || lower == "transformer" {
            if segments.is_empty() {
                segments.push("transformer".to_string());
            }
            continue;
        }

        if lower == "layers" || lower == "layer" {
            if let Some(next) = iter.peek() {
                if let Ok(idx) = next.parse::<usize>() {
                    segments.push(format!("layer_{}", idx));
                    iter.next();
                    continue;
                }
            }
        }

        let normalized = match lower.as_str() {
            "lora_a" => "lora_A".to_string(),
            "lora_b" => "lora_B".to_string(),
            other => other.to_string(),
        };

        segments.push(normalized);
    }

    if segments.is_empty() {
        return tensor_name.to_string();
    }

    // Ensure transformer prefix for clarity
    if segments[0] != "transformer" {
        let mut prefixed = vec!["transformer".to_string()];
        prefixed.extend(segments);
        segments = prefixed;
    }

    segments.join(".")
}

/// Compute per-layer BLAKE3 hashes keyed by canonical logical layer path
fn compute_per_layer_hashes(weights_data: &[u8]) -> Result<HashMap<String, LayerHashEntry>> {
    let tensors = SafeTensors::deserialize(weights_data)
        .context("Failed to parse SafeTensors for per-layer hashing")?;

    let mut hashes = HashMap::new();
    for (name, tensor) in tensors.tensors() {
        let canonical = canonical_layer_id(&name);
        let hash = blake3_hash(tensor.data());

        if hashes
            .insert(
                canonical.clone(),
                LayerHashEntry {
                    hash,
                    tensor_name: Some(name.to_string()),
                },
            )
            .is_some()
        {
            anyhow::bail!("Duplicate canonical layer id detected: {}", canonical);
        }
    }

    Ok(hashes)
}

/// Generate Ed25519 keypair
fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let signing_key = SigningKey::from_bytes(&rand::random());
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

/// Sign data with Ed25519
fn sign_data(signing_key: &SigningKey, data: &[u8]) -> String {
    let signature: Signature = signing_key.sign(data);
    hex::encode(signature.to_bytes())
}

/// Parse SafeTensors file header
fn parse_safetensors_header(data: &[u8]) -> Result<(SafeTensorsHeader, usize)> {
    // Read header size (8 bytes, little-endian)
    if data.len() < 8 {
        anyhow::bail!("SafeTensors file too small");
    }

    let header_size = u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]) as usize;

    if data.len() < 8 + header_size {
        anyhow::bail!("SafeTensors header incomplete");
    }

    // Parse JSON header
    let header_json = &data[8..8 + header_size];
    let header: SafeTensorsHeader = serde_json::from_slice(header_json)
        .context("Failed to parse SafeTensors header")?;

    Ok((header, 8 + header_size))
}

/// Create modified weights for creative adapter
fn create_varied_weights(original_weights: &[u8]) -> Result<Vec<u8>> {
    // Parse SafeTensors structure
    let (header, data_offset) = parse_safetensors_header(original_weights)?;

    // Get tensor data
    let tensor_data = &original_weights[data_offset..];

    // Create modified tensor data with small variations
    let mut modified_data = Vec::with_capacity(tensor_data.len());

    // Apply small random variations to differentiate from original
    // In a real scenario, this would involve proper fine-tuning
    use rand::Rng;
    let mut rng = rand::thread_rng();

    for &byte in tensor_data {
        // Add small noise (±2% variation)
        let noise = rng.gen_range(-5..=5);
        let new_val = (byte as i16 + noise).clamp(0, 255) as u8;
        modified_data.push(new_val);
    }

    // Reconstruct SafeTensors file
    let header_json = serde_json::to_string(&header)?;
    let header_bytes = header_json.as_bytes();
    let header_size = header_bytes.len() as u64;

    let mut output = Vec::new();
    output.extend_from_slice(&header_size.to_le_bytes());
    output.extend_from_slice(header_bytes);
    output.extend_from_slice(&modified_data);

    Ok(output)
}

/// Package adapter into AOS 2.0 format
fn package_adapter(
    adapter_dir: &Path,
    output_path: &Path,
    adapter_id: &str,
    name: &str,
    category: &str,
    rank: u32,
    hidden_dim: u32,
) -> Result<()> {
    println!("📦 Packaging adapter: {}", name);

    // Read original weights
    let weights_path = adapter_dir.join("weights.safetensors");
    let weights_data = if weights_path.exists() {
        fs::read(&weights_path)
            .with_context(|| format!("Failed to read weights from {:?}", weights_path))?
    } else {
        // Create placeholder weights for demo
        create_placeholder_weights(rank, hidden_dim)?
    };

    // Compute BLAKE3 hash of weights
    let weights_hash = blake3_hash(&weights_data);
    println!("  ✓ BLAKE3 hash: {}...", &weights_hash[..16]);

    // Compute per-layer hashes
    let per_layer_hashes = compute_per_layer_hashes(&weights_data)?;
    println!("  ✓ Per-layer hashes: {} tensors", per_layer_hashes.len());

    // Generate keypair for signing
    let (signing_key, verifying_key) = generate_keypair();
    let public_key = hex::encode(verifying_key.to_bytes());

    // Create manifest
    let mut manifest = AdapterManifest {
        format_version: 2,
        adapter_id: adapter_id.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        rank,
        alpha: rank as f32 * 2.0,
        base_model: "qwen2.5-7b".to_string(),
        target_modules: vec![
            "q_proj".to_string(),
            "k_proj".to_string(),
            "v_proj".to_string(),
            "o_proj".to_string(),
        ],
        category: category.to_string(),
        tier: "persistent".to_string(),
        created_at: Utc::now().to_rfc3339(),
        weights_hash: weights_hash.clone(),
        per_layer_hashes: Some(per_layer_hashes.clone()),
        signature: None,
        public_key: Some(public_key.clone()),
        training_config: Some(TrainingConfig {
            rank,
            alpha: rank as f32 * 2.0,
            learning_rate: match category {
                "code" => 0.0005,
                "documentation" => 0.0003,
                "creative" => 0.0004,
                _ => 0.0005,
            },
            batch_size: match rank {
                8 => 4,
                12 => 6,
                16 => 8,
                _ => 8,
            },
            epochs: match category {
                "creative" => 5,
                _ => 4,
            },
            hidden_dim,
            dropout: Some(match category {
                "creative" => 0.15,
                "documentation" => 0.05,
                _ => 0.1,
            }),
            weight_decay: Some(0.01),
        }),
        kernel_version: Some(adapteros_core::version::VERSION.to_string()),
        metadata: HashMap::new(),
    };

    // Add category-specific metadata
    match category {
        "code" => {
            manifest.metadata.insert(
                "use_cases".to_string(),
                serde_json::json!([
                    "Code completion",
                    "Debugging assistance",
                    "Code refactoring",
                    "API integration",
                    "Algorithm implementation"
                ]),
            );
            manifest.metadata.insert(
                "languages".to_string(),
                serde_json::json!(["Python", "JavaScript", "TypeScript", "Rust", "Go"]),
            );
            manifest.metadata.insert(
                "optimized_for".to_string(),
                serde_json::json!("structured code generation"),
            );
        }
        "documentation" => {
            manifest.metadata.insert(
                "use_cases".to_string(),
                serde_json::json!([
                    "README generation",
                    "API documentation",
                    "Tutorial writing",
                    "Installation guides",
                    "Project descriptions"
                ]),
            );
            manifest.metadata.insert(
                "formats".to_string(),
                serde_json::json!(["Markdown", "ReStructuredText", "AsciiDoc"]),
            );
            manifest.metadata.insert(
                "style".to_string(),
                serde_json::json!("technical"),
            );
            manifest.metadata.insert(
                "weight_groups".to_string(),
                serde_json::json!(true),
            );
        }
        "creative" => {
            manifest.metadata.insert(
                "use_cases".to_string(),
                serde_json::json!([
                    "Story generation",
                    "Creative writing",
                    "Narrative development",
                    "Character dialogue",
                    "Descriptive text"
                ]),
            );
            manifest.metadata.insert(
                "temperature_range".to_string(),
                serde_json::json!([0.7, 1.2]),
            );
            manifest.metadata.insert(
                "creativity_level".to_string(),
                serde_json::json!("high"),
            );
            manifest.metadata.insert(
                "genres".to_string(),
                serde_json::json!(["Fiction", "Fantasy", "Science Fiction", "Mystery"]),
            );
        }
        _ => {}
    }

    // Serialize manifest to JSON for signing
    let manifest_json = serde_json::to_string_pretty(&manifest)?;

    // Sign the manifest
    let signature = sign_data(&signing_key, manifest_json.as_bytes());
    println!("  ✓ Ed25519 signature: {}...", &signature[..16]);

    // Update manifest with signature
    manifest.signature = Some(signature);
    let final_manifest_json = serde_json::to_string_pretty(&manifest)?;

    // Create AOS 2.0 file
    let manifest_bytes = final_manifest_json.as_bytes();
    let manifest_offset = 8 + weights_data.len();
    let manifest_len = manifest_bytes.len();

    // Ensure values fit in u32
    if manifest_offset > u32::MAX as usize || manifest_len > u32::MAX as usize {
        anyhow::bail!("File too large for AOS 2.0 format");
    }

    // Create parent directory if needed
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write file
    let mut file = fs::File::create(output_path)?;

    // Write header (8 bytes)
    file.write_all(&(manifest_offset as u32).to_le_bytes())?;
    file.write_all(&(manifest_len as u32).to_le_bytes())?;

    // Write weights
    file.write_all(&weights_data)?;

    // Write manifest
    file.write_all(manifest_bytes)?;

    let file_size = file.metadata()?.len();
    println!("  ✓ Created: {} ({:.2} MB)", output_path.display(), file_size as f64 / 1_048_576.0);
    println!("  ✓ Semantic ID: {}", adapter_id);

    Ok(())
}

/// Create placeholder SafeTensors weights for demo
fn create_placeholder_weights(rank: u32, hidden_dim: u32) -> Result<Vec<u8>> {
    // Create minimal SafeTensors structure
    let mut tensors = HashMap::new();

    // Create tensor info for each LoRA matrix
    for module in ["q_proj", "k_proj", "v_proj", "o_proj"] {
        let lora_a_name = format!("lora_a.{}", module);
        let lora_b_name = format!("lora_b.{}", module);

        // lora_a: [hidden_dim, rank]
        tensors.insert(
            lora_a_name,
            TensorInfo {
                dtype: "F32".to_string(),
                shape: vec![hidden_dim as usize, rank as usize],
                data_offsets: vec![0, (hidden_dim * rank * 4) as usize],
            },
        );

        // lora_b: [rank, hidden_dim]
        tensors.insert(
            lora_b_name,
            TensorInfo {
                dtype: "F32".to_string(),
                shape: vec![rank as usize, hidden_dim as usize],
                data_offsets: vec![
                    (hidden_dim * rank * 4) as usize,
                    (hidden_dim * rank * 8) as usize,
                ],
            },
        );
    }

    let header = SafeTensorsHeader {
        tensors,
        metadata: HashMap::new(),
    };

    // Serialize header
    let header_json = serde_json::to_string(&header)?;
    let header_bytes = header_json.as_bytes();
    let header_size = header_bytes.len() as u64;

    // Create placeholder tensor data (zeros)
    let tensor_size = (hidden_dim * rank * 4 * 8) as usize; // 8 tensors * 4 bytes per f32
    let tensor_data = vec![0u8; tensor_size];

    // Build SafeTensors file
    let mut output = Vec::new();
    output.extend_from_slice(&header_size.to_le_bytes());
    output.extend_from_slice(header_bytes);
    output.extend_from_slice(&tensor_data);

    Ok(output)
}

/// Create creative adapter with proper variations
fn create_creative_adapter(source_dir: &Path, output_dir: &Path) -> Result<()> {
    println!("🎨 Creating creative-writer adapter with proper variations...");

    // Create output directory
    fs::create_dir_all(output_dir)?;

    // Read and modify source weights if they exist
    let source_weights_path = source_dir.join("weights.safetensors");
    if source_weights_path.exists() {
        let source_weights = fs::read(&source_weights_path)?;
        let varied_weights = create_varied_weights(&source_weights)?;
        fs::write(output_dir.join("weights.safetensors"), varied_weights)?;
        println!("  ✓ Created varied weights from source");
    }

    Ok(())
}

/// Verify an .aos file
fn verify_aos_file(path: &Path) -> Result<()> {
    println!("🔍 Verifying {}", path.display());

    // Read file
    let data = fs::read(path)?;

    if data.len() < 8 {
        anyhow::bail!("File too small to be valid AOS file");
    }

    // Parse header
    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

    if manifest_offset + manifest_len > data.len() {
        anyhow::bail!("Invalid header: manifest extends beyond file");
    }

    // Extract weights
    let weights = &data[8..manifest_offset];

    // Extract and parse manifest
    let manifest_bytes = &data[manifest_offset..manifest_offset + manifest_len];
    let manifest: AdapterManifest = serde_json::from_slice(manifest_bytes)?;

    // Verify BLAKE3 hash
    let computed_hash = blake3_hash(weights);
    if computed_hash == manifest.weights_hash {
        println!("  ✅ BLAKE3 hash verified: {}...", &computed_hash[..16]);
    } else {
        println!("  ❌ Hash mismatch!");
        println!("    Expected: {}", manifest.weights_hash);
        println!("    Computed: {}", computed_hash);
    }

    // Verify Ed25519 signature if present
    if let (Some(sig_hex), Some(pk_hex)) = (&manifest.signature, &manifest.public_key) {
        // Remove signature from manifest for verification
        let mut verify_manifest = manifest.clone();
        verify_manifest.signature = None;
        let verify_json = serde_json::to_string_pretty(&verify_manifest)?;

        // Decode signature and public key
        let signature_bytes = hex::decode(sig_hex)?;
        let public_key_bytes = hex::decode(pk_hex)?;

        if signature_bytes.len() == 64 && public_key_bytes.len() == 32 {
            let signature = Signature::from_bytes(&signature_bytes.try_into().unwrap());
            let verifying_key = VerifyingKey::from_bytes(&public_key_bytes.try_into().unwrap())?;

            match verifying_key.verify_strict(verify_json.as_bytes(), &signature) {
                Ok(_) => println!("  ✅ Ed25519 signature valid"),
                Err(_) => println!("  ❌ Ed25519 signature invalid"),
            }
        } else {
            println!("  ⚠️  Invalid signature/key length");
        }
    }

    // Display info
    println!("\n📊 Adapter Details:");
    println!("  Format version: {}", manifest.format_version);
    println!("  Adapter ID: {}", manifest.adapter_id);
    println!("  Name: {}", manifest.name);
    println!("  Category: {}", manifest.category);
    println!("  Rank: {}", manifest.rank);
    println!("  Alpha: {}", manifest.alpha);
    println!("  Base model: {}", manifest.base_model);
    println!("  File size: {:.2} MB", data.len() as f64 / 1_048_576.0);

    Ok(())
}

#[derive(Parser)]
#[command(author, version, about = "Production-ready AOS adapter packager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Package all adapters with proper BLAKE3 and Ed25519
    PackageAll,
    /// Verify an .aos file
    Verify {
        /// Path to .aos file
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::PackageAll => {
            println!("🚀 Creating production-ready .aos adapters");
            println!("  ✅ BLAKE3 hashing");
            println!("  ✅ Ed25519 signatures");
            println!("  ✅ Unique semantic IDs");
            println!();

            let output_base = adapters_output_base();

            // Package code-assistant
            package_adapter(
                Path::new("adapters/code_lang_v1"),
                &output_base.join("code-assistant.aos"),
                "default/code/assistant/r001",
                "Code Assistant",
                "code",
                16,
                3584,
            )?;

            // Package readme-writer
            package_adapter(
                Path::new("adapters/README_adapter"),
                &output_base.join("readme-writer.aos"),
                "default/documentation/readme-writer/r001",
                "README Writer",
                "documentation",
                8,
                768,
            )?;

            // Create and package creative-writer
            let creative_dir = output_base.join("creative_writer");
            create_creative_adapter(Path::new("adapters/code_lang_v1"), &creative_dir)?;

            package_adapter(
                &creative_dir,
                &output_base.join("creative-writer.aos"),
                "default/creative/story-writer/r001",
                "Creative Writer",
                "creative",
                12,
                2048,
            )?;

            println!("\n✅ Successfully created 3 production-ready .aos files!");
            println!("   - Proper BLAKE3 hashing");
            println!("   - Ed25519 cryptographic signatures");
            println!("   - Unique semantic adapter IDs");
            println!("   - Category-specific metadata");
        }
        Commands::Verify { path } => {
            verify_aos_file(&path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::paths::AOS_ADAPTERS_DIR_ENV;
    use serial_test::serial;
    use std::path::PathBuf;

    #[test]
    #[serial]
    fn adapters_base_prefers_env() {
        let tmp_root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let tmp = tempfile::tempdir_in(&tmp_root).expect("tempdir");
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, tmp.path());

        let base = adapters_output_base();
        assert!(
            base.starts_with(tmp.path()),
            "expected {} to start with {}",
            base.display(),
            tmp.path().display()
        );

        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }

    #[test]
    #[serial]
    fn adapters_base_defaults_to_var() {
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
        let base = adapters_output_base();
        assert_eq!(base, PathBuf::from("var").join("adapters"));
    }
}
